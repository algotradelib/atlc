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
        &["build", "tests/programs/mean-reversion.atls"],
        &["run", "sma.toml"],
        &["frobnicate"],
        &[],
    ] {
        let (code, out, err) = atlc(args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty() && !err.is_empty(), "{args:?}");
    }
    let (_, _, err) = atlc(&["build", "x.atls"]);
    assert!(err.contains("not implemented"), "{err}");
}
