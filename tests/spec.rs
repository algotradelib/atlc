//! `atlc spec`: every outcome read by a JSON parser, the layers and paths it resolves, and that it builds nothing,
//! with no compiler, CMake, DuckDB or cache on hand.

#[path = "support/json.rs"]
mod json;

use std::path::{Path, PathBuf};
use std::process::Command;

use json::{Json, parse};

/// A fresh directory for one test's files.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("spec").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// What `atlc spec` gave.
struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

/// `atlc spec` with arguments, from `cwd`.
fn spec_in(cwd: &Path, args: &[&str]) -> Out {
    let out = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .current_dir(cwd)
        .arg("spec")
        .args(args)
        .output()
        .unwrap();
    Out {
        code: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

/// `atlc spec --json`: the exit code and the one object it printed, after checking the envelope every reply has.
fn spec_json(cwd: &Path, args: &[&str]) -> (i32, Json) {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let out = spec_in(cwd, &all);
    assert!(out.stderr.is_empty(), "--json wrote to stderr: {}", out.stderr);
    assert_eq!(out.stdout.matches('\n').count(), 1, "one line: {}", out.stdout);
    let v = parse(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", out.stdout));
    assert_eq!(
        v.keys(),
        [
            "atlc_spec",
            "status",
            "atlc",
            "program",
            "build",
            "store",
            "orders",
            "args",
            "warnings",
            "errors"
        ]
    );
    assert_eq!(v.get("atlc_spec"), &Json::Num(1.0));
    assert_eq!(v.get("atlc").str(), env!("CARGO_PKG_VERSION"));
    let errors = v.get("errors").arr();
    assert_eq!(v.get("status").str(), if errors.is_empty() { "ok" } else { "error" });
    for d in errors.iter().chain(v.get("warnings").arr()) {
        assert_eq!(d.keys(), ["severity", "code", "message", "file", "line", "origin"]);
        assert!(
            ["file", "command_line", "program"].contains(&d.get("origin").str()),
            "{d:?}"
        );
        // A place is a file and maybe a line, never a line alone.
        assert!(d.get("file") != &Json::Null || d.get("line") == &Json::Null, "{d:?}");
    }
    let resolved = ["program", "build", "store", "orders", "args"];
    for k in resolved {
        assert_eq!(v.get(k) == &Json::Null, !errors.is_empty(), "{k}: {v:?}");
    }
    assert_eq!(out.code == 0, errors.is_empty());
    (out.code, v)
}

/// The `args` pairs as `key=value` strings, each pair one argument.
fn args_of(v: &Json) -> Vec<String> {
    v.get("args")
        .arr()
        .iter()
        .map(|pair| {
            let [k, value] = pair.arr() else {
                panic!("not a pair: {pair:?}");
            };
            format!("{}={}", k.str(), value.str())
        })
        .collect()
}

/// The one error of a reply.
fn the_error(v: &Json) -> &Json {
    let errors = v.get("errors").arr();
    assert_eq!(errors.len(), 1, "{v:?}");
    &errors[0]
}

const PROGRAM: &str = "params {\n  int<lower=2> lookback = 5;\n  bool flag = false;\n}\nseries {\n  real m = sma(close, lookback);\n}\non_bar {\n  if (close < m) {\n    target(flag ? -1 : 1);\n  }\n}\n";

#[test]
fn the_reader_is_strict() {
    assert_eq!(
        parse(r#" {"a": [1, -2.5e3, true, null], "b": "\u00e9\ud83d\ude00\n"} "#).unwrap(),
        Json::Obj(vec![
            (
                "a".into(),
                Json::Arr(vec![Json::Num(1.0), Json::Num(-2500.0), Json::Bool(true), Json::Null])
            ),
            ("b".into(), Json::Str("é\u{1F600}\n".into())),
        ])
    );
    for bad in [
        "",
        "{",
        "[1,]",
        "{\"a\":1,}",
        "01",
        "1.",
        "\"\t\"",
        "\"\\x\"",
        "\"\\ud83d\"",
        "{} {}",
        "{\"a\":1,\"a\":2}",
        "nul",
    ] {
        assert!(parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_valid_spec_gives_its_settings() {
    let dir = scratch("valid");
    std::fs::write(dir.join("m.atls"), PROGRAM).unwrap();
    std::fs::write(
        dir.join("s.toml"),
        "strategy = 'm.atls'\n[data]\nsource = \"csv\"\nfile = 'bars.csv'\nsymbol = \"A\"\n[replay]\nshorts = true\n\
         [params]\nflag = true\n[orders]\ntype = \"limit\"\noffset = 0.299999999999999999\n[build]\nindicators = \"exact\"\n",
    )
    .unwrap();
    let (code, v) = spec_json(&dir, &["s.toml", "-P", "lookback=7"]);
    assert_eq!(code, 0);
    let program = dir.join("m.atls");
    assert_eq!(v.get("program").get("path").str(), program.display().to_string());
    assert_eq!(v.get("program").get("struct").str(), "M");
    let build = v.get("build");
    assert_eq!(build.get("algotradelib"), &Json::Null);
    assert_eq!(build.get("duckdb"), &Json::Null);
    assert_eq!(build.get("indicators").str(), "exact");
    assert_eq!(v.get("store"), &Json::Bool(false));
    assert_eq!(v.get("orders").get("type").str(), "limit");
    assert_eq!(v.get("orders").get("offset").str(), "0.299999999999999999");
    // Overrides only, in order, each pair one `key=value`: a setting's boolean as `true`, a parameter's as `1`.
    assert_eq!(
        args_of(&v),
        [
            "data.source=csv".to_string(),
            format!("data.file={}", dir.join("bars.csv").display()),
            "data.symbol=A".into(),
            "replay.shorts=true".into(),
            "orders.type=limit".into(),
            "orders.offset=0.299999999999999999".into(),
            "param.flag=1".into(),
            "param.lookback=7".into(),
        ]
    );
    // The text form says the same, and puts nothing on stderr.
    let out = spec_in(&dir, &["s.toml", "-P", "lookback=7"]);
    assert_eq!((out.code, out.stderr.as_str()), (0, ""));
    assert!(out.stdout.contains("(atls::M)") && out.stdout.contains("orders: limit, offset 0.299999999999999999"));
    assert!(out.stdout.contains("  param.flag=1\n") && out.stdout.contains("  replay.shorts=true\n"));
    // Without [orders]: the defaults, and no argument.
    std::fs::write(dir.join("plain.toml"), "strategy = 'm.atls'\n[data]\nfile = 'c.csv'\n").unwrap();
    let (_, v) = spec_json(&dir, &["plain.toml"]);
    assert_eq!(v.get("orders").get("type").str(), "market");
    assert_eq!(v.get("orders").get("offset").str(), "0");
    assert!(args_of(&v).iter().all(|a| !a.starts_with("orders.")));
}

#[test]
fn warnings_alone_keep_the_status_ok() {
    let dir = scratch("warnings");
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui/check/unused-series.atls");
    std::fs::write(
        dir.join("s.toml"),
        format!("strategy = '{}'\n[data]\nfile = 'c.csv'\n", program.display()),
    )
    .unwrap();
    let (code, v) = spec_json(&dir, &["s.toml"]);
    assert_eq!(code, 0);
    let warnings = v.get("warnings").arr();
    assert_eq!(warnings.len(), 1, "{v:?}");
    let w = &warnings[0];
    assert_eq!(
        (w.get("severity").str(), w.get("code").str(), w.get("origin").str()),
        ("warning", "unused-series", "program")
    );
    assert_eq!(w.get("file").str(), program.display().to_string());
    assert_eq!(w.get("line"), &Json::Num(2.0));
    let out = spec_in(&dir, &["s.toml"]);
    assert_eq!(out.code, 0);
    assert!(out.stderr.starts_with("warning[unused-series]:"), "{}", out.stderr);
}

#[test]
fn every_failure_is_one_error_object() {
    let dir = scratch("failures");
    std::fs::write(dir.join("m.atls"), PROGRAM).unwrap();
    let spec = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    let wide = "strategy = 'm.atls'\n[data]\nfile = 'c.csv'\n";

    // Spec errors, each at its file and line.
    spec("bad.toml", &format!("{wide}[replay]\ncash = 0\nfill = \"close\"\n"));
    let (code, v) = spec_json(&dir, &["bad.toml"]);
    assert_eq!(code, 2);
    let errors = v.get("errors").arr();
    assert_eq!(errors.len(), 2, "{v:?}");
    for (e, line) in errors.iter().zip([5.0, 6.0]) {
        assert_eq!((e.get("code").str(), e.get("origin").str()), ("spec", "file"));
        assert_eq!(e.get("file").str(), dir.join("bad.toml").display().to_string());
        assert_eq!(e.get("line"), &Json::Num(line));
    }
    assert_eq!(
        errors[0].get("message").str(),
        "`replay.cash` is a number above 0, not 0"
    );
    // A setting no layer gives has no place.
    spec("missing.toml", "strategy = 'm.atls'\n[data]\nsource = \"csv\"\n");
    let (code, v) = spec_json(&dir, &["missing.toml"]);
    let e = the_error(&v);
    assert_eq!(code, 2);
    assert_eq!(
        (e.get("file"), e.get("line"), e.get("origin").str()),
        (&Json::Null, &Json::Null, "file")
    );

    // `-P`: malformed, repeated, or naming no parameter; none has a file or line.
    spec("ok.toml", wide);
    for (args, code, says) in [
        (&["-P", "lookback"][..], "usage", "-P takes name=value"),
        (&["-P", "a=1", "-P", "a=2"], "usage", "`params.a` is given twice"),
        (&["-P"], "usage", "`-P` needs a value"),
        (&["-P", "nope=1"], "spec", "the program has no parameter `nope`"),
    ] {
        let mut all = vec!["ok.toml"];
        all.extend_from_slice(args);
        let (exit, v) = spec_json(&dir, &all);
        assert_eq!(exit, 2, "{args:?}");
        let e = the_error(&v);
        assert_eq!(e.get("code").str(), code, "{args:?}");
        assert_eq!(e.get("origin").str(), "command_line");
        assert_eq!((e.get("file"), e.get("line")), (&Json::Null, &Json::Null));
        assert!(e.get("message").str().contains(says), "{args:?}: {e:?}");
    }

    // Usage: an unknown option, no spec, two specs, or a help request.
    for (args, says) in [
        (&["s.toml", "--bogus"][..], "unknown option `--bogus`"),
        (&[], "expected a run spec"),
        (&["a.toml", "b.toml"], "expected one run spec"),
    ] {
        let (code, v) = spec_json(&dir, args);
        assert_eq!(code, 2);
        let e = the_error(&v);
        assert_eq!((e.get("code").str(), e.get("origin").str()), ("usage", "command_line"));
        assert!(e.get("message").str().contains(says), "{args:?}: {e:?}");
        let out = spec_in(&dir, args);
        assert_eq!(out.code, 2);
        assert!(
            out.stdout.is_empty() && out.stderr.contains(says),
            "{args:?}: {}",
            out.stderr
        );
    }

    // Files: no spec, no program.
    let (code, v) = spec_json(&dir, &["no-such.toml"]);
    assert_eq!(code, 2);
    let e = the_error(&v);
    assert_eq!(e.get("code").str(), "unreadable");
    assert!(e.get("message").str().contains("cannot read"), "{e:?}");
    spec("lost.toml", "strategy = 'gone.atls'\n[data]\nfile = 'c.csv'\n");
    let (code, v) = spec_json(&dir, &["lost.toml"]);
    assert_eq!(code, 2);
    let e = the_error(&v);
    assert_eq!(e.get("code").str(), "unreadable");
    assert_eq!(e.get("file").str(), dir.join("lost.toml").display().to_string());
    assert_eq!(e.get("line"), &Json::Null);
    // A program file named so that no C++ struct can be named after it.
    std::fs::write(dir.join("2x.atls"), PROGRAM).unwrap();
    spec("name.toml", "strategy = '2x.atls'\n[data]\nfile = 'c.csv'\n");
    let (code, v) = spec_json(&dir, &["name.toml"]);
    assert_eq!((code, the_error(&v).get("code").str()), (2, "program-name"));

    // The program's errors: exit 1, at their line, from the program.
    let broken = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui/check/unknown-name.atls");
    spec(
        "broken.toml",
        &format!("strategy = '{}'\n[data]\nfile = 'c.csv'\n", broken.display()),
    );
    let (code, v) = spec_json(&dir, &["broken.toml"]);
    assert_eq!(code, 1);
    let errors = v.get("errors").arr();
    assert!(!errors.is_empty());
    for e in errors {
        assert_eq!((e.get("severity").str(), e.get("origin").str()), ("error", "program"));
        assert_eq!(e.get("file").str(), broken.display().to_string());
        assert!(matches!(e.get("line"), Json::Num(n) if *n >= 1.0), "{e:?}");
    }
    assert_eq!(errors[0].get("code").str(), "unknown-name");
    let out = spec_in(&dir, &["broken.toml"]);
    assert!(out.code == 1 && out.stdout.is_empty() && out.stderr.starts_with("error[unknown-name]:"));

    // A build path CMake cannot take, at its setting.
    spec("path.toml", &format!("{wide}[build]\nalgotradelib = '/opt/a;b'\n"));
    let (code, v) = spec_json(&dir, &["path.toml"]);
    let e = the_error(&v);
    assert_eq!(code, 2);
    assert_eq!(e.get("line"), &Json::Num(5.0));
    assert!(e.get("message").str().contains("cannot be passed to CMake"), "{e:?}");

    // Offsets: an exponent is refused, the largest 18-digit fraction below 0.3 is kept as written.
    spec(
        "exponent.toml",
        &format!("{wide}[orders]\ntype = \"limit\"\noffset = 2e-3\n"),
    );
    let (code, v) = spec_json(&dir, &["exponent.toml"]);
    assert_eq!(code, 2);
    assert!(
        the_error(&v).get("message").str().contains("without an exponent"),
        "{v:?}"
    );
    spec(
        "edge.toml",
        &format!("{wide}[orders]\ntype = \"limit\"\noffset = 0.299999999999999999\n"),
    );
    let (code, v) = spec_json(&dir, &["edge.toml"]);
    assert_eq!(code, 0);
    assert_eq!(v.get("orders").get("offset").str(), "0.299999999999999999");
}

#[test]
fn layers_and_paths_resolve_from_anywhere() {
    let root = scratch("layers");
    // Spaces, quotes, non-ASCII and (on Unix) a backslash in the directories' names.
    let odd = if cfg!(windows) {
        "a dir é 한"
    } else {
        "a dir \"q\" é 한 back\\slash"
    };
    let programs = root.join(odd).join("programs");
    let specs = root.join(odd).join("specs");
    let elsewhere = root.join("elsewhere");
    for d in [&programs, &specs, &elsewhere] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(
        programs.join("layered.atls"),
        format!(
            "replay {{\n  data.file = \"closes.csv\";\n  replay.cash = 1;\n  replay.commission = 0.1;\n  orders.offset = 0.002;\n}}\n{PROGRAM}"
        ),
    )
    .unwrap();
    std::fs::write(
        specs.join("s.toml"),
        "strategy = '../programs/layered.atls'\n[data]\nopen = 'opens.csv'\n[replay]\ncash = 2\n[params]\nlookback = 10\n\
         [orders]\ntype = \"limit\"\n",
    )
    .unwrap();
    let relative = Path::new("..").join(odd).join("specs").join("s.toml");
    let relative = relative.to_str().unwrap();
    let (code, v) = spec_json(&elsewhere, &[relative, "-P", "lookback=20"]);
    assert_eq!(code, 0, "{v:?}");
    let program = programs.join("layered.atls");
    let shown = v.get("program").get("path").str();
    assert_eq!(
        std::fs::canonicalize(shown).unwrap(),
        std::fs::canonicalize(&program).unwrap()
    );
    assert!(Path::new(shown).is_absolute());
    let args = args_of(&v);
    let arg = |key: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("no {key} in {args:?}"))
            .to_string()
    };
    // replay {} < spec < -P; each path against the file that sets it.
    assert_eq!(arg("replay.cash"), "2");
    assert_eq!(arg("replay.commission"), "0.1");
    assert_eq!(arg("param.lookback"), "20");
    assert_eq!(arg("orders.type"), "limit");
    assert_eq!(arg("orders.offset"), "0.002");
    for (key, dir, file) in [
        ("data.file", &programs, "closes.csv"),
        ("data.open", &specs, "opens.csv"),
    ] {
        let p = PathBuf::from(arg(key));
        assert!(p.is_absolute(), "{key}: {}", p.display());
        assert_eq!(p.file_name().unwrap(), file);
        assert_eq!(
            std::fs::canonicalize(p.parent().unwrap()).unwrap(),
            std::fs::canonicalize(dir).unwrap(),
            "{key}"
        );
    }
    assert!(arg("data.file").contains(odd), "{}", arg("data.file"));
}

#[test]
fn nothing_is_built_and_no_toolchain_is_needed() {
    let dir = scratch("toolchain");
    std::fs::write(dir.join("m.atls"), PROGRAM).unwrap();
    // A store run, whose data, store and DuckDB do not exist: nothing is opened.
    std::fs::write(
        dir.join("s.toml"),
        "strategy = 'm.atls'\n[data]\nstore = \"kis-krx/1d/005930\"\nstore_root = 'no-store'\n\
         [build]\nalgotradelib = 'no-prefix'\nduckdb = 'no-duckdb'\n",
    )
    .unwrap();
    let empty = dir.join("empty-path");
    std::fs::create_dir_all(&empty).unwrap();
    let cache = dir.join("cache");
    let out = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .current_dir(&dir)
        .args(["spec", "s.toml", "--json"])
        .env("PATH", &empty)
        .env("ATLC_CACHE_DIR", &cache)
        .env_remove("CXX")
        .env_remove("DUCKDB_ROOT")
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{text}");
    let v = parse(&text).unwrap();
    assert_eq!(v.get("store"), &Json::Bool(true));
    assert_eq!(
        v.get("build").get("duckdb").str(),
        dir.join("no-duckdb").display().to_string()
    );
    assert!(!cache.exists(), "the cache was touched");
    // `atlc run` from the same place needs what `atlc spec` did not: it stops at the compiler or CMake.
    let run = Command::new(env!("CARGO_BIN_EXE_atlc"))
        .current_dir(&dir)
        .args(["run", "s.toml"])
        .env("PATH", &empty)
        .env("ATLC_CACHE_DIR", &cache)
        .env_remove("CXX")
        .output()
        .unwrap();
    assert_ne!(run.status.code(), Some(0));
}
