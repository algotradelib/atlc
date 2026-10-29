//! The C++ back end: the printer (one strategy struct per program) and the runtime headers it calls, compiled
//! into the binary so that a generated header and the runtime written out beside it always match.

mod cpp;

use std::path::Path;

use crate::ir::Ir;

/// How windows are summed ([types and definedness](../../docs/reference/types-and-definedness.md),
/// "Arithmetic").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Indicators {
    /// Running sums over rings sized by the inferred lookback.
    Incremental,
    /// Every window computed again every bar, oldest value first; least squares solved with Eigen.
    Naive,
    /// Sliding sums without subtraction (two stacks): amortized O(1), no cancellation residue.
    Exact,
}

impl Indicators {
    /// The option's value on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Indicators::Incremental => "incremental",
            Indicators::Naive => "naive",
            Indicators::Exact => "exact",
        }
    }

    /// The mode named `s` on the command line.
    pub fn from_name(s: &str) -> Option<Indicators> {
        Some(match s {
            "incremental" => Indicators::Incremental,
            "naive" => Indicators::Naive,
            "exact" => Indicators::Exact,
            _ => return None,
        })
    }
}

/// What the printer needs beyond the checked program.
#[derive(Clone, Debug)]
pub struct Options {
    /// The strategy struct's name (in namespace `atls`).
    pub name: String,
    /// The source file's name, for comments.
    pub source_name: String,
    pub indicators: Indicators,
    /// One ring per input series, shared by every indicator over it (the checker merges identical
    /// expressions under the same setting).
    pub share_rings: bool,
    /// Declare `reads_bar_history = false`: the series keep their own lookback.
    pub drop_history: bool,
}

/// The C++ header of a checked program.
pub fn print(ir: &Ir, options: &Options) -> String {
    cpp::print(ir, options)
}

/// The runtime headers, by path under an include directory, as compiled into this binary.
pub const RUNTIME: [(&str, &str); 3] = [
    ("atls/ops.hpp", include_str!("../../runtime/include/atls/ops.hpp")),
    (
        "atls/universe.hpp",
        include_str!("../../runtime/include/atls/universe.hpp"),
    ),
    (
        "atls/version.hpp",
        include_str!("../../runtime/include/atls/version.hpp"),
    ),
];

/// The runtime's compatibility identifier: 48 bits of the FNV-1a hash of every runtime header but
/// `atls/version.hpp`, which states it as `ATLS_RUNTIME_ID` (a test keeps the two equal). Every generated
/// header checks it, so a header and a runtime from different builds of `atlc` do not compile together.
pub fn runtime_id() -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (path, text) in RUNTIME.iter().filter(|(p, _)| *p != "atls/version.hpp") {
        for b in path.bytes().chain([0]).chain(text.bytes()).chain([0]) {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash >> 16
}

/// `ATLS_RUNTIME_ID` as `atls/version.hpp` and the generated headers write it.
pub fn runtime_id_text() -> String {
    format!("0x{:012x}", runtime_id())
}

/// Writes the runtime headers under `dir` (`dir/atls/ops.hpp`, ...), replacing any already there.
pub fn emit_runtime(dir: &Path) -> std::io::Result<()> {
    for (path, text) in RUNTIME {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `atls/version.hpp` states the identifier of the other headers; `BLESS=1` writes it again.
    #[test]
    fn version_header_states_the_runtime_id() {
        let line = format!("#define ATLS_RUNTIME_ID {}", runtime_id_text());
        let version = RUNTIME[2].1;
        if std::env::var_os("BLESS").is_some() && !version.contains(&line) {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/include/atls/version.hpp");
            let text: Vec<String> = version
                .lines()
                .map(|l| {
                    if l.starts_with("#define ATLS_RUNTIME_ID ") {
                        line.clone()
                    } else {
                        l.to_string()
                    }
                })
                .collect();
            std::fs::write(path, text.join("\n") + "\n").unwrap();
            return;
        }
        assert!(
            version.contains(&line),
            "runtime/include/atls/version.hpp must say `{line}` (run the tests with BLESS=1 to write it)"
        );
    }

    #[test]
    fn indicator_names_round_trip() {
        for m in [Indicators::Incremental, Indicators::Naive, Indicators::Exact] {
            assert_eq!(Indicators::from_name(m.name()), Some(m));
        }
        assert_eq!(Indicators::from_name("pairwise"), None);
    }
}
