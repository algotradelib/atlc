//! The atls grammar of `docs/reference/syntax.md` as an LR(1) grammar for LALRPOP (`atls.lalrpop`).
//!
//! Building this package generates the parser, and LALRPOP refuses a grammar with a conflict, so a
//! successful build proves the grammar unambiguous. The tests compare it with atlc's hand-written parser,
//! the reference implementation: both accept and refuse the same programs at the syntax stage and build the
//! same tree, except for the forms the hand parser accepts only to explain them later.

use atlc::frontend::ast::{BinOp, Expr, ExprKind, Program};
use atlc::frontend::lexer::{Tok, lex};
use atlc::frontend::parser::KEYWORDS;

lalrpop_util::lalrpop_mod!(
    #[allow(clippy::all, unused)]
    atls,
    "/atls.rs"
);

/// A terminal of the grammar: keywords and the words reserved in indicators and bounds are told apart
/// from names here, since LALRPOP matches tokens by pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum T {
    Num(f64, bool),
    Ident(String),
    Str(String),
    Kw(&'static str),
    P(&'static str),
}

/// Reserved only inside an indicator (`window value update enter leave`) or in bounds (`lower upper`).
pub const CONTEXTUAL: [&str; 7] = ["window", "value", "update", "enter", "leave", "lower", "upper"];

fn terminal(tok: Tok) -> T {
    match tok {
        Tok::Num(v, int) => T::Num(v, int),
        Tok::Ident(s) => match KEYWORDS.iter().chain(&CONTEXTUAL).find(|k| **k == s) {
            Some(k) => T::Kw(k),
            None => T::Ident(s),
        },
        Tok::Str(s) => T::Str(s),
        Tok::Punct(p) => T::P(p),
        Tok::Eof => unreachable!("the end of the file is not passed to the parser"),
    }
}

/// Parses with the generated parser: the tree, or `None` when the program is not in the grammar.
pub fn parse(source: &str) -> Option<Program> {
    let tokens = lex(source).ok()?;
    let input = tokens
        .into_iter()
        .filter(|t| t.tok != Tok::Eof)
        .map(|t| Ok::<_, ()>((t.span.start, terminal(t.tok), t.span.end)));
    atls::ProgramParser::new().parse(input).ok()
}

pub(crate) fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    let span = a.span.to(b.span);
    Expr {
        kind: ExprKind::Binary(op, Box::new(a), Box::new(b)),
        span,
    }
}
