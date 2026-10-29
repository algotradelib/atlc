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

/// The expected headers: each program with the default settings, and the pairs program with `naive` (Eigen's
/// least squares).
#[test]
fn expected_headers() {
    let cases = [
        ("mean-reversion", "MeanReversion", Indicators::Incremental),
        ("pairs", "Pairs", Indicators::Incremental),
        ("trailing-stop", "TrailingStop", Indicators::Incremental),
        ("universe", "Universe", Indicators::Incremental),
        ("user-indicator", "UserIndicator", Indicators::Incremental),
        ("pairs", "Pairs", Indicators::Naive),
    ];
    let bless = std::env::var_os("BLESS").is_some();
    let mut failed = vec![];
    for (program, name, indicators) in cases {
        let file = format!("{program}.atls");
        let source = std::fs::read_to_string(root().join("tests/programs").join(&file)).unwrap();
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
    let rows: [Row; 22] = [
        (
            "restart",
            "sma(close, 5)",
            [("Sma", running), ("Sma", again), ("SmaExact", running)],
        ),
        (
            "restart",
            "ts_sum(close, 5)",
            [("TsSum", running), ("TsSum", again), ("TsSumExact", running)],
        ),
        (
            "restart",
            "rsi(close, 5)",
            [("Rsi", running), ("Rsi", again), ("RsiExact", running)],
        ),
        (
            "restart",
            "stdev(close, 5)",
            [("Stdev", running), ("Stdev", again), ("Stdev", running)],
        ),
        (
            "restart",
            "stdev_pop(close, 5)",
            [("StdevPop", running), ("StdevPop", again), ("StdevPop", running)],
        ),
        (
            "restart",
            "zscore(close, 5)",
            [("Zscore", running), ("Zscore", again), ("Zscore", running)],
        ),
        (
            "restart",
            "ts_max(close, 5)",
            [("TsMax", running), ("TsMax", again), ("TsMax", running)],
        ),
        (
            "restart",
            "ts_min(close, 5)",
            [("TsMin", running), ("TsMin", again), ("TsMin", running)],
        ),
        (
            "restart",
            "ols_slope(close, high, 5)",
            [("OlsSlope", running), ("OlsLlt<true>", running), ("OlsSlope", running)],
        ),
        (
            "restart",
            "ols_intercept(close, high, 5)",
            [
                ("OlsIntercept", running),
                ("OlsLlt<false>", running),
                ("OlsIntercept", running),
            ],
        ),
        (
            "skip",
            "sma(close, 5)",
            [("SmaSkip", running), ("SmaSkip", again), ("SmaSkip", running)],
        ),
        (
            "skip",
            "ts_sum(close, 5)",
            [("TsSumSkip", running), ("TsSumSkip", again), ("TsSumSkip", running)],
        ),
        (
            "skip",
            "stdev(close, 5)",
            [("StdevSkip", running), ("StdevSkip", again), ("StdevSkip", running)],
        ),
        (
            "skip",
            "stdev_pop(close, 5)",
            [
                ("StdevPopSkip", running),
                ("StdevPopSkip", again),
                ("StdevPopSkip", running),
            ],
        ),
        (
            "skip",
            "zscore(close, 5)",
            [("ZscoreSkip", running), ("ZscoreSkip", again), ("ZscoreSkip", running)],
        ),
        (
            "restart",
            "ts_sum(close, 5, 2)",
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
        (
            "restart",
            "kf_beta(close, high, 0.0001, 0.001)",
            [
                ("KalmanHedge", running),
                ("KalmanHedge", running),
                ("KalmanHedge", running),
            ],
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
            "sma(close, 5)",
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
