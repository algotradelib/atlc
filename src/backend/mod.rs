//! The C++ back end: the printer (one strategy struct per program) and the runtime headers it calls, compiled
//! into the binary so that `atlc` always carries the runtime it generates code for.

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

/// The strategy struct's name for a file stem: the stem in CamelCase (`mean-reversion` gives `MeanReversion`), or why
/// it cannot name one (it must start with a letter, and must not make `Params` or `Lane`).
pub fn struct_name(stem: &str) -> Result<String, String> {
    let name: String = stem
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect();
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) || name == "Params" || name == "Lane" {
        return Err(format!(
            "cannot name the strategy struct after `{stem}`: the file name must start with a letter, and must not make \
             `Params` or `Lane`"
        ));
    }
    Ok(name)
}

/// The C++ header of a checked program.
pub fn print(ir: &Ir, options: &Options) -> String {
    cpp::print(ir, options)
}

/// The runtime headers, by path under an include directory, as compiled into this binary: the headers the code it
/// generates is written for.
pub const RUNTIME: [(&str, &str); 4] = [
    ("atls/args.hpp", include_str!("../../runtime/include/atls/args.hpp")),
    ("atls/ops.hpp", include_str!("../../runtime/include/atls/ops.hpp")),
    ("atls/run.hpp", include_str!("../../runtime/include/atls/run.hpp")),
    (
        "atls/universe.hpp",
        include_str!("../../runtime/include/atls/universe.hpp"),
    ),
];

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

    #[test]
    fn indicator_names_round_trip() {
        for m in [Indicators::Incremental, Indicators::Naive, Indicators::Exact] {
            assert_eq!(Indicators::from_name(m.name()), Some(m));
        }
        assert_eq!(Indicators::from_name("pairwise"), None);
    }
}
