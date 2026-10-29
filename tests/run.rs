//! `atlc run` against an installed algotradelib: a CSV run checked against a replay computed here, the build cache,
//! the library search, and Parquet store runs.
//!
//! They need CMake and a C++17 compiler, an algotradelib installed with its CMake package, whose prefix
//! `ATLC_TEST_PREFIX` names, and for the store test DuckDB's C library, whose directory `ATLC_TEST_DUCKDB` names;
//! without them they pass with a message and run nothing. `ATLC_TEST_SANITIZE=1` also replays the CSV run built with
//! AddressSanitizer and UndefinedBehaviorSanitizer. The caches are under the system's temporary directory, outside
//! any checkout.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// An environment variable the tests need, or `None` after saying the test is skipped.
fn needs(var: &str, test: &str, what: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => {
            // Straight to the terminal: the test harness captures `eprintln!` of a passing test.
            let _ = writeln!(std::io::stderr(), "\n{test}: skipped, set {var} to {what} to run it");
            None
        }
    }
}

fn prefix(test: &str) -> Option<PathBuf> {
    needs("ATLC_TEST_PREFIX", test, "an algotradelib installation prefix")
}

/// A fresh directory for one test's files.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("run").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fresh directory outside any checkout, for a test's cache.
fn outside(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("atlc-run-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A path for a single-quoted TOML string.
fn toml_path(p: &Path) -> String {
    format!("'{}'", p.display())
}

/// What a run gave.
struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    fn built(&self) -> bool {
        self.stderr.contains("atlc: building")
    }

    fn ok(self) -> Out {
        assert_eq!(self.code, 0, "{}", self.stderr);
        self
    }
}

/// `atlc run` with arguments, from `cwd`, with the cache in `cache`, without the variables that steer CMake's search
/// except those in `env`.
fn atlc_run(cwd: &Path, cache: &Path, args: &[&str], env: &[(&str, &str)]) -> Out {
    let mut c = Command::new(env!("CARGO_BIN_EXE_atlc"));
    c.current_dir(cwd).arg("run").args(args).env("ATLC_CACHE_DIR", cache);
    for v in [
        "CMAKE_PREFIX_PATH",
        "algotradelib_DIR",
        "algotradelib_ROOT",
        "ALGOTRADELIB_ROOT",
        "DUCKDB_ROOT",
    ] {
        c.env_remove(v);
    }
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().unwrap();
    Out {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A field of the report, as written (a string keeps its quotes).
fn field(json: &str, name: &str) -> String {
    let key = format!("\"{name}\": ");
    let at = json.find(&key).unwrap_or_else(|| panic!("no {name} in {json}")) + key.len();
    let rest = &json[at..];
    rest[..rest.find([',', '\n']).unwrap_or(rest.len())].to_string()
}

/// Copies a directory tree.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), &target).unwrap();
        }
    }
}

/// Sets a file's modification time a day later than it was: a change at the same path.
fn touch(path: &Path) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    let was = f.metadata().unwrap().modified().unwrap();
    f.set_modified(was + Duration::from_secs(86400)).unwrap();
}

/// One bar of the generated series.
#[derive(Clone, Copy)]
struct Bar {
    day: i64,
    open: i64,
    close: i64,
}

/// A deterministic walk of whole-number prices (sums stay exact), 2020-01-01 onward, one bar a day.
fn walk(days: i64) -> Vec<Bar> {
    let mut state: u64 = 7;
    let mut next = |spread: i64| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) % (2 * spread as u64 + 1)) as i64 - spread
    };
    let mut close = 1000;
    (0..days)
        .map(|day| {
            let open = close + next(3);
            close = (close + next(25)).max(500);
            Bar { day, open, close }
        })
        .collect()
}

/// The bars as an OHLCV CSV with Unix times.
fn write_csv(bars: &[Bar], path: &Path) {
    let mut text = String::from("ts,open,high,low,close,volume\n");
    for b in bars {
        let ts = 1577836800 + b.day * 86400;
        text += &format!(
            "{ts},{},{},{},{},1000\n",
            b.open,
            b.open.max(b.close) + 2,
            b.open.min(b.close) - 2,
            b.close
        );
    }
    std::fs::write(path, text).unwrap();
}

/// The fills `mean-reversion.atls` must give, replayed here: the sample z-score of the closes over `lookback` bars,
/// a target of `quantity` below `-entry` and of 0 above `exit`, each order filled at the next bar's open.
fn expected_fills(
    bars: &[Bar],
    lookback: usize,
    entry: f64,
    exit: f64,
    quantity: i64,
) -> Vec<(i64, &'static str, i64, i64)> {
    let mut fills = vec![];
    let mut position = 0;
    let mut pending: i64 = 0;
    for (i, b) in bars.iter().enumerate() {
        if pending != 0 {
            fills.push((b.day, if pending > 0 { "buy" } else { "sell" }, pending.abs(), b.open));
            position += pending;
            pending = 0;
        }
        if i + 1 < lookback {
            continue;
        }
        let window: Vec<f64> = bars[i + 1 - lookback..=i].iter().map(|b| b.close as f64).collect();
        let mean = window.iter().sum::<f64>() / lookback as f64;
        let var = window.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (lookback - 1) as f64;
        if var == 0.0 {
            continue;
        }
        let z = (b.close as f64 - mean) / var.sqrt();
        let target = if z < -entry {
            quantity
        } else if z > exit {
            0
        } else {
            position
        };
        pending = target - position;
    }
    fills
}

/// The fills file as (day, side, quantity, price).
fn read_fills(path: &Path) -> Vec<(i64, String, i64, i64)> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("at,symbol,side,quantity,price,commission,tax"));
    lines
        .map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            let (y, m, d) = (&c[0][..4], &c[0][5..7], &c[0][8..10]);
            assert_eq!((y, m, &c[0][10..]), ("2020", m, "T00:00:00Z"), "{l}");
            let day = days_into_2020(m.parse().unwrap(), d.parse().unwrap());
            assert_eq!(c[1], "AAA");
            (day, c[2].to_string(), c[3].parse().unwrap(), c[4].parse().unwrap())
        })
        .collect()
}

/// Days from 2020-01-01 to a day of 2020 (a leap year).
fn days_into_2020(month: i64, day: i64) -> i64 {
    let before = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];
    before[(month - 1) as usize] + day - 1
}

/// Runs the spec and checks the fills and the report against the replay computed here.
fn check_csv_run(dir: &Path, cache: &Path, bars: &[Bar], lookback: usize, env: &[(&str, &str)]) -> Out {
    let fills = dir.join(format!("fills-{lookback}.csv"));
    let p = format!("lookback={lookback}");
    let out = atlc_run(
        dir,
        cache,
        &["mr.toml", "-P", &p, "--fills", fills.to_str().unwrap()],
        env,
    )
    .ok();
    let want = expected_fills(bars, lookback, 1.0, 0.0, 100);
    let got = read_fills(&fills);
    let want_text: Vec<(i64, String, i64, i64)> = want.iter().map(|(d, s, q, p)| (*d, s.to_string(), *q, *p)).collect();
    assert_eq!(got, want_text, "lookback {lookback}");
    assert!(want.len() >= 6, "the series must trade: {} fills", want.len());
    let position: i64 = want.iter().map(|(_, s, q, _)| if *s == "buy" { *q } else { -q }).sum();
    let cash: i64 = 100000
        - want
            .iter()
            .map(|(_, s, q, p)| if *s == "buy" { q * p } else { -q * p })
            .sum::<i64>();
    let equity = cash + position * bars.last().unwrap().close;
    assert_eq!(field(&out.stdout, "executions"), want.len().to_string());
    assert_eq!(field(&out.stdout, "bars"), bars.len().to_string());
    assert_eq!(field(&out.stdout, "opening_equity"), "\"100000\"");
    let closing: f64 = field(&out.stdout, "closing_equity").trim_matches('"').parse().unwrap();
    assert_eq!(closing, equity as f64);
    assert_eq!(field(&out.stdout, "first_bar"), "\"2020-01-01T00:00:00Z\"");
    assert_eq!(field(&out.stdout, "rejected"), "0");
    out
}

/// Every example's spec is valid for its program, and the programs are the tested ones (no C++ needed).
#[test]
fn the_examples_check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut specs = 0;
    for e in std::fs::read_dir(root.join("examples")).unwrap().flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "atls") {
            let tested = root.join("tests/programs").join(path.file_name().unwrap());
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                std::fs::read_to_string(&tested).unwrap(),
                "{}",
                path.display()
            );
        }
        if path.extension().is_none_or(|x| x != "toml") {
            continue;
        }
        specs += 1;
        let (strategy, entries) = atlc::spec::read(&path).unwrap_or_else(|e| panic!("{e:?}"));
        let source = std::fs::read_to_string(&strategy).unwrap();
        let ir = atlc::check(&source).unwrap_or_else(|_| panic!("{}", strategy.display()));
        let replay = atlc::spec::from_replay(&ir.replay, &strategy, &source).unwrap();
        let run = atlc::spec::resolve(&[replay, entries], &atlc::spec::Program::of(&ir))
            .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        assert!(!run.store);
        let data = run.args.iter().find(|(k, _)| k == "data.file").unwrap();
        assert!(Path::new(&data.1).is_file(), "{}", data.1);
    }
    assert_eq!(specs, 5);
}

#[test]
fn csv_run_matches_a_replay_computed_here() {
    let Some(prefix) = prefix("csv_run_matches_a_replay_computed_here") else {
        return;
    };
    let dir = scratch("csv");
    let cache = outside("csv");
    let bars = walk(120);
    write_csv(&bars, &dir.join("bars.csv"));
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/mean-reversion.atls");
    std::fs::write(
        dir.join("mr.toml"),
        format!(
            "strategy = {}\n\n[data]\nsource = \"csv\"\nfile = \"bars.csv\"\nsymbol = \"AAA\"\n\n[replay]\ncash = 100_000\n\n\
             [params]\nentry = 1\n\n[build]\nalgotradelib = {}\n",
            toml_path(&program),
            toml_path(&prefix)
        ),
    )
    .unwrap();
    assert!(check_csv_run(&dir, &cache, &bars, 5, &[]).built());
    // A parameter is an argument: no build.
    assert!(!check_csv_run(&dir, &cache, &bars, 8, &[]).built());
    // A bound is checked when the strategy is built, and the run stops.
    let out = atlc_run(&dir, &cache, &["mr.toml", "-P", "lookback=1"], &[]);
    assert_eq!(out.code, 4, "{}", out.stderr);
    assert!(
        out.stderr.contains("lower bound") && out.stdout.is_empty(),
        "{}",
        out.stderr
    );
    // A bound on another parameter is checked against the final values, whatever their order.
    std::fs::write(
        dir.join("bounded.atls"),
        "params {\n  real<lower=0> entry = 2;\n  real<lower=0, upper=entry> exit = 1;\n}\nseries {\n  real z = zscore(close, 5);\n}\n\
         on_bar {\n  if (z < -entry) {\n    target(10);\n  } else if (z > exit) {\n    target(0);\n  }\n}\n",
    )
    .unwrap();
    let bounded = |params: &str| {
        let spec = format!(
            "strategy = 'bounded.atls'\n[data]\nsource = \"csv\"\nfile = \"bars.csv\"\nsymbol = \"AAA\"\n[params]\n{params}\n\
             [build]\nalgotradelib = {}\n",
            toml_path(&prefix)
        );
        std::fs::write(dir.join("bounded.toml"), spec).unwrap();
        atlc_run(&dir, &cache, &["bounded.toml"], &[])
    };
    let out = bounded("entry = 0.5\nexit = 0.2");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let out = bounded("entry = 0.5\nexit = 0.9");
    assert_eq!(out.code, 4, "{}", out.stderr);
    assert!(
        out.stderr.contains("exit = 0.9 is above its upper bound"),
        "{}",
        out.stderr
    );
    // A spec error stops before anything is built.
    std::fs::write(dir.join("bad.toml"), "strategy = 'x.atls'\n[replay]\ncash = 0\n").unwrap();
    let out = atlc_run(&dir, &cache, &["bad.toml"], &[]);
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(
        out.stderr
            .contains("bad.toml:3: `replay.cash` is a number above 0, not 0"),
        "{}",
        out.stderr
    );
    let sanitize = std::env::var("ATLC_TEST_SANITIZE").is_ok_and(|v| v == "1") && !cfg!(windows);
    let flags = "-fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer";
    let sanitized = [("CXXFLAGS", flags), ("LDFLAGS", "-fsanitize=address,undefined")];
    if sanitize {
        assert!(check_csv_run(&dir, &cache, &bars, 5, &sanitized).built());
    }
    // Times and rows are read as written or refused, never read differently.
    let data = |name: &str, data: &str, csv: &str, env: &[(&str, &str)]| {
        std::fs::write(dir.join(format!("{name}.csv")), csv).unwrap();
        let spec = format!(
            "strategy = {}\n[data]\nsource = \"csv\"\nfile = \"{name}.csv\"\n{data}\n[build]\nalgotradelib = {}\n",
            toml_path(&program),
            toml_path(&prefix)
        );
        std::fs::write(dir.join(format!("{name}.toml")), spec).unwrap();
        atlc_run(&dir, &cache, &[&format!("{name}.toml")], env)
    };
    let long = "ts,open,high,low,close,volume\n";
    for env in if sanitize {
        vec![&[][..], &sanitized[..]]
    } else {
        vec![&[][..]]
    } {
        let iso = "symbol = \"AAA\"\ntime_format = \"iso\"";
        let out = data(
            "offset",
            iso,
            &format!("{long}2020-01-01T09:00:00+09:00,1,1,1,1,1\n2020-01-02T09:00+09:00,1,1,1,1,1\n"),
            env,
        );
        assert_eq!(out.code, 0, "{}", out.stderr);
        assert!(!out.built(), "{}", out.stderr);
        assert_eq!(field(&out.stdout, "first_bar"), "\"2020-01-01T00:00:00Z\"");
        for (name, settings, csv, says) in [
            ("no-day", iso, format!("{long}2020-02-30,1,1,1,1,1\n"), "not a date"),
            (
                "junk",
                iso,
                format!("{long}2020-01-01junk,1,1,1,1,1\n"),
                "not an ISO time",
            ),
            (
                "hour",
                iso,
                format!("{long}2020-01-01T24:00,1,1,1,1,1\n"),
                "not an ISO time",
            ),
            (
                "far",
                "symbol = \"AAA\"",
                format!("{long}9223372036854775807,1,1,1,1,1\n"),
                "outside the years 1 to 9999",
            ),
            (
                "short",
                "layout = \"wide\"",
                "ts,AAA,stamp\n1577836800,100\n".to_string(),
                "expected 3 cells, found 2",
            ),
            (
                "stamps",
                "symbol = \"AAA\"",
                "ts,stamp,open,high,low,close,volume\n1577836800,close,1,1,1,1,1\n1577923200,open,1,1,1,1,1\n"
                    .to_string(),
                "share one stamp",
            ),
        ] {
            let out = data(name, settings, &csv, env);
            assert_eq!(out.code, 4, "{name}: {}", out.stderr);
            assert!(
                out.stderr.contains(says) && out.stdout.is_empty(),
                "{name}: {}",
                out.stderr
            );
        }
    }
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn the_cache_rebuilds_on_what_the_build_used() {
    let Some(prefix) = prefix("the_cache_rebuilds_on_what_the_build_used") else {
        return;
    };
    let dir = scratch("cache");
    let cache = outside("cache");
    let copy = dir.join("prefix");
    copy_tree(&prefix, &copy);
    write_csv(&walk(40), &dir.join("bars.csv"));
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/trailing-stop.atls");
    std::fs::write(
        dir.join("t.toml"),
        format!(
            "strategy = {}\n[data]\nsource = \"csv\"\nfile = \"bars.csv\"\nsymbol = \"AAA\"\n[build]\nalgotradelib = {}\n",
            toml_path(&program),
            toml_path(&copy)
        ),
    )
    .unwrap();
    let run = |args: &[&str], env: &[(&str, &str)]| {
        let mut all = vec!["t.toml"];
        all.extend_from_slice(args);
        atlc_run(&dir, &cache, &all, env).ok()
    };
    let first = run(&[], &[]);
    assert!(first.built());
    assert!(!run(&[], &[]).built(), "unchanged");
    let other = run(&["-P", "lookback=5"], &[]);
    assert!(!other.built(), "a parameter");
    assert_ne!(other.stdout, first.stdout, "the parameter must change the run");
    // A change in the library's package at the same prefix.
    touch(&copy.join("include/algotradelib/version"));
    let out = run(&[], &[]);
    assert!(out.built() && out.stderr.contains("has changed"), "{}", out.stderr);
    assert!(!run(&[], &[]).built());
    if cfg!(windows) {
        let _ = writeln!(
            std::io::stderr(),
            "the compiler half of this test needs a shell script compiler"
        );
        return;
    }
    // Another compiler, then the same compiler changed in place.
    let real = std::env::var("CXX")
        .ok()
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "c++".into());
    let wrapper = dir.join("cxx");
    std::fs::write(&wrapper, format!("#!/bin/sh\nexec '{real}' \"$@\"\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let cxx = wrapper.to_str().unwrap();
    assert!(run(&[], &[("CXX", cxx)]).built(), "another compiler");
    assert!(!run(&[], &[("CXX", cxx)]).built());
    std::fs::write(&wrapper, format!("#!/bin/sh\n# upgraded\nexec '{real}' \"$@\"\n")).unwrap();
    assert!(run(&[], &[("CXX", cxx)]).built(), "the compiler changed in place");
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn the_library_search() {
    let Some(prefix) = prefix("the_library_search") else {
        return;
    };
    let dir = scratch("search");
    let cache = outside("search");
    let good = dir.join("with spaces").join("algotradelib prefix");
    copy_tree(&prefix, &good);
    let broken = dir.join("broken");
    copy_tree(&prefix, &broken);
    std::fs::write(
        broken.join("lib/cmake/algotradelib/algotradelibConfig.cmake"),
        "message(FATAL_ERROR \"the broken package\")\n",
    )
    .unwrap();
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    write_csv(&walk(30), &dir.join("bars.csv"));
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/mean-reversion.atls");
    let spec = |name: &str, build: &str| {
        std::fs::write(
            dir.join(name),
            format!(
                "strategy = {}\n[data]\nsource = \"csv\"\nfile = \"bars.csv\"\nsymbol = \"AAA\"\n{build}",
                toml_path(&program)
            ),
        )
        .unwrap();
    };
    let broken_env = broken.display().to_string();
    let broken_dir = broken.join("lib/cmake/algotradelib").display().to_string();
    let env = [
        ("CMAKE_PREFIX_PATH", broken_env.as_str()),
        ("algotradelib_DIR", broken_dir.as_str()),
    ];
    // The spec's prefix, holding spaces, beats the environment's.
    spec(
        "explicit.toml",
        &format!("[build]\nalgotradelib = {}\n", toml_path(&good)),
    );
    let out = atlc_run(&dir, &cache, &["explicit.toml"], &env).ok();
    assert!(out.built() && out.stdout.contains("\"bars\": 30"), "{}", out.stdout);
    // Without it, the environment's package is the one found.
    spec("implicit.toml", "");
    let out = atlc_run(
        &dir,
        &cache,
        &["implicit.toml"],
        &[("CMAKE_PREFIX_PATH", broken_env.as_str())],
    );
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(out.stderr.contains("the broken package"), "{}", out.stderr);
    // No package where the spec says: no other one is taken.
    spec(
        "missing.toml",
        &format!("[build]\nalgotradelib = {}\n", toml_path(&empty)),
    );
    let out = atlc_run(&dir, &cache, &["missing.toml"], &env);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(
        out.stderr.contains("atlc run: no algotradelib package in") && out.stderr.contains("[build] algotradelib"),
        "{}",
        out.stderr
    );
    assert!(out.stdout.is_empty());
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn store_runs() {
    let Some(prefix) = prefix("store_runs") else {
        return;
    };
    let Some(duckdb) = needs("ATLC_TEST_DUCKDB", "store_runs", "the directory of DuckDB's C library") else {
        return;
    };
    let dir = scratch("store");
    let work = outside("store");
    let cache = work.join("cache");
    // The store, written by DuckDB.
    let maker = dir.join("make-store");
    let configure = Command::new("cmake")
        .arg("-S")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/run"))
        .arg("-B")
        .arg(&maker)
        .arg(format!(
            "-DDUCKDB_DIR={}",
            duckdb.display().to_string().replace('\\', "/")
        ))
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .output()
        .unwrap();
    assert!(
        configure.status.success(),
        "{}",
        String::from_utf8_lossy(&configure.stderr)
    );
    let build = Command::new("cmake")
        .arg("--build")
        .arg(&maker)
        .args(["--config", "Release"])
        .output()
        .unwrap();
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stdout));
    let store = dir.join("store");
    let made = Command::new(
        maker
            .join("bin")
            .join(if cfg!(windows) { "make-store.exe" } else { "make-store" }),
    )
    .arg(&store)
    .output()
    .unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));

    // A program that buys one share and holds it: the closing equity tells the last close read.
    std::fs::write(work.join("hold.atls"), "on_bar {\n  target(1);\n}\n").unwrap();
    let spec = |name: &str, data: &str, build: &str| {
        std::fs::write(
            work.join(name),
            format!(
                "strategy = \"hold.atls\"\n[data]\n{data}\n[build]\nalgotradelib = {}\n{build}",
                toml_path(&prefix)
            ),
        )
        .unwrap();
    };
    let at_duckdb = format!("duckdb = {}\n", toml_path(&duckdb));
    let store_data = format!("store = \"test/1d/AAA\"\nstore_root = {}", toml_path(&store));
    spec("latest.toml", &store_data, &at_duckdb);
    let fills = work.join("fills.csv");
    let out = atlc_run(&work, &cache, &["latest.toml", "--fills", fills.to_str().unwrap()], &[]).ok();
    assert!(out.built());
    let bought: f64 = std::fs::read_to_string(&fills)
        .unwrap()
        .lines()
        .nth(1)
        .unwrap()
        .split(',')
        .nth(4)
        .unwrap()
        .parse()
        .unwrap();
    let equity = |json: &str| -> f64 { field(json, "closing_equity").trim_matches('"').parse().unwrap() };
    assert_eq!(field(&out.stdout, "bars"), "11");
    assert_eq!(field(&out.stdout, "last_bar"), "\"2020-01-11T00:00:00Z\"");
    assert_eq!(
        equity(&out.stdout),
        100000.0 - bought + 113.0,
        "the latest version of each bar"
    );
    // As of an instant between the two fetches: the first version, and no build.
    spec(
        "as-of.toml",
        &format!("{store_data}\nas_of = \"2026-01-15T00:00:00Z\""),
        &at_duckdb,
    );
    let out = atlc_run(&work, &cache, &["as-of.toml"], &[]).ok();
    assert!(!out.built(), "as_of is a run setting");
    assert_eq!(field(&out.stdout, "bars"), "10");
    assert_eq!(field(&out.stdout, "last_bar"), "\"2020-01-10T00:00:00Z\"");
    assert_eq!(
        equity(&out.stdout),
        100000.0 - bought + 110.0,
        "the version fetched before 2026-01-15"
    );
    // A coordinate the store does not hold.
    spec(
        "missing.toml",
        &format!("store = \"test/1d/ZZZ\"\nstore_root = {}", toml_path(&store)),
        &at_duckdb,
    );
    let out = atlc_run(&work, &cache, &["missing.toml"], &[]);
    assert_eq!(out.code, 4, "{}", out.stderr);
    assert!(out.stderr.contains("the store has no test/1d/ZZZ"), "{}", out.stderr);
    // DuckDB from DUCKDB_ROOT.
    spec("root.toml", &store_data, "");
    let root = duckdb.display().to_string();
    let out = atlc_run(&work, &cache, &["root.toml"], &[("DUCKDB_ROOT", root.as_str())]).ok();
    assert_eq!(field(&out.stdout, "bars"), "11");
    // DuckDB absent: a store run says so, and a CSV run needs none.
    let empty = work.join("no-duckdb");
    std::fs::create_dir_all(&empty).unwrap();
    spec("absent.toml", &store_data, &format!("duckdb = {}\n", toml_path(&empty)));
    let out = atlc_run(&work, &cache, &["absent.toml"], &[]);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(
        out.stderr.contains("atlc run: no DuckDB (duckdb.h and its library) in"),
        "{}",
        out.stderr
    );
    write_csv(&walk(11), &work.join("bars.csv"));
    spec(
        "csv.toml",
        "source = \"csv\"\nfile = \"bars.csv\"\nsymbol = \"AAA\"",
        "",
    );
    let nothing = empty.display().to_string();
    let out = atlc_run(&work, &cache, &["csv.toml"], &[("DUCKDB_ROOT", nothing.as_str())]).ok();
    assert_eq!(field(&out.stdout, "bars"), "11");
    // The cached store build runs from anywhere: its DuckDB is found by an rpath (or a copied DLL).
    let elsewhere = outside("store-elsewhere");
    let out = atlc_run(&elsewhere, &cache, &[work.join("latest.toml").to_str().unwrap()], &[]).ok();
    assert!(!out.built());
    assert_eq!(field(&out.stdout, "bars"), "11");
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(&elsewhere);
}
