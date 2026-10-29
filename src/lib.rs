//! atlc: the compiler for atls, a strategy language for algotradelib, transpiled to C++.
//!
//! The front end (lexer, parser, diagnostics), the checker and the typed intermediate representation it
//! produces live in this library, so that the command line and, later, an editor's language server share
//! them. The C++ back end is planned.

pub mod frontend;
pub mod ir;

/// The compiler's version; until the first release the language has no version of its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
