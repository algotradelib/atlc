//! Constant expressions (numbers and parameters, fixed before the run), their values with the defaults
//! and their ranges over every value the parameters' bounds allow.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

impl Checker<'_> {
    pub(super) fn eval(&self, c: &CExpr) -> f64 {
        match c {
            CExpr::Num(v) | CExpr::Real(v) => *v,
            CExpr::Param(i) => self.defaults[*i],
            CExpr::Neg(a) => -self.eval(a),
            CExpr::Bin(op, a, b) => {
                let (a, b) = (self.eval(a), self.eval(b));
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    _ => a / b,
                }
            }
            CExpr::Max(v) => v.iter().map(|x| self.eval(x)).fold(f64::NEG_INFINITY, f64::max),
        }
    }

    /// The range a constant expression can take over every parameter value the bounds allow.
    pub(super) fn interval(&self, c: &CExpr) -> (f64, f64) {
        fn mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
            let ps = [a.0 * b.0, a.0 * b.1, a.1 * b.0, a.1 * b.1];
            let ps: Vec<f64> = ps.iter().map(|x| if x.is_nan() { 0.0 } else { *x }).collect();
            (
                ps.iter().cloned().fold(f64::INFINITY, f64::min),
                ps.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            )
        }
        match c {
            CExpr::Num(v) | CExpr::Real(v) => (*v, *v),
            CExpr::Param(i) => self.intervals[*i],
            CExpr::Neg(a) => {
                let (lo, hi) = self.interval(a);
                (-hi, -lo)
            }
            CExpr::Bin(op, a, b) => {
                let (a, b) = (self.interval(a), self.interval(b));
                match op {
                    BinOp::Add => (a.0 + b.0, a.1 + b.1),
                    BinOp::Sub => (a.0 - b.1, a.1 - b.0),
                    BinOp::Mul => mul(a, b),
                    _ => (f64::NEG_INFINITY, f64::INFINITY),
                }
            }
            CExpr::Max(v) => {
                let mut lo = f64::NEG_INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for x in v {
                    let (l, h) = self.interval(x);
                    lo = lo.max(l);
                    hi = hi.max(h);
                }
                (lo, hi)
            }
        }
    }

    /// A constant expression: numbers and params, `+ - * /`, unary minus. `int_only` refuses reals
    /// and `/` (windows, the warm-up, loop bounds).
    pub(super) fn cexpr(&mut self, e: &Expr, what: &str, int_only: bool) -> Option<(CExpr, Ty)> {
        match &e.kind {
            ExprKind::Num(v, int) => {
                if int_only && !int {
                    self.err(Diag::new(
                        Code::NotWholeNumber,
                        format!("{} is a whole number of bars", what),
                        e.span,
                        "not a whole number",
                    ));
                    return None;
                }
                Some(if *int {
                    (CExpr::Num(*v), Ty::Int)
                } else {
                    (CExpr::Real(*v), Ty::Real)
                })
            }
            ExprKind::Name(n) => match self.names.get(n).cloned() {
                Some((Name::Poisoned, _)) => None,
                Some((Name::Param(i), _)) => {
                    let ty = self.ir.params[i].ty;
                    if ty == Ty::Bool || (int_only && ty != Ty::Int) {
                        self.err(Diag::new(
                            Code::ConstParamNotInt,
                            if int_only {
                                format!("{what} is built from `int` params")
                            } else {
                                format!("{what} is built from `int` and `real` params")
                            },
                            e.span,
                            format!("`{}` is `{}`", n, ty.name()),
                        ));
                        return None;
                    }
                    Some((CExpr::Param(i), ty))
                }
                Some((_, decl)) => {
                    self.err(
                        Diag::new(
                            Code::NotConstant,
                            format!("{} must be fixed before the run", what),
                            e.span,
                            format!("`{}` changes during the run", n),
                        )
                        .also(decl, "declared here")
                        .note("use numbers and params only"),
                    );
                    None
                }
                None => {
                    if field_of(n).is_some() || n == "held" {
                        self.err(
                            Diag::new(
                                Code::NotConstant,
                                format!("{} must be fixed before the run", what),
                                e.span,
                                format!("`{}` changes every bar", n),
                            )
                            .note("use numbers and params only"),
                        );
                    } else {
                        self.unknown(n, e.span);
                    }
                    None
                }
            },
            ExprKind::Unary(UnOp::Neg, a) => {
                let (a, ty) = self.cexpr(a, what, int_only)?;
                Some((CExpr::Neg(Box::new(a)), ty))
            }
            ExprKind::Binary(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), a, b) => {
                if int_only && *op == BinOp::Div {
                    self.err(
                        Diag::new(
                            Code::CountDivision,
                            format!("{} cannot divide", what),
                            e.span,
                            "`/` gives a real",
                        )
                        .note("a count of bars is a whole number"),
                    );
                    return None;
                }
                let (a, ta) = self.cexpr(a, what, int_only)?;
                let (b, tb) = self.cexpr(b, what, int_only)?;
                let ty = if *op == BinOp::Div { Ty::Real } else { join_ty(ta, tb) };
                Some((CExpr::Bin(*op, Box::new(a), Box::new(b)), ty))
            }
            _ => {
                self.err(Diag::new(
                    Code::NotConstant,
                    format!("{} must be fixed before the run", what),
                    e.span,
                    "not built from numbers and params",
                ));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, ok};
    use crate::frontend::diag::Code;

    #[test]
    fn constants() {
        ok(
            "params { int<lower=1> n = 3; } state { real s = n * 2 - 1; int c = -n; } on_bar { for (i in 1:n * 2) { s = s + i; } target(s + c); }",
        );
        assert_eq!(
            errors("state { real s = close; } on_bar { target(s); }"),
            [Code::NotConstant]
        );
        assert_eq!(
            errors("state { real s = abs(1); } on_bar { target(s); }"),
            [Code::NotConstant]
        );
        assert_eq!(
            errors("state { real a = 1; real b = a; } on_bar { target(b); }"),
            [Code::NotConstant]
        );
        assert_eq!(
            errors("params { bool f = true; } state { real s = f; } on_bar { target(s); }"),
            [Code::ConstParamNotInt]
        );
        // A bool state takes `true` or `false`, not a bool parameter.
        assert_eq!(
            errors("params { bool b = true; } state { bool c = b; } on_bar { target(1); }"),
            [Code::ConstParamNotInt]
        );
        ok(
            "params { real<lower=0> r = 1.5; int n = 2; } state { bool c = false; real s = r * n - 1 / 2; } on_bar { target(s); }",
        );
        assert_eq!(
            errors("on_bar { for (i in 1:held) { } target(1); }"),
            [Code::NotConstant]
        );
        assert_eq!(
            errors("params { int<lower=1> n = 4; } on_bar { for (i in 1:n / 2) { } target(1); }"),
            [Code::CountDivision]
        );
        assert_eq!(
            errors("on_bar { for (i in 1:2.5) { } target(1); }"),
            [Code::NotWholeNumber]
        );
        assert_eq!(
            errors("on_bar { for (i in 1:nope) { } target(1); }"),
            [Code::UnknownName]
        );
    }

    #[test]
    fn ranges_over_the_bounds() {
        // Ranges are intervals: n * n - 3 is at least 1 from n >= 2, but m - n is not proved positive from
        // m >= n + 1, since the two ranges are not related.
        ok("params { int<lower=2> n = 2; } series { real b = sma(close, n * n - 3); } on_bar { target(b); }");
        assert_eq!(
            errors(
                "params { int<lower=2> n = 2; int<lower=n + 1> m = 5; } series { real a = sma(close, m - n); } on_bar { target(a); }"
            ),
            [Code::WindowBelowOne]
        );
        assert_eq!(
            errors(
                "params { int<lower=2> n = 2; int m = 5; } series { real a = sma(close, n * m); } on_bar { target(a); }"
            ),
            [Code::WindowBelowOne]
        );
        assert_eq!(
            errors(
                "params { int<lower=-3, upper=4> n = 2; } series { real a = sma(close, n * n + 1); } on_bar { target(a); }"
            ),
            [Code::WindowBelowOne]
        );
    }
}
