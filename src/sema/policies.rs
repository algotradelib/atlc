//! Missing-value policies: `options { missing = ...; }` and the `missing =` and `min =` arguments.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

impl Checker<'_> {
    /// One setting of `options {}`.
    pub(super) fn option(&mut self, key: &Ident, value: &Expr) {
        match key.name.as_str() {
            "missing" => {
                if let Some(p) = self.policy_of(value) {
                    self.ir.missing = p;
                }
            }
            _ => self.err(
                Diag::new(
                    Code::UnknownOption,
                    format!("unknown option `{}`", key.name),
                    key.span,
                    "not an option",
                )
                .note("options: missing = restart | skip | poison"),
            ),
        }
    }

    pub(super) fn policy_of(&mut self, value: &Expr) -> Option<Policy> {
        if let ExprKind::Name(n) = &value.kind {
            if let Some(p) = Policy::from_name(n) {
                return Some(p);
            }
        }
        self.err(Diag::new(Code::UnknownPolicy, format!("`{}` is not a missing-value policy", crate::frontend::ast_text(value)), value.span, "expected restart, skip or poison")
            .note("restart: the window starts again after a missing value; skip: a missing value is left out of the window's result; poison: undefined for good from the first missing value"));
        None
    }

    /// An indicator's `missing = ...` and `min = ...` arguments; the program's policy by default.
    pub(super) fn miss_of(
        &mut self,
        ind: Ind,
        named: &[(&Ident, &Expr)],
        window: Option<&CExpr>,
        span: Span,
    ) -> Option<Miss> {
        let mut miss = Miss {
            policy: self.ir.missing,
            min: CExpr::Num(1.0),
        };
        let mut min_span = None;
        for (k, v) in named {
            match k.name.as_str() {
                "missing" => miss.policy = self.policy_of(v)?,
                "min" => {
                    let (m, _) = self.cexpr(v, "`min`", true)?;
                    miss.min = m;
                    min_span = Some(v.span);
                }
                _ => {
                    self.err(
                        Diag::new(
                            Code::UnknownNamedArgument,
                            format!("`{}` takes no argument `{}`", ind.name(), k.name),
                            k.span,
                            "unknown argument",
                        )
                        .note("named arguments: missing = restart | skip | poison, and min = N with skip"),
                    );
                    return None;
                }
            }
        }
        if let Some(ms) = min_span {
            if miss.policy != Policy::Skip {
                self.err(
                    Diag::new(
                        Code::MinWithoutSkip,
                        "`min` applies to `missing = skip` only",
                        ms,
                        "not used",
                    )
                    .note("restart and poison need a full window of values"),
                );
                return None;
            }
            let lo = self.interval(&miss.min).0;
            let room = window
                .map(|w| {
                    self.interval(&CExpr::Bin(BinOp::Sub, Box::new(w.clone()), Box::new(miss.min.clone())))
                        .0
                })
                .unwrap_or(0.0);
            if lo < 1.0 || room < 0.0 {
                self.err(Diag::new(
                    Code::MinOutOfRange,
                    "`min` must be between 1 and the window",
                    ms,
                    "may be outside within the params' bounds",
                ));
                return None;
            }
        }
        if miss.policy == Policy::Skip && matches!(ind, Ind::Rsi | Ind::TsMax | Ind::TsMin) {
            self.err(
                Diag::new(
                    Code::SkipUnsupported,
                    format!("`{}` has no `skip` rule yet", ind.name()),
                    span,
                    "restart or poison only",
                )
                .note(format!(
                    "write `missing = restart` or `missing = poison` in the call{}",
                    if self.ir.missing == Policy::Skip {
                        " (the program's default is skip)"
                    } else {
                        ""
                    }
                )),
            );
            return None;
        }
        Some(miss)
    }

    /// Positional and named arguments of a call.
    pub(super) fn split_args(args: &[Expr]) -> (Vec<&Expr>, Vec<(&Ident, &Expr)>) {
        let mut pos = vec![];
        let mut named = vec![];
        for a in args {
            match &a.kind {
                ExprKind::Named(k, v) => named.push((k, v.as_ref())),
                _ => pos.push(a),
            }
        }
        (pos, named)
    }

    pub(super) fn no_named(&mut self, what: &str, args: &[Expr]) -> bool {
        for a in args {
            if let ExprKind::Named(k, _) = &a.kind {
                self.err(Diag::new(Code::NamedArgumentNotAllowed, format!("`{}` takes no named argument", what), k.span, "not here")
                    .note("only indicators over one series take `missing = ...` (sma ema lag ts_sum stdev stdev_pop zscore, and rsi ts_max ts_min with restart or poison)"));
                return false;
            }
        }
        true
    }
}
