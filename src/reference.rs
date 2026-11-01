//! The language reference, compiled into the binary so that `atlc reference` prints the reference of the atlc
//! that runs, whatever installed it.

/// The reference's index, `docs/reference/README.md`.
pub const INDEX: &str = include_str!("../docs/reference/README.md");

/// Each chapter of `docs/reference/`, by its file stem.
pub const CHAPTERS: [(&str, &str); 9] = [
    ("builtins", include_str!("../docs/reference/builtins.md")),
    (
        "functions-and-indicators",
        include_str!("../docs/reference/functions-and-indicators.md"),
    ),
    ("generated-code", include_str!("../docs/reference/generated-code.md")),
    ("instruments", include_str!("../docs/reference/instruments.md")),
    ("missing-values", include_str!("../docs/reference/missing-values.md")),
    (
        "program-structure",
        include_str!("../docs/reference/program-structure.md"),
    ),
    ("run-spec", include_str!("../docs/reference/run-spec.md")),
    ("syntax", include_str!("../docs/reference/syntax.md")),
    (
        "types-and-definedness",
        include_str!("../docs/reference/types-and-definedness.md"),
    ),
];

/// A chapter's text by its name, or `None` for a name that is not a chapter.
pub fn chapter(name: &str) -> Option<&'static str> {
    CHAPTERS.iter().find(|(n, _)| *n == name).map(|(_, text)| *text)
}
