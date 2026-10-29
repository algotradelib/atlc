//! atlc: the compiler for atls, a strategy language for algotradelib, transpiled to C++.
//!
//! Planned: the front end (lexer, parser, checker), the typed intermediate representation and the C++
//! back end live in this library, so that the command line and, later, an editor's language server share them.

/// The compiler's version; until the first release the language has no version of its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
