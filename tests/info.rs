//! `atlc check --info`: every outcome read by a JSON parser, each a single object on standard output with nothing on
//! standard error.

#[path = "support/json.rs"]
mod json;

use std::path::{Path, PathBuf};
use std::process::Command;

use json::{Json, parse};

/// A fresh directory for one test's files.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("info").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `atlc check` with `args`, from `cwd`: the exit code and the one object it printed, after checking the envelope
/// every reply has.
fn info_in(cwd: &Path, args: &[&str]) -> (i32, Json) {
    let out = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .current_dir(cwd)
        .arg("check")
        .args(args)
        .output()
        .unwrap();
    let code = out.status.code().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.is_empty(), "{args:?} wrote to stderr: {stderr}");
    assert!(
        stdout.ends_with('\n') && stdout.matches('\n').count() == 1,
        "{args:?}: one line: {stdout}"
    );
    let v = parse(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    assert_eq!(v.keys(), ["atlc_info", "status", "atlc", "program", "diagnostics"]);
    assert_eq!(v.get("atlc_info"), &Json::Num(1.0));
    assert_eq!(v.get("atlc").str(), env!("CARGO_PKG_VERSION"));
    let diagnostics = v.get("diagnostics").arr();
    let failed = diagnostics.iter().any(|d| d.get("severity").str() == "error");
    assert_eq!(v.get("status").str(), if failed { "error" } else { "ok" }, "{v:?}");
    assert_eq!(v.get("program") == &Json::Null, failed, "{v:?}");
    for d in diagnostics {
        assert_eq!(d.keys(), ["severity", "code", "message", "file", "line", "origin"]);
        assert!(["error", "warning"].contains(&d.get("severity").str()), "{d:?}");
        match d.get("origin").str() {
            "program" => assert!(Path::new(d.get("file").str()).is_absolute(), "{d:?}"),
            "command_line" => {
                assert_eq!((d.get("file"), d.get("line")), (&Json::Null, &Json::Null), "{d:?}");
                assert_eq!(d.get("code").str(), "usage", "{d:?}");
            }
            other => panic!("origin {other}: {d:?}"),
        }
    }
    if let Json::Obj(_) = v.get("program") {
        let p = v.get("program");
        assert_eq!(p.keys(), ["path", "struct", "mode", "instruments", "legs", "params"]);
        assert!(Path::new(p.get("path").str()).is_absolute(), "{p:?}");
        for param in p.get("params").arr() {
            assert_eq!(param.keys(), ["name", "type"]);
            assert!(["int", "real", "bool"].contains(&param.get("type").str()), "{param:?}");
        }
    }
    assert!(
        if failed { code == 1 || code == 2 } else { code == 0 },
        "exit {code}: {v:?}"
    );
    (code, v)
}

/// `atlc check --info` with `args`, from the package's root.
fn info(args: &[&str]) -> (i32, Json) {
    let mut all = vec!["--info"];
    all.extend_from_slice(args);
    info_in(Path::new(env!("CARGO_MANIFEST_DIR")), &all)
}

/// The one diagnostic of a reply.
fn the_diagnostic(v: &Json) -> &Json {
    let d = v.get("diagnostics").arr();
    assert_eq!(d.len(), 1, "{v:?}");
    &d[0]
}

/// The names and types of a reply's parameters.
fn params(v: &Json) -> Vec<(String, String)> {
    v.get("program")
        .get("params")
        .arr()
        .iter()
        .map(|p| (p.get("name").str().to_string(), p.get("type").str().to_string()))
        .collect()
}

/// The names of a reply's instruments.
fn instruments(v: &Json) -> Vec<&str> {
    v.get("program")
        .get("instruments")
        .arr()
        .iter()
        .map(Json::str)
        .collect()
}

/// Whether `shown` names the same file as `expected`.
fn same_file(shown: &str, expected: &Path) -> bool {
    std::fs::canonicalize(shown).unwrap() == std::fs::canonicalize(expected).unwrap()
}

#[test]
fn a_single_instrument_program() {
    let dir = scratch("single");
    let file = dir.join("my-mean_rev2.atls");
    std::fs::write(
        &file,
        "params {\n  int<lower=2> lookback = 5;\n  real entry = 1.5;\n  bool flag = false;\n}\nseries {\n  \
         real m = sma(close, lookback);\n}\non_bar {\n  if (close < m - entry) {\n    target(flag ? -1 : 1);\n  }\n}\n",
    )
    .unwrap();
    // A relative path, from another directory.
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let (code, v) = info_in(&elsewhere, &["--info", "../my-mean_rev2.atls"]);
    assert_eq!(code, 0);
    let p = v.get("program");
    assert!(same_file(p.get("path").str(), &file), "{p:?}");
    assert_eq!(p.get("struct").str(), "MyMeanRev2");
    assert_eq!(p.get("mode").str(), "single");
    assert!(instruments(&v).is_empty());
    assert_eq!(p.get("legs"), &Json::Bool(false));
    assert_eq!(
        params(&v),
        [("lookback", "int"), ("entry", "real"), ("flag", "bool")].map(|(n, t)| (n.to_string(), t.to_string()))
    );
    assert!(v.get("diagnostics").arr().is_empty());
}

#[test]
fn named_instruments_in_order() {
    let (code, v) = info(&["examples/pairs.atls"]);
    assert_eq!(code, 0);
    let p = v.get("program");
    assert_eq!((p.get("struct").str(), p.get("mode").str()), ("Pairs", "named"));
    assert_eq!(instruments(&v), ["y", "x"]);
    assert_eq!(p.get("legs"), &Json::Bool(false));
    let dir = scratch("named");
    std::fs::write(
        dir.join("three.atls"),
        "instruments { c; a; b; }\non_bar {\n  target(c, 1);\n  target(a, 0);\n  target(b, -1);\n}\n",
    )
    .unwrap();
    // `--info` after the file.
    let (code, v) = info_in(&dir, &["three.atls", "--info"]);
    assert_eq!(code, 0);
    assert_eq!(instruments(&v), ["c", "a", "b"]);
    assert!(params(&v).is_empty());
}

#[test]
fn a_universe_with_legs() {
    let (code, v) = info(&["examples/universe.atls"]);
    assert_eq!(code, 0);
    let p = v.get("program");
    assert_eq!((p.get("struct").str(), p.get("mode").str()), ("Universe", "universe"));
    assert!(instruments(&v).is_empty());
    assert_eq!(p.get("legs"), &Json::Bool(true));
}

#[test]
fn warnings_keep_the_program() {
    let (code, v) = info(&["tests/ui/check/unused-series.atls"]);
    assert_eq!(code, 0);
    assert_eq!(v.get("status").str(), "ok");
    assert_eq!(v.get("program").get("struct").str(), "UnusedSeries");
    let d = the_diagnostic(&v);
    assert_eq!(
        (d.get("severity").str(), d.get("code").str()),
        ("warning", "unused-series")
    );
    assert!(same_file(
        d.get("file").str(),
        Path::new("tests/ui/check/unused-series.atls")
    ));
    assert_eq!((d.get("line"), d.get("origin").str()), (&Json::Num(2.0), "program"));
}

#[test]
fn errors_have_no_program() {
    let (code, v) = info(&["tests/ui/check/unknown-name.atls"]);
    assert_eq!(code, 1);
    let d = v.get("diagnostics").arr();
    assert!(d.len() >= 2, "{v:?}");
    assert_eq!(
        (d[0].get("code").str(), d[0].get("line")),
        ("unknown-name", &Json::Num(5.0))
    );
    assert!(same_file(
        d[0].get("file").str(),
        Path::new("tests/ui/check/unknown-name.atls")
    ));
    // A syntax error, and a removed `replay {}`.
    let (code, v) = info(&["tests/ui/syntax/replay-block.atls"]);
    assert_eq!(code, 1);
    let d = the_diagnostic(&v);
    assert_eq!((d.get("code").str(), d.get("line")), ("unknown-block", &Json::Num(7.0)));
}

#[test]
fn a_file_name_no_struct_can_take() {
    let dir = scratch("name");
    std::fs::write(dir.join("1x.atls"), "on_bar { target(1); }\n").unwrap();
    std::fs::write(dir.join("2x.atls"), "on_bar { target(y); }\n").unwrap();
    for name in ["1x.atls", "2x.atls"] {
        let (code, v) = info_in(&dir, &["--info", name]);
        assert_eq!(code, 2, "{name}");
        let d = the_diagnostic(&v);
        assert_eq!(
            (d.get("code").str(), d.get("origin").str()),
            ("program-name", "program")
        );
        assert!(same_file(d.get("file").str(), &dir.join(name)));
        assert_eq!(d.get("line"), &Json::Null);
    }
    // Without `--info`, the file name is not checked.
    let out = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .current_dir(&dir)
        .args(["check", "1x.atls"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn an_unreadable_file() {
    let dir = scratch("unreadable");
    let (code, v) = info_in(&dir, &["--info", "no-such.atls"]);
    assert_eq!(code, 2);
    let d = the_diagnostic(&v);
    assert_eq!((d.get("code").str(), d.get("origin").str()), ("unreadable", "program"));
    let shown = Path::new(d.get("file").str());
    assert!(shown.is_absolute() && shown.ends_with("no-such.atls"), "{d:?}");
    assert!(same_file(shown.parent().unwrap().to_str().unwrap(), &dir), "{d:?}");
    assert_eq!(d.get("line"), &Json::Null);
    // A directory is not a program either.
    std::fs::create_dir_all(dir.join("d.atls")).unwrap();
    let (code, v) = info_in(&dir, &["--info", "d.atls"]);
    assert_eq!((code, the_diagnostic(&v).get("code").str()), (2, "unreadable"));
}

#[test]
fn usage_errors() {
    let program = "examples/pairs.atls";
    for (args, says) in [
        (&[][..], "expected one file, got 0"),
        (&[program, "examples/universe.atls"], "expected one file, got 2"),
        (&["--color", program], "unknown option `--color`"),
        (&[program, "--info"], "`--info` is given twice"),
        (&[program, "--json"], "`--info` and `--json`"),
        (&["--json", program], "`--info` and `--json`"),
        (&[program, "--help"], "usage: atlc"),
        (&["-h", program], "usage: atlc"),
    ] {
        let (code, v) = info(args);
        assert_eq!(code, 2, "{args:?}");
        let d = the_diagnostic(&v);
        assert_eq!((d.get("code").str(), d.get("origin").str()), ("usage", "command_line"));
        assert!(d.get("message").str().contains(says), "{args:?}: {d:?}");
    }
}
