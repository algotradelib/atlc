//! The command line: exit codes and where output goes.

use std::process::Command;

fn atlc(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn version() {
    let (code, out, _) = atlc(&["--version"]);
    assert_eq!(
        (code, out.as_str()),
        (0, concat!("atlc ", env!("CARGO_PKG_VERSION"), "\n"))
    );
}

#[test]
fn a_valid_program_prints_nothing() {
    for args in [
        &["check", "tests/programs/mean-reversion.atls"][..],
        &["check", "--json", "tests/programs/mean-reversion.atls"],
    ] {
        assert_eq!(atlc(args), (0, String::new(), String::new()), "{args:?}");
    }
}

#[test]
fn warnings_keep_exit_zero() {
    let (code, out, err) = atlc(&["check", "tests/ui/check/unused-series.atls"]);
    assert_eq!(code, 0);
    assert!(out.is_empty() && err.starts_with("warning[unused-series]:"), "{err}");
}

#[test]
fn errors_exit_one() {
    let (code, out, err) = atlc(&["check", "tests/ui/check/unknown-name.atls"]);
    assert_eq!(code, 1);
    assert!(out.is_empty() && err.starts_with("error[unknown-name]:"), "{err}");
    let (code, out, err) = atlc(&["check", "tests/ui/check/unknown-name.atls", "--json"]);
    assert_eq!(code, 1);
    assert!(
        err.is_empty() && out.starts_with("{\"severity\":\"error\",\"code\":\"unknown-name\""),
        "{out}"
    );
}

#[test]
fn usage_and_file_errors_exit_two() {
    for args in [
        &["check"][..],
        &["check", "a.atls", "b.atls"],
        &["check", "--color", "tests/programs/mean-reversion.atls"],
        &["check", "no/such/file.atls"],
        &["check", "--json", "no/such/file.atls"],
        &["build"],
        &["build", "no/such/file.atls"],
        // The prototype's options that were not carried over.
        &[
            "build",
            "tests/programs/mean-reversion.atls",
            "--indicators",
            "pairwise",
        ],
        &["build", "tests/programs/mean-reversion.atls", "--submit", "changes"],
        &["build", "tests/programs/mean-reversion.atls", "--params", "constexpr"],
        &["build", "tests/programs/mean-reversion.atls", "--checks", "all"],
        &["build", "tests/programs/mean-reversion.atls", "--share", "yes"],
        &[
            "build",
            "tests/programs/mean-reversion.atls",
            "-o",
            "a.hpp",
            "-o",
            "b.hpp",
        ],
        &["build", "tests/programs/mean-reversion.atls", "--history"],
        &["frobnicate"],
        &[],
    ] {
        let (code, out, err) = atlc(args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty() && !err.is_empty(), "{args:?}");
    }
}

#[test]
fn run_and_spec_are_unknown_commands() {
    for args in [
        &["run"][..],
        &["run", "examples/mean-reversion.toml"],
        &["spec", "examples/mean-reversion.toml", "--json"],
    ] {
        let (code, out, err) = atlc(args);
        assert_eq!((code, out.as_str()), (2, ""), "{args:?}");
        assert!(err.starts_with("usage: atlc <command>"), "{args:?}: {err}");
        assert!(!err.contains("run SPEC") && !err.contains("spec SPEC"), "{err}");
    }
}

#[test]
fn build_prints_the_header_or_writes_it() {
    let (code, out, err) = atlc(&["build", "tests/programs/mean-reversion.atls"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.contains("struct MeanReversion {"), "{out}");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-build");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("mean-reversion.hpp");
    let target = target.to_str().unwrap();
    let (code, written, err) = atlc(&["build", "tests/programs/mean-reversion.atls", "-o", target]);
    assert_eq!((code, written.as_str(), err.as_str()), (0, "", ""));
    assert_eq!(std::fs::read_to_string(target).unwrap(), out);
    // A program with errors writes nothing.
    let (code, out, err) = atlc(&["build", "tests/ui/check/unknown-name.atls"]);
    assert_eq!(code, 1);
    assert!(out.is_empty() && err.starts_with("error[unknown-name]:"), "{err}");
    // A file name that cannot name a C++ struct.
    let bad = dir.join("2x.atls");
    std::fs::copy("tests/programs/mean-reversion.atls", &bad).unwrap();
    let (code, out, err) = atlc(&["build", bad.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(out.is_empty() && err.contains("start with a letter"), "{err}");
}

#[test]
fn emit_runtime_writes_every_header() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-runtime");
    let _ = std::fs::remove_dir_all(&dir);
    let (code, out, err) = atlc(&["build", "--emit-runtime", dir.to_str().unwrap()]);
    assert_eq!((code, out.as_str(), err.as_str()), (0, "", ""));
    for (path, text) in atlc::backend::RUNTIME {
        assert_eq!(std::fs::read_to_string(dir.join(path)).unwrap(), text, "{path}");
    }
    // With a program that has errors, nothing is written, the runtime included.
    let other = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-runtime-refused");
    let _ = std::fs::remove_dir_all(&other);
    let (code, _, _) = atlc(&[
        "build",
        "tests/ui/check/unknown-name.atls",
        "--emit-runtime",
        other.to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert!(!other.exists());
}

#[test]
fn reference_prints_the_index_and_each_chapter_unchanged() {
    let (code, out, _) = atlc(&["reference"]);
    assert_eq!(code, 0);
    assert_eq!(out, std::fs::read_to_string("docs/reference/README.md").unwrap());
    for (name, _) in atlc::reference::CHAPTERS {
        let (code, out, err) = atlc(&["reference", name]);
        assert_eq!((code, err.as_str()), (0, ""), "{name}");
        assert_eq!(
            out,
            std::fs::read_to_string(format!("docs/reference/{name}.md")).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn reference_refuses_an_unknown_chapter_with_one_and_a_bad_call_with_two() {
    for name in ["indicators", "", "README", "../README", "diagnostics", "Syntax"] {
        let (code, out, err) = atlc(&["reference", name]);
        assert_eq!((code, out.as_str()), (1, ""), "{name}");
        assert!(err.contains(&format!("no chapter `{name}`")), "{err}");
        assert_eq!(err.lines().count(), 1, "{err}");
        for (chapter, _) in atlc::reference::CHAPTERS {
            assert!(err.contains(chapter), "{err}");
        }
    }
    for args in [
        &["reference", "syntax", "builtins"][..],
        &["reference", "--json"][..],
        &["reference", "-h"][..],
        &["reference", "-"][..],
        &["reference", "--", "syntax"][..],
    ] {
        let (code, out, err) = atlc(args);
        assert_eq!((code, out.as_str()), (2, ""), "{args:?}");
        assert!(err.starts_with("atlc reference: "), "{err}");
    }
}

#[test]
fn every_reference_file_is_a_chapter_and_every_chapter_a_file() {
    for (name, _) in atlc::reference::CHAPTERS {
        assert!(atlc::reference::is_chapter_name(name), "{name}");
    }
    for name in ["", "-h", "1st", "Syntax", "a_b", "../x"] {
        assert!(!atlc::reference::is_chapter_name(name), "{name}");
    }
    let entries: Vec<std::fs::DirEntry> = std::fs::read_dir("docs/reference")
        .unwrap()
        .map(|e| e.unwrap())
        .collect();
    // A chapter in a subdirectory would be read by the program tests but never embedded.
    for e in &entries {
        assert!(e.file_type().unwrap().is_file(), "{:?}", e.path());
    }
    let mut files: Vec<String> = entries
        .iter()
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|f| f.ends_with(".md") && f != "README.md")
        .map(|f| f.trim_end_matches(".md").to_string())
        .collect();
    files.sort();
    let mut chapters: Vec<String> = atlc::reference::CHAPTERS.iter().map(|(n, _)| n.to_string()).collect();
    chapters.sort();
    assert_eq!(files, chapters);
}
