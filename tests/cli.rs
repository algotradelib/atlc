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
        &["run", "sma.toml"],
        &["frobnicate"],
        &[],
    ] {
        let (code, out, err) = atlc(args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty() && !err.is_empty(), "{args:?}");
    }
    for (args, says) in [
        (&["run"][..], "expected a run spec"),
        (&["run", "x.toml", "y.toml"], "expected one run spec"),
        (&["run", "x.toml", "--set", "a=1"], "unknown option `--set`"),
        (&["run", "x.toml", "-P"], "`-P` needs a value"),
        (&["run", "x.toml", "-P", "lookback"], "-P takes name=value"),
        (
            &["run", "x.toml", "-P", "a=1", "-P", "a=2"],
            "`params.a` is given twice",
        ),
        (
            &["run", "x.toml", "--fills", "a", "--fills", "b"],
            "`--fills` is given twice",
        ),
        (&["run", "x.toml"], "cannot read"),
    ] {
        let (code, out, err) = atlc(args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty() && err.contains(says), "{args:?}: {err}");
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
