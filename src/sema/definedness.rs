//! Definedness: which values may be undefined, narrowing by `defined(x)`, provably nonzero divisors,
//! and states that may hold an undefined value.

use crate::frontend::ast::*;
use crate::ir::*;

use super::*;

impl Checker<'_> {
    pub(super) fn state_maybe(&self) -> Vec<bool> {
        fn walk(stmts: &[HStmt], out: &mut Vec<bool>) {
            for s in stmts {
                match s {
                    HStmt::Assign { state, value } => {
                        if value.maybe {
                            out[*state] = true;
                        }
                    }
                    HStmt::If { then, els, .. } => {
                        walk(then, out);
                        walk(els, out);
                    }
                    HStmt::For { body, .. } => walk(body, out),
                    _ => {}
                }
            }
        }
        let mut out = vec![false; self.ir.state.len()];
        for k in [
            HandlerKind::OnStart,
            HandlerKind::OnBar,
            HandlerKind::OnFill,
            HandlerKind::OnStop,
            HandlerKind::OnOpen,
            HandlerKind::OnClose,
        ] {
            if let Some(b) = self.ir.handlers.get(k) {
                walk(b, &mut out);
            }
        }
        out
    }

    /// The values a condition proves defined when it is true: `defined(x)`, and both sides of `and`.
    pub(super) fn defined_refs(h: &HExpr) -> Vec<HKind> {
        match &h.kind {
            HKind::Call(Builtin::Defined, args) => match &args[0].kind {
                k @ (HKind::Node(_)
                | HKind::State(_)
                | HKind::Local(_)
                | HKind::UArg(_)
                | HKind::UState(_)
                | HKind::UValue(_)) => {
                    vec![k.clone()]
                }
                _ => vec![],
            },
            HKind::Binary(BinOp::And, a, b) => {
                let mut v = Self::defined_refs(a);
                v.extend(Self::defined_refs(b));
                v
            }
            _ => vec![],
        }
    }

    pub(super) fn refine(&self, mut h: HExpr) -> HExpr {
        if h.maybe && self.refined.contains(&h.kind) {
            h.maybe = false;
        }
        h
    }

    pub(super) fn node_nonzero(&self, id: usize) -> bool {
        match &self.ir.nodes[id].kind {
            NodeKind::Num(v) => *v != 0.0,
            NodeKind::Param(i) => {
                let (lo, hi) = self.intervals[*i];
                lo > 0.0 || hi < 0.0
            }
            _ => false,
        }
    }

    pub(super) fn hexpr_nonzero(&self, h: &HExpr) -> bool {
        match &h.kind {
            HKind::Num(v) => *v != 0.0,
            HKind::UArg(i) => self.uctx.as_ref().is_some_and(|c| c.nonzero.contains(i)),
            HKind::Param(i) => {
                let (lo, hi) = self.intervals[*i];
                lo > 0.0 || hi < 0.0
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, ok};
    use super::super::testing_ir::series;
    use crate::frontend::diag::Code;
    use crate::ir::Class;

    #[test]
    fn classes() {
        let ir = ok("params { int<lower=1> n = 5; real r = 0; } series { \
            real a = close * 2; real b = open; real c = sma(close, n); real d = close / 2; real e = close / close; \
            real f = close / n; real g = close / r; real h = zscore(close, 5); real i = log(close); \
            real j = sma(open, 3); bool k = defined(open); real l = sma(close, 3, missing = poison); \
            real m = sma(open, 3, missing = skip); } \
            on_bar { target(a + b + c + d + e + f + g + h + i + j + l + m); if (k) { target(0); } }");
        let class = |name| series(&ir, name).class;
        assert_eq!(class("a"), Class::Always);
        assert_eq!(class("b"), Class::Maybe);
        assert_eq!(class("c"), Class::Eventually);
        assert_eq!(class("d"), Class::Always);
        assert_eq!(class("e"), Class::Maybe);
        assert_eq!(class("f"), Class::Always);
        assert_eq!(class("g"), Class::Maybe);
        assert_eq!(class("h"), Class::Maybe);
        assert_eq!(class("i"), Class::Maybe);
        assert_eq!(class("j"), Class::Maybe);
        assert_eq!(class("k"), Class::Always);
        assert_eq!(class("l"), Class::Eventually);
        assert_eq!(class("m"), Class::Maybe);
        // A universe on a period clock: a missing row reaches every window.
        let ir = ok(
            "universe {} options { missing = skip; } series { real a = close; real b = sma(close, 3); } on_bar { target(a + b); }",
        );
        assert_eq!(series(&ir, "a").class, Class::Maybe);
        assert_eq!(series(&ir, "b").class, Class::Maybe);
    }

    #[test]
    fn defined_int_and_bool_values() {
        assert_eq!(
            errors("state { bool up = false; } on_bar { up = open > 1; target(1); }"),
            [Code::MaybeUndefined]
        );
        assert_eq!(
            errors("on_bar { bool up = open > 1; target(1); }"),
            [Code::MaybeUndefined]
        );
        // Narrowing by `defined`, in `?:` and in `if`, and through `and`.
        ok("state { bool up = false; } on_bar { up = defined(open) ? open > 1 : false; target(1); }");
        ok("state { bool up = false; } on_bar { if (defined(open)) { up = open > 1; } target(1); }");
        ok(
            "series { real o = open; } state { bool up = false; } on_bar { if (defined(o) and close > 1) { up = o > 1; } target(1); }",
        );
        assert_eq!(
            errors("state { bool up = false; } on_bar { if (defined(open)) { } else { up = open > 1; } target(1); }"),
            [Code::MaybeUndefined]
        );
    }

    #[test]
    fn real_states_that_may_be_undefined() {
        // A real state assigned a maybe-undefined value is itself maybe undefined, which reaches an int
        // assignment through it: found by the fixed point.
        let ir = ok("state { real a = 0; real b = 0; } on_bar { b = a; a = open; target(b); }");
        assert!(ir.state[0].maybe && ir.state[1].maybe);
        assert_eq!(
            errors(
                "state { real a = 0; real b = 0; bool up = false; } on_bar { up = b > 1; b = a; a = open; target(1); }"
            ),
            [Code::MaybeUndefined]
        );
        assert!(!ok("state { real a = 0; } on_bar { a = close; target(a); }").state[0].maybe);
    }
}
