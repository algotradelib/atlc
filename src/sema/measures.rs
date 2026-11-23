//! Measures: `measures {}` declared before every other block and checked after the handlers; what a measure reads
//! (the equity curve, earlier measures, functions), and the refusals of a measure or a reduction anywhere else.

use crate::builtins;
use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

/// The curve's names inside `measures {}`, and what each is. They are contextual: resolved before the program's own
/// names there, ordinary names everywhere else.
const CURVE: [(&str, Curve, &str); 5] = [
    ("equity", Curve::Equity, "the curve's level"),
    ("ret", Curve::Ret, "the curve's period return"),
    ("drawdown", Curve::Drawdown, "the curve's decline from its peak"),
    ("periods_per_year", Curve::PeriodsPerYear, "the run's periods per year"),
    ("fills", Curve::Fills, "the replay's number of fills"),
];

const READS_NOTE: &str = "a measure reads the curve (`equity`, `ret` and `drawdown` inside a reduction; `periods_per_year`, `fills`), earlier measures, numbers and functions";

/// The curve's names, for suggestions.
pub(super) fn curve_names() -> impl Iterator<Item = &'static str> {
    CURVE.iter().map(|c| c.0)
}

impl Checker<'_> {
    /// Pass 1: each measure's name, so that every later pass knows it.
    pub(super) fn declare_measures(&mut self, program: &Program) {
        for item in &program.items {
            let Item::Measures(decls, _) = item else {
                continue;
            };
            for d in decls {
                if let Some((n, _, what)) = CURVE.iter().find(|c| c.0 == d.name.name) {
                    self.err(
                        Diag::new(
                            Code::BuiltInName,
                            format!("`{}` is a built-in name", n),
                            d.name.span,
                            "choose another name",
                        )
                        .note(format!("in `measures {{}}`, `{}` is {}", n, what)),
                    );
                    continue;
                }
                if d.ty != Ty::Real {
                    self.err(
                        Diag::new(
                            Code::MeasureNotReal,
                            format!("measure `{}` is `{}`", d.name.name, d.ty.name()),
                            d.ty_span,
                            "write `real`",
                        )
                        .note("a measure is one number for the whole replay, undefined where it cannot be computed"),
                    );
                    self.poison(&d.name);
                    continue;
                }
                let i = self.ir.measures.len();
                if self.declare(&d.name, Name::Measure(i)) {
                    let expr = Self::h(HKind::Num(f64::NAN), Ty::Real, true);
                    self.ir.measures.push(Measure {
                        name: d.name.name.clone(),
                        expr,
                    });
                }
            }
        }
    }

    /// Pass 2: each measure's expression, in declaration order, once every other name is declared.
    pub(super) fn measures(&mut self, program: &Program) {
        let mut next = 0;
        for item in &program.items {
            let Item::Measures(decls, _) = item else {
                continue;
            };
            for d in decls {
                let declared = match self.names.get(&d.name.name) {
                    Some((Name::Measure(i), span)) if *span == d.name.span => Some(*i),
                    _ => None,
                };
                // A measure refused in pass 1 is still checked, as if declared where it is.
                let i = declared.unwrap_or(next);
                next = i + usize::from(declared.is_some());
                self.measure = Some(i);
                let v = self.hexpr(&d.init, HandlerKind::Measures);
                self.measure = None;
                if let (Some(v), Some(i)) = (v, declared)
                    && self.assignable(Ty::Real, v.ty, false, d.init.span, &d.name.name)
                {
                    self.ir.measures[i].expr = v;
                }
            }
        }
    }

    /// In a measure: the expressions `hexpr` must not see (a name, a field, a lag, an indicator, a reduction), or
    /// `None` for those it checks as in a handler (numbers, operators, functions).
    pub(super) fn measure_expr(&mut self, e: &Expr) -> Option<Option<HExpr>> {
        match &e.kind {
            ExprKind::Name(n) => Some(self.measure_name(n, e.span)),
            ExprKind::Call(name, _) if matches!(self.names.get(&name.name), Some((Name::Function(_), _))) => None,
            ExprKind::Call(name, args) if builtins::in_reduction_family(&name.name) => {
                Some(self.reduce(name, args, e.span))
            }
            ExprKind::Call(name, _)
                if !builtins::in_cross_section_family(&name.name)
                    && !builtins::is_indicator_name(&name.name)
                    && !matches!(self.names.get(&name.name), Some((Name::Indicator(_), _))) =>
            {
                None
            }
            ExprKind::Call(..) | ExprKind::Field(..) | ExprKind::Index(..) | ExprKind::Member(..) => {
                self.not_in_measures(&crate::frontend::ast_text(e), e.span);
                Some(None)
            }
            _ => None,
        }
    }

    /// A name in a measure: the curve first, then earlier measures; nothing else of the program.
    fn measure_name(&mut self, n: &str, span: Span) -> Option<HExpr> {
        if let Some(&(_, c, what)) = CURVE.iter().find(|c| c.0 == n) {
            if c.per_point() && !self.reducing {
                self.err(
                    Diag::new(
                        Code::PointOutsideReduction,
                        format!("`{}` is one value per point of the curve", n),
                        span,
                        "outside a reduction",
                    )
                    .note(format!(
                        "a measure is one number: reduce {}, as `cum_mean({})`",
                        what, n
                    )),
                );
                return None;
            }
            return Some(Self::h(HKind::Curve(c), Ty::Real, true));
        }
        match self.names.get(n).cloned() {
            Some((Name::Poisoned, _)) => None,
            Some((Name::Measure(j), decl)) => {
                let i = self.measure.unwrap_or(0);
                if j >= i {
                    let mut d = Diag::new(
                        Code::MeasureOrder,
                        format!("measure `{}` is not computed yet", n),
                        span,
                        if j == i { "the measure itself" } else { "declared later" },
                    );
                    if j > i {
                        d = d.also(decl, "declared here");
                    }
                    self.err(d.note("measures are computed in declaration order: a measure reads the ones before it"));
                    return None;
                }
                Some(Self::h(HKind::Measure(j), Ty::Real, true))
            }
            Some(_) => {
                self.not_in_measures(n, span);
                None
            }
            None if field_of(n).is_some() || ["held", "keep", "bars", "periods", "fill"].contains(&n) => {
                self.not_in_measures(n, span);
                None
            }
            None => {
                self.unknown(n, span);
                None
            }
        }
    }

    fn not_in_measures(&mut self, shown: &str, span: Span) {
        self.err(
            Diag::new(
                Code::NotInMeasures,
                format!("`{}` has no value in `measures {{}}`", shown),
                span,
                "not the curve",
            )
            .note(READS_NOTE),
        );
    }

    /// `cum_mean(x)`: one positional argument, read at every point of the curve.
    fn reduce(&mut self, name: &Ident, args: &[Expr], span: Span) -> Option<HExpr> {
        let op = self.reduction_known(name)?;
        if self.reducing {
            self.err(
                Diag::new(
                    Code::ReductionOutsideMeasures,
                    format!("`{}` inside a reduction's argument", name.name),
                    span,
                    "not here",
                )
                .note("name the inner reduction as an earlier measure (`real m = cum_mean(ret);`) and read `m` here"),
            );
            return None;
        }
        if !self.no_named(&name.name, args) {
            return None;
        }
        if args.len() != 1 {
            self.err(Diag::new(
                Code::FunctionArity,
                format!("`{}` takes 1 argument(s)", name.name),
                span,
                format!("{} given", args.len()),
            ));
            return None;
        }
        self.reducing = true;
        let a = self.hexpr(&args[0], HandlerKind::Measures);
        self.reducing = false;
        let a = a?;
        let wants_bool = builtins::reduction_takes_bool(op);
        if (a.ty == Ty::Bool) != wants_bool {
            self.err(Diag::new(
                Code::ArgumentType,
                if wants_bool {
                    format!("`{}` counts the points where a `bool` is true", name.name)
                } else {
                    format!("`{}` takes a number", name.name)
                },
                args[0].span,
                format!("this is `{}`", a.ty.name()),
            ));
            return None;
        }
        Some(Self::h(HKind::Reduce(op, Box::new(a)), Ty::Real, true))
    }

    /// The reduction `name` names, or `None` after refusing an unknown name of the family.
    fn reduction_known(&mut self, name: &Ident) -> Option<CumOp> {
        let op = builtins::reduction(&name.name);
        if op.is_none() {
            self.err(
                Diag::new(
                    Code::UnknownFunction,
                    format!("unknown function `{}`", name.name),
                    name.span,
                    "not a function",
                )
                .note(builtins::unknown_reduction_note()),
            );
        }
        op
    }

    /// A `cum_` call outside `measures {}`.
    pub(super) fn reduction_elsewhere(&mut self, name: &Ident, span: Span) {
        if self.reduction_known(name).is_some() {
            self.err(
                Diag::new(
                    Code::ReductionOutsideMeasures,
                    format!("`{}` is computed in `measures {{}}`", name.name),
                    span,
                    "not here",
                )
                .note("a reduction runs over the finished replay's equity curve: nothing during the replay reads one"),
            );
        }
    }

    /// A measure read or assigned outside `measures {}`.
    pub(super) fn measure_read(&mut self, n: &str, span: Span, decl: Span, assigned: bool) {
        self.err(
            Diag::new(
                Code::MeasureRead,
                if assigned {
                    format!("cannot assign to measure `{}`", n)
                } else {
                    format!("`{}` is a measure", n)
                },
                span,
                "computed after the replay",
            )
            .also(decl, "declared in `measures {}`")
            .note("measures are computed after the replay; nothing during it reads one"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, message, ok};
    use crate::frontend::diag::Code;
    use crate::ir::{Curve, HKind};

    const BAR: &str = "on_bar { target(1); } ";

    #[test]
    fn measures_read_the_curve_and_earlier_measures() {
        // The curve's `ret`, not the series of that name; whatever the block order.
        let ir = ok(
            "measures { real s = cum_mean(ret); real t = s * sqrt(periods_per_year) + fills; } \
             series { real ret = close / close[1] - 1; } on_bar { target(ret > 0 ? 1 : 0); }",
        );
        assert_eq!(ir.measures.len(), 2);
        let HKind::Reduce(_, a) = &ir.measures[0].expr.kind else {
            panic!("{:?}", ir.measures[0].expr)
        };
        assert_eq!(a.kind, HKind::Curve(Curve::Ret));
        ok(&format!(
            "{BAR} measures {{ real c = cum_count(defined(ret) and drawdown > 0.2); real one = 1; }}"
        ));
        ok(&format!(
            "functions {{ int twice(int k) {{ return 2 * k; }} }} {BAR} measures {{ real a = cum_sum(twice(3)); }}"
        ));
        ok(&format!("{BAR} measures {{ }}"));
    }

    #[test]
    fn what_a_measure_refuses() {
        let e = |m: &str| errors(&format!("{BAR} measures {{ {m} }}"));
        assert_eq!(e("int n = 1;"), [Code::MeasureNotReal]);
        assert_eq!(e("real a = ret;"), [Code::PointOutsideReduction]);
        assert_eq!(e("real a = cum_mean(cum_mean(ret));"), [Code::ReductionOutsideMeasures]);
        assert_eq!(e("real a = b; real b = 1;"), [Code::MeasureOrder]);
        assert_eq!(e("real a = a + 1;"), [Code::MeasureOrder]);
        assert_eq!(e("real a = cum_median(ret);"), [Code::UnknownFunction]);
        assert_eq!(e("real a = cum_mean();"), [Code::FunctionArity]);
        assert_eq!(
            e("real a = cum_mean(ret, missing = skip);"),
            [Code::NamedArgumentNotAllowed]
        );
        assert_eq!(e("real a = cum_sum(ret > 0);"), [Code::ArgumentType]);
        assert_eq!(e("real a = cum_count(ret);"), [Code::ArgumentType]);
        assert_eq!(e("real a = ret > 0;"), [Code::PointOutsideReduction]);
        assert_eq!(e("real a = true;"), [Code::TypeMismatch]);
        assert_eq!(e("real a = cum_mean(close[1]);"), [Code::NotInMeasures]);
        assert_eq!(e("real a = cum_sum(ma(close, 2));"), [Code::NotInMeasures]);
        assert_eq!(e("real a = held + keep;"), [Code::NotInMeasures, Code::NotInMeasures]);
        assert_eq!(e("real a = target(1);"), [Code::TargetAsValue]);
        assert_eq!(e("real ret = 1;"), [Code::BuiltInName]);
        assert_eq!(e("real cum_sum = 1;"), [Code::BuiltInName]);
        assert_eq!(
            errors(&format!("params {{ int n = 1; }} {BAR} measures {{ real a = n; }}")),
            [Code::NotInMeasures]
        );
        assert_eq!(
            errors("universe {} on_bar { target(1); } measures { real a = xs_mean(1); }"),
            [Code::NotInMeasures]
        );
        assert_eq!(
            message(&format!("{BAR} measures {{ real a = fill.price; }}")),
            "`fill.price` has no value in `measures {}`"
        );
    }

    #[test]
    fn nothing_else_reads_a_measure() {
        let m = "measures { real m = 1; }";
        assert_eq!(errors(&format!("{m} on_bar {{ target(m); }}")), [Code::MeasureRead]);
        assert_eq!(
            errors(&format!("{m} on_bar {{ m = 1; target(1); }}")),
            [Code::MeasureRead]
        );
        assert_eq!(
            errors(&format!("{m} series {{ real s = close * m; }} on_bar {{ target(s); }}")),
            [Code::MeasureRead]
        );
        assert_eq!(
            errors(&format!("{m} state {{ real s = m; }} {BAR}")),
            [Code::MeasureRead]
        );
        assert_eq!(
            errors(&format!("{m} params {{ int<lower=m> n = 1; }} {BAR}")),
            [Code::MeasureRead]
        );
        assert_eq!(
            errors(&format!(
                "{m} functions {{ real f(real x) {{ return x * m; }} }} on_bar {{ target(f(1)); }}"
            )),
            [Code::MeasureRead]
        );
        assert_eq!(
            errors(&format!("params {{ int m = 1; }} {m} {BAR}")),
            [Code::DuplicateName]
        );
        assert_eq!(
            errors("on_bar { target(cum_mean(close)); }"),
            [Code::ReductionOutsideMeasures]
        );
        assert_eq!(
            errors("series { real s = cum_sum(close); } on_bar { target(s); }"),
            [Code::ReductionOutsideMeasures]
        );
        assert_eq!(
            errors(&format!("state {{ real s = cum_sum(1); }} {BAR}")),
            [Code::ReductionOutsideMeasures]
        );
        assert_eq!(errors("on_bar { target(cum_median(close)); }"), [Code::UnknownFunction]);
        // A program without measures may name a series `ret` and `drawdown`.
        ok("series { real ret = close / close[1] - 1; real drawdown = ret; } on_bar { target(drawdown); }");
    }

    #[test]
    fn suggestions_follow_the_context() {
        let notes = |src: &str| crate::check(src).unwrap_err()[0].notes.clone();
        assert_eq!(
            notes(&format!("{BAR} measures {{ real a = cum_mean(rett); }}")),
            ["did you mean `ret`?"]
        );
        assert_eq!(
            notes(&format!("{BAR} measures {{ real sharpe = 1; real b = sharp; }}")),
            ["did you mean `sharpe`?"]
        );
        assert!(notes("measures { real sharpe = 1; } on_bar { target(sharp); }").is_empty());
    }
}
