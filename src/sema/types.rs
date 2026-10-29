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

impl Checker<'_> {
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
