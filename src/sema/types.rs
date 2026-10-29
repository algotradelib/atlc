//! Types: operators, built-in functions' signatures, assignment, and the magnitude of `int` literals.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

pub(super) fn join_ty(a: Ty, b: Ty) -> Ty {
    if a == Ty::Int && b == Ty::Int {
        Ty::Int
    } else {
        Ty::Real
    }
}

/// The largest magnitude of an `int` literal: an `int` passes through a double on its way into the
/// strategy (literals, defaults and parameter values), and the strategy's setter refuses more than this.
pub(super) const INT_LITERAL_MAX: f64 = 9e15;

/// Calls `f` on every expression of the program, outermost first (`replay {}` holds settings, not
/// expressions).
fn each_expr(program: &Program, f: &mut dyn FnMut(&Expr)) {
    fn expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
        f(e);
        match &e.kind {
            ExprKind::Num(..) | ExprKind::Bool(_) | ExprKind::Name(_) | ExprKind::Field(..) => {}
            ExprKind::Unary(_, a) | ExprKind::Named(_, a) | ExprKind::Member(a, _) => expr(a, f),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                expr(a, f);
                expr(b, f);
            }
            ExprKind::Ternary(c, a, b) => {
                expr(c, f);
                expr(a, f);
                expr(b, f);
            }
            ExprKind::Call(_, args) => args.iter().for_each(|a| expr(a, f)),
        }
    }
    fn stmts(body: &[Stmt], f: &mut dyn FnMut(&Expr)) {
        for s in body {
            match s {
                Stmt::Local(d) => expr(&d.init, f),
                Stmt::Assign { value, .. } | Stmt::Target { value, .. } | Stmt::Return { value, .. } => expr(value, f),
                Stmt::Expr { expr: e, .. } => expr(e, f),
                Stmt::If { cond, then, els, .. } => {
                    expr(cond, f);
                    stmts(then, f);
                    stmts(els.as_deref().unwrap_or_default(), f);
                }
                Stmt::For { from, to, body, .. } => {
                    expr(from, f);
                    expr(to, f);
                    stmts(body, f);
                }
                Stmt::While { cond, body, .. } => {
                    expr(cond, f);
                    stmts(body, f);
                }
            }
        }
    }
    for item in &program.items {
        match item {
            Item::Params(decls, _) => {
                for d in decls {
                    d.lower.iter().chain(&d.upper).for_each(|e| expr(e, f));
                    expr(&d.default, f);
                }
            }
            Item::Series(decls, _) | Item::State(decls, _) => decls.iter().for_each(|d| expr(&d.init, f)),
            Item::Warmup(e, _) => expr(e, f),
            Item::Handler(_, body, _) => stmts(body, f),
            Item::Options(settings, _) => settings.iter().for_each(|(_, v)| expr(v, f)),
            Item::Functions(defs, _) => defs.iter().for_each(|d| stmts(&d.body, f)),
            Item::Indicator(d) => {
                for part in &d.parts {
                    match part {
                        IndPart::Window { size: e, .. } | IndPart::Warmup(e, _) => expr(e, f),
                        IndPart::State(v) | IndPart::Value(v) => expr(&v.init, f),
                        IndPart::Update(body, _) | IndPart::Enter(_, body, _) | IndPart::Leave(_, body, _) => {
                            stmts(body, f)
                        }
                    }
                }
            }
            Item::Instruments(..) | Item::Universe(_) | Item::Replay(..) => {}
        }
    }
}

impl Checker<'_> {
    /// Every `int` literal is at most 9e15 in magnitude, wherever it is written (the prototype accepted
    /// larger ones and lost precision past 2^53).
    pub(super) fn int_literals(&mut self, program: &Program) {
        let mut spans = vec![];
        each_expr(program, &mut |e| {
            if let ExprKind::Num(v, true) = e.kind
                && (v.abs() > INT_LITERAL_MAX || v.is_nan())
            {
                spans.push(e.span);
            }
        });
        for span in spans {
            self.err(
                Diag::new(
                    Code::IntLiteralTooLarge,
                    "this `int` literal is above 9e15 in magnitude",
                    span,
                    "too large",
                )
                .note("an `int` passes through a double on its way into the strategy, and loses precision past 2^53")
                .note("write a `real` with an exponent (`1e16`) if a real is meant"),
            );
        }
    }

    pub(super) fn builtin(&mut self, name: &Ident, n: usize, span: Span) -> Option<Builtin> {
        let (b, arity) = match name.name.as_str() {
            "abs" => (Builtin::Abs, 1),
            "min" => (Builtin::Min, 2),
            "max" => (Builtin::Max, 2),
            "defined" => (Builtin::Defined, 1),
            "sign" => (Builtin::Sign, 1),
            "log" => (Builtin::Log, 1),
            "sqrt" => (Builtin::Sqrt, 1),
            "round" => (Builtin::Round, 1),
            "floor" => (Builtin::Floor, 1),
            "target" => {
                self.err(Diag::new(
                    Code::TargetAsValue,
                    "`target` is a statement",
                    span,
                    "write `target(x);` on its own in `on_bar`",
                ));
                return None;
            }
            _ => {
                self.err(Diag::new(Code::UnknownFunction, format!("unknown function `{}`", name.name), name.span, "not a function")
                    .note("functions: abs min max defined sign log sqrt round floor; indicators (series only): sma ema rsi lag ts_max ts_min ts_sum stdev zscore ols_slope ols_intercept kf_beta kf_error kf_sd; cross-sectional (universe handlers): xs_mean xs_sum xs_count xs_rank"));
                return None;
            }
        };
        if n != arity {
            self.err(Diag::new(
                Code::FunctionArity,
                format!("`{}` takes {} argument(s)", name.name, arity),
                span,
                format!("{} given", n),
            ));
            return None;
        }
        Some(b)
    }

    pub(super) fn builtin_ty(&mut self, b: Builtin, tys: &[Ty], args: &[Expr]) -> Option<Ty> {
        match b {
            Builtin::Defined => Some(Ty::Bool),
            Builtin::Abs => {
                if !tys[0].numeric() {
                    self.err(Diag::new(
                        Code::ArgumentType,
                        "`abs` needs a number",
                        args[0].span,
                        format!("this is `{}`", tys[0].name()),
                    ));
                    return None;
                }
                Some(tys[0])
            }
            Builtin::Sign | Builtin::Log | Builtin::Sqrt | Builtin::Round | Builtin::Floor => {
                if !tys[0].numeric() {
                    self.err(Diag::new(
                        Code::ArgumentType,
                        "this function needs a number",
                        args[0].span,
                        format!("this is `{}`", tys[0].name()),
                    ));
                    return None;
                }
                Some(Ty::Real)
            }
            Builtin::Min | Builtin::Max => {
                for (t, a) in tys.iter().zip(args) {
                    if !t.numeric() {
                        self.err(Diag::new(
                            Code::ArgumentType,
                            "`min`/`max` need numbers",
                            a.span,
                            format!("this is `{}`", t.name()),
                        ));
                        return None;
                    }
                }
                Some(join_ty(tys[0], tys[1]))
            }
        }
    }

    pub(super) fn unary_ty(&mut self, op: UnOp, t: Ty, span: Span) -> Option<Ty> {
        match op {
            UnOp::Neg if t.numeric() => Some(t),
            UnOp::Not if t == Ty::Bool => Some(Ty::Bool),
            UnOp::Neg => {
                self.err(Diag::new(
                    Code::OperandType,
                    "type mismatch: `-` needs a number",
                    span,
                    format!("operand is `{}`", t.name()),
                ));
                None
            }
            UnOp::Not => {
                self.err(
                    Diag::new(
                        Code::OperandType,
                        "type mismatch: `not` needs a `bool`",
                        span,
                        format!("operand is `{}`", t.name()),
                    )
                    .note("compare first, e.g. `not (x > 0)`"),
                );
                None
            }
        }
    }

    pub(super) fn binary_ty(&mut self, op: BinOp, a: Ty, b: Ty, sa: Span, sb: Span, _span: Span) -> Option<Ty> {
        match op {
            BinOp::And | BinOp::Or => {
                for (t, s) in [(a, sa), (b, sb)] {
                    if t != Ty::Bool {
                        self.err(
                            Diag::new(
                                Code::OperandType,
                                format!("type mismatch: `{}` needs `bool` operands", op.text()),
                                s,
                                format!("this is `{}`", t.name()),
                            )
                            .note("a number is not a truth value here; compare it, e.g. `x != 0`"),
                        );
                        return None;
                    }
                }
                Some(Ty::Bool)
            }
            BinOp::Eq | BinOp::Ne if a == Ty::Bool && b == Ty::Bool => Some(Ty::Bool),
            _ => {
                for (t, s) in [(a, sa), (b, sb)] {
                    if !t.numeric() {
                        self.err(Diag::new(
                            Code::OperandType,
                            format!("type mismatch: `{}` needs numbers", op.text()),
                            s,
                            format!("this is `{}`", t.name()),
                        ));
                        return None;
                    }
                }
                Some(if op.is_comparison() {
                    Ty::Bool
                } else if op == BinOp::Div {
                    Ty::Real
                } else {
                    join_ty(a, b)
                })
            }
        }
    }

    pub(super) fn ternary_ty(&mut self, a: Ty, b: Ty, sa: Span, sb: Span) -> Option<Ty> {
        if a == b {
            return Some(a);
        }
        if a.numeric() && b.numeric() {
            return Some(Ty::Real);
        }
        self.err(
            Diag::new(
                Code::TernaryArms,
                "type mismatch: the two arms of `?:` differ",
                sb,
                format!("this is `{}`", b.name()),
            )
            .also(sa, format!("this is `{}`", a.name())),
        );
        None
    }

    pub(super) fn assignable(&mut self, to: Ty, from: Ty, maybe: bool, span: Span, name: &str) -> bool {
        let ok = to == from || (to == Ty::Real && from == Ty::Int);
        if !ok {
            let mut d = Diag::new(
                Code::TypeMismatch,
                format!(
                    "type mismatch: `{}` is `{}` but the value is `{}`",
                    name,
                    to.name(),
                    from.name()
                ),
                span,
                format!("this is `{}`", from.name()),
            );
            if to == Ty::Int && from == Ty::Real {
                d = d.note("there is no implicit conversion from `real` to `int`");
            }
            self.err(d);
            return false;
        }
        if maybe && to != Ty::Real {
            self.err(
                Diag::new(
                    Code::MaybeUndefined,
                    format!(
                        "`{}` is `{}`, which is always defined, but this value may be undefined",
                        name,
                        to.name()
                    ),
                    span,
                    "may be undefined",
                )
                .note("test it first: `defined(x) ? x > 1 : false`, or assign inside `if (defined(x)) { ... }`"),
            );
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, message, ok};
    use crate::frontend::ast::Ty;
    use crate::frontend::diag::Code;
    use crate::ir::{HExpr, HKind, HStmt};

    #[test]
    fn int_literals_up_to_9e15_are_accepted() {
        let ir = ok(
            "params { int<lower=-9000000000000000> n = 9000000000000000; } on_bar { int k = -9000000000000000; target(n + k); }",
        );
        assert_eq!(ir.params[0].default, 9e15);
        let on_bar = ir.handlers.on_bar.as_ref().unwrap();
        assert!(
            matches!(&on_bar[0], HStmt::Local { value: HExpr { kind: HKind::Unary(_, a), ty: Ty::Int, .. }, .. } if a.kind == HKind::Num(9e15))
        );
    }

    #[test]
    fn int_literals_above_9e15_are_refused() {
        // A departure from the prototype, which accepted these and lost precision past 2^53.
        for src in [
            "params { int n = 9000000000000001; } on_bar { target(n); }",
            "params { int n = -9000000000000001; } on_bar { target(n); }",
            "params { int n = 9007199254740993; } on_bar { target(n); }",
            "on_bar { int n = 9007199254740993; target(n); }",
            "series { real big = 10000000000000000; } on_bar { target(big); }",
            "params { int<upper=99999999999999999999> n = 1; } on_bar { target(n); }",
            "series { real a = sma(close, 9000000000000001); } on_bar { target(a); }",
            "functions { int f(int a) { return a + 9000000000000001; } } on_bar { target(f(1)); }",
        ] {
            assert_eq!(errors(src)[0], Code::IntLiteralTooLarge, "{src}");
        }
        assert_eq!(
            message("on_bar { target(9007199254740993); }"),
            "this `int` literal is above 9e15 in magnitude"
        );
        // Each literal once, however often the checker reads it.
        assert_eq!(
            errors("on_bar { int a = 9000000000000001; int b = 9000000000000002; target(a + b); }"),
            [Code::IntLiteralTooLarge, Code::IntLiteralTooLarge]
        );
        // A real literal of any size is not an `int` literal.
        ok("params { real big = 1e300; } series { real s = close * 1e16; } on_bar { target(s / big); }");
        // Settings of `replay {}` are run-spec values, not `int`s.
        ok("replay { cash = 100000000000000000; } on_bar { target(1); }");
    }

    #[test]
    fn operators_and_assignment() {
        assert_eq!(
            errors("on_bar { real x = -(close > 1); target(x); }"),
            [Code::OperandType]
        );
        assert_eq!(errors("on_bar { bool b = not close; target(1); }"), [Code::OperandType]);
        assert_eq!(
            errors("on_bar { bool b = close and true; target(1); }"),
            [Code::OperandType]
        );
        assert_eq!(errors("on_bar { real x = true + 1; target(x); }"), [Code::OperandType]);
        assert_eq!(
            errors("on_bar { real x = close > 1 ? 1 : true; target(x); }"),
            [Code::TernaryArms]
        );
        assert_eq!(
            errors("on_bar { int n = round(close); target(n); }"),
            [Code::TypeMismatch]
        );
        assert_eq!(
            errors("on_bar { real x = abs(true); target(x); }"),
            [Code::ArgumentType]
        );
        assert_eq!(
            errors("on_bar { real x = max(1, true); target(x); }"),
            [Code::ArgumentType]
        );
        assert_eq!(
            errors("on_bar { real x = sqrt(1, 2); target(x); }"),
            [Code::FunctionArity]
        );
        assert_eq!(
            errors("on_bar { real x = cube(2); target(x); }"),
            [Code::UnknownFunction]
        );
        assert_eq!(
            errors("on_bar { real x = target(2); target(x); }"),
            [Code::TargetAsValue]
        );
        // `==` on bools, int widening, and min/max keeping int.
        let ir = ok("state { int n = 0; } on_bar { bool same = true == false; real r = n; n = max(n, 2); target(r); }");
        assert_eq!(ir.locals[0].ty, Ty::Bool);
    }
}
