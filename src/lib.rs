//! atlc: the compiler for atls, a strategy language for algotradelib, transpiled to C++.
//!
//! The front end (lexer, parser, diagnostics), the checker and the typed intermediate representation it
//! produces live in this library, so that the command line and, later, an editor's language server share
//! them. The C++ back end is planned.

pub mod frontend;
pub mod ir;
pub mod sema;

/// The compiler's version; until the first release the language has no version of its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parses and checks a program: the checked IR (with its warnings), or every error found.
pub fn check(source: &str) -> Result<ir::Ir, Vec<frontend::diag::Diag>> {
    let program = frontend::parse(source)?;
    sema::check(&program, &sema::Options { share: true })
}
