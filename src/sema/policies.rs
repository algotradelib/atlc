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
            // The prototype accepted `min` here and ignored it.
            if matches!(ind, Ind::Lag | Ind::Ema) {
                let rule = if ind == Ind::Lag {
                    "under `skip`, a lag reads the value in that exact slot, which may be missing"
                } else {
                    "under `skip`, an EMA ignores a missing value and keeps its previous output"
                };
                self.err(
                    Diag::new(
                        Code::MinNotApplicable,
                        format!("`{}` takes no `min`", ind.name()),
                        ms,
                        "it would have no effect",
                    )
                    .note(rule),
                );
                return None;
            }
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

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, ok};
    use crate::frontend::diag::Code;
    use crate::ir::{CExpr, Ind, NodeKind, Policy};

    fn policies(src: &str) -> Vec<(Ind, Policy, CExpr)> {
        let ir = ok(src);
        ir.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Ind(i, _, _, m) => Some((*i, m.policy, m.min.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn program_default_and_overrides() {
        let p = policies(
            "options { missing = skip; } series { real a = sma(close, 5); real b = sma(close, 6, missing = poison); } on_bar { target(a + b); }",
        );
        assert_eq!(
            p,
            [
                (Ind::Sma, Policy::Skip, CExpr::Num(1.0)),
                (Ind::Sma, Policy::Poison, CExpr::Num(1.0))
            ]
        );
        let p = policies("series { real a = stdev(close, 5, missing = skip, min = 3); } on_bar { target(a); }");
        assert_eq!(p, [(Ind::Stdev, Policy::Skip, CExpr::Num(3.0))]);
        assert_eq!(
            errors("options { missing = drop; } on_bar { target(1); }"),
            [Code::UnknownPolicy]
        );
        assert_eq!(
            errors("options { fill = skip; } on_bar { target(1); }"),
            [Code::UnknownOption]
        );
    }

    #[test]
    fn named_arguments() {
        assert_eq!(
            errors("series { real a = sma(close, 5, window = 3); } on_bar { target(a); }"),
            [Code::UnknownNamedArgument]
        );
        assert_eq!(
            errors("series { real a = abs(close, missing = skip); } on_bar { target(a); }"),
            [Code::NamedArgumentNotAllowed]
        );
        assert_eq!(
            errors("series { real a = ols_slope(close, open, 5, missing = skip); } on_bar { target(a); }"),
            [Code::NamedArgumentNotAllowed]
        );
        assert_eq!(
            errors("series { real a = ts_sum(close, 5, 3, missing = skip); } on_bar { target(a); }"),
            [Code::NamedArgumentNotAllowed]
        );
        assert_eq!(
            errors("series { real a = sma(close, 5, min = 2); } on_bar { target(a); }"),
            [Code::MinWithoutSkip]
        );
        assert_eq!(
            errors("series { real a = sma(close, 5, missing = skip, min = 6); } on_bar { target(a); }"),
            [Code::MinOutOfRange]
        );
        assert_eq!(
            errors("series { real a = sma(close, 5, missing = skip, min = 0); } on_bar { target(a); }"),
            [Code::MinOutOfRange]
        );
        assert_eq!(
            errors("series { real a = rsi(close, 5, missing = skip); } on_bar { target(a); }"),
            [Code::SkipUnsupported]
        );
        assert_eq!(
            errors("options { missing = skip; } series { real a = ts_max(close, 5); } on_bar { target(a); }"),
            [Code::SkipUnsupported]
        );
        ok(
            "options { missing = skip; } series { real a = ts_max(close, 5, missing = restart); } on_bar { target(a); }",
        );
    }

    #[test]
    fn min_is_refused_on_lag_and_ema() {
        // A departure from the prototype, which accepted `min` on these and ignored it.
        for call in [
            "lag(close, 3, missing = skip, min = 2)",
            "ema(close, 5, missing = skip, min = 2)",
            "lag(close, 3, min = 1)",
            "ema(close, 5, missing = restart, min = 1)",
        ] {
            let src = format!("series {{ real a = {call}; }} on_bar {{ target(a); }}");
            assert_eq!(errors(&src), [Code::MinNotApplicable], "{call}");
        }
        // Without `min` both keep every policy.
        let p = policies(
            "series { real a = lag(close, 3, missing = skip); real b = ema(close, 5, missing = poison); } on_bar { target(a + b); }",
        );
        assert_eq!(
            p,
            [
                (Ind::Lag, Policy::Skip, CExpr::Num(1.0)),
                (Ind::Ema, Policy::Poison, CExpr::Num(1.0))
            ]
        );
    }
}
