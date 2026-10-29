//! The C++ printer: one struct per program that passes `ATL_ASSERT_STRATEGY`, calling only into the runtime
//! (`atls/ops.hpp`) for rings and indicators.

use std::collections::HashMap;
use std::fmt::Write;

use crate::frontend::ast::{BinOp, Ty, UnOp};
use crate::ir::*;

use super::{Indicators, Options};

/// A ring: its member name, its input node, and the indicators reading it with the slots each needs beyond its
/// window.
type RingOwner = (String, usize, Vec<(usize, i64)>);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Repr {
    Plain,
    Checked,
}

#[derive(Clone, Debug)]
struct V {
    code: String,
    ty: Ty,
    repr: Repr,
}

/// The runtime's checked integer operation for `+`, `-` or `*`.
fn int_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        _ => "mul",
    }
}

/// A condition without the parentheses around the whole of it, for `if (...)`.
fn bare(code: &str) -> &str {
    let Some(inner) = code.strip_prefix('(').and_then(|c| c.strip_suffix(')')) else {
        return code;
    };
    let mut depth = 0i32;
    for ch in inner.chars() {
        depth += match ch {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if depth < 0 {
            return code;
        }
    }
    inner
}

fn checked(v: &V) -> String {
    match (v.ty, v.repr) {
        (Ty::Bool, Repr::Plain) => format!("({} ? 1.0 : 0.0)", v.code),
        (Ty::Int, _) => format!("static_cast<double>({})", v.code),
        _ => v.code.clone(),
    }
}

fn plain(v: &V) -> String {
    match (v.ty, v.repr) {
        (Ty::Bool, Repr::Checked) => format!("({} != 0)", v.code),
        _ => v.code.clone(),
    }
}

fn num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else if v.is_infinite() {
        if v > 0.0 {
            "std::numeric_limits<double>::infinity()".into()
        } else {
            "-std::numeric_limits<double>::infinity()".into()
        }
    } else {
        // Debug prints the shortest text that reads back as the same double: `0.05`, `1e300`.
        format!("{:?}", v)
    }
}

/// A whole number as a C++ integer literal: every `int` the language accepts (up to 9e15) prints as digits. A
/// literal past 32 bits is a `long` or `long long` in C++, and every integer operation takes 64-bit operands.
fn inum(v: f64) -> String {
    format!("{}", v as i64)
}

fn dnum(v: f64) -> String {
    let s = num(v);
    if s.chars().all(|c| c.is_ascii_digit() || c == '-') {
        format!("{}.0", s)
    } else {
        s
    }
}

pub struct Printer<'a> {
    ir: &'a Ir,
    o: &'a Options,
    out: String,
    node_repr: Vec<Repr>,
    local_repr: HashMap<usize, Repr>,
    in_on_bar: bool,
    tmp: usize,
    /// Prefix of a lane's members in a universe handler (`L.`), empty elsewhere.
    prefix: String,
    /// Emitting a universe lane's `update`: series are members, assigned rather than declared.
    lane: bool,
    /// The code of each cross-sectional value, by id, once computed.
    xs_code: HashMap<usize, String>,
    /// Members the handlers need (universe: one value per instrument), declared at the end.
    members: Vec<String>,
    /// Inside a function or indicator body: its locals, arguments and states.
    fn_locals: Option<&'a [Local]>,
    uargs: Vec<(String, Ty)>,
    ustates: Vec<(String, Ty)>,
    uvalues: Vec<String>,
    /// Every real and bool three-valued (function and indicator bodies, whatever the arguments).
    force_checked: bool,
    /// In an indicator body: the int arguments known nonzero.
    unonzero_args: Vec<usize>,
}

pub fn print(ir: &Ir, o: &Options) -> String {
    let mut p = Printer {
        ir,
        o,
        out: String::new(),
        node_repr: vec![],
        local_repr: HashMap::new(),
        in_on_bar: false,
        tmp: 0,
        prefix: String::new(),
        lane: false,
        xs_code: HashMap::new(),
        members: vec![],
        fn_locals: None,
        uargs: vec![],
        ustates: vec![],
        uvalues: vec![],
        force_checked: false,
        unonzero_args: vec![],
    };
    p.node_repr = ir
        .nodes
        .iter()
        .map(|n| {
            if n.ty == Ty::Int || n.class == Class::Always {
                Repr::Plain
            } else {
                Repr::Checked
            }
        })
        .collect();
    p.program();
    p.out
}

fn ring_need(ind: Ind, mode: Indicators, policy: Policy) -> Option<i64> {
    // Extra slots beyond the window the ring must hold, or None if the indicator keeps its own state.
    if policy == Policy::Skip {
        // Skip windows are running sums (incremental and exact) or summed again (naive).
        return match (ind, mode) {
            (Ind::Ema, _) => None,
            (Ind::Lag, _) => Some(1),
            (_, Indicators::Naive) => Some(0),
            _ => Some(1),
        };
    }
    if mode == Indicators::Exact {
        return match ind {
            Ind::Lag | Ind::Stdev | Ind::Zscore | Ind::StdevPop => Some(1),
            _ => None,
        };
    }
    let naive = mode == Indicators::Naive;
    match (ind, naive) {
        (Ind::Ema, _) => None,
        (Ind::TsMax | Ind::TsMin, false) => None,
        (Ind::TsMax | Ind::TsMin, true) => Some(0),
        (Ind::Sma | Ind::TsSum, false) => Some(1),
        (Ind::Sma | Ind::TsSum, true) => Some(0),
        (Ind::Lag, _) => Some(1),
        (Ind::Rsi, false) => Some(2),
        (Ind::Rsi, true) => Some(1),
        (Ind::Stdev | Ind::Zscore | Ind::StdevPop, false) => Some(1),
        (Ind::Stdev | Ind::Zscore | Ind::StdevPop, true) => Some(0),
    }
}

/// The policy of an indicator node.
fn policy_of(kind: &NodeKind) -> Policy {
    match kind {
        NodeKind::Ind(_, _, _, m) => m.policy,
        _ => Policy::Restart,
    }
}

impl<'a> Printer<'a> {
    fn param_ref(&self, i: usize) -> String {
        format!("params_.{}_", self.ir.params[i].name)
    }

    fn cty(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => "std::int64_t",
            Ty::Real => "double",
            Ty::Bool => "bool",
        }
    }

    /// The type of a constant expression: `int` unless a `real` literal, a `real` param or `/` is in it.
    fn cexpr_ty(&self, c: &CExpr) -> Ty {
        match c {
            CExpr::Num(_) | CExpr::Max(_) => Ty::Int,
            CExpr::Real(_) | CExpr::Bin(BinOp::Div, _, _) => Ty::Real,
            CExpr::Param(i) => self.ir.params[*i].ty,
            CExpr::Neg(a) => self.cexpr_ty(a),
            CExpr::Bin(_, a, b) => {
                if self.cexpr_ty(a) == Ty::Int && self.cexpr_ty(b) == Ty::Int {
                    Ty::Int
                } else {
                    Ty::Real
                }
            }
        }
    }

    /// A constant expression in C++: integer arithmetic checked for overflow for ints, doubles otherwise.
    fn cexpr(&self, c: &CExpr) -> String {
        match c {
            CExpr::Num(v) => inum(*v),
            CExpr::Real(v) => dnum(*v),
            CExpr::Param(i) => self.param_ref(*i),
            CExpr::Neg(a) if self.cexpr_ty(a) == Ty::Int => format!("atls::ops::neg_int({})", self.cexpr(a)),
            CExpr::Neg(a) => format!("(-{})", self.cexpr(a)),
            CExpr::Bin(BinOp::Div, a, b) => format!("(static_cast<double>({}) / {})", self.cexpr(a), self.cexpr(b)),
            CExpr::Bin(op, a, b) if self.cexpr_ty(c) == Ty::Int => {
                format!("atls::ops::{}_int({}, {})", int_op(*op), self.cexpr(a), self.cexpr(b))
            }
            CExpr::Bin(op, a, b) => format!("({} {} {})", self.cexpr(a), op.text(), self.cexpr(b)),
            CExpr::Max(v) => {
                let parts: Vec<String> = v
                    .iter()
                    .map(|x| format!("static_cast<std::int64_t>({})", self.cexpr(x)))
                    .collect();
                format!("std::max<std::int64_t>({{{}}})", parts.join(", "))
            }
        }
    }

    fn size_expr(&self, c: &CExpr) -> String {
        format!("static_cast<std::size_t>({})", self.cexpr(c))
    }

    fn live_inds(&self) -> Vec<usize> {
        (0..self.ir.nodes.len())
            .filter(|&i| self.ir.live[i] && matches!(self.ir.nodes[i].kind, NodeKind::Ind(..)))
            .collect()
    }

    /// Ring owners: (ring name, input node, [(indicator node, extra)]). Shared: one per input and policy
    /// (`ring3_` for restart, `ring3s_` for skip, `ring3p_` for poison).
    fn rings(&self) -> Vec<RingOwner> {
        let mode = self.o.indicators;
        let mut out: Vec<RingOwner> = vec![];
        for i in self.live_inds() {
            if let NodeKind::Ind(ind, input, _, miss) = &self.ir.nodes[i].kind {
                if let Some(extra) = ring_need(*ind, mode, miss.policy) {
                    if self.o.share_rings {
                        let name = Self::ring_name(*input, miss.policy);
                        if let Some(r) = out.iter_mut().find(|r| r.0 == name) {
                            r.2.push((i, extra));
                            continue;
                        }
                        out.push((name, *input, vec![(i, extra)]));
                    } else {
                        out.push((format!("ring{}_", i), *input, vec![(i, extra)]));
                    }
                }
            }
        }
        out
    }

    fn ring_name(input: usize, policy: Policy) -> String {
        match policy {
            Policy::Restart => format!("ring{}_", input),
            Policy::Skip => format!("ring{}s_", input),
            Policy::Poison => format!("ring{}p_", input),
        }
    }

    fn ring_capacity(&self, members: &[(usize, i64)]) -> CExpr {
        let mut terms = vec![];
        for (i, extra) in members {
            if let NodeKind::Ind(_, _, w, _) = &self.ir.nodes[*i].kind {
                let t = if *extra == 0 {
                    w.clone()
                } else {
                    match w {
                        CExpr::Num(v) => CExpr::Num(v + *extra as f64),
                        _ => CExpr::Bin(BinOp::Add, Box::new(w.clone()), Box::new(CExpr::Num(*extra as f64))),
                    }
                };
                if !terms.contains(&t) {
                    terms.push(t);
                }
            }
        }
        if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            CExpr::Max(terms)
        }
    }

    fn ind_type(&self, i: usize) -> String {
        let NodeKind::Ind(ind, _, _, miss) = &self.ir.nodes[i].kind else {
            unreachable!()
        };
        let exact = self.o.indicators == Indicators::Exact;
        if miss.policy == Policy::Skip {
            let class = match ind {
                Ind::Sma => "SmaSkip",
                Ind::TsSum => "TsSumSkip",
                Ind::Stdev => "StdevSkip",
                Ind::StdevPop => "StdevPopSkip",
                Ind::Zscore => "ZscoreSkip",
                Ind::Lag => "Lag",
                Ind::Ema => "Ema",
                _ => unreachable!("refused by the checker"),
            };
            return format!("atls::ops::{}", class);
        }
        let class = match ind {
            Ind::Sma if exact => "SmaExact",
            Ind::TsSum if exact => "TsSumExact",
            Ind::Rsi if exact => "RsiExact",
            Ind::Sma => "Sma",
            Ind::Ema => "Ema",
            Ind::Rsi => "Rsi",
            Ind::Lag => "Lag",
            Ind::TsSum => "TsSum",
            Ind::TsMax => "TsMax",
            Ind::TsMin => "TsMin",
            Ind::Stdev => "Stdev",
            Ind::Zscore => "Zscore",
            Ind::StdevPop => "StdevPop",
        };
        format!("atls::ops::{}", class)
    }

    fn node_name(&self, i: usize) -> String {
        if let Some((name, _)) = self.ir.series.iter().find(|s| s.1 == i) {
            return format!("s_{}", name);
        }
        match &self.ir.nodes[i].kind {
            NodeKind::Field(k, f) => match &self.ir.mode {
                Mode::Named(names) => format!("f_{}_{}", names[*k], f.name()),
                _ => format!("f_{}", f.name()),
            },
            _ => format!("n{}", i),
        }
    }

    fn kept_name(&self, i: usize) -> String {
        format!("kept_{}_", self.node_name(i))
    }

    /// A node's value, read in the current handler.
    fn node_v(&self, i: usize) -> V {
        let n = &self.ir.nodes[i];
        match &n.kind {
            NodeKind::Num(v) => V {
                code: if n.ty == Ty::Int { inum(*v) } else { dnum(*v) },
                ty: n.ty,
                repr: Repr::Plain,
            },
            NodeKind::Bool(b) => V {
                code: b.to_string(),
                ty: Ty::Bool,
                repr: Repr::Plain,
            },
            NodeKind::Param(p) => V {
                code: self.param_ref(*p),
                ty: n.ty,
                repr: Repr::Plain,
            },
            _ => V {
                code: if self.in_on_bar {
                    format!("{}{}", self.prefix, self.node_name(i))
                } else {
                    self.kept_name(i)
                },
                ty: n.ty,
                repr: self.node_repr[i],
            },
        }
    }

    fn local(&self, i: usize) -> &Local {
        match self.fn_locals {
            Some(l) => &l[i],
            None => &self.ir.locals[i],
        }
    }

    fn result_repr(&self, ty: Ty, maybe: bool) -> Repr {
        if self.force_checked && ty != Ty::Int {
            return Repr::Checked;
        }
        if ty == Ty::Int || !maybe {
            Repr::Plain
        } else {
            Repr::Checked
        }
    }

    fn unary(&self, op: UnOp, a: &V, ty: Ty, repr: Repr) -> V {
        let code = match (op, repr) {
            (UnOp::Neg, Repr::Plain) if ty == Ty::Int => format!("atls::ops::neg_int({})", plain(a)),
            (UnOp::Neg, Repr::Plain) => format!("(-{})", plain(a)),
            (UnOp::Neg, Repr::Checked) => format!("(-{})", checked(a)),
            (UnOp::Not, Repr::Plain) => format!("(!{})", plain(a)),
            (UnOp::Not, Repr::Checked) => format!("atls::ops::not3({})", checked(a)),
        };
        V { code, ty, repr }
    }

    fn binary(&self, op: BinOp, a: &V, b: &V, ty: Ty, repr: Repr) -> V {
        let code = match repr {
            Repr::Plain => match op {
                BinOp::And => format!("({} && {})", plain(a), plain(b)),
                BinOp::Or => format!("({} || {})", plain(a), plain(b)),
                BinOp::Div => format!("(static_cast<double>({}) / {})", plain(a), plain(b)),
                BinOp::Add | BinOp::Sub | BinOp::Mul if ty == Ty::Int => {
                    format!("atls::ops::{}_int({}, {})", int_op(op), plain(a), plain(b))
                }
                _ => format!("({} {} {})", plain(a), op.text(), plain(b)),
            },
            Repr::Checked => {
                let (x, y) = (checked(a), checked(b));
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul => format!("({} {} {})", x, op.text(), y),
                    BinOp::Div => format!("atls::ops::div({}, {})", x, y),
                    BinOp::Lt => format!("atls::ops::lt({}, {})", x, y),
                    BinOp::Le => format!("atls::ops::le({}, {})", x, y),
                    BinOp::Gt => format!("atls::ops::gt({}, {})", x, y),
                    BinOp::Ge => format!("atls::ops::ge({}, {})", x, y),
                    BinOp::Eq => format!("atls::ops::eq({}, {})", x, y),
                    BinOp::Ne => format!("atls::ops::ne({}, {})", x, y),
                    BinOp::And => format!("atls::ops::and3({}, {})", x, y),
                    BinOp::Or => format!("atls::ops::or3({}, {})", x, y),
                }
            }
        };
        V { code, ty, repr }
    }

    fn ternary(&self, c: &V, a: &V, b: &V, ty: Ty, repr: Repr) -> V {
        let code = match repr {
            Repr::Plain => {
                let cast = if ty == Ty::Real { "static_cast<double>" } else { "" };
                format!("({} ? {}({}) : {}({}))", plain(c), cast, plain(a), cast, plain(b))
            }
            Repr::Checked => format!("atls::ops::pick({}, {}, {})", checked(c), checked(a), checked(b)),
        };
        V { code, ty, repr }
    }

    fn call(&self, b: Builtin, args: &[V], ty: Ty, repr: Repr) -> V {
        match b {
            Builtin::Defined => V {
                code: format!("(!std::isnan({}))", checked(&args[0])),
                ty: Ty::Bool,
                repr: Repr::Plain,
            },
            Builtin::Abs => {
                let code = match (repr, ty) {
                    (Repr::Plain, Ty::Int) => format!("atls::ops::abs_int({})", plain(&args[0])),
                    (Repr::Plain, _) => format!("std::fabs({})", plain(&args[0])),
                    (Repr::Checked, _) => format!("std::fabs({})", checked(&args[0])),
                };
                V { code, ty, repr }
            }
            Builtin::Sign | Builtin::Log | Builtin::Sqrt | Builtin::Round | Builtin::Floor => {
                let f = match b {
                    Builtin::Sign => "sign",
                    Builtin::Log => "log",
                    Builtin::Sqrt => "sqrt",
                    Builtin::Round => "round",
                    _ => "floor",
                };
                V {
                    code: format!("atls::ops::{}_of({})", f, checked(&args[0])),
                    ty,
                    repr,
                }
            }
            Builtin::Min | Builtin::Max => {
                let f = if b == Builtin::Min { "min" } else { "max" };
                let code = match (repr, ty) {
                    (Repr::Plain, Ty::Int) => {
                        format!("std::{}<std::int64_t>({}, {})", f, plain(&args[0]), plain(&args[1]))
                    }
                    (Repr::Plain, _) => format!("std::{}<double>({}, {})", f, plain(&args[0]), plain(&args[1])),
                    (Repr::Checked, _) => format!("atls::ops::{}3({}, {})", f, checked(&args[0]), checked(&args[1])),
                };
                V { code, ty, repr }
            }
        }
    }

    fn series_node_code(&self, i: usize) -> Option<String> {
        let n = &self.ir.nodes[i];
        let repr = self.node_repr[i];
        let v = match &n.kind {
            NodeKind::Unary(op, a) => self.unary(*op, &self.node_v(*a), n.ty, repr),
            NodeKind::Binary(op, a, b) => self.binary(*op, &self.node_v(*a), &self.node_v(*b), n.ty, repr),
            NodeKind::Ternary(c, a, b) => {
                self.ternary(&self.node_v(*c), &self.node_v(*a), &self.node_v(*b), n.ty, repr)
            }
            NodeKind::Call(b, args) => {
                let vs: Vec<V> = args.iter().map(|a| self.node_v(*a)).collect();
                self.call(*b, &vs, n.ty, repr)
            }
            NodeKind::UCall(f, args) => {
                let vs: Vec<V> = args.iter().map(|a| self.node_v(*a)).collect();
                self.user_call(*f, &vs)
            }
            _ => return None,
        };
        Some(if v.repr == repr {
            v.code
        } else if repr == Repr::Checked {
            checked(&v)
        } else {
            plain(&v)
        })
    }

    /// Inside a user body: an expression built from whole numbers only (ints are never undefined), so it
    /// needs no three-valued operators even where reals are all checked.
    fn whole_only(e: &HExpr) -> bool {
        match &e.kind {
            HKind::Num(_) | HKind::Bool(_) => true,
            HKind::Unary(_, a) => Self::whole_only(a),
            HKind::Binary(op, a, b) => op != &BinOp::Div && Self::whole_only(a) && Self::whole_only(b),
            HKind::Ternary(c, a, b) => Self::whole_only(c) && Self::whole_only(a) && Self::whole_only(b),
            _ => e.ty == Ty::Int,
        }
    }

    /// How an expression's value is held: plain, or three-valued in a double.
    fn repr_of(&self, e: &HExpr) -> Repr {
        if self.force_checked && !e.maybe && Self::whole_only(e) {
            Repr::Plain
        } else {
            self.result_repr(e.ty, e.maybe)
        }
    }

    /// Whether evaluating `e` can throw: it has integer arithmetic (overflow) or calls a function (whose body may).
    /// Where it cannot, evaluating an unused arm or side changes nothing, so the eager form stays.
    fn may_throw(e: &HExpr) -> bool {
        match &e.kind {
            HKind::Binary(BinOp::Add | BinOp::Sub | BinOp::Mul, _, _) | HKind::Unary(UnOp::Neg, _)
                if e.ty == Ty::Int =>
            {
                true
            }
            HKind::Call(Builtin::Abs, _) if e.ty == Ty::Int => true,
            HKind::UserCall(..) => true,
            HKind::Unary(_, a) | HKind::Xs(_, a, _) => Self::may_throw(a),
            HKind::Binary(_, a, b) => Self::may_throw(a) || Self::may_throw(b),
            HKind::Ternary(c, a, b) => Self::may_throw(c) || Self::may_throw(a) || Self::may_throw(b),
            HKind::Call(_, args) => args.iter().any(Self::may_throw),
            _ => false,
        }
    }

    fn hexpr(&mut self, e: &HExpr) -> V {
        let repr = self.repr_of(e);
        match &e.kind {
            HKind::Num(v) => V {
                code: if e.ty == Ty::Int { inum(*v) } else { dnum(*v) },
                ty: e.ty,
                repr: Repr::Plain,
            },
            HKind::Bool(b) => V {
                code: b.to_string(),
                ty: Ty::Bool,
                repr: Repr::Plain,
            },
            HKind::Param(i) => V {
                code: self.param_ref(*i),
                ty: e.ty,
                repr: Repr::Plain,
            },
            HKind::Node(i) => self.node_v(*i),
            HKind::State(i) => V {
                code: format!("{}st_{}_", self.prefix, self.ir.state[*i].name),
                ty: e.ty,
                repr: if self.ir.state[*i].ty == Ty::Real {
                    Repr::Checked
                } else {
                    Repr::Plain
                },
            },
            HKind::Local(i) => V {
                code: if self.local(*i).top {
                    format!("lv_{}_[ln]", self.local(*i).name)
                } else {
                    format!("l_{}", self.local(*i).name)
                },
                ty: e.ty,
                repr: *self.local_repr.get(i).unwrap_or(&Repr::Plain),
            },
            HKind::Held => V {
                code: if self.ir.mode == Mode::Universe {
                    "L.held_"
                } else {
                    "held"
                }
                .to_string(),
                ty: Ty::Real,
                repr: Repr::Plain,
            },
            HKind::HeldOf(k) => V {
                code: format!("held_{}", self.inst_name(*k)),
                ty: Ty::Real,
                repr: Repr::Plain,
            },
            HKind::Bars => V {
                code: "static_cast<std::int64_t>(L.bars_)".into(),
                ty: Ty::Int,
                repr: Repr::Plain,
            },
            HKind::Periods => V {
                code: "static_cast<std::int64_t>(periods_)".into(),
                ty: Ty::Int,
                repr: Repr::Plain,
            },
            HKind::OpenNow => V {
                code: format!("{}open_now_", self.prefix),
                ty: Ty::Real,
                repr: Repr::Plain,
            },
            HKind::Xs(_, _, id) => V {
                code: self
                    .xs_code
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("/* xs{} not computed */", id)),
                ty: Ty::Real,
                repr: if e.maybe { Repr::Checked } else { Repr::Plain },
            },
            HKind::Keep => V {
                code: "atls::ops::NaN".into(),
                ty: Ty::Real,
                repr: Repr::Checked,
            },
            HKind::FillPrice => V {
                code: "fill_price".into(),
                ty: Ty::Real,
                repr: Repr::Plain,
            },
            HKind::FillQuantity => V {
                code: "fill_quantity".into(),
                ty: Ty::Real,
                repr: Repr::Plain,
            },
            HKind::FillBuy => V {
                code: "fill_buy".into(),
                ty: Ty::Bool,
                repr: Repr::Plain,
            },
            HKind::Unary(op, a) => {
                let a = self.hexpr(a);
                self.unary(*op, &a, e.ty, repr)
            }
            HKind::Binary(BinOp::Div, a, b) if self.force_checked && self.unonzero(b) => {
                // A divisor known nonzero (a number, or a window length): no check.
                let (a, b) = (self.hexpr(a), self.hexpr(b));
                V {
                    code: format!("({} / {})", checked(&a), checked(&b)),
                    ty: Ty::Real,
                    repr: Repr::Checked,
                }
            }
            HKind::Binary(op @ (BinOp::And | BinOp::Or), a, b) if repr == Repr::Checked && Self::may_throw(b) => {
                // The right side runs only when the left does not decide the result (a defined false for `and`, a
                // defined true for `or`).
                let (x, y) = (self.hexpr(a), self.hexpr(b));
                let (y, decided) = (checked(&y), if *op == BinOp::And { "0.0" } else { "1.0" });
                let code = if x.repr == Repr::Plain {
                    if *op == BinOp::And {
                        format!("({} ? {} : 0.0)", plain(&x), y)
                    } else {
                        format!("({} ? 1.0 : {})", plain(&x), y)
                    }
                } else {
                    let (test, f) = if *op == BinOp::And {
                        ("left_ == 0", "and3")
                    } else {
                        ("left_ != 0", "or3")
                    };
                    format!(
                        "[&]() -> double {{ ATLS_NO_FMA const double left_ = {}; return !std::isnan(left_) && {test} ? {decided} : atls::ops::{f}(left_, {y}); }}()",
                        checked(&x)
                    )
                };
                V { code, ty: e.ty, repr }
            }
            HKind::Binary(op, a, b) => {
                let (a, b) = (self.hexpr(a), self.hexpr(b));
                self.binary(*op, &a, &b, e.ty, repr)
            }
            HKind::Ternary(c, a, b)
                if repr == Repr::Checked
                    && self.repr_of(c) == Repr::Checked
                    && b.kind == HKind::Keep
                    && matches!(&c.kind, HKind::Binary(op, _, _) if op.is_comparison() && *op != BinOp::Ne) =>
            {
                // `x < y ? v : keep` on a condition that may be undefined: an undefined comparison and a false one both
                // give `keep`, so the comparison needs no three-valued form (NaN compares false). Not `!=`: NaN != y
                // is true. Not a condition that is never undefined (between ints, say): it is compared as it is
                // typed, below.
                let HKind::Binary(op, x, y) = &c.kind else {
                    unreachable!()
                };
                let (x, y, a) = (self.hexpr(x), self.hexpr(y), self.hexpr(a));
                V {
                    code: format!(
                        "(({} {} {}) ? {} : atls::ops::NaN)",
                        checked(&x),
                        op.text(),
                        checked(&y),
                        checked(&a)
                    ),
                    ty: e.ty,
                    repr: Repr::Checked,
                }
            }
            HKind::Ternary(c, a, b) if repr == Repr::Checked && (Self::may_throw(a) || Self::may_throw(b)) => {
                // Only the arm the condition selects runs, and neither when it is undefined.
                let (c, a, b) = (self.hexpr(c), self.hexpr(a), self.hexpr(b));
                let code = if c.repr == Repr::Plain {
                    format!("({} ? {} : {})", plain(&c), checked(&a), checked(&b))
                } else {
                    format!(
                        "[&]() -> double {{ ATLS_NO_FMA const double cond_ = {}; return std::isnan(cond_) ? atls::ops::NaN : (cond_ != 0 ? {} : {}); }}()",
                        c.code,
                        checked(&a),
                        checked(&b)
                    )
                };
                V { code, ty: e.ty, repr }
            }
            HKind::Ternary(c, a, b) => {
                let (c, a, b) = (self.hexpr(c), self.hexpr(a), self.hexpr(b));
                self.ternary(&c, &a, &b, e.ty, repr)
            }
            HKind::Call(b, args) => {
                let vs: Vec<V> = args.iter().map(|a| self.hexpr(a)).collect();
                self.call(*b, &vs, e.ty, repr)
            }
            HKind::UserCall(f, args) => {
                let vs: Vec<V> = args.iter().map(|a| self.hexpr(a)).collect();
                self.user_call(*f, &vs)
            }
            HKind::UArg(i) => {
                let (name, ty) = self.uargs[*i].clone();
                V {
                    code: format!("a_{}", name),
                    ty,
                    repr: if ty == Ty::Int { Repr::Plain } else { Repr::Checked },
                }
            }
            HKind::UState(i) => {
                let (name, ty) = self.ustates[*i].clone();
                V {
                    code: format!("s_{}", name),
                    ty,
                    repr: if ty == Ty::Int { Repr::Plain } else { Repr::Checked },
                }
            }
            HKind::UValue(i) => V {
                code: format!("v_{}", self.uvalues[*i]),
                ty: Ty::Real,
                repr: Repr::Checked,
            },
            HKind::UCount => V {
                code: "static_cast<std::int64_t>(count_)".into(),
                ty: Ty::Int,
                repr: Repr::Plain,
            },
        }
    }

    /// A call of a function of `functions {}`: reals and bools passed as doubles (three-valued).
    fn user_call(&self, f: usize, args: &[V]) -> V {
        let def = &self.ir.functions[f];
        let parts: Vec<String> = args
            .iter()
            .zip(&def.args)
            .map(|(v, (_, ty))| if *ty == Ty::Int { plain(v) } else { checked(v) })
            .collect();
        let code = format!("f_{}({})", def.name, parts.join(", "));
        if def.ret == Ty::Int {
            V {
                code,
                ty: Ty::Int,
                repr: Repr::Plain,
            }
        } else {
            V {
                code,
                ty: def.ret,
                repr: Repr::Checked,
            }
        }
    }

    fn line(&mut self, indent: usize, text: &str) {
        let _ = writeln!(self.out, "{}{}", "  ".repeat(indent), text);
    }

    fn stmts(&mut self, stmts: &[HStmt], indent: usize) {
        for s in stmts {
            match s {
                HStmt::Local { id, value } => {
                    let v = self.hexpr(value);
                    let l = self.local(*id).clone();
                    let (ty, code) = match (l.ty, v.repr) {
                        (Ty::Bool, Repr::Checked) => ("double", v.code.clone()),
                        (Ty::Real, _) => ("double", checked(&v)),
                        (t, _) => (Self::cty(t), plain(&v)),
                    };
                    self.local_repr.insert(
                        *id,
                        if l.ty == Ty::Bool {
                            v.repr
                        } else if l.ty == Ty::Real {
                            Repr::Checked
                        } else {
                            Repr::Plain
                        },
                    );
                    let name = l.name.clone();
                    if l.top {
                        self.members.push(format!("std::vector<{}> lv_{}_;", ty, name));
                        self.line(indent, &format!("lv_{}_[ln] = {};", name, code));
                    } else {
                        self.line(indent, &format!("[[maybe_unused]] const {} l_{} = {};", ty, name, code));
                    }
                }
                HStmt::Assign { state, value } => {
                    let v = self.hexpr(value);
                    let st = &self.ir.state[*state];
                    let code = if st.ty == Ty::Real { checked(&v) } else { plain(&v) };
                    let name = st.name.clone();
                    let prefix = self.prefix.clone();
                    self.line(indent, &format!("{}st_{}_ = {};", prefix, name, code));
                }
                HStmt::Target(k, value) => {
                    let v = self.hexpr(value);
                    let code = if v.ty == Ty::Int {
                        format!("static_cast<std::int64_t>({})", plain(&v))
                    } else {
                        format!("static_cast<double>({})", checked(&v))
                    };
                    let who = match &self.ir.mode {
                        Mode::Single => "target".to_string(),
                        Mode::Named(_) => format!("target_{}", self.inst_name(*k)),
                        Mode::Universe => "tg_[ln]".to_string(),
                    };
                    self.line(indent, &format!("{}.state({});", who, code));
                }
                HStmt::If { cond, then, els } => {
                    let c = self.hexpr(cond);
                    if c.repr == Repr::Plain {
                        self.line(indent, &format!("if ({}) {{", bare(&c.code)));
                        self.stmts(then, indent + 1);
                        if !els.is_empty() {
                            self.line(indent, "} else {");
                            self.stmts(els, indent + 1);
                        }
                        self.line(indent, "}");
                    } else {
                        // An undefined condition takes neither branch.
                        self.tmp += 1;
                        let t = format!("c{}", self.tmp);
                        self.line(indent, &format!("const double {} = {};", t, c.code));
                        self.line(indent, &format!("if (!std::isnan({})) {{", t));
                        self.line(indent + 1, &format!("if ({} != 0) {{", t));
                        self.stmts(then, indent + 2);
                        if !els.is_empty() {
                            self.line(indent + 1, "} else {");
                            self.stmts(els, indent + 2);
                        }
                        self.line(indent + 1, "}");
                        self.line(indent, "}");
                    }
                }
                HStmt::Return(value) => {
                    let v = self.hexpr(value);
                    let code = if v.ty == Ty::Int { plain(&v) } else { checked(&v) };
                    self.line(indent, &format!("return {};", code));
                }
                HStmt::UAssign { state, value } => {
                    let v = self.hexpr(value);
                    let (name, ty) = self.ustates[*state].clone();
                    let code = if ty == Ty::Int { plain(&v) } else { checked(&v) };
                    self.line(indent, &format!("s_{} = {};", name, code));
                }
                HStmt::UFor { var, from, to, body } => {
                    let name = self.local(*var).name.clone();
                    self.local_repr.insert(*var, Repr::Plain);
                    let (f, t) = (self.hexpr(from), self.hexpr(to));
                    self.inclusive_loop(indent, &name, &plain(&f), &plain(&t), body);
                }
                HStmt::For { var, from, to, body } => {
                    let name = self.local(*var).name.clone();
                    self.local_repr.insert(*var, Repr::Plain);
                    let (f, t) = (self.cexpr(from), self.cexpr(to));
                    self.inclusive_loop(indent, &name, &f, &t, body);
                }
            }
        }
    }

    /// `for (name in from:to)`: the bounds evaluated once, and no increment past the last value (which may be the
    /// largest 64-bit integer).
    fn inclusive_loop(&mut self, indent: usize, name: &str, from: &str, to: &str, body: &[HStmt]) {
        self.line(
            indent,
            &format!("for (std::int64_t l_{name} = {from}, e_{name} = {to}; l_{name} <= e_{name}; ++l_{name}) {{"),
        );
        self.stmts(body, indent + 1);
        self.line(indent + 1, &format!("if (l_{name} == e_{name}) {{"));
        self.line(indent + 2, "break;");
        self.line(indent + 1, "}");
        self.line(indent, "}");
    }

    fn handler_reads_fill(stmts: &[HStmt]) -> bool {
        format!("{:?}", stmts).contains("Fill")
    }

    fn inst_name(&self, k: usize) -> String {
        match &self.ir.mode {
            Mode::Named(names) => names[k].clone(),
            _ => "main".into(),
        }
    }

    fn inst_count(&self) -> usize {
        match &self.ir.mode {
            Mode::Named(names) => names.len(),
            _ => 1,
        }
    }

    /// Live indicator-like objects that keep their own buffers: partial sums, pair fits, Kalman filters
    /// (one per distinct filter, at its first output).
    fn live_objs(&self) -> Vec<usize> {
        let mut seen: Vec<String> = vec![];
        let mut out = vec![];
        for i in 0..self.ir.nodes.len() {
            if !self.ir.live[i] {
                continue;
            }
            match &self.ir.nodes[i].kind {
                NodeKind::Partial(..) | NodeKind::Pair(..) => out.push(i),
                NodeKind::UInd { .. } => {
                    let key = self.uind_key(i);
                    if !seen.contains(&key) {
                        seen.push(key);
                        out.push(i);
                    }
                }
                NodeKind::Kalman(..) => {
                    let key = self.kalman_key(i);
                    if !seen.contains(&key) {
                        seen.push(key);
                        out.push(i);
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// What identifies one user indicator instance: everything but the value read.
    fn uind_key(&self, i: usize) -> String {
        match &self.ir.nodes[i].kind {
            NodeKind::UInd {
                def,
                inputs,
                consts,
                miss,
                ..
            } => format!("u {} {:?} {:?} {:?}", def, inputs, consts, miss),
            _ => String::new(),
        }
    }

    fn uind_owner(&self, i: usize) -> usize {
        let key = self.uind_key(i);
        (0..self.ir.nodes.len())
            .find(|&j| self.ir.live[j] && self.uind_key(j) == key)
            .unwrap_or(i)
    }

    fn uind_type(def: &IndIr, policy: Policy) -> String {
        format!("Ind_{}_{}", def.name, policy.name())
    }

    fn kalman_key(&self, i: usize) -> String {
        match &self.ir.nodes[i].kind {
            NodeKind::Kalman(a, b, d, v, _) => format!("{} {} {:?} {:?}", a, b, d, v),
            _ => String::new(),
        }
    }

    /// The member holding a Kalman node's filter: the first live node with the same filter.
    fn kalman_owner(&self, i: usize) -> usize {
        let key = self.kalman_key(i);
        (0..self.ir.nodes.len())
            .find(|&j| self.ir.live[j] && self.kalman_key(j) == key)
            .unwrap_or(i)
    }

    fn obj_type(&self, i: usize) -> String {
        let naive = self.o.indicators == Indicators::Naive;
        match &self.ir.nodes[i].kind {
            NodeKind::Partial(..) => "atls::ops::TsSumMin".into(),
            NodeKind::Pair(PairInd::OlsSlope, ..) if naive => "atls::ops::OlsLlt<true>".into(),
            NodeKind::Pair(PairInd::OlsIntercept, ..) if naive => "atls::ops::OlsLlt<false>".into(),
            NodeKind::Pair(PairInd::OlsSlope, ..) => "atls::ops::OlsSlope".into(),
            NodeKind::Pair(PairInd::OlsIntercept, ..) => "atls::ops::OlsIntercept".into(),
            NodeKind::Kalman(..) => "atls::ops::KalmanHedge".into(),
            NodeKind::UInd { def, miss, .. } => Self::uind_type(&self.ir.indicators[*def], miss.policy),
            _ => unreachable!(),
        }
    }

    /// The source of a bar field, by mode.
    fn field_code(&self, k: usize, f: Field) -> String {
        if self.lane && self.ir.period_clock {
            // A period without this instrument's bar is a row of undefined values.
            return if f == Field::Open {
                "bar != nullptr && bar->bar_.open_ ? atls::ops::to_double(*bar->bar_.open_) : atls::ops::NaN"
                    .to_string()
            } else {
                format!(
                    "bar != nullptr ? atls::ops::to_double(bar->bar_.{}_) : atls::ops::NaN",
                    f.name()
                )
            };
        }
        let bar = match &self.ir.mode {
            Mode::Named(_) => format!("bar_{}->bar_", self.inst_name(k)),
            _ => "bar.bar_".to_string(),
        };
        if f == Field::Open {
            format!("{b}.open_ ? atls::ops::to_double(*{b}.open_) : atls::ops::NaN", b = bar)
        } else {
            format!("atls::ops::to_double({}.{}_)", bar, f.name())
        }
    }

    /// A declaration of a value computed every bar: a local in `on_bar`, a member's assignment in a lane.
    fn decl(&mut self, indent: usize, ty: &str, name: &str, code: &str) {
        if self.lane {
            self.line(indent, &format!("{} = {};", name, code));
        } else {
            self.line(indent, &format!("const {} {} = {};", ty, name, code));
        }
    }

    /// Fields, series and indicator updates, in dependency order.
    fn series_lines(&mut self, indent: usize) {
        let ir = self.ir;
        let o = self.o;
        let rings = self.rings();
        let inds = self.live_inds();
        for i in 0..ir.nodes.len() {
            if !ir.live[i] {
                continue;
            }
            let n = &ir.nodes[i];
            match &n.kind {
                NodeKind::Field(k, f) => {
                    let code = self.field_code(*k, *f);
                    let name = self.node_name(i);
                    self.decl(indent, "double", &name, &code);
                }
                NodeKind::Num(_) | NodeKind::Bool(_) | NodeKind::Param(_) => {}
                NodeKind::Ind(_, input, _, _) => {
                    if !o.share_rings {
                        let own = format!("ring{}_", i);
                        self.ind_update(
                            &[i],
                            *input,
                            rings.iter().find(|r| r.0 == own).map(|r| r.0.clone()),
                            indent,
                        );
                    }
                }
                NodeKind::Partial(input, _, _) => {
                    let x = checked(&self.node_v(*input));
                    let check = ir.nodes[*input].class != Class::Always;
                    self.obj_update(indent, i, &[x], check, "update");
                }
                NodeKind::Pair(_, a, b, _) => {
                    let (x, y) = (checked(&self.node_v(*a)), checked(&self.node_v(*b)));
                    let check = ir.nodes[*a].class != Class::Always || ir.nodes[*b].class != Class::Always;
                    self.obj_update(indent, i, &[x, y], check, "update");
                }
                NodeKind::UInd {
                    inputs,
                    miss,
                    value,
                    def,
                    ..
                } => {
                    let owner = self.uind_owner(i);
                    if owner == i {
                        let xs: Vec<String> = inputs.iter().map(|&a| checked(&self.node_v(a))).collect();
                        let check = inputs.iter().any(|&a| ir.nodes[a].class != Class::Always);
                        let nan: Vec<String> = xs.iter().map(|x| format!("std::isnan({})", x)).collect();
                        let args = xs.join(", ");
                        match miss.policy {
                            Policy::Skip => self.line(indent, &format!("obj{}_.step_skip({});", i, args)),
                            Policy::Restart if check => {
                                self.line(indent, &format!("if ({}) {{", nan.join(" || ")));
                                self.line(indent + 1, &format!("obj{}_.reset();", i));
                                self.line(indent, "} else {");
                                self.line(indent + 1, &format!("obj{}_.step({});", i, args));
                                self.line(indent, "}");
                            }
                            Policy::Poison if check => {
                                let lbs: Vec<CExpr> = inputs.iter().map(|&a| ir.nodes[a].lookback.clone()).collect();
                                let lb = if lbs.len() == 1 {
                                    self.cexpr(&lbs[0])
                                } else {
                                    self.cexpr(&CExpr::Max(lbs))
                                };
                                self.line(indent, &format!("if (obj{}_.poisoned_ || {}) {{", i, nan.join(" || ")));
                                self.line(
                                    indent + 1,
                                    &format!(
                                        "if (!obj{i}_.poisoned_ && bars_ >= static_cast<std::size_t>({lb})) {{",
                                        i = i,
                                        lb = lb
                                    ),
                                );
                                self.line(indent + 2, &format!("obj{}_.poisoned_ = true;", i));
                                self.line(indent + 1, "}");
                                self.line(indent + 1, &format!("obj{}_.reset();", i));
                                self.line(indent, "} else {");
                                self.line(indent + 1, &format!("obj{}_.step({});", i, args));
                                self.line(indent, "}");
                            }
                            _ => self.line(indent, &format!("obj{}_.step({});", i, args)),
                        }
                    }
                    let name = self.node_name(i);
                    let code = format!("obj{}_.v_{}", owner, ir.indicators[*def].values[*value].name);
                    self.decl(indent, "double", &name, &code);
                }
                NodeKind::Kalman(a, b, _, _, out) => {
                    let owner = self.kalman_owner(i);
                    let name = self.node_name(i);
                    let read = match out {
                        KOut::Beta => "beta",
                        KOut::Error => "error",
                        KOut::Sd => "sd",
                    };
                    if owner == i {
                        let (x, y) = (checked(&self.node_v(*a)), checked(&self.node_v(*b)));
                        let check = ir.nodes[*a].class != Class::Always || ir.nodes[*b].class != Class::Always;
                        if check {
                            self.line(indent, &format!("if (std::isnan({x}) || std::isnan({y})) {{ obj{i}_.restart(); }} else {{ obj{i}_.update({x}, {y}); }}", x = x, y = y, i = i));
                        } else {
                            self.line(indent, &format!("obj{}_.update({}, {});", i, x, y));
                        }
                    }
                    let code = format!("obj{}_.{}()", owner, read);
                    self.decl(indent, "double", &name, &code);
                }
                _ => {
                    let code = self.series_node_code(i).unwrap();
                    let ty = if self.node_repr[i] == Repr::Plain {
                        Self::cty(n.ty)
                    } else {
                        "double"
                    };
                    let name = self.node_name(i);
                    self.decl(indent, ty, &name, &code);
                }
            }
            if o.share_rings {
                for policy in [Policy::Restart, Policy::Skip, Policy::Poison] {
                    let consumers: Vec<usize> = inds
                        .iter()
                        .copied()
                        .filter(|&k| matches!(&ir.nodes[k].kind, NodeKind::Ind(_, inp, _, m) if *inp == i && m.policy == policy))
                        .collect();
                    if !consumers.is_empty() {
                        let name = Self::ring_name(i, policy);
                        let ring = rings.iter().find(|r| r.0 == name).map(|r| r.0.clone());
                        self.ind_update(&consumers, i, ring, indent);
                    }
                }
            }
        }
    }

    /// An object indicator's update: restart on an undefined input, else take the inputs.
    fn obj_update(&mut self, indent: usize, i: usize, args: &[String], check: bool, f: &str) {
        let name = self.node_name(i);
        if !self.lane {
            self.line(indent, &format!("double {};", name));
        }
        let call = format!("{} = obj{}_.{}({});", name, i, f, args.join(", "));
        if check {
            let cond: Vec<String> = args.iter().map(|a| format!("std::isnan({})", a)).collect();
            self.line(indent, &format!("if ({}) {{", cond.join(" || ")));
            self.line(indent + 1, &format!("obj{}_.reset();", i));
            self.line(indent + 1, &format!("{} = atls::ops::NaN;", name));
            self.line(indent, "} else {");
            self.line(indent + 1, &call);
            self.line(indent, "}");
        } else {
            self.line(indent, &call);
        }
    }

    /// The warm-up: the longest lookback any live node needs, or the stated one if longer.
    fn warmup_code(&self) -> String {
        let ir = self.ir;
        let mut terms: Vec<CExpr> = vec![];
        let read_roots: Vec<usize> = (0..ir.nodes.len())
            .filter(|&i| {
                ir.live[i]
                    && !matches!(
                        ir.nodes[i].kind,
                        NodeKind::Num(_) | NodeKind::Bool(_) | NodeKind::Param(_)
                    )
            })
            .collect();
        for i in read_roots {
            match ir.nodes[i].lookback.clone() {
                CExpr::Max(v) => {
                    for x in v {
                        if !terms.contains(&x) {
                            terms.push(x);
                        }
                    }
                }
                x => {
                    if !terms.contains(&x) {
                        terms.push(x);
                    }
                }
            }
        }
        if let Some(w) = &ir.warmup {
            terms.push(w.clone());
        }
        terms.retain(|t| !matches!(t, CExpr::Num(v) if *v <= 1.0));
        match terms.len() {
            0 => "1".to_string(),
            1 => self.cexpr(&terms[0]),
            _ => self.cexpr(&CExpr::Max(terms)),
        }
    }

    /// Sizes and resets the rings and indicators (into a lane's members when `lane`).
    fn reset_lines(&mut self, indent: usize) {
        let ir = self.ir;
        let rings = self.rings();
        for &i in &self.live_inds() {
            if let NodeKind::Ind(ind, _, w, miss) = &ir.nodes[i].kind {
                let s = self.size_expr(w);
                self.line(indent, &format!("ind{}_.set({});", i, s));
                if miss.policy == Policy::Skip && !matches!(ind, Ind::Lag | Ind::Ema) {
                    let m = self.size_expr(&miss.min);
                    self.line(indent, &format!("ind{}_.set_min({});", i, m));
                }
                self.line(indent, &format!("ind{}_.reset();", i));
            }
        }
        for i in self.live_objs() {
            match &ir.nodes[i].kind {
                NodeKind::Partial(_, w, m) => {
                    let (w, m) = (self.size_expr(w), self.size_expr(m));
                    self.line(indent, &format!("obj{}_.set({}, {});", i, w, m));
                }
                NodeKind::Pair(_, _, _, w) => {
                    let w = self.size_expr(w);
                    self.line(indent, &format!("obj{}_.set({});", i, w));
                }
                NodeKind::Kalman(_, _, d, v, _) => {
                    let (d, v) = (self.cexpr(d), self.cexpr(v));
                    self.line(
                        indent,
                        &format!("obj{}_.set(static_cast<double>({}), static_cast<double>({}));", i, d, v),
                    );
                }
                NodeKind::UInd { consts, .. } => {
                    let cs: Vec<String> = consts
                        .iter()
                        .map(|c| format!("static_cast<std::int64_t>({})", self.cexpr(c)))
                        .collect();
                    self.line(indent, &format!("obj{}_.set({});", i, cs.join(", ")));
                    self.line(indent, &format!("obj{}_.poisoned_ = false;", i));
                }
                _ => {}
            }
            if !matches!(ir.nodes[i].kind, NodeKind::Kalman(..)) {
                self.line(indent, &format!("obj{}_.reset();", i));
            } else {
                self.line(indent, &format!("obj{}_.restart();", i));
            }
        }
        for (r, _, members) in &rings {
            let cap = self.size_expr(&self.ring_capacity(members));
            self.line(indent, &format!("{}.reset({});", r, cap));
            self.line(indent, &format!("{}.clear();", r));
        }
        for p in self.poison_flags() {
            self.line(indent, &format!("{} = false;", p));
        }
        for st in &ir.state {
            let v = self.cexpr(&st.init);
            let v = match st.ty {
                Ty::Bool => format!("({} != 0)", v),
                Ty::Real => format!("static_cast<double>({})", v),
                Ty::Int => v,
            };
            self.line(indent, &format!("st_{}_ = {};", st.name, v));
        }
    }

    /// Ring, indicator and state members.
    fn object_members(&mut self, indent: usize) {
        let ir = self.ir;
        for (r, _, _) in &self.rings() {
            self.line(indent, &format!("atls::ops::Ring {};", r));
        }
        for p in self.poison_flags() {
            self.line(indent, &format!("bool {}{{false}};", p));
        }
        for &i in &self.live_inds() {
            let ty = self.ind_type(i);
            let NodeKind::Ind(ind, input, w, miss) = &ir.nodes[i].kind else {
                unreachable!()
            };
            let policy = if miss.policy == Policy::Restart {
                String::new()
            } else {
                format!(", missing = {}", miss.policy.name())
            };
            let comment = format!(
                "{}({}, {}{})",
                ind.name(),
                self.node_name(*input),
                self.cexpr(w),
                policy
            );
            self.line(indent, &format!("{} ind{}_;  // {}", ty, i, comment));
        }
        for i in self.live_objs() {
            let ty = self.obj_type(i);
            self.line(indent, &format!("{} obj{}_;", ty, i));
        }
        for st in &ir.state {
            self.line(indent, &format!("{} st_{}_{{}};", Self::cty(st.ty), st.name));
        }
    }

    /// One flag per input with poisoned indicators: set by the first missing value after the input's warm-up.
    fn poison_flags(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for i in self.live_inds() {
            if let NodeKind::Ind(_, input, _, m) = &self.ir.nodes[i].kind {
                if m.policy == Policy::Poison {
                    let name = if self.o.share_rings {
                        format!("poisoned{}_", input)
                    } else {
                        format!("poisoned{}_", i)
                    };
                    if !out.contains(&name) {
                        out.push(name);
                    }
                }
            }
        }
        out
    }

    fn header(&mut self) {
        let o = self.o;
        self.line(
            0,
            &format!(
                "// Generated by atlc {} from {}. Do not edit.",
                crate::VERSION,
                o.source_name
            ),
        );
        self.line(
            0,
            &format!(
                "// --indicators {} --share {} --history {}",
                o.indicators.name(),
                if o.share_rings { "on" } else { "off" },
                if o.drop_history { "drop" } else { "keep" }
            ),
        );
        self.line(0, "#pragma once");
        self.line(0, "");
        self.line(0, "#include <atls/ops.hpp>");
        self.line(0, "");
        let id = super::runtime_id_text();
        self.line(0, &format!("#if !defined(ATLS_RUNTIME_ID) || ATLS_RUNTIME_ID != {id}"));
        self.line(0, &format!(
            "#error \"{} was generated by atlc {} for the atls runtime {id}: write the runtime out with `atlc build --emit-runtime DIR` from the same atlc\"",
            o.source_name,
            crate::VERSION
        ));
        self.line(0, "#endif");
        self.line(0, "");
        self.line(0, "namespace atls {");
        self.line(0, "");
        self.line(0, "namespace atl = algotradelib;");
        self.line(0, "");
        self.line(0, &format!("/** The strategy in `{}`, compiled. */", o.source_name));
        self.line(0, &format!("struct {} {{", o.name));
    }

    fn params_struct(&mut self) {
        let ir = self.ir;
        self.line(
            1,
            "/** The parameters: constructor fields, so one binary runs every point of a grid. */",
        );
        self.line(1, "struct Params {");
        for p in &ir.params {
            let val = match p.ty {
                Ty::Int => inum(p.default),
                Ty::Real => dnum(p.default),
                Ty::Bool => (p.default != 0.0).to_string(),
            };
            self.line(2, &format!("{} {}_{{{}}};", Self::cty(p.ty), p.name, val));
        }
        self.line(1, "};");
        self.line(0, "");
        if self.o.drop_history {
            self.line(
                1,
                "/** Series keep their own lookback, so the runner keeps no bar history. */",
            );
            self.line(1, "static constexpr bool reads_bar_history = false;");
            self.line(0, "");
        }
        let mode = match &ir.mode {
            Mode::Single => "single",
            Mode::Named(_) => "named",
            Mode::Universe => "universe",
        };
        self.line(
            1,
            "/** How the program sees instruments, for a runner: single, named or universe. */",
        );
        self.line(1, &format!("static constexpr const char* atls_mode = \"{}\";", mode));
        self.line(
            1,
            "/** Whether the program reads each day as an open and a close (`on_open`, `on_close`). */",
        );
        self.line(1, &format!("static constexpr bool atls_legs = {};", ir.legs));
        let names: Vec<String> = match &ir.mode {
            Mode::Named(n) => n.iter().map(|x| format!("\"{}\"", x)).collect(),
            _ => vec![],
        };
        self.line(1, "/** The names in `instruments {}`, in order; empty otherwise. */");
        self.line(
            1,
            &format!(
                "static std::vector<std::string> atls_instruments() {{ return {{{}}}; }}",
                names.join(", ")
            ),
        );
        self.line(1, "/** The parameter names, in order. */");
        let pn: Vec<String> = ir.params.iter().map(|p| format!("\"{}\"", p.name)).collect();
        self.line(
            1,
            &format!(
                "static std::vector<std::string> atls_params() {{ return {{{}}}; }}",
                pn.join(", ")
            ),
        );
        self.line(0, "");
        self.line(
            1,
            "/** Bars before the handlers run: the stated warm-up or what the series need, whichever is longer. */",
        );
        self.line(1, "std::size_t warmup_bars() const { return warmup_; }");
        self.line(0, "");
    }

    fn set_method(&mut self) {
        let ir = self.ir;
        self.line(
            1,
            "/** Sets one parameter by name and checks the bounds again; an unknown name throws. */",
        );
        // A program without parameters only refuses the name.
        let value = if ir.params.is_empty() {
            "[[maybe_unused]] double value"
        } else {
            "double value"
        };
        self.line(1, &format!("void set(std::string_view name, {value}) {{"));
        let mut first = true;
        for p in &ir.params {
            let conv = match p.ty {
                Ty::Int => "atls::ops::whole(name, value)".to_string(),
                Ty::Real => "value".to_string(),
                Ty::Bool => "value != 0".to_string(),
            };
            self.line(
                2,
                &format!("{}if (name == \"{}\") {{", if first { "" } else { "} else " }, p.name),
            );
            self.line(3, &format!("params_.{}_ = {};", p.name, conv));
            first = false;
        }
        if first {
            self.line(
                2,
                "throw std::invalid_argument(\"the program has no parameter '\" + std::string(name) + \"'\");",
            );
        } else {
            self.line(2, "} else {");
            self.line(
                3,
                "throw std::invalid_argument(\"the program has no parameter '\" + std::string(name) + \"'\");",
            );
            self.line(2, "}");
            self.line(2, "setup();");
        }
        self.line(1, "}");
        self.line(0, "");
    }

    fn bound_checks(&mut self, indent: usize) {
        let ir = self.ir;
        for (i, p) in ir.params.iter().enumerate() {
            for (bound, cmp, word) in [(&p.lower, "<", "below its lower"), (&p.upper, ">", "above its upper")] {
                if let Some((c, text)) = bound {
                    let b = self.cexpr(c);
                    let me = self.param_ref(i);
                    self.line(indent, &format!("if ({} {} {}) {{", me, cmp, b));
                    self.line(indent + 1, &format!(
                        "throw std::invalid_argument(\"{} = \" + atls::ops::text({}) + \" is {} bound {} = \" + atls::ops::text({}));",
                        p.name, me, word, text.replace('"', "'"), b
                    ));
                    self.line(indent, "}");
                }
            }
        }
    }

    fn submit_lines(&mut self, indent: usize, target: &str, id: &str) {
        self.line(indent, &format!("if ({}.stated()) {{", target));
        self.line(indent + 1, "atl::strategy::TargetPosition order;");
        self.line(indent + 1, &format!("order.instrument_ = {};", id));
        self.line(indent + 1, "order.tag_ = tag_;");
        self.line(
            indent + 1,
            &format!("order.target_quantity_ = atl::Decimal64({}.shares(), 0);", target),
        );
        self.line(indent + 1, "ctx.submit(order);");
        self.line(indent, "}");
    }

    fn program(&mut self) {
        if self.ir.mode == Mode::Universe {
            return self.program_universe();
        }
        let ir = self.ir;
        let o = self.o;
        let named = matches!(ir.mode, Mode::Named(_));
        self.header();
        self.params_struct();
        self.user_defs();
        // Constructors
        if named {
            self.line(
                1,
                "/** Trades the named instruments, in the order of `instruments {}`, with the default parameters. */",
            );
            self.line(
                1,
                &format!(
                    "explicit {}(std::vector<atl::InstrumentId> ids) : ids_(std::move(ids)) {{",
                    o.name
                ),
            );
            self.line(2, &format!("if (ids_.size() != {}) {{", self.inst_count()));
            self.line(
                3,
                &format!(
                    "throw std::invalid_argument(\"the program names {} instruments\");",
                    self.inst_count()
                ),
            );
            self.line(2, "}");
            self.line(2, "setup();");
            self.line(1, "}");
        } else {
            self.line(1, "/** Trades `instrument` with the default parameters. */");
            self.line(
                1,
                &format!(
                    "explicit {}(atl::InstrumentId instrument) : instrument_(std::move(instrument)) {{",
                    o.name
                ),
            );
            self.line(2, "setup();");
            self.line(1, "}");
            self.line(0, "");
            self.line(1, "/** Trades the one instrument in `ids`, for a runner. */");
            self.line(
                1,
                &format!(
                    "explicit {}(std::vector<atl::InstrumentId> ids) : {}(ids.at(0)) {{}}",
                    o.name, o.name
                ),
            );
            self.line(0, "");
            self.line(
                1,
                "/** Trades `instrument` with `params`; throws `std::invalid_argument` if a bound is violated. */",
            );
            self.line(1, &format!("{}(atl::InstrumentId instrument, Params params) : instrument_(std::move(instrument)), params_(params) {{", o.name));
            self.line(2, "setup();");
            self.line(1, "}");
        }
        self.line(0, "");
        self.set_method();
        if let Some(body) = &ir.handlers.on_start {
            self.line(1, "/** `on_start`. */");
            self.line(1, "template <class Ctx>");
            self.line(1, "void on_start(Ctx&) {");
            self.fp_contract();
            self.in_on_bar = false;
            self.stmts(body, 2);
            self.line(1, "}");
            self.line(0, "");
        }
        // on_bar / on_bars
        self.line(1, "/** Series every bar, then `on_bar` once the warm-up is over. */");
        self.line(1, "template <class Ctx>");
        if named {
            self.line(1, "void on_bars(Ctx& ctx, const atl::strategy::BarSlice& slice) {");
        } else {
            self.line(1, "void on_bar(Ctx& ctx, const atl::strategy::BarEvent& bar) {");
        }
        self.fp_contract();
        if named {
            self.line(2, "// A period missing any named instrument is skipped.");
            for k in 0..self.inst_count() {
                let n = self.inst_name(k);
                self.line(2, &format!("const auto* bar_{} = slice.find(ids_[{}]);", n, k));
                self.line(2, &format!("if (bar_{} == nullptr) {{", n));
                self.line(3, "return;");
                self.line(2, "}");
            }
        } else {
            self.line(2, "if (!(bar.instrument_ == instrument_)) {");
            self.line(3, "return;");
            self.line(2, "}");
        }
        self.line(2, "++bars_;");
        self.in_on_bar = true;
        self.series_lines(2);
        for &k in &ir.kept {
            let v = self.node_v(k);
            if matches!(
                ir.nodes[k].kind,
                NodeKind::Num(_) | NodeKind::Bool(_) | NodeKind::Param(_)
            ) {
                continue;
            }
            let name = self.kept_name(k);
            self.line(2, &format!("{} = {};", name, v.code));
        }
        self.line(2, "if (bars_ < warmup_) {");
        self.line(3, "return;");
        self.line(2, "}");
        if ir.uses_held {
            if named {
                for k in 0..self.inst_count() {
                    let n = self.inst_name(k);
                    self.line(
                        2,
                        &format!("const auto net_{} = ctx.portfolio().net_quantity_of(ids_[{}]);", n, k),
                    );
                    self.line(2, &format!("if (!net_{}) {{", n));
                    self.line(3, "return;");
                    self.line(2, "}");
                    self.line(
                        2,
                        &format!(
                            "[[maybe_unused]] const double held_{n} = atls::ops::to_double(*net_{n});",
                            n = n
                        ),
                    );
                }
            } else {
                self.line(2, "const auto net = ctx.portfolio().net_quantity_of(instrument_);");
                self.line(2, "if (!net) {");
                self.line(3, "return;");
                self.line(2, "}");
                self.line(2, "const double held = atls::ops::to_double(*net);");
            }
        }
        if named {
            for k in 0..self.inst_count() {
                let n = self.inst_name(k);
                self.line(2, &format!("atls::ops::Target target_{};", n));
            }
        } else {
            self.line(2, "atls::ops::Target target;");
        }
        if let Some(body) = &ir.handlers.on_bar {
            self.stmts(body, 2);
        }
        if named {
            for k in 0..self.inst_count() {
                let n = self.inst_name(k);
                self.submit_lines(2, &format!("target_{}", n), &format!("ids_[{}]", k));
            }
        } else {
            self.submit_lines(2, "target", "instrument_");
        }
        self.line(1, "}");
        self.line(0, "");
        self.in_on_bar = false;
        if let Some(body) = &ir.handlers.on_fill {
            self.line(1, "/** `on_fill`, for this instrument's fills. */");
            self.line(1, "template <class Ctx>");
            self.line(1, "void on_fill(Ctx&, const atl::strategy::Fill& fill) {");
            self.fp_contract();
            self.line(2, "if (!(fill.instrument_ == instrument_)) {");
            self.line(3, "return;");
            self.line(2, "}");
            if Self::handler_reads_fill(body) {
                self.line(
                    2,
                    "[[maybe_unused]] const double fill_price = atls::ops::to_double(fill.price_);",
                );
                self.line(
                    2,
                    "[[maybe_unused]] const double fill_quantity = atls::ops::to_double(fill.quantity_);",
                );
                self.line(
                    2,
                    "[[maybe_unused]] const bool fill_buy = fill.side_ == atl::OrderSide::BUY;",
                );
            }
            self.stmts(body, 2);
            self.line(1, "}");
            self.line(0, "");
        }
        if let Some(body) = &ir.handlers.on_stop {
            self.line(1, "/** `on_stop`. */");
            self.line(1, "template <class Ctx>");
            self.line(1, "void on_stop(Ctx&) {");
            self.fp_contract();
            self.stmts(body, 2);
            self.line(1, "}");
            self.line(0, "");
        }
        self.line(
            1,
            "/** Checks the bounds, sizes the rings and resets every series and state. */",
        );
        self.line(1, "void setup() {");
        self.fp_contract();
        self.bound_checks(2);
        self.line(2, "bars_ = 0;");
        self.reset_lines(2);
        let w = self.warmup_code();
        self.line(2, &format!("warmup_ = static_cast<std::size_t>({});", w));
        self.line(1, "}");
        self.line(0, "");
        // Members
        if named {
            self.line(1, "std::vector<atl::InstrumentId> ids_;");
        } else {
            self.line(1, "atl::InstrumentId instrument_;");
        }
        self.line(1, "Params params_;");
        self.line(1, &format!("std::string tag_{{\"{}\"}};", o.name.to_lowercase()));
        self.line(1, "std::size_t bars_{0};");
        self.line(1, "std::size_t warmup_{0};");
        self.object_members(1);
        for &k in &ir.kept {
            if matches!(
                ir.nodes[k].kind,
                NodeKind::Num(_) | NodeKind::Bool(_) | NodeKind::Param(_)
            ) {
                continue;
            }
            let ty = if self.node_repr[k] == Repr::Plain {
                Self::cty(ir.nodes[k].ty)
            } else {
                "double"
            };
            let name = self.kept_name(k);
            self.line(1, &format!("{} {}{{}};", ty, name));
        }
        self.line(0, "};");
        self.line(0, "");
        self.line(0, "} // namespace atls");
    }

    /// The cross-sectional values one statement reads, innermost first.
    fn xs_in(s: &HStmt, out: &mut Vec<(XsOp, HExpr, usize)>) {
        fn walk(e: &HExpr, out: &mut Vec<(XsOp, HExpr, usize)>) {
            match &e.kind {
                HKind::Xs(op, a, id) => {
                    walk(a, out);
                    out.push((*op, (**a).clone(), *id));
                }
                HKind::Unary(_, a) => walk(a, out),
                HKind::Binary(_, a, b) => {
                    walk(a, out);
                    walk(b, out);
                }
                HKind::Ternary(a, b, c) => {
                    walk(a, out);
                    walk(b, out);
                    walk(c, out);
                }
                HKind::Call(_, args) | HKind::UserCall(_, args) => args.iter().for_each(|a| walk(a, out)),
                _ => {}
            }
        }
        match s {
            HStmt::Local { value, .. } | HStmt::Assign { value, .. } | HStmt::Target(_, value) => walk(value, out),
            HStmt::If { cond, then, els } => {
                walk(cond, out);
                then.iter().chain(els.iter()).for_each(|t| Self::xs_in(t, out));
            }
            HStmt::For { body, .. } | HStmt::UFor { body, .. } => body.iter().for_each(|t| Self::xs_in(t, out)),
            HStmt::Return(value) | HStmt::UAssign { value, .. } => walk(value, out),
        }
    }

    /// A universe handler: each top-level statement runs across every active instrument before the next,
    /// so a cross-sectional value sees every instrument's earlier values.
    fn universe_body(&mut self, body: &[HStmt], indent: usize) {
        for s in body {
            let mut xs = vec![];
            Self::xs_in(s, &mut xs);
            for (op, arg, id) in xs {
                self.prefix = "L.".into();
                let v = self.hexpr(&arg);
                let a = checked(&v);
                match op {
                    XsOp::Rank => {
                        self.members.push(format!("std::vector<double> xs{}_;", id));
                        self.line(indent, "xs_sort_.clear();");
                        self.line(indent, "for (std::size_t ln = 0; ln < lanes_.size(); ++ln) {");
                        self.line(indent + 1, &format!("xs{}_[ln] = atls::ops::NaN;", id));
                        self.line(indent + 1, "if (!active_[ln]) {");
                        self.line(indent + 2, "continue;");
                        self.line(indent + 1, "}");
                        self.line(indent + 1, "[[maybe_unused]] auto& L = lanes_[ln];");
                        self.line(indent + 1, &format!("const double v = {};", a));
                        self.line(indent + 1, "if (!std::isnan(v)) {");
                        self.line(indent + 2, "xs_sort_.emplace_back(v, ln);");
                        self.line(indent + 1, "}");
                        self.line(indent, "}");
                        self.line(indent, "std::sort(xs_sort_.begin(), xs_sort_.end());");
                        self.line(indent, "for (std::size_t k = 0; k < xs_sort_.size(); ++k) {");
                        self.line(
                            indent + 1,
                            &format!("xs{}_[xs_sort_[k].second] = static_cast<double>(k + 1);", id),
                        );
                        self.line(indent, "}");
                        self.xs_code.insert(id, format!("xs{}_[ln]", id));
                    }
                    _ => {
                        // Only what the operator reads: a sum, a count, or both for the mean.
                        let (sum, count) = (op != XsOp::Count, op != XsOp::Sum);
                        if sum {
                            self.line(indent, &format!("double xs{}_sum = 0;", id));
                        }
                        if count {
                            self.line(indent, &format!("std::size_t xs{}_n = 0;", id));
                        }
                        self.line(indent, "for (std::size_t ln = 0; ln < lanes_.size(); ++ln) {");
                        self.line(indent + 1, "if (!active_[ln]) {");
                        self.line(indent + 2, "continue;");
                        self.line(indent + 1, "}");
                        self.line(indent + 1, "[[maybe_unused]] auto& L = lanes_[ln];");
                        self.line(indent + 1, &format!("const double v = {};", a));
                        self.line(indent + 1, "if (!std::isnan(v)) {");
                        if sum {
                            self.line(indent + 2, &format!("xs{}_sum += v;", id));
                        }
                        if count {
                            self.line(indent + 2, &format!("++xs{}_n;", id));
                        }
                        self.line(indent + 1, "}");
                        self.line(indent, "}");
                        let code = match op {
                            XsOp::Mean => format!(
                                "(xs{i}_n > 0 ? xs{i}_sum / static_cast<double>(xs{i}_n) : atls::ops::NaN)",
                                i = id
                            ),
                            XsOp::Count => format!("static_cast<double>(xs{}_n)", id),
                            _ => format!("xs{}_sum", id),
                        };
                        self.line(indent, &format!("const double xs{} = {};", id, code));
                        self.xs_code.insert(id, format!("xs{}", id));
                    }
                }
            }
            self.prefix = "L.".into();
            self.line(indent, "for (std::size_t ln = 0; ln < lanes_.size(); ++ln) {");
            self.line(indent + 1, "if (!active_[ln]) {");
            self.line(indent + 2, "continue;");
            self.line(indent + 1, "}");
            self.line(indent + 1, "[[maybe_unused]] auto& L = lanes_[ln];");
            self.stmts(std::slice::from_ref(s), indent + 1);
            self.line(indent, "}");
        }
        self.prefix.clear();
    }

    fn program_universe(&mut self) {
        let ir = self.ir;
        let o = self.o;
        self.header();
        self.params_struct();
        self.user_defs();
        // The lane: one instrument's series, indicators and state.
        self.line(1, "/** One instrument's series, indicators and state. */");
        self.line(1, "struct Lane {");
        self.line(2, "std::size_t bars_{0};");
        if ir.legs {
            self.line(2, "double open_now_{atls::ops::NaN};");
        }
        if ir.uses_held {
            self.line(2, "double held_{0};");
        }
        for i in 0..ir.nodes.len() {
            if !ir.live[i]
                || matches!(
                    ir.nodes[i].kind,
                    NodeKind::Num(_) | NodeKind::Bool(_) | NodeKind::Param(_)
                )
            {
                continue;
            }
            let ty = if self.node_repr[i] == Repr::Plain {
                Self::cty(ir.nodes[i].ty)
            } else {
                "double"
            };
            let name = self.node_name(i);
            self.line(2, &format!("{} {}{{}};", ty, name));
        }
        self.object_members(2);
        self.line(0, "");
        self.line(2, "/** Sizes the rings and resets every series and state. */");
        self.line(2, "void setup([[maybe_unused]] const Params& params_) {");
        self.fp_contract();
        self.line(3, "bars_ = 0;");
        self.reset_lines(3);
        self.line(2, "}");
        self.line(0, "");
        if ir.period_clock {
            self.line(2, "/** Takes this instrument's bar, or a missing period (`nullptr`): fields, then series in dependency order. */");
            self.line(
                2,
                "void update([[maybe_unused]] const Params& params_, const atl::strategy::BarEvent* bar) {",
            );
        } else {
            self.line(
                2,
                "/** Takes this instrument's bar: fields, then series in dependency order. */",
            );
            self.line(
                2,
                "void update([[maybe_unused]] const Params& params_, const atl::strategy::BarEvent& bar) {",
            );
        }
        self.fp_contract();
        self.line(3, "++bars_;");
        self.lane = true;
        self.in_on_bar = true;
        self.series_lines(3);
        self.lane = false;
        self.line(2, "}");
        self.line(1, "};");
        self.line(0, "");
        self.line(
            1,
            "/** Trades every instrument in `ids` with the default parameters. */",
        );
        self.line(
            1,
            &format!(
                "explicit {}(std::vector<atl::InstrumentId> ids) : ids_(std::move(ids)) {{",
                o.name
            ),
        );
        self.line(2, "setup();");
        self.line(1, "}");
        self.line(0, "");
        self.set_method();
        if ir.legs {
            self.line(1, "/** The leg of a day a slice is: 0 the open, 1 the day's bar, 2 the close (minutes past midnight, less one). */");
            self.line(1, "static int part_of(std::chrono::system_clock::time_point end) {");
            self.line(2, "const auto midnight = std::chrono::floor<std::chrono::hours>(end);");
            self.line(2, "return static_cast<int>(std::chrono::duration_cast<std::chrono::minutes>(end - midnight).count()) - 1;");
            self.line(1, "}");
            self.line(0, "");
        }
        self.line(
            1,
            "/** Each instrument's series, then the handlers across every instrument past its warm-up. */",
        );
        self.line(1, "template <class Ctx>");
        self.line(1, "void on_bars(Ctx& ctx, const atl::strategy::BarSlice& slice) {");
        self.fp_contract();
        if ir.legs {
            self.line(2, "const int part = part_of(slice.period_end_);");
            self.line(2, "if (part != 0 && part != 1) {");
            self.line(3, "return;");
            self.line(2, "}");
            self.line(2, "if (part == 1) {");
            self.line(3, "++periods_;");
            self.line(2, "}");
        } else {
            self.line(2, "++periods_;");
        }
        self.line(2, "for (std::size_t ln = 0; ln < lanes_.size(); ++ln) {");
        self.line(3, "active_[ln] = 0;");
        self.line(3, "tg_[ln] = atls::ops::Target{};");
        self.line(3, "const auto* bar = slice.find(ids_[ln]);");
        self.line(3, "if (bar == nullptr) {");
        if ir.period_clock {
            // Every instrument takes every period: a missing bar is a row of undefined values.
            if ir.legs {
                self.line(4, "if (part == 1) {");
                self.line(5, "lanes_[ln].update(params_, nullptr);");
                self.line(4, "}");
            } else {
                self.line(4, "lanes_[ln].update(params_, nullptr);");
            }
        }
        self.line(4, "continue;");
        self.line(3, "}");
        if ir.legs {
            self.line(3, "if (part == 0) {");
            self.line(4, "if (!bar->bar_.open_) {");
            self.line(5, "continue;");
            self.line(4, "}");
            self.line(4, "lanes_[ln].open_now_ = atls::ops::to_double(*bar->bar_.open_);");
            self.line(3, "} else {");
            self.line(
                4,
                &format!(
                    "lanes_[ln].update(params_, {});",
                    if ir.period_clock { "bar" } else { "*bar" }
                ),
            );
            self.line(3, "}");
        } else {
            self.line(
                3,
                &format!(
                    "lanes_[ln].update(params_, {});",
                    if ir.period_clock { "bar" } else { "*bar" }
                ),
            );
        }
        self.line(3, "active_[ln] = lanes_[ln].bars_ >= warmup_ ? 1 : 0;");
        if ir.uses_held {
            self.line(3, "if (active_[ln]) {");
            self.line(4, "const auto net = ctx.portfolio().net_quantity_of(ids_[ln]);");
            self.line(4, "if (!net) {");
            self.line(5, "active_[ln] = 0;");
            self.line(4, "} else {");
            self.line(5, "lanes_[ln].held_ = atls::ops::to_double(*net);");
            self.line(4, "}");
            self.line(3, "}");
        }
        self.line(2, "}");
        self.in_on_bar = true;
        if ir.legs {
            if let Some(body) = &ir.handlers.on_open {
                self.line(2, "if (part == 0) {");
                self.universe_body(body, 3);
                self.line(2, "}");
            }
            if let Some(body) = &ir.handlers.on_close {
                self.line(2, "if (part == 1) {");
                self.universe_body(body, 3);
                self.line(2, "}");
            }
        } else if let Some(body) = &ir.handlers.on_bar {
            self.universe_body(body, 2);
        }
        self.line(2, "for (std::size_t ln = 0; ln < lanes_.size(); ++ln) {");
        self.line(3, "if (!active_[ln]) {");
        self.line(4, "continue;");
        self.line(3, "}");
        self.submit_lines(3, "tg_[ln]", "ids_[ln]");
        self.line(2, "}");
        self.line(1, "}");
        self.line(0, "");
        self.in_on_bar = false;
        self.line(1, "/** Checks the bounds and resets every instrument. */");
        self.line(1, "void setup() {");
        self.fp_contract();
        self.bound_checks(2);
        self.line(2, "periods_ = 0;");
        self.line(2, "lanes_.assign(ids_.size(), Lane{});");
        self.line(2, "for (auto& lane : lanes_) {");
        self.line(3, "lane.setup(params_);");
        self.line(2, "}");
        self.line(2, "active_.assign(ids_.size(), 0);");
        self.line(2, "tg_.assign(ids_.size(), atls::ops::Target{});");
        let members = self.members.clone();
        for m in &members {
            // `std::vector<T> name_;` -> `name_.assign(n, T{})`
            let name = m.trim_end_matches(';').rsplit(' ').next().unwrap_or("").to_string();
            self.line(2, &format!("{}.assign(ids_.size(), {{}});", name));
        }
        let w = self.warmup_code();
        self.line(2, &format!("warmup_ = static_cast<std::size_t>({});", w));
        self.line(1, "}");
        self.line(0, "");
        self.line(1, "std::vector<atl::InstrumentId> ids_;");
        self.line(1, "Params params_;");
        self.line(1, &format!("std::string tag_{{\"{}\"}};", o.name.to_lowercase()));
        self.line(1, "std::size_t warmup_{0};");
        self.line(1, "std::size_t periods_{0};");
        self.line(1, "std::vector<Lane> lanes_;");
        self.line(1, "std::vector<unsigned char> active_;");
        self.line(1, "std::vector<atls::ops::Target> tg_;");
        self.line(1, "std::vector<std::pair<double, std::size_t>> xs_sort_;");
        for m in &members {
            self.line(1, m);
        }
        self.line(0, "};");
        self.line(0, "");
        self.line(0, "} // namespace atls");
    }

    /// The functions of `functions {}` (static members) and one struct per user indicator and policy used.
    fn user_defs(&mut self) {
        let ir = self.ir;
        for f in &ir.functions {
            let args: Vec<String> = f
                .args
                .iter()
                .map(|(n, t)| format!("{} a_{}", if *t == Ty::Int { "std::int64_t" } else { "double" }, n))
                .collect();
            self.line(
                1,
                &format!(
                    "/** `{}`, from `functions {{}}`: an undefined argument makes the call undefined. */",
                    f.name
                ),
            );
            self.line(
                1,
                &format!(
                    "static {} f_{}({}) {{",
                    if f.ret == Ty::Int { "std::int64_t" } else { "double" },
                    f.name,
                    args.join(", ")
                ),
            );
            self.fp_contract();
            let nan: Vec<String> = f
                .args
                .iter()
                .filter(|a| a.1 != Ty::Int)
                .map(|a| format!("std::isnan(a_{})", a.0))
                .collect();
            if !nan.is_empty() {
                self.line(2, &format!("if ({}) {{", nan.join(" || ")));
                self.line(3, "return atls::ops::NaN;");
                self.line(2, "}");
            }
            self.enter_user(&f.locals, f.args.clone(), vec![], vec![]);
            self.stmts(&f.body, 2);
            self.leave_user();
            self.line(1, "}");
            self.line(0, "");
        }
        let mut done: Vec<String> = vec![];
        for i in 0..ir.nodes.len() {
            if !ir.live[i] {
                continue;
            }
            if let NodeKind::UInd { def, miss, .. } = &ir.nodes[i].kind {
                let ty = Self::uind_type(&ir.indicators[*def], miss.policy);
                if !done.contains(&ty) {
                    done.push(ty);
                    self.indicator_struct(*def, miss.policy);
                }
            }
        }
    }

    /// Whether a divisor inside a user body is known nonzero.
    fn unonzero(&self, b: &HExpr) -> bool {
        match &b.kind {
            HKind::Num(v) => *v != 0.0,
            HKind::UArg(i) => self.unonzero_args.contains(i),
            _ => false,
        }
    }

    fn enter_user(
        &mut self,
        locals: &'a [Local],
        args: Vec<(String, Ty)>,
        states: Vec<(String, Ty)>,
        values: Vec<String>,
    ) {
        self.fn_locals = Some(locals);
        self.uargs = args;
        self.ustates = states;
        self.uvalues = values;
        self.force_checked = true;
        self.local_repr.clear();
    }

    fn leave_user(&mut self) {
        self.fn_locals = None;
        self.uargs.clear();
        self.ustates.clear();
        self.uvalues.clear();
        self.force_checked = false;
        self.local_repr.clear();
    }

    /// A user indicator as a struct: fixed arguments, the window's rings, states, values, and the step.
    fn indicator_struct(&mut self, d: usize, policy: Policy) {
        let ir = self.ir;
        let def = &ir.indicators[d];
        let ty = Self::uind_type(def, policy);
        let states: Vec<(String, Ty)> = def.states.iter().map(|s| (s.name.clone(), s.ty)).collect();
        let values: Vec<String> = def.values.iter().map(|v| v.name.clone()).collect();
        let series = def.series_args();
        let consts = def.const_args();
        let cty = |t: Ty| if t == Ty::Int { "std::int64_t" } else { "double" };
        self.enter_user(&def.locals, def.args.clone(), states.clone(), values.clone());
        self.unonzero_args = def.nonzero.clone();
        self.line(
            1,
            &format!(
                "/** The indicator `{}` with `missing = {}`, from its definition. */",
                def.name,
                policy.name()
            ),
        );
        self.line(1, &format!("struct {} {{", ty));
        for &c in &consts {
            self.line(2, &format!("std::int64_t a_{}{{0}};", def.args[c].0));
        }
        if def.window.is_some() {
            for &s in &series {
                self.line(2, &format!("atls::ops::Ring w_{}_;", def.args[s].0));
            }
        }
        self.line(2, "std::size_t count_{0};");
        self.line(2, "std::size_t steps_{0};");
        self.line(2, "bool poisoned_{false};");
        for st in &def.states {
            self.line(2, &format!("{} s_{}{{0}};", cty(st.ty), st.name));
        }
        for v in &def.values {
            self.line(2, &format!("double v_{}{{atls::ops::NaN}};", v.name));
        }
        let size = def.window.as_ref().map(|(_, _, h)| {
            let v = self.hexpr(h);
            format!("static_cast<std::size_t>({})", plain(&v))
        });
        let warm = def.warmup.as_ref().map(|(_, h)| {
            let v = self.hexpr(h);
            format!("static_cast<std::size_t>({})", plain(&v))
        });
        // set
        let params: Vec<String> = consts
            .iter()
            .map(|&c| format!("std::int64_t {}", def.args[c].0))
            .collect();
        self.line(2, &format!("void set({}) {{", params.join(", ")));
        for &c in &consts {
            self.line(3, &format!("a_{n} = {n};", n = def.args[c].0));
        }
        if let Some(sz) = &size {
            for &s in &series {
                self.line(3, &format!("w_{}_.reset({} + 1);", def.args[s].0, sz));
            }
        }
        self.line(2, "}");
        // reset
        self.line(2, "void reset() {");
        self.fp_contract();
        if size.is_some() {
            for &s in &series {
                self.line(3, &format!("w_{}_.clear();", def.args[s].0));
            }
        }
        self.line(3, "count_ = 0;");
        self.line(3, "steps_ = 0;");
        for st in &def.states {
            let v = self.hexpr(&st.init);
            let code = if st.ty == Ty::Int { plain(&v) } else { checked(&v) };
            self.line(3, &format!("s_{} = {};", st.name, code));
        }
        for v in &def.values {
            self.line(3, &format!("v_{} = atls::ops::NaN;", v.name));
        }
        self.line(2, "}");
        let sargs: Vec<String> = series.iter().map(|&s| format!("double a_{}", def.args[s].0)).collect();
        let snames: Vec<String> = series.iter().map(|&s| format!("a_{}", def.args[s].0)).collect();
        // step: defined inputs
        self.line(2, "/** One step on defined inputs: update, the window, the values. */");
        self.line(2, &format!("void step({}) {{", sargs.join(", ")));
        self.fp_contract();
        self.step_body(d, &size, false);
        self.line(2, "}");
        if policy == Policy::Skip {
            self.line(
                2,
                "/** One step that may be missing: a missing step takes its slot in the window and runs nothing. */",
            );
            self.line(2, &format!("void step_skip({}) {{", sargs.join(", ")));
            self.fp_contract();
            let nan: Vec<String> = snames.iter().map(|n| format!("std::isnan({})", n)).collect();
            self.line(3, &format!("const bool ok = !({});", nan.join(" || ")));
            self.step_body(d, &size, true);
            self.line(2, "}");
        }
        // values
        // The values need not read the current input.
        let vargs: Vec<String> = sargs.iter().map(|a| format!("[[maybe_unused]] {a}")).collect();
        self.line(2, &format!("void values({}) {{", vargs.join(", ")));
        self.fp_contract();
        let mut cond = vec![];
        if let Some(sz) = &size {
            cond.push(format!("w_{}_.seen() < {}", def.args[series[0]].0, sz));
        }
        if let Some(w) = &warm {
            cond.push(format!("steps_ < {}", w));
        }
        if !cond.is_empty() {
            self.line(3, &format!("if ({}) {{", cond.join(" || ")));
            for v in &def.values {
                self.line(4, &format!("v_{} = atls::ops::NaN;", v.name));
            }
            self.line(4, "return;");
            self.line(3, "}");
        }
        for v in &def.values {
            let x = self.hexpr(&v.expr);
            self.line(3, &format!("v_{} = {};", v.name, checked(&x)));
        }
        self.line(2, "}");
        self.line(1, "};");
        self.line(0, "");
        self.leave_user();
        self.unonzero_args.clear();
    }

    /// The body of a step: `update`, the window (enter the new values, leave the oldest), the values.
    fn step_body(&mut self, d: usize, size: &Option<String>, skip: bool) {
        let ir = self.ir;
        let def = &ir.indicators[d];
        let series = def.series_args();
        let snames: Vec<String> = series.iter().map(|&s| format!("a_{}", def.args[s].0)).collect();
        let (ind, guard) = if skip { (4, true) } else { (3, false) };
        if !def.update.is_empty() {
            if guard {
                self.line(3, "if (ok) {");
            }
            self.line(ind, "{");
            self.stmts(&def.update, ind + 1);
            self.line(ind, "}");
            if guard {
                self.line(3, "}");
            }
        }
        if let Some(sz) = size {
            for (k, &s) in series.iter().enumerate() {
                let v = if skip {
                    format!("ok ? {} : atls::ops::NaN", snames[k])
                } else {
                    snames[k].clone()
                };
                self.line(3, &format!("w_{}_.push({});", def.args[s].0, v));
            }
            if guard {
                self.line(3, "if (ok) {");
            }
            self.line(ind, "{");
            if let Some((ids, body)) = &def.enter {
                for (k, id) in ids.iter().enumerate() {
                    let name = def.locals[*id].name.clone();
                    self.local_repr.insert(*id, Repr::Checked);
                    self.line(
                        ind + 1,
                        &format!("[[maybe_unused]] const double l_{} = {};", name, snames[k]),
                    );
                }
                self.stmts(body, ind + 1);
            }
            if def.uses_count {
                self.line(ind + 1, "++count_;");
            }
            self.line(ind, "}");
            if guard {
                self.line(3, "}");
            }
            let first = def.args[series[0]].0.clone();
            self.line(3, &format!("if (w_{}_.seen() > {}) {{", first, sz));
            let olds: Vec<String> = series
                .iter()
                .map(|&s| format!("w_{}_.ago({})", def.args[s].0, sz))
                .collect();
            if skip {
                self.line(4, &format!("if (!std::isnan({})) {{", olds[0]));
            }
            let li = if skip { 5 } else { 4 };
            if let Some((ids, body)) = &def.leave {
                for (k, id) in ids.iter().enumerate() {
                    let name = def.locals[*id].name.clone();
                    self.local_repr.insert(*id, Repr::Checked);
                    self.line(li, &format!("[[maybe_unused]] const double l_{} = {};", name, olds[k]));
                }
                self.stmts(body, li);
            }
            if def.uses_count {
                self.line(li, "--count_;");
            }
            if skip {
                self.line(4, "}");
            }
            self.line(3, "}");
        }
        if skip {
            self.line(3, "if (ok) {");
            self.line(4, "++steps_;");
            self.line(3, "}");
        } else {
            self.line(3, "++steps_;");
        }
        self.line(3, &format!("values({});", snames.join(", ")));
    }

    /// No fused multiply-add: `a * b - c` rounds twice, as the interpreter does, whatever the
    /// includer's flags (clang fuses by default, which changed targets in the fuzz).
    fn fp_contract(&mut self) {
        self.line(0, "#if defined(__clang__)");
        self.line(2, "#pragma clang fp contract(off)");
        self.line(0, "#endif");
    }

    /// Updates indicators over one input that share a policy. Restart: an undefined input restarts them
    /// all. Skip: the undefined value takes its slot and is left out. Poison: the first undefined value
    /// after the input's own warm-up makes them undefined for good.
    fn ind_update(&mut self, consumers: &[usize], input: usize, ring: Option<String>, at: usize) {
        let ir = self.ir;
        let mode = self.o.indicators;
        let policy = consumers
            .first()
            .map(|&c| policy_of(&ir.nodes[c].kind))
            .unwrap_or(Policy::Restart);
        let naive = mode == Indicators::Naive;
        let input_v = self.node_v(input);
        let x = checked(&input_v);
        let check = ir.nodes[input].class != Class::Always;
        if !self.lane {
            for &c in consumers {
                let name = self.node_name(c);
                self.line(at, &format!("double {};", name));
            }
        }
        let mut body: Vec<String> = vec![];
        let share = self.o.share_rings;
        let own_ring = |c: usize| -> String {
            if share {
                ring.clone().unwrap()
            } else {
                format!("ring{}_", c)
            }
        };
        let mut pushed = false;
        for &c in consumers {
            let NodeKind::Ind(ind, _, _, _) = &ir.nodes[c].kind else {
                continue;
            };
            let name = self.node_name(c);
            match ring_need(*ind, mode, policy) {
                Some(_) => {
                    let r = own_ring(c);
                    if !share || !pushed {
                        body.push(format!("{}.push({});", r, x));
                        pushed = share;
                    }
                    let f = if naive { "recompute" } else { "update" };
                    body.push(format!("{} = ind{}_.{}({});", name, c, f, r));
                }
                None if policy == Policy::Skip => {
                    // An EMA skips a missing value: it keeps its last value.
                    body.push(format!(
                        "{n} = std::isnan({x}) ? ind{c}_.value_ : ind{c}_.update({x});",
                        n = name,
                        x = x,
                        c = c
                    ));
                }
                None => body.push(format!("{} = ind{}_.update({});", name, c, x)),
            }
        }
        if policy == Policy::Skip || !check {
            for b in &body {
                self.line(at, b);
            }
            return;
        }
        let restart_lines = |p: &mut Self, depth: usize| {
            let mut cleared = false;
            for &c in consumers {
                let NodeKind::Ind(ind, _, _, _) = &ir.nodes[c].kind else {
                    continue;
                };
                if ring_need(*ind, mode, policy).is_some() {
                    let r = own_ring(c);
                    if !share || !cleared {
                        p.line(depth, &format!("{}.clear();", r));
                        cleared = share;
                    }
                }
                let name = p.node_name(c);
                p.line(depth, &format!("ind{}_.reset();", c));
                p.line(depth, &format!("{} = atls::ops::NaN;", name));
            }
        };
        if policy == Policy::Poison {
            let flag = if share {
                format!("poisoned{}_", input)
            } else {
                format!("poisoned{}_", consumers[0])
            };
            let lb = self.cexpr(&ir.nodes[input].lookback);
            self.line(at, &format!("if ({} || std::isnan({})) {{", flag, x));
            self.line(
                at + 1,
                &format!(
                    "if (!{f} && bars_ >= static_cast<std::size_t>({lb})) {{",
                    f = flag,
                    lb = lb
                ),
            );
            self.line(at + 2, &format!("{} = true;", flag));
            self.line(at + 1, "}");
            restart_lines(self, at + 1);
        } else {
            self.line(at, &format!("if (std::isnan({})) {{", x));
            restart_lines(self, at + 1);
        }
        self.line(at, "} else {");
        for b in &body {
            self.line(at + 1, b);
        }
        self.line(at, "}");
    }
}
