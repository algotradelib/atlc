//! The checker: AST -> typed IR. One traversal over the blocks in dependency order; its rules are split
//! by pass into the modules below, each an `impl Checker` block.

mod consts;
mod definedness;
mod functions;
mod handlers;
mod instruments;
mod names;
mod params;
mod policies;
mod series;
mod types;
mod windows;

use std::collections::HashMap;

use crate::frontend::ast::*;
use crate::frontend::diag::Diag;
use crate::frontend::fmt_num;
use crate::ir::*;
use names::is_reserved;
use types::join_ty;

pub struct Options {
    /// Merge identical series expressions (`sma(close, fast)` written twice is one node).
    pub share: bool,
}

const INDICATOR_NOTE: &str = "an indicator keeps the history of its input, so it must see every bar; under an `if` it would skip bars and change meaning";

#[derive(Clone)]
enum Name {
    /// A declaration that failed to check: reads of it report nothing more.
    Poisoned,
    Param(usize),
    Series(usize, Span),
    State(usize),
    Local,
    Instrument(usize),
    /// A function of `functions {}`.
    Function(usize),
    /// A user indicator.
    Indicator(usize),
}

/// What a function's or indicator's body may see: its own names only.
#[derive(Clone, Default)]
struct UCtx {
    name: String,
    is_fn: bool,
    args: Vec<(String, Ty)>,
    states: Vec<(String, Ty, bool)>,
    values: Vec<(String, Ty)>,
    window: Option<String>,
    /// Assigning state is allowed (update, enter, leave).
    assign: bool,
    /// Arguments are visible (not in a state's initial value or the window's length).
    see_args: bool,
    ret: Option<Ty>,
    returns_maybe: bool,
    /// Int arguments known to be at least 1 (an indicator's window length).
    nonzero: Vec<usize>,
    uses_count: bool,
}

struct Checker<'a> {
    opts: &'a Options,
    ir: Ir,
    errors: Vec<Diag>,
    names: HashMap<String, (Name, Span)>,
    intervals: Vec<(f64, f64)>,
    defaults: Vec<f64>,
    consed: HashMap<String, usize>,
    scopes: Vec<HashMap<String, usize>>,
    /// Which nodes each handler reads directly.
    reads: Vec<(HandlerKind, usize)>,
    /// Values known to be defined here: inside `defined(x) ? ... : ...` or `if (defined(x)) { ... }`.
    refined: Vec<HKind>,
    /// How deep inside `if`/`for` the current handler statement is.
    nest: usize,
    /// Inside a function or indicator body.
    uctx: Option<UCtx>,
}

pub fn check(program: &Program, opts: &Options) -> Result<Ir, Vec<Diag>> {
    let mut c = Checker {
        opts,
        ir: Ir::default(),
        errors: vec![],
        names: HashMap::new(),
        intervals: vec![],
        defaults: vec![],
        consed: HashMap::new(),
        scopes: vec![],
        reads: vec![],
        refined: vec![],
        nest: 0,
        uctx: None,
    };
    c.run(program);
    if c.errors.is_empty() { Ok(c.ir) } else { Err(c.errors) }
}

fn kalman_out(name: &str) -> Option<KOut> {
    Some(match name {
        "kf_beta" => KOut::Beta,
        "kf_error" => KOut::Error,
        "kf_sd" => KOut::Sd,
        _ => return None,
    })
}

fn field_of(name: &str) -> Option<Field> {
    Some(match name {
        "open" => Field::Open,
        "high" => Field::High,
        "low" => Field::Low,
        "close" => Field::Close,
        "volume" => Field::Volume,
        _ => return None,
    })
}

impl Checker<'_> {
    fn err(&mut self, d: Diag) {
        self.errors.push(d);
    }

    fn run(&mut self, program: &Program) {
        // Blocks may come in any order in the file, but are checked in dependency order.
        self.int_literals(program);
        self.duplicate_blocks(program);
        self.modes(program);
        for item in &program.items {
            if let Item::Params(decls, _) = item {
                for d in decls {
                    self.param(d);
                }
            }
        }
        for item in &program.items {
            if let Item::Functions(defs, _) = item {
                for d in defs {
                    self.function_def(d);
                }
            }
        }
        for item in &program.items {
            if let Item::Indicator(d) = item {
                self.indicator_def(d);
            }
        }
        for item in &program.items {
            if let Item::State(decls, _) = item {
                for d in decls {
                    self.state_decl(d);
                }
            }
        }
        for item in &program.items {
            if let Item::Series(decls, _) = item {
                for d in decls {
                    self.series_decl(d);
                }
            }
        }
        for item in &program.items {
            if let Item::Warmup(e, _) = item {
                self.warmup_item(e);
            }
        }
        self.handlers(program);
        self.handler_rules(program);
        self.liveness();
    }

    pub(super) fn poison(&mut self, ident: &Ident) {
        if !self.names.contains_key(&ident.name) && !is_reserved(&ident.name) {
            self.names.insert(ident.name.clone(), (Name::Poisoned, ident.span));
        }
    }

    pub(super) fn lookup_local(&self, n: &str) -> Option<usize> {
        for s in self.scopes.iter().rev() {
            if let Some(&i) = s.get(n) {
                return Some(i);
            }
        }
        None
    }

    pub(super) fn h(kind: HKind, ty: Ty, maybe: bool) -> HExpr {
        HExpr { kind, ty, maybe }
    }
}

/// Helpers for the passes' unit tests.
#[cfg(test)]
pub(crate) mod testing {
    use crate::frontend::diag::Code;
    use crate::ir::Ir;

    /// The checked IR of a program that must be valid.
    pub fn ok(source: &str) -> Ir {
        match crate::check(source) {
            Ok(ir) => ir,
            Err(e) => panic!(
                "expected a valid program, got {:?}",
                e.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>()
            ),
        }
    }

    /// The codes of a program's errors, in order (the program must parse).
    pub fn errors(source: &str) -> Vec<Code> {
        let program = crate::frontend::parse(source).expect("the program parses");
        match super::check(&program, &super::Options { share: true }) {
            Ok(_) => vec![],
            Err(e) => e.iter().map(|d| d.code).collect(),
        }
    }

    /// The first error's message.
    pub fn message(source: &str) -> String {
        match crate::check(source) {
            Ok(_) => panic!("expected an error"),
            Err(e) => e[0].message.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) mod testing_ir {
    //! Reading the IR in the passes' unit tests.
    use crate::frontend::ast::BinOp;
    use crate::ir::{CExpr, Ir, Node};

    /// A constant expression's value with the parameters' defaults.
    pub fn value(ir: &Ir, c: &CExpr) -> f64 {
        match c {
            CExpr::Num(v) | CExpr::Real(v) => *v,
            CExpr::Param(i) => ir.params[*i].default,
            CExpr::Neg(a) => -value(ir, a),
            CExpr::Bin(op, a, b) => {
                let (a, b) = (value(ir, a), value(ir, b));
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    _ => a / b,
                }
            }
            CExpr::Max(v) => v.iter().map(|x| value(ir, x)).fold(f64::NEG_INFINITY, f64::max),
        }
    }

    /// A series' node.
    pub fn series<'a>(ir: &'a Ir, name: &str) -> &'a Node {
        let id = ir
            .series
            .iter()
            .find(|s| s.0 == name)
            .unwrap_or_else(|| panic!("no series {name}"))
            .1;
        &ir.nodes[id]
    }

    /// A series' lookback with the defaults.
    pub fn lookback(ir: &Ir, name: &str) -> f64 {
        value(ir, &series(ir, name).lookback)
    }
}
