//! Diagnostics as users see them: every `tests/ui/**/NAME.atls` is checked by the `atlc` binary, and its
//! stderr and its `--json` output must equal `NAME.stderr` and `NAME.json`. `BLESS=1 cargo test --test ui`
//! writes the expectations instead.
//!
//! `tests/ui/syntax/` holds programs refused by the parser, `tests/ui/check/` programs refused (or warned
//! about) by the checker. A file named after a diagnostic code shows that code.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use atlc::frontend::diag::Code;

/// Codes no program can produce today: kept so that a later change that makes them reachable reports
/// them under a stable name.
const UNREACHABLE: [Code; 2] = [
    // Every token the lexer reads as a number is a valid `f64`.
    Code::InvalidNumber,
    // Statements that assign run only where assigning a state is allowed, or in functions, which have none.
    Code::AssignStateOutsideStep,
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn programs(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            programs(&p, out);
        } else if p.extension().is_some_and(|e| e == "atls") {
            out.push(p);
        }
    }
}

/// Runs `atlc check` on `rel` (relative to the package, so messages name it the same on every machine).
fn run(rel: &str, json: bool) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_atlc"));
    cmd.current_dir(root()).arg("check").arg(rel);
    if json {
        cmd.arg("--json");
    }
    let out = cmd.output().expect("atlc runs");
    (
        out.status.code().expect("atlc exits"),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// The codes in a `.json` expectation, read without a JSON library: each line holds one `"code":"..."`
/// at the top level, before the message.
fn codes_in(json: &str) -> Vec<(String, String)> {
    json.lines()
        .map(|line| {
            let field = |key: &str| {
                let start = line.find(&format!("\"{key}\":\"")).unwrap() + key.len() + 4;
                line[start..start + line[start..].find('"').unwrap()].to_string()
            };
            (field("severity"), field("code"))
        })
        .collect()
}

#[test]
fn ui() {
    let bless = std::env::var_os("BLESS").is_some_and(|v| v == "1");
    let mut files = vec![];
    programs(&root().join("tests/ui"), &mut files);
    assert!(files.len() > 100, "found only {} programs", files.len());
    let mut failures = vec![];
    let mut seen = BTreeSet::new();
    for path in &files {
        let rel = path.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/");
        let (status, stdout, stderr) = run(&rel, false);
        let (json_status, json, json_stderr) = run(&rel, true);
        assert!(stdout.is_empty(), "{rel}: text mode wrote to stdout");
        assert!(json_stderr.is_empty(), "{rel}: --json wrote to stderr: {json_stderr}");
        assert_eq!(status, json_status, "{rel}: the exit code depends on --json");
        let codes = codes_in(&json);
        let errors = codes.iter().any(|(severity, _)| severity == "error");
        assert_eq!(
            status,
            if errors { 1 } else { 0 },
            "{rel}: exit code against the diagnostics"
        );
        assert!(!codes.is_empty(), "{rel}: no diagnostic");
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if let Some(code) = Code::ALL.iter().find(|c| c.as_str() == stem) {
            assert!(
                codes.iter().any(|(_, c)| c == code.as_str()),
                "{rel}: does not show `{stem}`"
            );
        }
        seen.extend(codes.into_iter().map(|(_, c)| c));
        for (ext, actual) in [("stderr", &stderr), ("json", &json)] {
            let expected_path = path.with_extension(ext);
            if bless {
                std::fs::write(&expected_path, actual).unwrap();
                continue;
            }
            let expected = std::fs::read_to_string(&expected_path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            if &expected != actual {
                failures.push(format!(
                    "{rel}: {ext} differs\n--- expected\n{expected}--- actual\n{actual}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}\n(BLESS=1 cargo test --test ui rewrites the expectations)",
        failures.join("\n")
    );
    // Every code is shown by some program, unless it cannot be reached.
    for code in Code::ALL {
        let shown = seen.contains(code.as_str());
        assert_eq!(shown, !UNREACHABLE.contains(code), "`{}`", code.as_str());
    }
}

#[test]
fn every_expectation_has_its_program() {
    let mut files = vec![];
    fn all(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                all(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    all(&root().join("tests/ui"), &mut files);
    for p in files {
        let ext = p.extension().unwrap().to_string_lossy().to_string();
        assert!(["atls", "stderr", "json"].contains(&ext.as_str()), "{}", p.display());
        assert!(p.with_extension("atls").exists(), "{} has no program", p.display());
    }
}
