//! Deep programs through the built binary: refused with `nesting-too-deep` and exit code 1 past the limit,
//! accepted up to it, and never a stack overflow (which would abort the process). `cargo test --release
//! --test depth` runs the same tests against a release build.

use std::process::Command;

/// Checks `source` with the binary, by way of a file; returns the exit code (None for a signal) and stderr.
fn check(name: &str, source: &str) -> (Option<i32>, String) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("depth");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.atls"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    (out.status.code(), String::from_utf8(out.stderr).unwrap())
}

fn accepted(name: &str, source: &str) {
    let (code, err) = check(name, source);
    assert_eq!(code, Some(0), "{name}: {err}");
}

fn refused(name: &str, source: &str) {
    let (code, err) = check(name, source);
    assert_eq!(code, Some(1), "{name}: not refused normally (a signal is None): {err}");
    assert!(
        err.starts_with("error[nesting-too-deep]: "),
        "{name}: {}",
        &err[..err.len().min(300)]
    );
}

fn chain(terms: usize) -> String {
    format!("on_bar {{ target(1{}); }}", " + 1".repeat(terms - 1))
}

fn nested_ifs(n: usize) -> String {
    format!("on_bar {{ {} target(1); {} }}", "if (true) { ".repeat(n), "}".repeat(n))
}

#[test]
fn operator_chains() {
    // In `on_bar` (one level) and `target(...)` (one more), a chain of n terms is n levels deep.
    accepted("chain-254", &chain(254));
    refused("chain-255", &chain(255));
    refused("chain-20000", &chain(20_000));
    refused("chain-200000", &chain(200_000));
    refused(
        "lags-2000",
        &format!(
            "series {{ real a = close{}; }} on_bar {{ target(a); }}",
            "[1]".repeat(2000)
        ),
    );
}

#[test]
fn nested_blocks() {
    accepted("ifs-254", &nested_ifs(254));
    refused("ifs-255", &nested_ifs(255));
    refused("ifs-2000", &nested_ifs(2000));
    let else_ifs = |n| {
        format!(
            "on_bar {{ if (close > 1) {{ target(1); }}{} }}",
            " else if (close > 2) { target(2); }".repeat(n)
        )
    };
    accepted("else-ifs-200", &else_ifs(200));
    refused("else-ifs-2000", &else_ifs(2000));
}

#[test]
fn parentheses_calls_and_lookbacks() {
    refused(
        "parens-300",
        &format!("on_bar {{ target({}1{}); }}", "(".repeat(300), ")".repeat(300)),
    );
    refused(
        "calls-300",
        &format!("on_bar {{ target({}1{}); }}", "abs(".repeat(300), ")".repeat(300)),
    );
    let lookbacks = |n: usize| {
        let mut s = String::from("params { int<lower=2> k = 2; } series { real s0 = sma(close, k);");
        for i in 1..n {
            s += &format!(" real s{i} = sma(s{}, k);", i - 1);
        }
        s + &format!(" }} on_bar {{ target(s{}); }}", n - 1)
    };
    accepted("lookbacks-200", &lookbacks(200));
    refused("lookbacks-20000", &lookbacks(20_000));
}
