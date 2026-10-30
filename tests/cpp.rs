//! The C++ tests, against an installed algotradelib: the runtime's own tests (`runtime/tests/`), generated
//! strategies replayed in the library's backtester (`tests/generated/`), and the differential gate against
//! hand-written strategies (`tests/gate/`).
//!
//! They need CMake, a C++17 compiler, Catch2 3 and an algotradelib installed with its CMake package (built with
//! Eigen), whose prefix `ATLC_TEST_PREFIX` names; without it they pass with a message and run nothing.
//! `ATLC_TEST_SANITIZE=1` builds and runs everything a second time with AddressSanitizer and
//! UndefinedBehaviorSanitizer (Clang or GCC).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The settings each program is generated with, by the suffix of its struct's name.
const VARIANTS: [(&str, &[&str]); 5] = [
    ("incremental", &[]),
    ("naive", &["--indicators", "naive"]),
    ("exact", &["--indicators", "exact"]),
    ("noshare", &["--share", "off"]),
    ("keep", &["--history", "keep"]),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The algotradelib prefix, or `None` after saying the test is skipped.
fn prefix(test: &str) -> Option<String> {
    match std::env::var("ATLC_TEST_PREFIX") {
        Ok(p) if !p.is_empty() => Some(p),
        _ => {
            // Straight to the terminal: the test harness captures `eprintln!` of a passing test.
            let _ = writeln!(
                std::io::stderr(),
                "\n{test}: skipped, set ATLC_TEST_PREFIX to an algotradelib installation prefix to run it"
            );
            None
        }
    }
}

fn sanitize() -> bool {
    std::env::var("ATLC_TEST_SANITIZE").is_ok_and(|v| v == "1")
}

/// A fresh directory for one test's files.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cpp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs a command, its output into `log`; panics with the log's end if it fails.
fn run(command: &mut Command, log: &Path) {
    let out = command
        .output()
        .unwrap_or_else(|e| panic!("cannot run {command:?}: {e}"));
    let mut text = out.stdout;
    text.extend_from_slice(&out.stderr);
    std::fs::write(log, &text).unwrap();
    if !out.status.success() {
        let text = String::from_utf8_lossy(&text);
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines[lines.len().saturating_sub(80)..].join("\n");
        panic!(
            "{command:?} failed ({}); the log is {}:\n{tail}",
            out.status,
            log.display()
        );
    }
}

/// `atlc build` of `source` as `out/<name>.hpp`, by way of a copy named `<name>.atls` (the struct is named after
/// the file), with `edit` applied to the copy.
fn generate(out: &Path, name: &str, source: &Path, args: &[&str], edit: Option<(&str, &str)>) {
    let mut text = std::fs::read_to_string(source).unwrap();
    if let Some((from, to)) = edit {
        assert!(text.contains(from), "{}: no `{from}`", source.display());
        text = text.replace(from, to);
    }
    let copy = out.join("src").join(format!("{name}.atls"));
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::write(&copy, text).unwrap();
    let header = out.join("include").join(format!("{name}.hpp"));
    std::fs::create_dir_all(header.parent().unwrap()).unwrap();
    run(
        Command::new(env!("CARGO_BIN_EXE_atlc"))
            .arg("build")
            .arg(&copy)
            .arg("-o")
            .arg(&header)
            .args(args),
        &out.join(format!("{name}.log")),
    );
}

/// Writes the runtime beside the generated headers, as a project including them would.
fn emit_runtime(out: &Path) {
    run(
        Command::new(env!("CARGO_BIN_EXE_atlc"))
            .arg("build")
            .arg("--emit-runtime")
            .arg(out.join("include")),
        &out.join("runtime.log"),
    );
}

/// Configures, builds and runs the CMake project in `source` (with `-DATLC_OUT=<out>/include` when given), and
/// again with the sanitizers when `ATLC_TEST_SANITIZE=1`.
fn cmake(name: &str, source: &Path, out: Option<&Path>, prefix: &str) {
    let jobs = std::env::var("CMAKE_BUILD_PARALLEL_LEVEL").unwrap_or_else(|_| "4".into());
    let mut builds = vec![false];
    if sanitize() {
        builds.push(true);
    }
    for sanitized in builds {
        let dir = scratch(&format!("{name}-build{}", if sanitized { "-sanitized" } else { "" }));
        // Paths with forward slashes: CMake can read a backslash as an escape.
        let mut configure = Command::new("cmake");
        configure
            .arg("-S")
            .arg(source)
            .arg("-B")
            .arg(&dir)
            .arg(format!("-DCMAKE_PREFIX_PATH={}", prefix.replace('\\', "/")))
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .arg(format!("-DATLC_SANITIZE={}", if sanitized { "ON" } else { "OFF" }));
        if let Some(out) = out {
            let include = out.join("include").display().to_string().replace('\\', "/");
            configure.arg(format!("-DATLC_OUT={include}"));
        }
        run(&mut configure, &dir.join("configure.log"));
        run(
            Command::new("cmake")
                .arg("--build")
                .arg(&dir)
                .args(["--config", "Release", "--parallel", &jobs]),
            &dir.join("build.log"),
        );
        run(
            Command::new("ctest")
                .arg("--test-dir")
                .arg(&dir)
                .args(["-C", "Release", "--output-on-failure"]),
            &dir.join("test.log"),
        );
    }
}

#[test]
fn runtime_headers() {
    let Some(prefix) = prefix("runtime_headers") else {
        return;
    };
    cmake("runtime", &root().join("runtime/tests"), None, &prefix);
}

#[test]
fn generated_strategies() {
    let Some(prefix) = prefix("generated_strategies") else {
        return;
    };
    let out = scratch("generated");
    for program in ["mean-reversion", "pairs", "trailing-stop", "universe", "user-indicator"] {
        let source = root().join("tests/programs").join(format!("{program}.atls"));
        for (variant, args) in VARIANTS {
            let name = format!("{}_{variant}", program.replace('-', "_"));
            generate(&out, &name, &source, args, None);
        }
    }
    let programs = root().join("tests/generated/programs");
    generate(&out, "identity", &programs.join("identity.atls"), &[], None);
    generate(&out, "identity_pair", &programs.join("identity-pair.atls"), &[], None);
    generate(
        &out,
        "identity_missing",
        &programs.join("identity-missing.atls"),
        &[],
        None,
    );
    let poison = Some(("missing = skip;", "missing = poison;"));
    generate(
        &out,
        "identity_missing_poison",
        &programs.join("identity-missing.atls"),
        &[],
        poison,
    );
    // The control: one copy off by a part in 1e15 must be seen.
    let perturbed = Some(("value real mean = s / n;", "value real mean = s / n * (1 + 1e-15);"));
    generate(
        &out,
        "identity_perturbed",
        &programs.join("identity.atls"),
        &[],
        perturbed,
    );
    for program in [
        "missing-open",
        "integers",
        "universe-function",
        "contraction",
        "int-compare",
        "loop-edge",
        "lazy",
        "skip-value",
        "xs-guard",
        "xs-reductions",
        "pricing",
        "pricing-held",
        "pricing-once",
        "pricing-named",
        "pricing-named-held",
        "pricing-universe",
        "pricing-universe-held",
        "pricing-legs",
    ] {
        generate(
            &out,
            &program.replace('-', "_"),
            &programs.join(format!("{program}.atls")),
            &[],
            None,
        );
    }
    // The universe under each missing-value policy: `restart` (the default), `skip` and `poison`.
    for policy in ["skip", "poison"] {
        let options = format!("universe {{}}\noptions {{ missing = {policy}; }}");
        generate(
            &out,
            &format!("pricing_universe_{policy}"),
            &programs.join("pricing-universe.atls"),
            &[],
            Some(("universe {}", options.as_str())),
        );
    }
    generate(
        &out,
        "pricing_legs_held",
        &programs.join("pricing-legs.atls"),
        &[],
        Some(("target(at_close);", "target(held * 0 + at_close);")),
    );
    emit_runtime(&out);
    cmake("generated", &root().join("tests/generated"), Some(&out), &prefix);
}

#[test]
fn differential_gate() {
    let Some(prefix) = prefix("differential_gate") else {
        return;
    };
    let out = scratch("gate");
    for program in ["pullback", "trail", "sma"] {
        let source = root().join("tests/gate/programs").join(format!("{program}.atls"));
        for (variant, args) in VARIANTS.iter().filter(|(v, _)| *v != "keep") {
            generate(&out, &format!("{program}_{variant}"), &source, args, None);
        }
    }
    emit_runtime(&out);
    cmake("gate", &root().join("tests/gate"), Some(&out), &prefix);
}
