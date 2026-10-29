//! `series {}` and `state {}`: series expressions become nodes of the series DAG.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

impl Checker<'_> {
    pub(super) fn add_node(&mut self, kind: NodeKind, ty: Ty, class: Class, lookback: CExpr) -> usize {
        let key = format!("{:?}", kind);
        // Bar fields are the inputs: always one node each, so handlers and series agree.
        let cons = self.opts.share || matches!(kind, NodeKind::Field(..));
        if cons && let Some(&id) = self.consed.get(&key) {
            return id;
        }
        let id = self.ir.nodes.len();
        self.ir.nodes.push(Node {
            kind,
            ty,
            class,
            lookback,
        });
        if cons {
            self.consed.insert(key, id);
        }
        id
    }

    pub(super) fn series_decl(&mut self, d: &VarDecl) {
        let Some(id) = self.sexpr(&d.init) else {
            self.poison(&d.name);
            return;
        };
        let ty = self.ir.nodes[id].ty;
        if !self.assignable(d.ty, ty, false, d.init.span, &d.name.name) {
            self.poison(&d.name);
            return;
        }
        if d.ty == Ty::Int {
            self.err(
                Diag::new(
                    Code::IntSeries,
                    "a series is `real` or `bool`",
                    d.ty_span,
                    "write `real`",
                )
                .note("bar values are reals; whole-number series are not supported"),
            );
            self.poison(&d.name);
            return;
        }
        if self.declare(&d.name, Name::Series(id, d.name.span)) {
            self.ir.series.push((d.name.name.clone(), id));
        }
    }

    pub(super) fn sexpr(&mut self, e: &Expr) -> Option<usize> {
        match &e.kind {
            ExprKind::Num(v, int) => Some(self.add_node(
                NodeKind::Num(*v),
                if *int { Ty::Int } else { Ty::Real },
                Class::Always,
                CExpr::Num(1.0),
            )),
            ExprKind::Bool(b) => Some(self.add_node(NodeKind::Bool(*b), Ty::Bool, Class::Always, CExpr::Num(1.0))),
            ExprKind::Name(n) => {
                if let Some(f) = field_of(n) {
                    if !self.bare_fields(n, e.span) {
                        return None;
                    }
                    let class = if f == Field::Open || self.ir.period_clock {
                        Class::Maybe
                    } else {
                        Class::Always
                    };
                    return Some(self.add_node(NodeKind::Field(0, f), Ty::Real, class, CExpr::Num(1.0)));
                }
                match self.names.get(n).cloned() {
                    Some((Name::Poisoned, _)) => None,
                    Some((Name::Param(i), _)) => {
                        let ty = self.ir.params[i].ty;
                        Some(self.add_node(NodeKind::Param(i), ty, Class::Always, CExpr::Num(1.0)))
                    }
                    Some((Name::Series(id, _), _)) => Some(id),
                    Some((Name::State(_), decl)) => {
                        self.err(
                            Diag::new(
                                Code::SeriesReadsState,
                                format!("series cannot read state `{}`", n),
                                e.span,
                                "state is written by handlers",
                            )
                            .also(decl, "declared in `state {}`")
                            .note("series are computed from bars and params only, before any handler runs"),
                        );
                        None
                    }
                    Some((Name::Local, _)) => None,
                    Some((Name::Function(_) | Name::Indicator(_), decl)) => {
                        self.err(
                            Diag::new(
                                Code::FunctionAsValue,
                                format!("`{}` is called with arguments", n),
                                e.span,
                                "not a value",
                            )
                            .also(decl, "declared here"),
                        );
                        None
                    }
                    Some((Name::Instrument(_), _)) => {
                        self.err(
                            Diag::new(
                                Code::InstrumentAsValue,
                                format!("`{}` is an instrument", n),
                                e.span,
                                "read one of its fields",
                            )
                            .note(format!("write `{}.close` (or open, high, low, volume)", n)),
                        );
                        None
                    }
                    None => {
                        if n == "held" {
                            self.err(
                                Diag::new(
                                    Code::SeriesReadsHeld,
                                    "series cannot read `held`",
                                    e.span,
                                    "the position is read in handlers",
                                )
                                .note("series are computed from bars and params only"),
                            );
                        } else if n == "keep" {
                            self.err(Diag::new(
                                Code::KeepInSeries,
                                "`keep` is a handler value",
                                e.span,
                                "not a series",
                            ));
                        } else {
                            self.unknown(n, e.span);
                        }
                        None
                    }
                }
            }
            ExprKind::Unary(op, a) => {
                let a = self.sexpr(a)?;
                let ta = self.ir.nodes[a].ty;
                let ty = self.unary_ty(*op, ta, e.span)?;
                let (class, lb) = (self.ir.nodes[a].class, self.ir.nodes[a].lookback.clone());
                Some(self.add_node(NodeKind::Unary(*op, a), ty, class, lb))
            }
            ExprKind::Binary(op, a, b) => {
                let (sa, sb) = (a.span, b.span);
                let a = self.sexpr(a);
                let b = self.sexpr(b);
                let (a, b) = (a?, b?);
                let ty = self.binary_ty(*op, self.ir.nodes[a].ty, self.ir.nodes[b].ty, sa, sb, e.span)?;
                let mut class = self.ir.nodes[a].class.max(self.ir.nodes[b].class);
                if *op == BinOp::Div && !self.node_nonzero(b) {
                    class = Class::Maybe;
                }
                let lb = self.lookback_max(&[a, b]);
                Some(self.add_node(NodeKind::Binary(*op, a, b), ty, class, lb))
            }
            ExprKind::Ternary(c, a, b) => {
                let cs = c.span;
                let (c, a2, b2) = (self.sexpr(c), self.sexpr(a), self.sexpr(b));
                let (c, x, y) = (c?, a2?, b2?);
                if self.ir.nodes[c].ty != Ty::Bool {
                    self.err(Diag::new(
                        Code::ConditionNotBool,
                        "a condition must be `bool`",
                        cs,
                        format!("this is `{}`", self.ir.nodes[c].ty.name()),
                    ));
                    return None;
                }
                let mut ty = self.ternary_ty(self.ir.nodes[x].ty, self.ir.nodes[y].ty, a.span, b.span)?;
                let class = self.ir.nodes[c]
                    .class
                    .max(self.ir.nodes[x].class)
                    .max(self.ir.nodes[y].class);
                if ty == Ty::Int && class != Class::Always {
                    // An int that may be undefined is carried as a real (NaN).
                    ty = Ty::Real;
                }
                let lb = self.lookback_max(&[c, x, y]);
                Some(self.add_node(NodeKind::Ternary(c, x, y), ty, class, lb))
            }
            ExprKind::Index(x, k) => {
                let miss = self.miss_of(Ind::Lag, &[], None, e.span)?;
                self.indicator(Ind::Lag, x, k, e.span, true, miss)
            }
            ExprKind::Named(k, _) => {
                self.err(Diag::new(
                    Code::NamedArgumentOutsideCall,
                    format!("`{} = ...` outside a call's arguments", k.name),
                    e.span,
                    "not an expression",
                ));
                None
            }
            ExprKind::Member(call, value) => {
                if let ExprKind::Call(name, args) = &call.kind
                    && let Some((Name::Indicator(d), _)) = self.names.get(&name.name).cloned()
                {
                    return self.user_indicator(d, args, Some(value), e.span);
                }
                self.err(
                    Diag::new(
                        Code::MemberNotIndicator,
                        format!("`.{}` reads a value of a user indicator", value.name),
                        e.span,
                        "not one",
                    )
                    .note("write `name(args).value` for an indicator with several values"),
                );
                None
            }
            ExprKind::Call(name, args) if matches!(self.names.get(&name.name), Some((Name::Indicator(_), _))) => {
                let Some((Name::Indicator(d), _)) = self.names.get(&name.name).cloned() else {
                    return None;
                };
                self.user_indicator(d, args, None, e.span)
            }
            ExprKind::Call(name, args) if matches!(self.names.get(&name.name), Some((Name::Function(_), _))) => {
                let Some((Name::Function(f), _)) = self.names.get(&name.name).cloned() else {
                    return None;
                };
                if !self.no_named(&name.name, args) {
                    return None;
                }
                let mut ids = vec![];
                for a in args {
                    ids.push(self.sexpr(a));
                }
                let ids: Vec<usize> = ids.into_iter().collect::<Option<_>>()?;
                let tys: Vec<Ty> = ids.iter().map(|&i| self.ir.nodes[i].ty).collect();
                let ret = self.fn_args_ok(f, &tys, args, e.span)?;
                let mut class = ids
                    .iter()
                    .map(|&i| self.ir.nodes[i].class)
                    .max()
                    .unwrap_or(Class::Always);
                if self.ir.functions[f].may_undef {
                    class = Class::Maybe;
                }
                let lb = self.lookback_max(&ids);
                Some(self.add_node(NodeKind::UCall(f, ids), ret, class, lb))
            }
            ExprKind::Call(name, args) => {
                if Ind::from_name(&name.name).is_some() {
                    let (pos, named) = Self::split_args(args);
                    if name.name == "ts_sum" && pos.len() == 3 {
                        if !self.no_named("ts_sum(x, n, m)", args) {
                            return None;
                        }
                        return self.partial(pos[0], pos[1], pos[2]);
                    }
                    let ind = Ind::from_name(&name.name).unwrap();
                    if pos.len() != 2 {
                        self.err(Diag::new(
                            Code::IndicatorArity,
                            format!("`{}` takes (series, window)", name.name),
                            e.span,
                            format!("{} argument(s)", pos.len()),
                        ));
                        return None;
                    }
                    let window = self.cexpr(pos[1], &format!("`{}`'s window", ind.name()), true)?.0;
                    let miss = self.miss_of(ind, &named, Some(&window), e.span)?;
                    return self.indicator(ind, pos[0], pos[1], e.span, false, miss);
                }
                if !self.no_named(&name.name, args) {
                    return None;
                }
                if name.name == "ols_slope" || name.name == "ols_intercept" {
                    let which = if name.name == "ols_slope" {
                        PairInd::OlsSlope
                    } else {
                        PairInd::OlsIntercept
                    };
                    return self.pair(which, args, e.span);
                }
                if let Some(out) = kalman_out(&name.name) {
                    return self.kalman(out, &name.name, args, e.span);
                }
                if name.name.starts_with("xs_") {
                    self.err(Diag::new(Code::XsInSeries, format!("`{}` is computed in handlers", name.name), e.span, "not in `series {}`")
                        .note("a cross-sectional value needs every instrument's series first; compute it at the top of `on_bar` (or `on_open`/`on_close`)"));
                    return None;
                }
                let b = self.builtin(name, args.len(), e.span)?;
                let mut ids = vec![];
                for a in args {
                    ids.push(self.sexpr(a));
                }
                let ids: Vec<usize> = ids.into_iter().collect::<Option<_>>()?;
                let tys: Vec<Ty> = ids.iter().map(|&i| self.ir.nodes[i].ty).collect();
                let ty = self.builtin_ty(b, &tys, args)?;
                let mut class = ids
                    .iter()
                    .map(|&i| self.ir.nodes[i].class)
                    .max()
                    .unwrap_or(Class::Always);
                if b == Builtin::Defined {
                    class = Class::Always;
                }
                if matches!(b, Builtin::Log | Builtin::Sqrt) {
                    class = Class::Maybe;
                }
                let lb = self.lookback_max(&ids);
                Some(self.add_node(NodeKind::Call(b, ids), ty, class, lb))
            }
            ExprKind::Field(obj, field) => {
                if let Some((Name::Instrument(i), _)) = self.names.get(&obj.name).cloned() {
                    if let Some(f) = field_of(&field.name) {
                        let class = if f == Field::Open { Class::Maybe } else { Class::Always };
                        return Some(self.add_node(NodeKind::Field(i, f), Ty::Real, class, CExpr::Num(1.0)));
                    }
                    self.err(
                        Diag::new(
                            Code::NotASeriesField,
                            format!("`{}.{}` is not a series", obj.name, field.name),
                            field.span,
                            "fields: open, high, low, close, volume",
                        )
                        .note("an instrument's position, `y.held`, is read in handlers"),
                    );
                    return None;
                }
                self.err(
                    Diag::new(
                        Code::FieldsInSeries,
                        format!("`{}` is not available in `series {{}}`", obj.name),
                        e.span,
                        "no fields here",
                    )
                    .note("`fill.*` is read in `on_fill`"),
                );
                None
            }
        }
    }

    pub(super) fn real_input(&mut self, id: usize, e: &Expr, what: &str) -> bool {
        if !self.ir.nodes[id].ty.numeric() {
            self.err(Diag::new(
                Code::SeriesNotNumeric,
                format!("`{}` needs a real series", what),
                e.span,
                format!("this is `{}`", self.ir.nodes[id].ty.name()),
            ));
            return false;
        }
        true
    }

    /// One declaration of `state {}`: initialized from literals and parameters.
    pub(super) fn state_decl(&mut self, d: &VarDecl) {
        let ty = d.ty;
        let init = match &d.init.kind {
            ExprKind::Bool(b) => Some((CExpr::Num(if *b { 1.0 } else { 0.0 }), Ty::Bool)),
            _ => self.cexpr(&d.init, "a state's initial value", false),
        };
        match init {
            Some((init, ity)) if self.assignable(ty, ity, false, d.init.span, &d.name.name) => {
                let id = self.ir.state.len();
                self.ir.state.push(StateVar {
                    name: d.name.name.clone(),
                    ty,
                    init,
                    maybe: false,
                    span: d.name.span,
                });
                self.declare(&d.name, Name::State(id));
            }
            _ => self.poison(&d.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, ok};
    use super::super::testing_ir::series;
    use crate::frontend::ast::Ty;
    use crate::frontend::diag::Code;
    use crate::ir::{Ind, NodeKind};
    use crate::sema::{Options, check};

    #[test]
    fn identical_expressions_share_one_node() {
        let src = "series { real a = sma(close, 5); real b = sma(close, 5) + 1; } on_bar { target(a + b); }";
        let ir = ok(src);
        let NodeKind::Binary(_, left, _) = series(&ir, "b").kind else {
            panic!()
        };
        assert_eq!(left, ir.series[0].1);
        // Without sharing, only bar fields are merged.
        let program = crate::frontend::parse(src).unwrap();
        let ir = check(&program, &Options { share: false }).unwrap();
        let smas = ir
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Ind(Ind::Sma, ..)))
            .count();
        assert_eq!(smas, 2);
        let fields = ir
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Field(..)))
            .count();
        assert_eq!(fields, 1);
    }

    #[test]
    fn series_rules() {
        let ir = ok("series { bool up = close > open; real n = up ? 1 : 0; } on_bar { target(n); }");
        assert_eq!(series(&ir, "up").ty, Ty::Bool);
        assert_eq!(series(&ir, "n").ty, Ty::Real);
        assert_eq!(errors("series { int n = 1; } on_bar { target(1); }"), [Code::IntSeries]);
        assert_eq!(
            errors("state { real s = 0; } series { real a = s; } on_bar { target(a); }"),
            [Code::SeriesReadsState]
        );
        assert_eq!(
            errors("series { real a = held; } on_bar { target(a); }"),
            [Code::SeriesReadsHeld]
        );
        assert_eq!(
            errors("series { real a = keep; } on_bar { target(a); }"),
            [Code::KeepInSeries]
        );
        assert_eq!(
            errors("series { real a = b; real b = close; } on_bar { target(a + b); }"),
            [Code::UnknownName]
        );
        assert_eq!(
            errors("series { real a = close > 1 ? 1 : 2; real b = a ? 1 : 2; } on_bar { target(b); }"),
            [Code::ConditionNotBool]
        );
        assert_eq!(
            errors("universe {} series { real a = xs_rank(close); } on_bar { target(a); }"),
            [Code::XsInSeries]
        );
        assert_eq!(
            errors("series { real a = sma(close, 5, 1, 2); } on_bar { target(a); }"),
            [Code::IndicatorArity]
        );
        assert_eq!(
            errors("series { real a = kf_beta(close, open, 0.1); } on_bar { target(a); }"),
            [Code::IndicatorArity]
        );
        assert_eq!(
            errors("series { real a = ols_slope(close > 1, open, 5); } on_bar { target(a); }"),
            [Code::SeriesNotNumeric]
        );
        // A failed declaration is poisoned: reading it reports nothing more.
        assert_eq!(
            errors("series { real a = nope; real b = a + 1; } on_bar { target(b); }"),
            [Code::UnknownName]
        );
    }
}
