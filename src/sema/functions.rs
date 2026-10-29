//! `functions {}` and user indicators: definitions, calls, and what their bodies may see.

use std::collections::HashMap;

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

/// Whether every path through a function's body ends in `return`.
pub(super) fn returns_all(stmts: &[HStmt]) -> bool {
    match stmts.last() {
        Some(HStmt::Return(_)) => true,
        Some(HStmt::If { then, els, .. }) => returns_all(then) && returns_all(els),
        _ => false,
    }
}

impl Checker<'_> {
    /// Checks one function of `functions {}`; it may call the functions declared before it (no recursion).
    pub(super) fn function_def(&mut self, d: &FunctionDef) {
        let mut args = vec![];
        for a in &d.args {
            if args.iter().any(|(n, _): &(String, Ty)| *n == a.name.name) {
                self.err(Diag::new(
                    Code::DuplicateArgument,
                    format!("argument `{}` twice", a.name.name),
                    a.name.span,
                    "second",
                ));
            }
            args.push((a.name.name.clone(), a.ty));
        }
        if d.ret == Ty::Int && args.iter().any(|a| a.1 != Ty::Int) {
            self.err(Diag::new(Code::IntFunctionRealArguments, "a function returning `int` takes `int` arguments only", d.name.span, "here")
                .note("an undefined real argument would make an undefined `int`; return `real` and use `round` or `floor`"));
        }
        let saved_locals = std::mem::take(&mut self.ir.locals);
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_refined = std::mem::take(&mut self.refined);
        self.uctx = Some(UCtx {
            name: d.name.name.clone(),
            is_fn: true,
            args: args.clone(),
            see_args: true,
            ret: Some(d.ret),
            ..Default::default()
        });
        let errors_before = self.errors.len();
        let body = self.block(&d.body, HandlerKind::User);
        let ctx = self.uctx.take().unwrap();
        let locals = std::mem::replace(&mut self.ir.locals, saved_locals);
        self.scopes = saved_scopes;
        self.refined = saved_refined;
        if !returns_all(&body) && self.errors.len() == errors_before {
            self.err(
                Diag::new(
                    Code::MissingReturn,
                    format!("`{}` may end without returning", d.name.name),
                    d.name.span,
                    "every path must end in `return`",
                )
                .note("end the body with `return x;`, or end both branches of the last `if`/`else` with one"),
            );
        }
        let id = self.ir.functions.len();
        self.ir.functions.push(FnIr {
            name: d.name.name.clone(),
            args,
            ret: d.ret,
            body,
            locals,
            may_undef: ctx.returns_maybe,
        });
        self.declare(&d.name, Name::Function(id));
    }

    /// The type a call returns, once its arguments are checked against the function's.
    pub(super) fn fn_args_ok(&mut self, f: usize, tys: &[Ty], args: &[Expr], span: Span) -> Option<Ty> {
        let def = self.ir.functions[f].clone();
        if tys.len() != def.args.len() {
            self.err(Diag::new(
                Code::FunctionArity,
                format!("`{}` takes {} argument(s)", def.name, def.args.len()),
                span,
                format!("{} given", tys.len()),
            ));
            return None;
        }
        for ((t, a), (n, want)) in tys.iter().zip(args).zip(&def.args) {
            let ok = t == want || (*want == Ty::Real && *t == Ty::Int);
            if !ok {
                self.err(Diag::new(
                    Code::ArgumentType,
                    format!("`{}`'s argument `{}` is `{}`", def.name, n, want.name()),
                    a.span,
                    format!("this is `{}`", t.name()),
                ));
                return None;
            }
        }
        Some(def.ret)
    }

    /// A name read inside a function or indicator: its locals, arguments, states, earlier values.
    pub(super) fn user_name(&mut self, ctx: &UCtx, n: &str, span: Span) -> Option<HExpr> {
        if ctx.see_args {
            if let Some(i) = ctx.args.iter().position(|a| a.0 == n) {
                return Some(Self::h(HKind::UArg(i), ctx.args[i].1, false));
            }
        } else if let Some(i) = ctx.args.iter().position(|a| a.0 == n && a.1 == Ty::Int) {
            return Some(Self::h(HKind::UArg(i), Ty::Int, false));
        }
        if let Some(i) = ctx.states.iter().position(|st| st.0 == n) {
            return Some(self.refine(Self::h(HKind::UState(i), ctx.states[i].1, ctx.states[i].2)));
        }
        if let Some(i) = ctx.values.iter().position(|v| v.0 == n) {
            // A value read by a later value: as maybe as the real values are.
            return Some(self.refine(Self::h(HKind::UValue(i), ctx.values[i].1, ctx.values[i].1 != Ty::Int)));
        }
        if n == "keep" {
            return Some(Self::h(HKind::Keep, Ty::Real, true));
        }
        let what = if ctx.is_fn {
            "a function sees only its arguments and locals"
        } else {
            "an indicator sees only its arguments, states, window and earlier values"
        };
        let mut d = Diag::new(
            Code::NotVisible,
            format!("`{}` is not visible in `{}`", n, ctx.name),
            span,
            "not here",
        )
        .note(what);
        if !ctx.see_args && ctx.args.iter().any(|a| a.0 == n) {
            d = d.note("a state's initial value and the window's length are built from numbers and the int arguments");
        } else if field_of(n).is_some() || n == "held" || self.names.contains_key(n) {
            d = d.note(format!("pass `{}` as an argument", n));
        }
        self.err(d);
        None
    }

    /// Checks one `indicator NAME(...) { ... }`.
    pub(super) fn indicator_def(&mut self, d: &IndicatorDef) {
        let mut args = vec![];
        for a in &d.args {
            if a.ty == Ty::Bool {
                self.err(
                    Diag::new(
                        Code::IndicatorBoolArgument,
                        "an indicator's arguments are `real` (series) or `int` (fixed)",
                        a.name.span,
                        "a `bool`",
                    )
                    .note("pass a condition as a series: `c ? 1 : 0`"),
                );
            }
            args.push((a.name.name.clone(), a.ty));
        }
        if !args.iter().any(|a| a.1 == Ty::Real) {
            self.err(
                Diag::new(
                    Code::IndicatorWithoutSeries,
                    format!("`{}` takes no series", d.name.name),
                    d.name.span,
                    "no `real` argument",
                )
                .note("an indicator follows at least one series; a computation on fixed values is a function"),
            );
        }
        let saved_locals = std::mem::take(&mut self.ir.locals);
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_refined = std::mem::take(&mut self.refined);
        let mut ctx = UCtx {
            name: d.name.name.clone(),
            args: args.clone(),
            ..Default::default()
        };
        // The window and warm-up: from numbers and the int arguments.
        let mut window = None;
        let mut warmup = None;
        for part in &d.parts {
            match part {
                IndPart::Window { name, size, span } => {
                    if window.is_some() {
                        self.err(
                            Diag::new(
                                Code::DuplicateWindow,
                                "a second window",
                                *span,
                                "one window per indicator",
                            )
                            .note("the window holds every series argument, so one is enough"),
                        );
                        continue;
                    }
                    // Named even if its length fails, so `enter` and `leave` report nothing more.
                    ctx.window = Some(name.name.clone());
                    if let ExprKind::Name(n) = &size.kind {
                        if let Some(i) = args.iter().position(|a| &a.0 == n && a.1 == Ty::Int) {
                            ctx.nonzero.push(i);
                        }
                    }
                    self.uctx = Some(ctx.clone());
                    if let Some(h) = self.hexpr(size, HandlerKind::User) {
                        if h.ty != Ty::Int {
                            self.err(Diag::new(
                                Code::WindowNotInt,
                                "a window's length is a whole number",
                                size.span,
                                format!("this is `{}`", h.ty.name()),
                            ));
                        }
                        window = Some((name.name.clone(), size.clone(), h));
                        ctx.window = Some(name.name.clone());
                    }
                }
                IndPart::Warmup(e, _) => {
                    self.uctx = Some(ctx.clone());
                    if let Some(h) = self.hexpr(e, HandlerKind::User) {
                        if h.ty != Ty::Int {
                            self.err(Diag::new(
                                Code::WarmupNotInt,
                                "a warm-up is a whole number of steps",
                                e.span,
                                format!("this is `{}`", h.ty.name()),
                            ));
                        }
                        warmup = Some((e.clone(), h));
                    }
                }
                _ => {}
            }
        }
        // States, with their initial values.
        let mut states: Vec<UStateIr> = vec![];
        for part in &d.parts {
            if let IndPart::State(v) = part {
                self.uctx = Some(ctx.clone());
                if let Some(h) = self.hexpr(&v.init, HandlerKind::User) {
                    if self.assignable(v.ty, h.ty, h.maybe && v.ty != Ty::Real, v.init.span, &v.name.name) {
                        if ctx.states.iter().any(|st| st.0 == v.name.name) || args.iter().any(|a| a.0 == v.name.name) {
                            self.err(Diag::new(
                                Code::ShadowedName,
                                format!("`{}` is already a name in `{}`", v.name.name, d.name.name),
                                v.name.span,
                                "choose another",
                            ));
                        }
                        ctx.states.push((v.name.name.clone(), v.ty, h.maybe));
                        states.push(UStateIr {
                            name: v.name.name.clone(),
                            ty: v.ty,
                            init: h,
                            maybe: false,
                        });
                    }
                }
            }
        }
        ctx.see_args = true;
        // The step: update, enter, leave; a real state assigned a possibly undefined value is itself
        // possibly undefined, so iterate to a fixed point as handlers do.
        let series: Vec<usize> = (0..args.len()).filter(|&i| args[i].1 == Ty::Real).collect();
        let before = self.errors.len();
        let mut update = vec![];
        let mut enter = None;
        let mut leave = None;
        for _ in 0..(states.len() + 2) {
            self.errors.truncate(before);
            self.ir.locals.clear();
            ctx.assign = true;
            update.clear();
            enter = None;
            leave = None;
            for part in &d.parts {
                match part {
                    IndPart::Update(body, _) => {
                        self.uctx = Some(ctx.clone());
                        update = self.block(body, HandlerKind::User);
                        ctx.uses_count |= self.uctx.as_ref().is_some_and(|c| c.uses_count);
                    }
                    IndPart::Enter(names, body, span) | IndPart::Leave(names, body, span) => {
                        let is_enter = matches!(part, IndPart::Enter(..));
                        if ctx.window.is_none() {
                            self.err(
                                Diag::new(
                                    Code::StepWithoutWindow,
                                    format!("`{}` without a window", if is_enter { "enter" } else { "leave" }),
                                    *span,
                                    "nothing enters or leaves",
                                )
                                .note("declare `window w[n];`"),
                            );
                            continue;
                        }
                        if names.len() != series.len() {
                            self.err(Diag::new(
                                Code::StepArity,
                                format!(
                                    "`{}` names {} value(s); the window holds {}",
                                    if is_enter { "enter" } else { "leave" },
                                    names.len(),
                                    series.len()
                                ),
                                *span,
                                "one name per series argument",
                            ));
                            continue;
                        }
                        let mut scope = HashMap::new();
                        let mut ids = vec![];
                        for n in names {
                            let id = self.ir.locals.len();
                            self.ir.locals.push(Local {
                                name: n.name.clone(),
                                ty: Ty::Real,
                                maybe: false,
                                top: false,
                            });
                            scope.insert(n.name.clone(), id);
                            ids.push(id);
                        }
                        self.scopes.push(scope);
                        self.uctx = Some(ctx.clone());
                        let b = self.block(body, HandlerKind::User);
                        ctx.uses_count |= self.uctx.as_ref().is_some_and(|c| c.uses_count);
                        self.scopes.pop();
                        if is_enter {
                            enter = Some((ids, b));
                        } else {
                            leave = Some((ids, b));
                        }
                    }
                    _ => {}
                }
            }
            let mut flags = vec![false; states.len()];
            fn walk(stmts: &[HStmt], out: &mut Vec<bool>) {
                for s in stmts {
                    match s {
                        HStmt::UAssign { state, value } => {
                            if value.maybe {
                                out[*state] = true;
                            }
                        }
                        HStmt::If { then, els, .. } => {
                            walk(then, out);
                            walk(els, out);
                        }
                        HStmt::UFor { body, .. } => walk(body, out),
                        _ => {}
                    }
                }
            }
            walk(&update, &mut flags);
            if let Some((_, b)) = &enter {
                walk(b, &mut flags);
            }
            if let Some((_, b)) = &leave {
                walk(b, &mut flags);
            }
            let mut changed = false;
            for (i, f) in flags.into_iter().enumerate() {
                if f && !ctx.states[i].2 {
                    ctx.states[i].2 = true;
                    states[i].maybe = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if enter.is_some() != leave.is_some() {
            self.err(
                Diag::new(
                    Code::EnterLeavePair,
                    "`enter` and `leave` come together",
                    d.name.span,
                    "only one of them",
                )
                .note("whatever `enter` adds for a value, `leave` takes back when that value leaves the window"),
            );
        }
        // Values, in order; each may read the earlier ones.
        ctx.assign = false;
        let mut values = vec![];
        for part in &d.parts {
            if let IndPart::Value(v) = part {
                self.uctx = Some(ctx.clone());
                let checked = self.hexpr(&v.init, HandlerKind::User);
                ctx.uses_count |= self.uctx.as_ref().is_some_and(|c| c.uses_count);
                if let Some(h) = checked {
                    if v.ty != Ty::Real {
                        self.err(Diag::new(
                            Code::ValueNotReal,
                            "an indicator's values are `real`",
                            v.ty_span,
                            "write `real`",
                        ));
                        continue;
                    }
                    if self.assignable(v.ty, h.ty, false, v.init.span, &v.name.name) {
                        ctx.values.push((v.name.name.clone(), v.ty));
                        values.push(UValueIr {
                            name: v.name.name.clone(),
                            ty: v.ty,
                            expr: h,
                        });
                    }
                }
            }
        }
        if values.is_empty() {
            self.err(
                Diag::new(
                    Code::IndicatorWithoutValue,
                    format!("`{}` reports nothing", d.name.name),
                    d.name.span,
                    "no `value`",
                )
                .note("add `value real v = ...;`"),
            );
        }
        self.uctx = None;
        let locals = std::mem::replace(&mut self.ir.locals, saved_locals);
        self.scopes = saved_scopes;
        self.refined = saved_refined;
        let id = self.ir.indicators.len();
        self.ir.indicators.push(IndIr {
            name: d.name.name.clone(),
            args,
            window,
            warmup,
            states,
            update,
            enter,
            leave,
            values,
            locals,
            nonzero: ctx.nonzero.clone(),
            uses_count: ctx.uses_count,
        });
        self.declare(&d.name, Name::Indicator(id));
    }

    /// An expression over an indicator's int arguments, with the call's fixed values put in.
    pub(super) fn subst(&mut self, e: &Expr, map: &HashMap<String, CExpr>) -> Option<CExpr> {
        match &e.kind {
            ExprKind::Num(v, true) => Some(CExpr::Num(*v)),
            ExprKind::Name(n) => map.get(n).cloned().or_else(|| {
                self.err(Diag::new(
                    Code::WindowNotConstant,
                    format!("`{}` is not an int argument", n),
                    e.span,
                    "a window is built from numbers and int arguments",
                ));
                None
            }),
            ExprKind::Unary(UnOp::Neg, a) => Some(CExpr::Neg(Box::new(self.subst(a, map)?))),
            ExprKind::Binary(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul), a, b) => {
                let (a, b) = (self.subst(a, map)?, self.subst(b, map)?);
                Some(CExpr::Bin(*op, Box::new(a), Box::new(b)))
            }
            _ => {
                self.err(Diag::new(
                    Code::WindowNotConstant,
                    "a window's length is built from numbers and int arguments with + - *",
                    e.span,
                    "not that",
                ));
                None
            }
        }
    }

    /// A call of a user indicator in `series {}`: series arguments, fixed int arguments, `missing = ...`.
    pub(super) fn user_indicator(
        &mut self,
        d: usize,
        args: &[Expr],
        value: Option<&Ident>,
        span: Span,
    ) -> Option<usize> {
        let def = self.ir.indicators[d].clone();
        let (pos, named) = Self::split_args(args);
        if pos.len() != def.args.len() {
            self.err(Diag::new(
                Code::IndicatorArity,
                format!("`{}` takes {} argument(s)", def.name, def.args.len()),
                span,
                format!("{} given", pos.len()),
            ));
            return None;
        }
        let mut inputs = vec![];
        let mut consts = vec![];
        let mut map = HashMap::new();
        let mut ok = true;
        for (a, (n, ty)) in pos.iter().zip(&def.args) {
            if *ty == Ty::Real {
                match self.sexpr(a) {
                    Some(id) if self.ir.nodes[id].ty.numeric() => inputs.push(id),
                    Some(id) => {
                        self.err(Diag::new(
                            Code::ArgumentType,
                            format!("`{}`'s argument `{}` is a real series", def.name, n),
                            a.span,
                            format!("this is `{}`", self.ir.nodes[id].ty.name()),
                        ));
                        ok = false;
                    }
                    None => ok = false,
                }
            } else {
                match self.cexpr(a, &format!("`{}`'s argument `{}`", def.name, n), true) {
                    Some((c, _)) => {
                        map.insert(n.clone(), c.clone());
                        consts.push(c);
                    }
                    None => ok = false,
                }
            }
        }
        if !ok {
            return None;
        }
        let mut miss = Miss {
            policy: self.ir.missing,
            min: CExpr::Num(1.0),
        };
        for (k, v) in &named {
            if k.name == "missing" {
                miss.policy = self.policy_of(v)?;
            } else {
                self.err(Diag::new(Code::UnknownNamedArgument, format!("`{}` takes no argument `{}`", def.name, k.name), k.span, "unknown argument")
                    .note("a user indicator takes `missing = restart | skip | poison`; a minimum count is written in its values"));
                return None;
            }
        }
        if miss.policy == Policy::Skip && def.window.is_none() && def.enter.is_none() {
            // Without a window, skip means: a missing step does not run `update`.
        }
        let which = match value {
            Some(v) => match def.values.iter().position(|x| x.name == v.name) {
                Some(i) => i,
                None => {
                    let names: Vec<String> = def.values.iter().map(|x| x.name.clone()).collect();
                    self.err(Diag::new(
                        Code::UnknownValue,
                        format!("`{}` has no value `{}`", def.name, v.name),
                        v.span,
                        format!("values: {}", names.join(", ")),
                    ));
                    return None;
                }
            },
            None if def.values.len() == 1 => 0,
            None => {
                let names: Vec<String> = def.values.iter().map(|x| x.name.clone()).collect();
                self.err(
                    Diag::new(
                        Code::AmbiguousValue,
                        format!("`{}` has several values", def.name),
                        span,
                        "which one?",
                    )
                    .note(format!(
                        "write `{}(...).{}`",
                        def.name,
                        names.first().cloned().unwrap_or_default()
                    )),
                );
                return None;
            }
        };
        let base = self.lookback_max(&inputs);
        let mut lb = base.clone();
        if let Some((_, size, _)) = &def.window {
            let w = self.subst(size, &map)?;
            if self.interval(&w).0 < 1.0 {
                self.err(
                    Diag::new(
                        Code::WindowBelowOne,
                        format!("`{}`'s window may be below 1", def.name),
                        span,
                        format!(
                            "`{}` can be below 1 within the params' bounds",
                            crate::frontend::ast_text(size)
                        ),
                    )
                    .note("raise the params' lower bounds"),
                );
                return None;
            }
            lb = if miss.policy == Policy::Skip {
                CExpr::Max(vec![w, base.clone()])
            } else {
                CExpr::Bin(
                    BinOp::Add,
                    Box::new(base.clone()),
                    Box::new(CExpr::Bin(BinOp::Sub, Box::new(w), Box::new(CExpr::Num(1.0)))),
                )
            };
        }
        if let Some((e, _)) = &def.warmup {
            let w = self.subst(e, &map)?;
            lb = CExpr::Max(vec![
                lb,
                CExpr::Bin(
                    BinOp::Add,
                    Box::new(base.clone()),
                    Box::new(CExpr::Bin(BinOp::Sub, Box::new(w), Box::new(CExpr::Num(1.0)))),
                ),
            ]);
        }
        let in_maybe = inputs.iter().any(|&i| self.ir.nodes[i].class == Class::Maybe);
        let class =
            if def.values[which].expr.maybe || miss.policy != Policy::Restart || self.ir.period_clock || in_maybe {
                Class::Maybe
            } else {
                Class::Eventually
            };
        if !self.lookback_fits(&lb, span) {
            return None;
        }
        Some(self.add_node(
            NodeKind::UInd {
                def: d,
                inputs,
                consts,
                miss,
                value: which,
            },
            Ty::Real,
            class,
            lb,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, ok};
    use super::super::testing_ir::{lookback, series};
    use crate::frontend::diag::Code;
    use crate::ir::{Class, NodeKind};

    const MEAN_SD: &str = "indicator mean_sd(real x, int n) { window w[n]; state real s = 0; state real q = 0; \
        enter(v) { s = s + v; q = q + v * v; } leave(v) { s = s - v; q = q - v * v; } \
        value real mean = s / w.count; value real sd = sqrt((q - s * s / w.count) / (w.count - 1)); }";

    #[test]
    fn functions() {
        let ir = ok(
            "functions { real clip(real z, real cap) { return min(max(z, -cap), cap); } \
                     real twice(real z) { if (z > 0) { return clip(z, 1) * 2; } else { return 0; } } } \
                     series { real a = twice(close); } on_bar { target(clip(a, 3)); }",
        );
        assert_eq!(ir.functions.len(), 2);
        assert!(!ir.functions[0].may_undef);
        assert!(matches!(series(&ir, "a").kind, NodeKind::UCall(1, _)));
        assert_eq!(
            errors("functions { real f(real x) { if (x > 0) { return 1; } } } on_bar { target(f(1)); }"),
            [Code::MissingReturn]
        );
        assert_eq!(
            errors("functions { real f(real x) { return close; } } on_bar { target(f(1)); }"),
            [Code::NotVisible]
        );
        assert_eq!(
            errors(
                "functions { real f(real x) { return g(x); } real g(real x) { return x; } } on_bar { target(f(1)); }"
            ),
            [Code::UnknownFunction]
        );
        assert_eq!(
            errors("functions { real f(real x) { return x; } } on_bar { target(f(1, 2)); }"),
            [Code::FunctionArity]
        );
        assert_eq!(
            errors("functions { real f(real x) { return x; } } on_bar { target(f(true)); }"),
            [Code::ArgumentType]
        );
        assert_eq!(
            errors("functions { int f(real x) { return 1; } } on_bar { target(f(1)); }"),
            [Code::IntFunctionRealArguments]
        );
        assert_eq!(
            errors("functions { real f(real x) { return sma(x, 3); } } on_bar { target(f(1)); }"),
            [Code::IndicatorInHandler]
        );
        // A function whose body may return an undefined value makes its calls maybe undefined.
        assert!(
            ok("functions { real inv(real x) { return 1 / x; } } on_bar { target(inv(close)); }").functions[0]
                .may_undef
        );
    }

    #[test]
    fn user_indicators() {
        let ir = ok(&format!(
            "{MEAN_SD} params {{ int<lower=2> n = 20; }} series {{ real m = mean_sd(close, n).mean; real s = mean_sd(close, n).sd; }} on_bar {{ target(m + s); }}"
        ));
        assert_eq!(ir.indicators.len(), 1);
        assert!(ir.indicators[0].uses_count);
        assert_eq!(lookback(&ir, "m"), 20.0);
        let NodeKind::UInd { value, .. } = series(&ir, "s").kind else {
            panic!()
        };
        assert_eq!(value, 1);
        assert_eq!(series(&ir, "m").class, Class::Maybe);
        // An EMA without a window: update and a warm-up.
        let ir = ok(
            "indicator e(real x, int n) { warmup n; state real v = 0; update { v = v + (x - v) * 2 / (n + 1); } value real out = v; } \
                     series { real a = e(close, 10); } on_bar { target(a); }",
        );
        assert_eq!(lookback(&ir, "a"), 10.0);
        // `/ (n + 1)` is not a provably nonzero divisor, so the state and the value may be undefined.
        assert_eq!(series(&ir, "a").class, Class::Maybe);
        let call = |series: &str| format!("{MEAN_SD} series {{ {series} }} on_bar {{ target(0); }}");
        assert_eq!(errors(&call("real a = mean_sd(close, 5);")), [Code::AmbiguousValue]);
        assert_eq!(
            errors(&call("real a = mean_sd(close, 5).median;")),
            [Code::UnknownValue]
        );
        assert_eq!(errors(&call("real a = mean_sd(close).mean;")), [Code::IndicatorArity]);
        assert_eq!(
            errors(&call("real a = mean_sd(close, 0).mean;")),
            [Code::WindowBelowOne]
        );
        assert_eq!(
            errors(&call("real a = mean_sd(close, 5, min = 2).mean;")),
            [Code::UnknownNamedArgument]
        );
        assert_eq!(
            errors(&call("real a = mean_sd(close > 1, 5).mean;")),
            [Code::ArgumentType]
        );
        ok(&call("real a = mean_sd(close, 5, missing = skip).mean;"));
        assert_eq!(
            errors(&format!("{MEAN_SD} on_bar {{ target(mean_sd(close, 5).mean); }}")),
            [Code::UserIndicatorInHandler]
        );
        assert_eq!(
            errors(&format!("{MEAN_SD} on_bar {{ target(mean_sd(close, 5)); }}")),
            [Code::IndicatorInHandler]
        );
    }

    #[test]
    fn indicator_bodies() {
        let body = |parts: &str| errors(&format!("indicator f(real x) {{ {parts} }} on_bar {{ target(0); }}"));
        assert_eq!(
            body("window w[3]; state real s = 0; enter(v) { s = s + v; } value real t = s;"),
            [Code::EnterLeavePair]
        );
        assert_eq!(
            body("state real s = 0; update { s = x; } value real t = s + close;"),
            [Code::NotVisible, Code::IndicatorWithoutValue]
        );
        assert_eq!(
            body("state real s = 0; value real t = s; update { t = 1; }"),
            [Code::AssignInFunction]
        );
        assert_eq!(
            body("window w[3]; window u[3]; value real t = x;"),
            [Code::DuplicateWindow]
        );
        assert_eq!(
            body("state real s = 0; state real s = 1; value real t = s;"),
            [Code::ShadowedName]
        );
        assert_eq!(
            body("value real t = xs_mean(x);"),
            [Code::CallInBody, Code::IndicatorWithoutValue]
        );
        assert_eq!(
            body("state real s = x; value real t = s;"),
            [Code::NotVisible, Code::NotVisible, Code::IndicatorWithoutValue]
        );
        assert_eq!(
            errors("indicator f(real x, bool b) { value real t = x; } on_bar { target(0); }"),
            [Code::IndicatorBoolArgument]
        );
        assert_eq!(
            errors("indicator f(int n) { value real t = n; } on_bar { target(0); }"),
            [Code::IndicatorWithoutSeries]
        );
        assert_eq!(
            errors("functions { real f(real a, real a) { return a; } } on_bar { target(0); }"),
            [Code::DuplicateArgument]
        );
    }
}
