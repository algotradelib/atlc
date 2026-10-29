//! Windows and lookback: built-in indicators' windows, proved at least 1 (or 2) from the parameters'
//! bounds, the lookback of every series value, the warm-up, and which series any handler reads.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::frontend::parser::MAX_DEPTH;
use crate::ir::*;

use super::*;

impl Checker<'_> {
    /// A lookback built from parameters deepens by one level per indicator over an indicator (`sma(s, k)`
    /// over `s = sma(close, k)`...): refused past the parser's nesting limit, so that no expression the
    /// checker builds is deeper than the program's own tree may be.
    pub(super) fn lookback_fits(&mut self, lb: &CExpr, span: Span) -> bool {
        fn deeper_than(c: &CExpr, limit: usize) -> bool {
            limit == 0
                || match c {
                    CExpr::Num(_) | CExpr::Param(_) => false,
                    CExpr::Neg(a) => deeper_than(a, limit - 1),
                    CExpr::Bin(_, a, b) => deeper_than(a, limit - 1) || deeper_than(b, limit - 1),
                    CExpr::Max(v) => v.iter().any(|x| deeper_than(x, limit - 1)),
                }
        }
        if deeper_than(lb, MAX_DEPTH) {
            self.err(
                Diag::new(
                    Code::NestingTooDeep,
                    format!("this series' lookback nests deeper than {MAX_DEPTH} levels"),
                    span,
                    "too deep",
                )
                .note("each indicator whose window is a parameter, over a series that has one too, adds a level; give some windows a number"),
            );
            return false;
        }
        true
    }

    pub(super) fn lookback_max(&self, ids: &[usize]) -> CExpr {
        let mut terms: Vec<CExpr> = vec![];
        for &i in ids {
            match &self.ir.nodes[i].lookback {
                CExpr::Num(v) if *v <= 1.0 => {}
                CExpr::Max(v) => {
                    for t in v {
                        if !terms.contains(t) {
                            terms.push(t.clone());
                        }
                    }
                }
                other => {
                    if !terms.contains(other) {
                        terms.push(other.clone());
                    }
                }
            }
        }
        match terms.len() {
            0 => CExpr::Num(1.0),
            1 => terms.pop().unwrap(),
            _ => CExpr::Max(terms),
        }
    }

    pub(super) fn indicator(
        &mut self,
        ind: Ind,
        x: &Expr,
        w: &Expr,
        span: Span,
        indexed: bool,
        miss: Miss,
    ) -> Option<usize> {
        let input = self.sexpr(x);
        let window = self.cexpr(w, &format!("`{}`'s window", ind.name()), true);
        let (input, (window, _)) = (input?, window?);
        if self.ir.nodes[input].ty != Ty::Real && self.ir.nodes[input].ty != Ty::Int {
            self.err(
                Diag::new(
                    Code::SeriesNotNumeric,
                    format!("`{}` needs a real series", ind.name()),
                    x.span,
                    format!("this is `{}`", self.ir.nodes[input].ty.name()),
                )
                .note("write `c ? 1 : 0` to count a condition"),
            );
            return None;
        }
        let (lo, _) = self.interval(&window);
        if lo < 1.0 {
            let literal = matches!(window, CExpr::Num(_) | CExpr::Neg(_));
            if ind == Ind::Lag && literal && self.eval(&window) <= 0.0 {
                let shown = if indexed {
                    format!("{}[{}]", crate::frontend::ast_text(x), crate::frontend::ast_text(w))
                } else {
                    format!(
                        "lag({}, {})",
                        crate::frontend::ast_text(x),
                        crate::frontend::ast_text(w)
                    )
                };
                self.err(
                    Diag::new(
                        Code::LookAhead,
                        format!("`{}` would read a future bar", shown),
                        w.span,
                        if self.eval(&window) == 0.0 {
                            "a lag of 0 is the current bar: write the series itself"
                        } else {
                            "a negative lag looks ahead"
                        },
                    )
                    .note("a strategy only sees bars up to the current one; a lag is at least 1"),
                );
            } else if lo == f64::NEG_INFINITY {
                self.err(
                    Diag::new(
                        Code::WindowBelowOne,
                        format!("`{}`'s window may be below 1", ind.name()),
                        w.span,
                        "nothing bounds this from below",
                    )
                    .note("give the param a lower bound, e.g. `int<lower=1> n = 20;`"),
                );
            } else {
                self.err(
                    Diag::new(
                        Code::WindowBelowOne,
                        format!("`{}`'s window may be below 1", ind.name()),
                        w.span,
                        format!("can be as low as {} within the params' bounds", fmt_num(lo)),
                    )
                    .note("raise the params' lower bounds so the window is at least 1"),
                );
            }
            return None;
        }
        let in_class = self.ir.nodes[input].class;
        let mut class = if in_class == Class::Maybe {
            Class::Maybe
        } else {
            Class::Eventually
        };
        // A missing row (period clock) can reach any window; poison or a skip window may stay undefined.
        if self.ir.period_clock || (miss.policy != Policy::Restart && in_class == Class::Maybe) {
            class = Class::Maybe;
        }
        if matches!(ind, Ind::Stdev | Ind::Zscore) || (ind == Ind::StdevPop && miss.policy == Policy::Restart) {
            if self.interval(&window).0 < 2.0 {
                self.err(
                    Diag::new(
                        Code::WindowBelowTwo,
                        format!("`{}`'s window may be below 2", ind.name()),
                        w.span,
                        "a deviation needs two values",
                    )
                    .note("raise the params' lower bounds so the window is at least 2"),
                );
                return None;
            }
            if ind == Ind::Zscore {
                // A window of equal values has no deviation.
                class = Class::Maybe;
            }
        }
        let base = self.ir.nodes[input].lookback.clone();
        let extra = if ind.steps_back() {
            window.clone()
        } else {
            match &window {
                CExpr::Num(v) => CExpr::Num(v - 1.0),
                _ => CExpr::Bin(BinOp::Sub, Box::new(window.clone()), Box::new(CExpr::Num(1.0))),
            }
        };
        let mut lb = match (&base, &extra) {
            (CExpr::Num(a), CExpr::Num(b)) => CExpr::Num(a + b),
            (_, CExpr::Num(b)) if *b == 0.0 => base.clone(),
            _ => CExpr::Bin(BinOp::Add, Box::new(base.clone()), Box::new(extra)),
        };
        if miss.policy == Policy::Skip && ind != Ind::Lag {
            // A skip window counts slots, missing or not: it is defined once it has seen its window
            // and holds `min` values, which an input defined from bar L has by bar L + min - 1.
            let need = match (&base, &miss.min) {
                (CExpr::Num(a), CExpr::Num(m)) => CExpr::Num(a + m - 1.0),
                _ => CExpr::Bin(
                    BinOp::Sub,
                    Box::new(CExpr::Bin(
                        BinOp::Add,
                        Box::new(base.clone()),
                        Box::new(miss.min.clone()),
                    )),
                    Box::new(CExpr::Num(1.0)),
                ),
            };
            lb = match (&window, &need) {
                (CExpr::Num(a), CExpr::Num(b)) => CExpr::Num(a.max(*b)),
                (_, CExpr::Num(b)) if *b <= 1.0 => window.clone(),
                _ => CExpr::Max(vec![window.clone(), need]),
            };
        }
        if !self.lookback_fits(&lb, span) {
            return None;
        }
        Some(self.add_node(NodeKind::Ind(ind, input, window, miss), Ty::Real, class, lb))
    }

    /// `ts_sum(x, n, min)`: a window of n defined once `min` values are in it.
    pub(super) fn partial(&mut self, x: &Expr, w: &Expr, m: &Expr) -> Option<usize> {
        let id = self.indicator(Ind::TsSum, x, w, x.span, false, Miss::default())?;
        let NodeKind::Ind(_, input, window, _) = self.ir.nodes[id].kind.clone() else {
            return None;
        };
        let (min, _) = self.cexpr(m, "`ts_sum`'s minimum count", true)?;
        let lo = self.interval(&min).0;
        let room = self
            .interval(&CExpr::Bin(BinOp::Sub, Box::new(window.clone()), Box::new(min.clone())))
            .0;
        if lo < 1.0 || room < 0.0 {
            self.err(
                Diag::new(
                    Code::MinOutOfRange,
                    "`ts_sum`'s minimum count must be between 1 and the window",
                    m.span,
                    format!(
                        "can be {} within the params' bounds",
                        if lo < 1.0 { "below 1" } else { "above the window" }
                    ),
                )
                .note("`ts_sum(x, n, m)` is defined once m of the last n values are in"),
            );
            return None;
        }
        let base = self.ir.nodes[input].lookback.clone();
        let extra = match &min {
            CExpr::Num(v) => CExpr::Num(v - 1.0),
            _ => CExpr::Bin(BinOp::Sub, Box::new(min.clone()), Box::new(CExpr::Num(1.0))),
        };
        let lb = match (&base, &extra) {
            (CExpr::Num(a), CExpr::Num(b)) => CExpr::Num(a + b),
            (_, CExpr::Num(b)) if *b == 0.0 => base.clone(),
            _ => CExpr::Bin(BinOp::Add, Box::new(base.clone()), Box::new(extra)),
        };
        let mut class = self.ir.nodes[id].class;
        if self.ir.period_clock {
            class = Class::Maybe;
        }
        if !self.lookback_fits(&lb, x.span.to(m.span)) {
            return None;
        }
        Some(self.add_node(NodeKind::Partial(input, window, min), Ty::Real, class, lb))
    }

    /// `ols_slope(y, x, n)`, `ols_intercept(y, x, n)`: least squares of y on x over the last n bars.
    pub(super) fn pair(&mut self, which: PairInd, args: &[Expr], span: Span) -> Option<usize> {
        if args.len() != 3 {
            self.err(Diag::new(
                Code::IndicatorArity,
                format!("`{}` takes (y, x, window)", which.name()),
                span,
                format!("{} argument(s)", args.len()),
            ));
            return None;
        }
        let (a, b) = (self.sexpr(&args[0]), self.sexpr(&args[1]));
        let window = self.cexpr(&args[2], &format!("`{}`'s window", which.name()), true);
        let (a, b, (window, _)) = (a?, b?, window?);
        if !self.real_input(a, &args[0], which.name()) || !self.real_input(b, &args[1], which.name()) {
            return None;
        }
        let lo = self.interval(&window).0;
        if lo < 2.0 {
            self.err(
                Diag::new(
                    Code::WindowBelowTwo,
                    format!("`{}`'s window may be below 2", which.name()),
                    args[2].span,
                    "a line needs two points",
                )
                .note("raise the params' lower bounds so the window is at least 2"),
            );
            return None;
        }
        let base = self.lookback_max(&[a, b]);
        let lb = CExpr::Bin(
            BinOp::Add,
            Box::new(base),
            Box::new(CExpr::Bin(
                BinOp::Sub,
                Box::new(window.clone()),
                Box::new(CExpr::Num(1.0)),
            )),
        );
        if !self.lookback_fits(&lb, span) {
            return None;
        }
        // A flat x makes the slope undefined.
        Some(self.add_node(NodeKind::Pair(which, a, b, window), Ty::Real, Class::Maybe, lb))
    }

    /// `kf_beta(y, x, delta, ve)` and its error and deviation: one Kalman filter of y on x and 1.
    pub(super) fn kalman(&mut self, out: KOut, name: &str, args: &[Expr], span: Span) -> Option<usize> {
        if args.len() != 4 {
            self.err(
                Diag::new(
                    Code::IndicatorArity,
                    format!("`{}` takes (y, x, delta, ve)", name),
                    span,
                    format!("{} argument(s)", args.len()),
                )
                .note("delta: how fast the hedge may drift; ve: the measurement variance"),
            );
            return None;
        }
        let (a, b) = (self.sexpr(&args[0]), self.sexpr(&args[1]));
        let delta = self.cexpr(&args[2], "the filter's delta", false);
        let ve = self.cexpr(&args[3], "the filter's measurement variance", false);
        let (a, b, (delta, _), (ve, _)) = (a?, b?, delta?, ve?);
        if !self.real_input(a, &args[0], name) || !self.real_input(b, &args[1], name) {
            return None;
        }
        let (dlo, dhi) = self.interval(&delta);
        let ve_lo = self.interval(&ve).0;
        if !(dlo >= 0.0 && dhi <= 1.0) || ve_lo.is_nan() || ve_lo < 0.0 {
            self.err(
                Diag::new(
                    Code::FilterParameters,
                    "the filter needs delta in [0, 1) and ve >= 0",
                    span,
                    "may be outside within the params' bounds",
                )
                .note("bound the params, e.g. `real<lower=0, upper=1> delta = 0.0001;`"),
            );
            return None;
        }
        let mut class = self.ir.nodes[a].class.max(self.ir.nodes[b].class);
        if self.ir.period_clock {
            class = Class::Maybe;
        }
        let lb = self.lookback_max(&[a, b]);
        Some(self.add_node(NodeKind::Kalman(a, b, delta, ve, out), Ty::Real, class, lb))
    }

    pub(super) fn liveness(&mut self) {
        let mut live = vec![false; self.ir.nodes.len()];
        let mut stack: Vec<usize> = self.reads.iter().map(|r| r.1).collect();
        while let Some(n) = stack.pop() {
            if live[n] {
                continue;
            }
            live[n] = true;
            match &self.ir.nodes[n].kind {
                NodeKind::Unary(_, a) => stack.push(*a),
                NodeKind::Binary(_, a, b) => {
                    stack.push(*a);
                    stack.push(*b)
                }
                NodeKind::Ternary(a, b, c) => {
                    stack.push(*a);
                    stack.push(*b);
                    stack.push(*c)
                }
                NodeKind::Call(_, args) => stack.extend(args.iter().copied()),
                NodeKind::Ind(_, a, _, _) | NodeKind::Partial(a, _, _) => stack.push(*a),
                NodeKind::Pair(_, a, b, _) | NodeKind::Kalman(a, b, _, _, _) => {
                    stack.push(*a);
                    stack.push(*b)
                }
                NodeKind::UCall(_, args) => stack.extend(args.iter().copied()),
                NodeKind::UInd { inputs, .. } => stack.extend(inputs.iter().copied()),
                _ => {}
            }
        }
        let mut kept: Vec<usize> = self
            .reads
            .iter()
            .filter(|r| r.0 != HandlerKind::OnBar)
            .map(|r| r.1)
            .collect();
        kept.sort();
        kept.dedup();
        self.ir.kept = kept;
        for (name, id) in self.ir.series.clone() {
            if !live[id] {
                let span = match self.names.get(&name) {
                    Some((Name::Series(_, s), _)) => *s,
                    _ => Span::default(),
                };
                self.ir.warnings.push(Diag::warning(
                    Code::UnusedSeries,
                    format!("series `{}` is never read", name),
                    span,
                    "not computed",
                ));
            }
        }
        self.ir.live = live;
    }

    /// `warmup EXPR;`: a whole number of bars built from numbers and int parameters.
    pub(super) fn warmup_item(&mut self, e: &Expr) {
        if let Some((w, ty)) = self.cexpr(e, "the warm-up", true) {
            if ty != Ty::Int {
                self.err(Diag::new(
                    Code::WarmupNotInt,
                    "the warm-up is a whole number of bars",
                    e.span,
                    format!("this is `{}`", ty.name()),
                ));
            }
            self.ir.warmup = Some(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, message, ok};
    use super::super::testing_ir::{lookback, value};
    use crate::frontend::diag::Code;

    fn program(series: &str) -> String {
        format!("params {{ int<lower=2> n = 20; int<lower=1> k = 3; }} series {{ {series} }} on_bar {{ target(0); }}")
    }

    #[test]
    fn lookbacks() {
        let ir = ok(&program(
            "real a = sma(close, n); real b = sma(sma(close, 20), 30); real c = lag(close, k); real d = close[2]; \
             real e = rsi(close, 14); real f = ts_sum(close, 20, 5); real g = ols_slope(close, open, n); \
             real h = kf_beta(close, open, 0.0001, 0.001); real i = a + c; real j = close; \
             real l = sma(close, 5) [2];",
        ));
        assert_eq!(lookback(&ir, "a"), 20.0);
        assert_eq!(lookback(&ir, "b"), 49.0);
        assert_eq!(lookback(&ir, "c"), 4.0);
        assert_eq!(lookback(&ir, "d"), 3.0);
        assert_eq!(lookback(&ir, "e"), 15.0);
        assert_eq!(lookback(&ir, "f"), 5.0);
        assert_eq!(lookback(&ir, "g"), 20.0);
        assert_eq!(lookback(&ir, "h"), 1.0);
        assert_eq!(lookback(&ir, "i"), 20.0);
        assert_eq!(lookback(&ir, "j"), 1.0);
        assert_eq!(lookback(&ir, "l"), 7.0);
    }

    #[test]
    fn skip_lookbacks() {
        // max(n, L + min - 1) for the skip windows; a lag keeps L + k.
        let ir = ok(&program(
            "real a = sma(close, 3, missing = skip); real b = sma(sma(close, 5), 3, missing = skip); \
             real c = sma(sma(close, 5), 3, missing = skip, min = 2); real d = lag(close, 3, missing = skip); \
             real e = stdev(sma(close, 10), 4, missing = skip, min = 3);",
        ));
        assert_eq!(lookback(&ir, "a"), 3.0);
        assert_eq!(lookback(&ir, "b"), 5.0);
        assert_eq!(lookback(&ir, "c"), 6.0);
        assert_eq!(lookback(&ir, "d"), 4.0);
        assert_eq!(lookback(&ir, "e"), 12.0);
    }

    #[test]
    fn windows_are_proved_from_the_bounds() {
        assert_eq!(
            errors("params { int n = 5; } series { real a = sma(close, n); } on_bar { target(a); }"),
            [Code::WindowBelowOne]
        );
        assert_eq!(
            errors("params { int<lower=0> n = 5; } series { real a = sma(close, n); } on_bar { target(a); }"),
            [Code::WindowBelowOne]
        );
        assert_eq!(
            message("params { int<lower=0> n = 5; } series { real a = sma(close, n); } on_bar { target(a); }"),
            "`sma`'s window may be below 1"
        );
        assert_eq!(
            errors("params { int<lower=1> n = 5; } series { real a = stdev(close, n); } on_bar { target(a); }"),
            [Code::WindowBelowTwo]
        );
        assert_eq!(
            errors("series { real a = ols_slope(close, open, 1); } on_bar { target(a); }"),
            [Code::WindowBelowTwo]
        );
        assert_eq!(
            errors("series { real a = lag(close, 0); } on_bar { target(a); }"),
            [Code::LookAhead]
        );
        assert_eq!(
            errors("series { real a = close[-1]; } on_bar { target(a); }"),
            [Code::LookAhead]
        );
        assert_eq!(
            errors("params { int<lower=2> n = 5; } series { real a = sma(close, n - 2); } on_bar { target(a); }"),
            [Code::WindowBelowOne]
        );
        assert_eq!(
            errors("series { real a = ts_sum(close, 5, 6); } on_bar { target(a); }"),
            [Code::MinOutOfRange]
        );
        assert_eq!(
            errors(
                "params { real<lower=0, upper=2> d = 0.5; } series { real a = kf_beta(close, open, d, 1); } on_bar { target(a); }"
            ),
            [Code::FilterParameters]
        );
        // stdev_pop takes a window of one under skip.
        ok("series { real a = stdev_pop(close, 1, missing = skip); } on_bar { target(a); }");
    }

    #[test]
    fn warmup_and_liveness() {
        let ir = ok(
            "params { int<lower=1> n = 4; } warmup n * 2 + 1; series { real a = sma(close, 3); real unused = sma(close, 9); } on_bar { target(a); }",
        );
        assert_eq!(value(&ir, ir.warmup.as_ref().unwrap()), 9.0);
        assert_eq!(ir.warnings.len(), 1);
        assert_eq!(ir.warnings[0].code, Code::UnusedSeries);
        let unused = ir.series.iter().find(|s| s.0 == "unused").unwrap().1;
        assert!(!ir.live[unused]);
        assert!(ir.live[ir.series[0].1]);
        assert_eq!(errors("warmup close; on_bar { target(1); }"), [Code::NotConstant]);
        assert_eq!(errors("warmup 2.5; on_bar { target(1); }"), [Code::NotWholeNumber]);
        // A series read only by a handler other than on_bar is kept for it.
        let ir =
            ok("series { real a = sma(close, 3); } state { real s = 0; } on_fill { s = a; } on_bar { target(s); }");
        assert_eq!(ir.kept, [ir.series[0].1]);
    }
}
