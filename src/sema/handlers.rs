//! Handlers: statements and handler expressions, where each value may be read, and targets.

use std::collections::HashMap;

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

impl Checker<'_> {
    pub(super) fn block(&mut self, body: &[Stmt], k: HandlerKind) -> Vec<HStmt> {
        self.scopes.push(HashMap::new());
        let mut out = vec![];
        for s in body {
            if let Some(h) = self.stmt(s, k) {
                out.push(h);
            }
        }
        self.scopes.pop();
        out
    }

    pub(super) fn stmt(&mut self, s: &Stmt, k: HandlerKind) -> Option<HStmt> {
        match s {
            Stmt::Local(d) => {
                let checked = self.local_decl(d, k);
                if checked.is_none()
                    && self.lookup_local(&d.name.name).is_none()
                    && !self.names.contains_key(&d.name.name)
                {
                    // Declared anyway, so later reads report nothing more.
                    let id = self.ir.locals.len();
                    self.ir.locals.push(Local {
                        name: d.name.name.clone(),
                        ty: d.ty,
                        maybe: false,
                        top: false,
                    });
                    self.scopes.last_mut().unwrap().insert(d.name.name.clone(), id);
                }
                checked
            }
            _ => self.stmt_rest(s, k),
        }
    }

    pub(super) fn local_decl(&mut self, d: &VarDecl, k: HandlerKind) -> Option<HStmt> {
        {
            {
                let v = self.hexpr(&d.init, k)?;
                if !self.assignable(d.ty, v.ty, false, d.init.span, &d.name.name) {
                    return None;
                }
                if is_reserved(&d.name.name)
                    || self.names.contains_key(&d.name.name)
                    || self.lookup_local(&d.name.name).is_some()
                {
                    self.err(Diag::new(
                        Code::ShadowedName,
                        format!("`{}` is already a name", d.name.name),
                        d.name.span,
                        "choose another name",
                    ));
                    return None;
                }
                if v.maybe && d.ty != Ty::Real {
                    self.assignable(d.ty, v.ty, true, d.init.span, &d.name.name);
                    return None;
                }
                let id = self.ir.locals.len();
                let top = self.ir.mode == Mode::Universe && self.nest == 0 && self.uctx.is_none();
                self.ir.locals.push(Local {
                    name: d.name.name.clone(),
                    ty: d.ty,
                    maybe: v.maybe,
                    top,
                });
                self.scopes.last_mut().unwrap().insert(d.name.name.clone(), id);
                Some(HStmt::Local { id, value: v })
            }
        }
    }

    pub(super) fn stmt_rest(&mut self, s: &Stmt, k: HandlerKind) -> Option<HStmt> {
        match s {
            Stmt::Local(_) => None,
            Stmt::Assign { name, value, .. } if self.uctx.is_some() && self.lookup_local(&name.name).is_none() => {
                let ctx = self.uctx.clone().unwrap();
                let Some(i) = ctx.states.iter().position(|st| st.0 == name.name) else {
                    self.err(
                        Diag::new(
                            Code::AssignInFunction,
                            format!("cannot assign to `{}` in `{}`", name.name, ctx.name),
                            name.span,
                            "not a state of this indicator",
                        )
                        .note(if ctx.is_fn {
                            "a function is pure: it declares locals and returns a value"
                        } else {
                            "an indicator assigns its own `state` values, in `update`, `enter` and `leave`"
                        }),
                    );
                    return None;
                };
                if !ctx.assign {
                    self.err(
                        Diag::new(
                            Code::AssignStateOutsideStep,
                            format!("cannot assign to state `{}` here", name.name),
                            name.span,
                            "values only read",
                        )
                        .note("states change in `update`, `enter` and `leave`"),
                    );
                    return None;
                }
                let v = self.hexpr(value, k)?;
                if !self.assignable(
                    ctx.states[i].1,
                    v.ty,
                    v.maybe && ctx.states[i].1 != Ty::Real,
                    value.span,
                    &name.name,
                ) {
                    return None;
                }
                Some(HStmt::UAssign { state: i, value: v })
            }
            Stmt::Return { value, span } => {
                let Some(ctx) = self.uctx.clone().filter(|c| c.is_fn) else {
                    self.err(
                        Diag::new(
                            Code::ReturnOutsideFunction,
                            "`return` outside a function",
                            *span,
                            "not here",
                        )
                        .note("handlers state targets; only functions of `functions {}` return a value"),
                    );
                    return None;
                };
                let v = self.hexpr(value, k)?;
                let ret = ctx.ret.unwrap_or(Ty::Real);
                if !self.assignable(
                    ret,
                    v.ty,
                    v.maybe && ret == Ty::Int,
                    value.span,
                    &format!("{}'s result", ctx.name),
                ) {
                    return None;
                }
                if v.maybe {
                    if let Some(c) = self.uctx.as_mut() {
                        c.returns_maybe = true;
                    }
                }
                Some(HStmt::Return(v))
            }
            Stmt::For {
                var, from, to, body, ..
            } if self.uctx.is_some() => {
                let (f, t) = (self.hexpr(from, k), self.hexpr(to, k));
                let id = self.ir.locals.len();
                self.ir.locals.push(Local {
                    name: var.name.clone(),
                    ty: Ty::Int,
                    maybe: false,
                    top: false,
                });
                self.scopes.push(HashMap::from([(var.name.clone(), id)]));
                self.nest += 1;
                let b = self.block(body, k);
                self.nest -= 1;
                self.scopes.pop();
                let (f, t) = (f?, t?);
                if f.ty != Ty::Int || t.ty != Ty::Int {
                    self.err(Diag::new(
                        Code::LoopBoundsNotInt,
                        "a loop's bounds are whole numbers",
                        from.span.to(to.span),
                        "not `int`",
                    ));
                    return None;
                }
                Some(HStmt::UFor {
                    var: id,
                    from: f,
                    to: t,
                    body: b,
                })
            }
            Stmt::Assign { name, value, span } => {
                let target = self
                    .lookup_local(&name.name)
                    .map(|_| (Name::Local, *span))
                    .or_else(|| self.names.get(&name.name).cloned());
                match target {
                    Some((Name::Poisoned, _)) => None,
                    Some((Name::State(i), _)) => {
                        let v = self.hexpr(value, k)?;
                        let (ty, sname) = (self.ir.state[i].ty, self.ir.state[i].name.clone());
                        if !self.assignable(ty, v.ty, v.maybe, value.span, &sname) {
                            return None;
                        }
                        Some(HStmt::Assign { state: i, value: v })
                    }
                    Some((Name::Param(_), decl)) => {
                        self.err(
                            Diag::new(
                                Code::AssignParam,
                                format!("cannot assign to parameter `{}`", name.name),
                                name.span,
                                "params are fixed for a whole run",
                            )
                            .also(decl, "declared in `params {}`")
                            .note("a value that changes during the run belongs in `state {}`"),
                        );
                        None
                    }
                    Some((Name::Series(_, _), decl)) => {
                        self.err(
                            Diag::new(
                                Code::AssignSeries,
                                format!("cannot assign to series `{}`", name.name),
                                name.span,
                                "series are computed from the bars, every bar",
                            )
                            .also(decl, "declared in `series {}`")
                            .note("a value a handler changes belongs in `state {}`"),
                        );
                        None
                    }
                    Some((Name::Instrument(_), _)) => {
                        self.err(Diag::new(
                            Code::AssignInstrument,
                            format!("cannot assign to instrument `{}`", name.name),
                            name.span,
                            "instruments are inputs",
                        ));
                        None
                    }
                    Some((Name::Function(_) | Name::Indicator(_), _)) => {
                        self.err(Diag::new(
                            Code::AssignFunction,
                            format!("cannot assign to `{}`", name.name),
                            name.span,
                            "a function or indicator",
                        ));
                        None
                    }
                    Some((Name::Local, _)) => {
                        self.err(
                            Diag::new(
                                Code::AssignLocal,
                                format!("cannot assign to local `{}`", name.name),
                                name.span,
                                "locals are fixed once declared",
                            )
                            .note("declare a new local, or use `state {}` for a value that changes"),
                        );
                        None
                    }
                    None => {
                        if field_of(&name.name).is_some() || name.name == "held" {
                            self.err(Diag::new(
                                Code::AssignReadOnly,
                                format!("cannot assign to `{}`", name.name),
                                name.span,
                                "read-only",
                            ));
                        } else {
                            self.unknown(&name.name, name.span);
                        }
                        None
                    }
                }
            }
            Stmt::Target { inst, value, span } => {
                if !matches!(k, HandlerKind::OnBar | HandlerKind::OnOpen | HandlerKind::OnClose) {
                    self.err(
                        Diag::new(
                            Code::TargetInHandler,
                            format!("`target` in `{}`", k.name()),
                            *span,
                            "targets are stated in `on_bar` (or `on_open`, `on_close`) only",
                        )
                        .note("record what you need in `state {}` and act on the next bar"),
                    );
                    return None;
                }
                let which = match (&self.ir.mode, inst) {
                    (Mode::Named(_), Some(i)) => match self.names.get(&i.name).cloned() {
                        Some((Name::Instrument(n), _)) => n,
                        _ => {
                            self.err(Diag::new(
                                Code::NotAnInstrument,
                                format!("`{}` is not an instrument", i.name),
                                i.span,
                                "not in `instruments {}`",
                            ));
                            return None;
                        }
                    },
                    (Mode::Named(names), None) => {
                        self.err(
                            Diag::new(
                                Code::TargetNeedsInstrument,
                                "which instrument's target?",
                                *span,
                                "the program names its instruments",
                            )
                            .note(format!(
                                "write `target({}, shares);`",
                                names.first().cloned().unwrap_or_default()
                            )),
                        );
                        return None;
                    }
                    (_, Some(i)) => {
                        self.err(
                            Diag::new(
                                Code::TargetNamesInstrument,
                                "`target(name, shares)` needs `instruments {}`",
                                i.span,
                                "no named instruments",
                            )
                            .note("write `target(shares);`"),
                        );
                        return None;
                    }
                    (_, None) => 0,
                };
                let v = self.hexpr(value, k)?;
                if !v.ty.numeric() {
                    self.err(Diag::new(
                        Code::TargetNotNumeric,
                        "a target is a number of shares",
                        value.span,
                        format!("this is `{}`", v.ty.name()),
                    ));
                    return None;
                }
                Some(HStmt::Target(which, v))
            }
            Stmt::If { cond, then, els, .. } => {
                let c = self.hexpr(cond, k);
                let refs = c.as_ref().map(Self::defined_refs).unwrap_or_default();
                let depth = self.refined.len();
                self.refined.extend(refs);
                self.nest += 1;
                let t = self.block(then, k);
                self.refined.truncate(depth);
                let e = els.as_ref().map(|b| self.block(b, k)).unwrap_or_default();
                self.nest -= 1;
                let c = c?;
                if c.ty != Ty::Bool {
                    self.err(
                        Diag::new(
                            Code::ConditionNotBool,
                            "type mismatch: a condition must be `bool`",
                            cond.span,
                            format!("this is `{}`", c.ty.name()),
                        )
                        .note("compare it, e.g. `x != 0`"),
                    );
                    return None;
                }
                Some(HStmt::If {
                    cond: c,
                    then: t,
                    els: e,
                })
            }
            Stmt::For {
                var, from, to, body, ..
            } => {
                let f = self.cexpr(from, "a loop bound", true);
                let t = self.cexpr(to, "a loop bound", true);
                let id = self.ir.locals.len();
                self.ir.locals.push(Local {
                    name: var.name.clone(),
                    ty: Ty::Int,
                    maybe: false,
                    top: false,
                });
                self.scopes.push(HashMap::from([(var.name.clone(), id)]));
                self.nest += 1;
                let b = self.block(body, k);
                self.nest -= 1;
                self.scopes.pop();
                Some(HStmt::For {
                    var: id,
                    from: f?.0,
                    to: t?.0,
                    body: b,
                })
            }
            Stmt::While { span, .. } => {
                self.err(
                    Diag::new(
                        Code::WhileLoop,
                        "`while` loops are not allowed",
                        Span::new(span.start, span.start + 5),
                        "may never end",
                    )
                    .note("every handler must finish: use `for (i in FROM:TO)` with bounds made of numbers and params"),
                );
                None
            }
            Stmt::Expr { expr, .. } => {
                let _ = self.hexpr(expr, k);
                self.err(
                    Diag::new(
                        Code::ExpressionStatement,
                        "this statement does nothing",
                        expr.span,
                        "its value is unused",
                    )
                    .note("a handler assigns state, declares locals, branches, loops, or states `target(...)`"),
                );
                None
            }
        }
    }

    pub(super) fn hexpr(&mut self, e: &Expr, k: HandlerKind) -> Option<HExpr> {
        let bar_side = matches!(
            k,
            HandlerKind::OnBar | HandlerKind::OnFill | HandlerKind::OnOpen | HandlerKind::OnClose
        );
        match &e.kind {
            ExprKind::Num(v, int) => Some(Self::h(HKind::Num(*v), if *int { Ty::Int } else { Ty::Real }, false)),
            ExprKind::Bool(b) => Some(Self::h(HKind::Bool(*b), Ty::Bool, false)),
            ExprKind::Name(n) => {
                if let Some(i) = self.lookup_local(n) {
                    let l = &self.ir.locals[i];
                    return Some(self.refine(Self::h(HKind::Local(i), l.ty, l.maybe)));
                }
                if let Some(ctx) = self.uctx.clone() {
                    return self.user_name(&ctx, n, e.span);
                }
                if let Some(f) = field_of(n) {
                    if !bar_side {
                        self.err(Diag::new(
                            Code::NoBar,
                            format!("`{}` has no bar to read in `{}`", n, k.name()),
                            e.span,
                            "no bar here",
                        ));
                        return None;
                    }
                    if !self.bare_fields(n, e.span) {
                        return None;
                    }
                    if k == HandlerKind::OnOpen {
                        if f == Field::Open {
                            // An instrument whose open bar has no open takes no part in `on_open`.
                            return Some(Self::h(HKind::OpenNow, Ty::Real, false));
                        }
                        self.err(
                            Diag::new(
                                Code::OpenReadsFuture,
                                format!("today's `{}` is not known at the open", n),
                                e.span,
                                "would read the future",
                            )
                            .note("in `on_open` only `open` is today's; series hold the last close's values")
                            .note(format!(
                                "declare `real last_{} = {};` in `series {{}}` to read the last close's {}",
                                n, n, n
                            )),
                        );
                        return None;
                    }
                    let class = if f == Field::Open || self.ir.period_clock {
                        Class::Maybe
                    } else {
                        Class::Always
                    };
                    let id = self.add_node(NodeKind::Field(0, f), Ty::Real, class, CExpr::Num(1.0));
                    self.reads.push((k, id));
                    // With a period clock a missing period is a row of undefined values, but handlers run
                    // only for instruments that have this period's bar, so its fields are defined there.
                    return Some(self.refine(Self::h(HKind::Node(id), Ty::Real, f == Field::Open)));
                }
                match self.names.get(n).cloned() {
                    Some((Name::Poisoned, _)) => None,
                    Some((Name::Param(i), _)) => Some(Self::h(HKind::Param(i), self.ir.params[i].ty, false)),
                    Some((Name::Series(id, _), _)) => {
                        if !bar_side {
                            self.err(Diag::new(
                                Code::NoBar,
                                format!("series `{}` has no value in `{}`", n, k.name()),
                                e.span,
                                "no bar here",
                            ));
                            return None;
                        }
                        self.reads.push((k, id));
                        let node = &self.ir.nodes[id];
                        Some(self.refine(Self::h(HKind::Node(id), node.ty, node.class == Class::Maybe)))
                    }
                    Some((Name::State(i), _)) => {
                        let s = &self.ir.state[i];
                        Some(self.refine(Self::h(HKind::State(i), s.ty, s.maybe)))
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
                            .note(format!("write `{}.close` or `{}.held`", n, n)),
                        );
                        None
                    }
                    None => {
                        if n == "held" {
                            if let Mode::Named(names) = &self.ir.mode {
                                self.err(
                                    Diag::new(
                                        Code::HeldNeedsInstrument,
                                        "whose position?",
                                        e.span,
                                        "the program names its instruments",
                                    )
                                    .note(format!("write `{}.held`", names.first().cloned().unwrap_or_default())),
                                );
                                return None;
                            }
                            if !matches!(k, HandlerKind::OnBar | HandlerKind::OnOpen | HandlerKind::OnClose) {
                                self.err(Diag::new(
                                    Code::HeldInHandler,
                                    format!("`held` is read in `on_bar`, not in `{}`", k.name()),
                                    e.span,
                                    "not here",
                                ));
                                return None;
                            }
                            self.ir.uses_held = true;
                            return Some(Self::h(HKind::Held, Ty::Real, false));
                        }
                        if n == "keep" {
                            return Some(Self::h(HKind::Keep, Ty::Real, true));
                        }
                        if n == "bars" || n == "periods" {
                            if self.ir.mode != Mode::Universe || !bar_side {
                                self.err(Diag::new(Code::BarsOutsideUniverse, format!("`{}` is read in a universe's handlers", n), e.span, "not here")
                                    .note("`bars` counts this instrument's bars, `periods` every period; `bars == periods` while it has missed none"));
                                return None;
                            }
                            return Some(Self::h(
                                if n == "bars" { HKind::Bars } else { HKind::Periods },
                                Ty::Int,
                                false,
                            ));
                        }
                        if n == "fill" {
                            self.err(Diag::new(
                                Code::FillWithoutField,
                                "`fill` has fields",
                                e.span,
                                "write `fill.price`, `fill.quantity` or `fill.buy`",
                            ));
                            return None;
                        }
                        self.unknown(n, e.span);
                        None
                    }
                }
            }
            ExprKind::Field(obj, field) if self.uctx.is_some() => {
                let ctx = self.uctx.clone().unwrap();
                if ctx.window.as_deref() == Some(obj.name.as_str()) && field.name == "count" {
                    if let Some(c) = self.uctx.as_mut() {
                        c.uses_count = true;
                    }
                    return Some(Self::h(HKind::UCount, Ty::Int, false));
                }
                self.err(
                    Diag::new(
                        Code::FieldInBody,
                        format!("`{}.{}` is not available in `{}`", obj.name, field.name, ctx.name),
                        e.span,
                        "not here",
                    )
                    .note("an indicator reads its window as `w.count`; nothing else has fields here"),
                );
                None
            }
            ExprKind::Member(_, _) => {
                self.err(
                    Diag::new(
                        Code::UserIndicatorInHandler,
                        "a user indicator is read in `series {}`",
                        e.span,
                        "not here",
                    )
                    .note(INDICATOR_NOTE),
                );
                None
            }
            ExprKind::Field(obj, field) if matches!(self.names.get(&obj.name), Some((Name::Instrument(_), _))) => {
                let Some((Name::Instrument(i), _)) = self.names.get(&obj.name).cloned() else {
                    return None;
                };
                if field.name == "held" {
                    if k != HandlerKind::OnBar {
                        self.err(Diag::new(
                            Code::HeldInHandler,
                            format!("`{}.held` is read in `on_bar`, not in `{}`", obj.name, k.name()),
                            e.span,
                            "not here",
                        ));
                        return None;
                    }
                    self.ir.uses_held = true;
                    return Some(Self::h(HKind::HeldOf(i), Ty::Real, false));
                }
                let Some(f) = field_of(&field.name) else {
                    self.err(Diag::new(
                        Code::UnknownField,
                        format!("`{}` has no field `{}`", obj.name, field.name),
                        field.span,
                        "fields: open, high, low, close, volume, held",
                    ));
                    return None;
                };
                if !bar_side {
                    self.err(Diag::new(
                        Code::NoBar,
                        format!("`{}.{}` has no bar to read in `{}`", obj.name, field.name, k.name()),
                        e.span,
                        "no bar here",
                    ));
                    return None;
                }
                let class = if f == Field::Open { Class::Maybe } else { Class::Always };
                let id = self.add_node(NodeKind::Field(i, f), Ty::Real, class, CExpr::Num(1.0));
                self.reads.push((k, id));
                Some(self.refine(Self::h(HKind::Node(id), Ty::Real, class == Class::Maybe)))
            }
            ExprKind::Field(obj, field) => {
                if obj.name != "fill" {
                    self.err(Diag::new(
                        Code::NoFields,
                        format!("`{}` has no fields", obj.name),
                        e.span,
                        "only `fill.*` has fields",
                    ));
                    return None;
                }
                if k != HandlerKind::OnFill {
                    self.err(Diag::new(
                        Code::FillOutsideOnFill,
                        format!("`fill` is read in `on_fill`, not in `{}`", k.name()),
                        obj.span,
                        "no fill here",
                    ));
                    return None;
                }
                match field.name.as_str() {
                    "price" => Some(Self::h(HKind::FillPrice, Ty::Real, false)),
                    "quantity" => Some(Self::h(HKind::FillQuantity, Ty::Real, false)),
                    "buy" => Some(Self::h(HKind::FillBuy, Ty::Bool, false)),
                    _ => {
                        self.err(Diag::new(
                            Code::UnknownField,
                            format!("`fill` has no field `{}`", field.name),
                            field.span,
                            "fields: price, quantity, buy",
                        ));
                        None
                    }
                }
            }
            ExprKind::Unary(op, a) => {
                let a = self.hexpr(a, k)?;
                let ty = self.unary_ty(*op, a.ty, e.span)?;
                let maybe = a.maybe;
                Some(Self::h(HKind::Unary(*op, Box::new(a)), ty, maybe))
            }
            ExprKind::Binary(op, a, b) => {
                let (sa, sb) = (a.span, b.span);
                let (a, b) = (self.hexpr(a, k), self.hexpr(b, k));
                let (a, b) = (a?, b?);
                let ty = self.binary_ty(*op, a.ty, b.ty, sa, sb, e.span)?;
                let mut maybe = a.maybe || b.maybe;
                if *op == BinOp::Div && !self.hexpr_nonzero(&b) {
                    maybe = true;
                }
                Some(Self::h(HKind::Binary(*op, Box::new(a), Box::new(b)), ty, maybe))
            }
            ExprKind::Ternary(c, a, b) => {
                let (cs, sa, sb) = (c.span, a.span, b.span);
                let c = self.hexpr(c, k);
                let refs = c.as_ref().map(Self::defined_refs).unwrap_or_default();
                let depth = self.refined.len();
                self.refined.extend(refs);
                let a = self.hexpr(a, k);
                self.refined.truncate(depth);
                let b = self.hexpr(b, k);
                let (c, a, b) = (c?, a?, b?);
                if c.ty != Ty::Bool {
                    self.err(Diag::new(
                        Code::ConditionNotBool,
                        "type mismatch: a condition must be `bool`",
                        cs,
                        format!("this is `{}`", c.ty.name()),
                    ));
                    return None;
                }
                let mut ty = self.ternary_ty(a.ty, b.ty, sa, sb)?;
                let maybe = c.maybe || a.maybe || b.maybe;
                if ty == Ty::Int && maybe {
                    ty = Ty::Real;
                }
                Some(Self::h(
                    HKind::Ternary(Box::new(c), Box::new(a), Box::new(b)),
                    ty,
                    maybe,
                ))
            }
            ExprKind::Call(name, args) if matches!(self.names.get(&name.name), Some((Name::Function(_), _))) => {
                let Some((Name::Function(f), _)) = self.names.get(&name.name).cloned() else {
                    return None;
                };
                if !self.no_named(&name.name, args) {
                    return None;
                }
                let mut hs = vec![];
                for a in args {
                    hs.push(self.hexpr(a, k));
                }
                let hs: Vec<HExpr> = hs.into_iter().collect::<Option<_>>()?;
                let tys: Vec<Ty> = hs.iter().map(|h| h.ty).collect();
                let ret = self.fn_args_ok(f, &tys, args, e.span)?;
                let maybe = self.ir.functions[f].may_undef || hs.iter().any(|h| h.maybe);
                Some(Self::h(HKind::UserCall(f, hs), ret, maybe))
            }
            ExprKind::Call(name, _)
                if self.uctx.is_some()
                    && (name.name.starts_with("xs_")
                        || matches!(self.names.get(&name.name), Some((Name::Indicator(_), _)))) =>
            {
                self.err(Diag::new(Code::CallInBody, format!("`{}` inside `{}`", name.name, self.uctx.as_ref().unwrap().name), e.span, "not here")
                    .note("functions and indicators see only their own arguments: indicators and cross-sectional values are used in a program's series and handlers"));
                None
            }
            ExprKind::Call(name, args) if name.name.starts_with("xs_") => self.xs(name, args, e.span, k),
            ExprKind::Call(name, args) => {
                if matches!(self.names.get(&name.name), Some((Name::Indicator(_), _)))
                    || Ind::from_name(&name.name).is_some()
                    || kalman_out(&name.name).is_some()
                    || name.name.starts_with("ols_")
                {
                    let shown = crate::frontend::ast_text(e);
                    self.err(
                        Diag::new(
                            Code::IndicatorInHandler,
                            format!("indicator `{}` called inside `{}`", name.name, k.name()),
                            e.span,
                            "indicators are computed in `series {}`",
                        )
                        .note(INDICATOR_NOTE)
                        .note(format!(
                            "declare `real x = {};` in `series {{}}` and read `x` here",
                            shown
                        )),
                    );
                    return None;
                }
                if !self.no_named(&name.name, args) {
                    return None;
                }
                let b = self.builtin(name, args.len(), e.span)?;
                let mut hs = vec![];
                for a in args {
                    hs.push(self.hexpr(a, k));
                }
                let hs: Vec<HExpr> = hs.into_iter().collect::<Option<_>>()?;
                let tys: Vec<Ty> = hs.iter().map(|h| h.ty).collect();
                let ty = self.builtin_ty(b, &tys, args)?;
                let maybe =
                    matches!(b, Builtin::Log | Builtin::Sqrt) || (b != Builtin::Defined && hs.iter().any(|h| h.maybe));
                Some(Self::h(HKind::Call(b, hs), ty, maybe))
            }
            ExprKind::Named(n, _) => {
                self.err(Diag::new(
                    Code::NamedArgumentOutsideCall,
                    format!("`{} = ...` outside a call's arguments", n.name),
                    e.span,
                    "not an expression",
                ));
                None
            }
            ExprKind::Index(_, _) => {
                self.err(
                    Diag::new(
                        Code::IndicatorInHandler,
                        format!("a lag inside `{}`", k.name()),
                        e.span,
                        "lags are computed in `series {}`",
                    )
                    .note(INDICATOR_NOTE)
                    .note(format!(
                        "declare `real x = {};` in `series {{}}` and read `x` here",
                        crate::frontend::ast_text(e)
                    )),
                );
                None
            }
        }
    }

    /// Every handler. A real state assigned a possibly undefined value is itself possibly undefined,
    /// which can change other assignments; iterate to a fixed point, keeping the last pass's errors.
    pub(super) fn handlers(&mut self, program: &Program) {
        let before = self.errors.len();
        for _ in 0..(self.ir.state.len() + 2) {
            self.errors.truncate(before);
            self.reads.clear();
            self.ir.locals.clear();
            let mut handlers = Handlers::default();
            for item in &program.items {
                if let Item::Handler(k, body, _) = item {
                    let stmts = self.block(body, *k);
                    match k {
                        HandlerKind::OnStart => handlers.on_start = Some(stmts),
                        HandlerKind::OnBar => handlers.on_bar = Some(stmts),
                        HandlerKind::OnFill => handlers.on_fill = Some(stmts),
                        HandlerKind::OnStop => handlers.on_stop = Some(stmts),
                        HandlerKind::OnOpen => handlers.on_open = Some(stmts),
                        HandlerKind::OnClose => handlers.on_close = Some(stmts),
                        HandlerKind::User => {}
                    }
                }
            }
            self.ir.handlers = handlers;
            let mut changed = false;
            let flags = self.state_maybe();
            for (i, f) in flags.into_iter().enumerate() {
                if f && !self.ir.state[i].maybe {
                    self.ir.state[i].maybe = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, ok};
    use crate::frontend::diag::Code;
    use crate::ir::{HKind, HStmt};

    #[test]
    fn statements() {
        let ir = ok(
            "params { int<lower=1> n = 3; } state { real s = 0; } on_bar { real x = close; \
                     for (i in 1:n) { s = s + i; } if (x > s) { target(1); } else if (x < s) { target(-1); } else { target(keep); } }",
        );
        let body = ir.handlers.on_bar.as_ref().unwrap();
        assert!(matches!(body[0], HStmt::Local { .. }));
        assert!(matches!(body[1], HStmt::For { .. }));
        assert!(matches!(&body[2], HStmt::If { els, .. } if matches!(els[0], HStmt::If { .. })));
        assert_eq!(errors("on_bar { while (true) { } target(1); }"), [Code::WhileLoop]);
        assert_eq!(errors("on_bar { close; target(1); }"), [Code::ExpressionStatement]);
        assert_eq!(errors("on_bar { return 1; }"), [Code::ReturnOutsideFunction]);
        assert_eq!(errors("on_bar { if (close) { target(1); } }"), [Code::ConditionNotBool]);
        assert_eq!(errors("on_bar { real x = 1; x = 2; target(x); }"), [Code::AssignLocal]);
        assert_eq!(
            errors("series { real a = close; } on_bar { a = 1; target(a); }"),
            [Code::AssignSeries]
        );
        assert_eq!(errors("on_bar { held = 1; target(1); }"), [Code::AssignReadOnly]);
        assert_eq!(errors("on_bar { nope = 1; target(1); }"), [Code::UnknownName]);
    }

    #[test]
    fn where_values_are_read() {
        assert_eq!(
            errors("on_bar { target(1); } on_fill { target(0); }"),
            [Code::TargetInHandler]
        );
        assert_eq!(
            errors("state { real s = 0; } on_start { s = close; } on_bar { target(1); }"),
            [Code::NoBar]
        );
        assert_eq!(
            errors("series { real a = close; } state { real s = 0; } on_stop { s = a; } on_bar { target(a); }"),
            [Code::NoBar]
        );
        assert_eq!(
            errors("state { real s = 0; } on_fill { s = held; } on_bar { target(1); }"),
            [Code::HeldInHandler]
        );
        assert_eq!(errors("on_bar { target(fill.price); }"), [Code::FillOutsideOnFill]);
        assert_eq!(
            errors("state { real s = 0; } on_fill { s = fill.cost; } on_bar { target(1); }"),
            [Code::UnknownField]
        );
        assert_eq!(
            errors("series { real a = sma(close, 3); } on_bar { target(sma(close, 3)); }"),
            [Code::IndicatorInHandler]
        );
        assert_eq!(errors("on_bar { target(close[1]); }"), [Code::IndicatorInHandler]);
        assert_eq!(errors("on_bar { target(bars); }"), [Code::BarsOutsideUniverse]);
        let ir = ok(
            "state { real p = 0; bool b = false; } on_fill { p = fill.price + fill.quantity; b = fill.buy; } on_bar { target(held); }",
        );
        assert!(ir.uses_held);
        // In `on_open`, only today's open is known.
        assert_eq!(
            errors("universe {} on_open { target(close); }"),
            [Code::OpenReadsFuture]
        );
        let ir = ok("universe {} on_open { target(open > 1 ? bars : periods); }");
        let HStmt::Target(_, value) = &ir.handlers.on_open.as_ref().unwrap()[0] else {
            panic!()
        };
        assert!(
            matches!(&value.kind, HKind::Ternary(c, _, _) if matches!(&c.kind, HKind::Binary(_, a, _) if a.kind == HKind::OpenNow))
        );
    }

    #[test]
    fn named_instruments() {
        let ir = ok(
            "instruments { y; x; } series { real spread = y.close - x.close; } on_bar { target(y, spread > 0 ? 1 : y.held); target(x, 0); }",
        );
        let body = ir.handlers.on_bar.as_ref().unwrap();
        assert!(matches!(body[0], HStmt::Target(0, _)) && matches!(body[1], HStmt::Target(1, _)));
        assert_eq!(
            errors("instruments { y; } on_bar { target(1); }"),
            [Code::TargetNeedsInstrument]
        );
        assert_eq!(
            errors("instruments { y; } on_bar { target(z, 1); }"),
            [Code::NotAnInstrument]
        );
        assert_eq!(
            errors("instruments { y; } on_bar { target(y, held); }"),
            [Code::HeldNeedsInstrument]
        );
        assert_eq!(
            errors("instruments { y; } on_bar { target(y, y.ask); }"),
            [Code::UnknownField]
        );
        assert_eq!(
            errors("instruments { y; } state { real s = 0; } on_start { s = y.held; } on_bar { target(y, 1); }"),
            [Code::HeldInHandler]
        );
        assert_eq!(errors("on_bar { target(y, 1); }"), [Code::TargetNamesInstrument]);
    }
}
