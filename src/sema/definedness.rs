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
                k @ (HKind::Node(_) | HKind::State(_) | HKind::Local(_) | HKind::UState(_) | HKind::UValue(_)) => {
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
