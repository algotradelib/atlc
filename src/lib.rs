//! atlc: the compiler for atls, a strategy language for algotradelib, transpiled to C++.
//!
//! The front end (lexer, parser, diagnostics), the checker and the typed intermediate representation it
//! produces live in this library, so that the command line and, later, an editor's language server share
//! them; so does the C++ back end and the runtime headers it calls.

pub mod backend;
pub mod frontend;
pub mod ir;
pub mod sema;

/// The compiler's version; until the first release the language has no version of its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parses and checks a program: the checked IR (with its warnings), or every error found.
pub fn check(source: &str) -> Result<ir::Ir, Vec<frontend::diag::Diag>> {
    check_with(source, true)
}

/// As [`check`]; `share` merges identical series expressions, which `--share off` turns off.
pub fn check_with(source: &str, share: bool) -> Result<ir::Ir, Vec<frontend::diag::Diag>> {
    let program = frontend::parse(source)?;
    sema::check(&program, &sema::Options { share })
}

/// Parses, checks and prints a program as a C++ header: the header and the warnings, or every error found.
pub fn build(
    source: &str,
    options: &backend::Options,
) -> Result<(String, Vec<frontend::diag::Diag>), Vec<frontend::diag::Diag>> {
    let ir = check_with(source, options.share_rings)?;
    let header = backend::print(&ir, options);
    Ok((header, ir.warnings))
}
