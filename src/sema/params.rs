//! Parameters: literal defaults, their types, and bounds that may refer to earlier parameters.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

impl Checker<'_> {
    pub(super) fn param(&mut self, d: &ParamDecl) {
        let lower = d
            .lower
            .as_ref()
            .and_then(|e| self.cexpr(e, "a bound", false).map(|(c, _)| (c, e)));
        let upper = d
            .upper
            .as_ref()
            .and_then(|e| self.cexpr(e, "a bound", false).map(|(c, _)| (c, e)));
        let default = match &d.default.kind {
            ExprKind::Num(v, int) => Some((*v, *int)),
            ExprKind::Unary(UnOp::Neg, inner) => match &inner.kind {
                ExprKind::Num(v, int) => Some((-*v, *int)),
                _ => None,
            },
            ExprKind::Bool(b) => Some((if *b { 1.0 } else { 0.0 }, false)),
            _ => None,
        };
        let Some((value, int_literal)) = default else {
            self.err(
                Diag::new(
                    Code::DefaultNotLiteral,
                    "a parameter's default is a number",
                    d.default.span,
                    "not a literal",
                )
                .note("defaults are literals; bounds may refer to earlier params"),
            );
            self.poison(&d.name);
            return;
        };
        match d.ty {
            Ty::Int if !int_literal => self.err(Diag::new(
                Code::IntDefaultNotWhole,
                format!("`{}` is an `int` but its default is not a whole number", d.name.name),
                d.default.span,
                "write a whole number, or declare it `real`",
            )),
            Ty::Bool if !matches!(d.default.kind, ExprKind::Bool(_)) => self.err(Diag::new(
                Code::BoolDefaultNotBool,
                format!("`{}` is a `bool`; its default is `true` or `false`", d.name.name),
                d.default.span,
                "not a bool",
            )),
            Ty::Real | Ty::Int if matches!(d.default.kind, ExprKind::Bool(_)) => self.err(Diag::new(
                Code::NumericDefaultIsBool,
                format!("`{}` is `{}` but its default is a bool", d.name.name, d.ty.name()),
                d.default.span,
                "not a number",
            )),
            _ => {}
        }
        let source_text = |e: &Expr| -> String { crate::frontend::ast_text(e) };
        let mut lo = f64::NEG_INFINITY;
        let mut hi = f64::INFINITY;
        if let Some((c, e)) = &lower {
            lo = self.interval(c).0;
            let at = self.eval(c);
            if value < at {
                self.err(
                    Diag::new(
                        Code::DefaultBelowBound,
                        format!(
                            "the default of `{}` ({}) is below its lower bound",
                            d.name.name,
                            fmt_num(value)
                        ),
                        d.default.span,
                        format!("must be at least {}", fmt_num(at)),
                    )
                    .also(
                        e.span,
                        format!("`{}` is {} with the defaults", source_text(e), fmt_num(at)),
                    ),
                );
            }
        }
        if let Some((c, e)) = &upper {
            hi = self.interval(c).1;
            let at = self.eval(c);
            if value > at {
                self.err(
                    Diag::new(
                        Code::DefaultAboveBound,
                        format!(
                            "the default of `{}` ({}) is above its upper bound",
                            d.name.name,
                            fmt_num(value)
                        ),
                        d.default.span,
                        format!("must be at most {}", fmt_num(at)),
                    )
                    .also(
                        e.span,
                        format!("`{}` is {} with the defaults", source_text(e), fmt_num(at)),
                    ),
                );
            }
        }
        if d.ty == Ty::Bool && (lower.is_some() || upper.is_some()) {
            self.err(Diag::new(
                Code::BoolBounds,
                "a `bool` has no bounds",
                d.ty_span,
                "remove the bounds",
            ));
        }
        let id = self.ir.params.len();
        self.ir.params.push(Param {
            name: d.name.name.clone(),
            ty: d.ty,
            default: value,
            lower: lower.map(|(c, e)| (c, source_text(e))),
            upper: upper.map(|(c, e)| (c, source_text(e))),
            span: d.name.span,
        });
        self.intervals.push((lo, hi));
        self.defaults.push(value);
        self.declare(&d.name, Name::Param(id));
    }
}
