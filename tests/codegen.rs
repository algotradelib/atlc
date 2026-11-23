//! The C++ the printer writes: expected headers for the programs of `tests/programs/` (`tests/codegen/`,
//! written again with `BLESS=1`), and, for every built-in indicator in every `--indicators` mode, the runtime
//! class and method the generated code calls, as the reference's arithmetic table names them
//! (`docs/reference/types-and-definedness.md`, "Arithmetic"; `runtime/tests/` checks each method's algorithm).

use std::path::PathBuf;

use atlc::backend::{Indicators, Options};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn options(name: &str, source_name: &str, indicators: Indicators) -> Options {
    Options {
        name: name.to_string(),
        source_name: source_name.to_string(),
        indicators,
        share_rings: true,
        drop_history: true,
    }
}

fn build(source: &str, options: &Options) -> String {
    match atlc::build(source, options) {
        Ok((header, warnings)) => {
            assert!(
                warnings.is_empty(),
                "{}: {:?}",
                options.source_name,
                warnings[0].message
            );
            header
        }
        Err(e) => panic!("{}: {}", options.source_name, e[0].message),
    }
}

/// The expected headers: each program with the default settings, the pairs program with `naive` (Eigen's least
/// squares), and programs with external indicators and functions and with measures (from `tests/generated/programs/`).
#[test]
fn expected_headers() {
    let cases = [
        ("mean-reversion", "MeanReversion", Indicators::Incremental),
        ("pairs", "Pairs", Indicators::Incremental),
        ("trailing-stop", "TrailingStop", Indicators::Incremental),
        ("universe", "Universe", Indicators::Incremental),
        ("user-indicator", "UserIndicator", Indicators::Incremental),
        ("pairs", "Pairs", Indicators::Naive),
        (
            "identity-extern-missing",
            "IdentityExternMissing",
            Indicators::Incremental,
        ),
        ("measures", "Measures", Indicators::Incremental),
    ];
    let bless = std::env::var_os("BLESS").is_some();
    let mut failed = vec![];
    for (program, name, indicators) in cases {
        let file = format!("{program}.atls");
        let mut path = root().join("tests/programs").join(&file);
        if !path.exists() {
            path = root().join("tests/generated/programs").join(&file);
        }
        let source = std::fs::read_to_string(path).unwrap();
        let header = build(&source, &options(name, &file, indicators));
        let suffix = if indicators == Indicators::Incremental {
            String::new()
        } else {
            format!(".{}", indicators.name())
        };
        let expected = root().join("tests/codegen").join(format!("{program}{suffix}.hpp"));
        if bless {
            std::fs::write(&expected, &header).unwrap();
        } else if std::fs::read_to_string(&expected).ok().as_deref() != Some(header.as_str()) {
            failed.push(expected.display().to_string());
        }
    }
    assert!(
        failed.is_empty(),
        "differ from the generated code (BLESS=1 writes them): {failed:?}"
    );
}

/// The runtime class of the member computing `v`, and the method the handler calls on it.
fn class_and_method(header: &str) -> (String, String) {
    let decl = header
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("atls::ops::") && (l.contains(" ind") || l.contains(" obj")))
        .unwrap_or_else(|| panic!("no indicator member in\n{header}"));
    let rest = &decl["atls::ops::".len()..];
    let (class, member) = rest.split_once(' ').unwrap();
    let member = member.split(';').next().unwrap();
    let needle = format!("{member}.");
    let call = header
        .match_indices(&needle)
        .find_map(|(at, _)| {
            let method = header[at + needle.len()..].split('(').next()?;
            matches!(method, "update" | "recompute").then(|| method.to_string())
        })
        .unwrap_or_else(|| panic!("no call of {member} in\n{header}"));
    (class.to_string(), call)
}

#[test]
fn each_mode_calls_the_algorithm_the_reference_names() {
    // (policy, call, [incremental, naive, exact] as (class, method)).
    type Row = (&'static str, &'static str, [(&'static str, &'static str); 3]);
    let running = "update";
    let again = "recompute";
    let rows: [Row; 20] = [
        (
            "restart",
            "ma(close, 5)",
            [("Sma", running), ("Sma", again), ("SmaExact", running)],
        ),
        (
            "restart",
            "roll_sum(close, 5)",
            [("TsSum", running), ("TsSum", again), ("TsSumExact", running)],
        ),
        (
            "restart",
            "rsi(close, 5)",
            [("Rsi", running), ("Rsi", again), ("RsiExact", running)],
        ),
        (
            "restart",
            "roll_stdev(close, 5)",
            [("Stdev", running), ("Stdev", again), ("Stdev", running)],
        ),
        (
            "restart",
            "roll_stdev_biased(close, 5)",
            [("StdevPop", running), ("StdevPop", again), ("StdevPop", running)],
        ),
        (
            "restart",
            "roll_zscore(close, 5)",
            [("Zscore", running), ("Zscore", again), ("Zscore", running)],
        ),
        (
            "restart",
            "roll_max(close, 5)",
            [("TsMax", running), ("TsMax", again), ("TsMax", running)],
        ),
        (
            "restart",
            "roll_min(close, 5)",
            [("TsMin", running), ("TsMin", again), ("TsMin", running)],
        ),
        (
            "restart",
            "roll_ols(close, high, 5).slope",
            [("OlsSums", running), ("OlsLlt<>", running), ("OlsSums", running)],
        ),
        (
            "skip",
            "ma(close, 5)",
            [("SmaSkip", running), ("SmaSkip", again), ("SmaSkip", running)],
        ),
        (
            "skip",
            "roll_sum(close, 5)",
            [("TsSumSkip", running), ("TsSumSkip", again), ("TsSumSkip", running)],
        ),
        (
            "skip",
            "roll_stdev(close, 5)",
            [("StdevSkip", running), ("StdevSkip", again), ("StdevSkip", running)],
        ),
        (
            "skip",
            "roll_stdev_biased(close, 5)",
            [
                ("StdevPopSkip", running),
                ("StdevPopSkip", again),
                ("StdevPopSkip", running),
            ],
        ),
        (
            "skip",
            "roll_zscore(close, 5)",
            [("ZscoreSkip", running), ("ZscoreSkip", again), ("ZscoreSkip", running)],
        ),
        (
            "restart",
            "roll_sum(close, 5, 2)",
            [("TsSumMin", running), ("TsSumMin", running), ("TsSumMin", running)],
        ),
        (
            "restart",
            "ema(close, 5)",
            [("Ema", running), ("Ema", running), ("Ema", running)],
        ),
        (
            "skip",
            "ema(close, 5)",
            [("Ema", running), ("Ema", running), ("Ema", running)],
        ),
        // A lag reads its ring; `recompute` is the same read.
        (
            "restart",
            "lag(close, 2)",
            [("Lag", running), ("Lag", again), ("Lag", running)],
        ),
        (
            "skip",
            "lag(close, 2)",
            [("Lag", running), ("Lag", again), ("Lag", running)],
        ),
        (
            "poison",
            "ma(close, 5)",
            [("Sma", running), ("Sma", again), ("SmaExact", running)],
        ),
    ];
    let modes = [Indicators::Incremental, Indicators::Naive, Indicators::Exact];
    for (policy, call, expected) in rows {
        let source =
            format!("options {{ missing = {policy}; }}\nseries {{ real v = {call}; }}\non_bar {{ target(v); }}\n");
        for (mode, (class, method)) in modes.iter().zip(expected) {
            let header = build(&source, &options("Probe", "probe.atls", *mode));
            assert_eq!(
                class_and_method(&header),
                (class.to_string(), method.to_string()),
                "{call} under {policy} with --indicators {}",
                mode.name()
            );
        }
    }
}

/// Identical `roll_ols` calls share one fit in every mode: one member and one update per distinct call, whichever of
/// its values are read and wherever the first one is unused; another window, swapped inputs or another grouping make
/// another fit; and
/// an input that may be undefined restarts the fit under every program default.
#[test]
fn roll_ols_shares_one_fit_per_call() {
    let modes = [Indicators::Incremental, Indicators::Naive, Indicators::Exact];
    for policy in ["restart", "skip", "poison"] {
        let source = format!(
            "options {{ missing = {policy}; }}\nseries {{\n  real u = roll_ols(close / 7, high, 5).slope;\n  \
             real b = roll_ols(close / 7, high, 5).intercept;\n  real b2 = roll_ols(close / 7, high, 5).intercept * 2;\n  \
             real c = roll_ols(close / 7, high, 6).intercept;\n  real d = roll_ols(high, close / 7, 5).intercept;\n  \
             real g = roll_ols(open, close, 5).slope;\n  real p = roll_ols(close - (open - high), high, 5).slope;\n  \
             real q = roll_ols(close - open - high, high, 5).slope;\n}}\non_bar {{ target(b + b2 + c + d + g + p + q); }}\n"
        );
        for share in [true, false] {
            for mode in modes {
                let what = format!("{policy}, share {share}, {}", mode.name());
                let options = Options {
                    share_rings: share,
                    ..options("Probe", "probe.atls", mode)
                };
                let (header, _) = atlc::build(&source, &options).unwrap_or_else(|e| panic!("{what}: {}", e[0].message));
                let class = if mode == Indicators::Naive {
                    "atls::ops::OlsLlt<> obj"
                } else {
                    "atls::ops::OlsSums obj"
                };
                let members: Vec<String> = header
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix(class))
                    .map(|rest| format!("obj{}", rest.trim_end_matches(';')))
                    .collect();
                assert_eq!(members.len(), 6, "{what}\n{header}");
                for m in &members {
                    assert_eq!(header.matches(&format!("{m}.update(")).count(), 1, "{what}: {m}");
                }
                // Every value is read from a member that is declared and updated.
                for read in [".slope()", ".intercept()"] {
                    for (at, _) in header.match_indices(read) {
                        let start = header[..at].rfind("obj").unwrap();
                        assert!(
                            members.iter().any(|m| *m == header[start..at]),
                            "{what}: {}",
                            &header[start..at]
                        );
                    }
                }
                // The fit over `open` restarts on an undefined input.
                let lines: Vec<&str> = header.lines().map(str::trim).collect();
                let guard = lines
                    .iter()
                    .position(|l| *l == "if (std::isnan(f_open) || std::isnan(f_close)) {")
                    .unwrap_or_else(|| panic!("{what}: no guard\n{header}"));
                let fit = lines[guard + 1].strip_suffix(".reset();").expect("a reset");
                assert_eq!(lines[guard + 3], format!("{fit}.update(f_open, f_close);"), "{what}");
            }
        }
    }
}
