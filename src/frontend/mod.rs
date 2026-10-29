//! The front end: tokens, the hand-written parser, the syntax tree with spans, and diagnostics.

pub mod ast;
pub mod diag;
pub mod lexer;
pub mod parser;

use ast::{BinOp, Expr, ExprKind, UnOp};

/// Parses a program with the hand-written parser, the reference implementation of the grammar.
///
/// The parser and the checker recurse over the syntax tree, so the parser refuses a program nested deeper
/// than [`parser::MAX_DEPTH`] levels (`nesting-too-deep`): blocks, `else if`, parentheses and every level of
/// an expression's tree (one per operator of a chain) count. Parsing and checking the deepest program it
/// accepts needs up to 16 MB of stack in a debug build and 2 MB in a release build, more than a spawned
/// thread's default: run on a thread with a larger stack, as `atlc check` does (64 MB).
pub fn parse(source: &str) -> Result<ast::Program, Vec<diag::Diag>> {
    parser::parse(source)
}

/// A number as messages print it: whole numbers without a decimal point.
pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// An expression printed back as source, for messages.
pub fn ast_text(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Num(v, _) => fmt_num(*v),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Name(n) => n.clone(),
        ExprKind::Unary(UnOp::Neg, a) => format!("-{}", ast_text(a)),
        ExprKind::Unary(UnOp::Not, a) => format!("not {}", ast_text(a)),
        ExprKind::Binary(op, a, b) => {
            let wrap = |x: &Expr| match &x.kind {
                ExprKind::Binary(o2, _, _) if prec(*o2) < prec(*op) => format!("({})", ast_text(x)),
                ExprKind::Ternary(..) => format!("({})", ast_text(x)),
                _ => ast_text(x),
            };
            format!("{} {} {}", wrap(a), op.text(), wrap(b))
        }
        ExprKind::Ternary(c, a, b) => {
            format!("{} ? {} : {}", ast_text(c), ast_text(a), ast_text(b))
        }
        ExprKind::Call(f, args) => {
            format!(
                "{}({})",
                f.name,
                args.iter().map(ast_text).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Index(x, k) => format!("{}[{}]", ast_text(x), ast_text(k)),
        ExprKind::Field(o, f) => format!("{}.{}", o.name, f.name),
        ExprKind::Named(n, v) => format!("{} = {}", n.name, ast_text(v)),
        ExprKind::Member(x, n) => format!("{}.{}", ast_text(x), n.name),
    }
}

fn prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => 3,
        BinOp::Add | BinOp::Sub => 4,
        BinOp::Mul | BinOp::Div => 5,
    }
}
