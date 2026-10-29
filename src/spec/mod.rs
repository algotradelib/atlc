//! Run specs ([run spec](../../docs/reference/run-spec.md)): what a program is replayed on, read from a TOML subset
//! (`toml`), from the program's `replay {}` block and from the command line, checked key by key and then as a
//! whole against the program, and flattened to the settings the replay driver (`runtime/include/atls/run.hpp`)
//! reads as `key=value` arguments.

mod toml;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::backend::Indicators;

/// Where a setting came from, for messages.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    /// A file and a line: the spec, or the program's `replay {}`.
    File(PathBuf, usize),
    /// `-P name=value`.
    CommandLine,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::File(path, line) => write!(f, "{}:{line}", path.display()),
            Origin::CommandLine => write!(f, "the command line"),
        }
    }
}

/// A value as written.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    /// The number's text (without underscores or a leading `+`) and its value: the text is what the replay reads,
    /// so a rate such as `0.00015` stays an exact decimal.
    Num(String, f64),
    Bool(bool),
    /// An array of strings or numbers.
    List(Vec<Value>),
}

impl Value {
    /// The value as the replay driver reads it.
    fn text(&self) -> String {
        match self {
            Value::Str(s) | Value::Num(s, _) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::List(items) => items.iter().map(Value::text).collect::<Vec<_>>().join(","),
        }
    }

    fn describe(&self) -> String {
        match self {
            Value::Str(s) => format!("\"{s}\""),
            Value::List(_) => format!("[{}]", self.text()),
            _ => self.text(),
        }
    }
}

/// One setting.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The flattened key, `table.key`.
    pub key: String,
    pub value: Value,
    pub origin: Origin,
}

/// What a key holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A file or directory, relative to the file that sets it.
    Path,
    /// A list of paths (a single string is a list of one).
    Paths,
    /// A string.
    Text,
    /// A list of strings (a single string is a list of one).
    Texts,
    /// One of these strings.
    Choice(&'static [&'static str]),
    /// A number at least `min` (above it when `strict`).
    Number {
        min: f64,
        strict: bool,
    },
    /// A whole number, zero or more.
    Count,
    Bool,
    /// A date: `"YYYY-MM-DD"`, or `yyyymmdd` as a number or a string.
    Date,
    /// An instant: `"YYYY-MM-DD"`, optionally with `THH:MM[:SS]` and an offset (`Z`, `+09:00`).
    Instant,
    /// A fixed offset from UTC: `"UTC"`, `"Z"` or `"+HH:MM"`.
    Offset,
    /// Store coordinates, `"SOURCE/INTERVAL/SYMBOL"`, one or a list.
    Coordinates,
}

/// The bar intervals the readers know.
pub const INTERVALS: &[&str] = &["1m", "5m", "15m", "30m", "1h", "1d", "1w", "1mo"];

/// One documented key: its name, what it holds, the data sources it applies to (empty: any), its default, and
/// what it means.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub name: &'static str,
    pub kind: Kind,
    pub sources: &'static [&'static str],
    pub default: &'static str,
    pub about: &'static str,
}

const fn key(
    name: &'static str,
    kind: Kind,
    sources: &'static [&'static str],
    default: &'static str,
    about: &'static str,
) -> Key {
    Key {
        name,
        kind,
        sources,
        default,
        about,
    }
}

const CSV: &[&str] = &["csv"];
const WIDE: &[&str] = &["wide"];
const STORE: &[&str] = &["store"];
const ANY: &[&str] = &[];
const POSITIVE: Kind = Kind::Number { min: 0.0, strict: true };
const RATE: Kind = Kind::Number {
    min: 0.0,
    strict: false,
};

/// Every key a run spec may set besides `instruments.NAME` and `params.NAME`; anything else is refused.
pub const KEYS: &[Key] = &[
    key(
        "strategy",
        Kind::Path,
        ANY,
        "required",
        "the program, relative to the spec",
    ),
    key(
        "data.source",
        Kind::Choice(&["wide", "csv", "store"]),
        ANY,
        "`store` with `data.store`, else `wide`",
        "where the bars come from",
    ),
    key(
        "data.file",
        Kind::Path,
        &["wide", "csv"],
        "",
        "`wide`: the table of closes (as `data.close`); `csv`: the one file",
    ),
    key("data.close", Kind::Path, WIDE, "", "the table of closes"),
    key("data.open", Kind::Path, WIDE, "", "the table of opens"),
    key("data.high", Kind::Path, WIDE, "", "the table of highs"),
    key("data.low", Kind::Path, WIDE, "", "the table of lows"),
    key(
        "data.names",
        Kind::Path,
        WIDE,
        "the tables' header",
        "a file whose second line names the tables' columns",
    ),
    key(
        "data.files",
        Kind::Paths,
        CSV,
        "",
        "one file per symbol, with `data.symbols`",
    ),
    key("data.symbol", Kind::Text, CSV, "", "the symbol of `data.file`"),
    key(
        "data.symbols",
        Kind::Texts,
        CSV,
        "",
        "the symbol of each of `data.files`",
    ),
    key(
        "data.symbol_column",
        Kind::Text,
        CSV,
        "",
        "the column naming each row's symbol, in a long file of many symbols",
    ),
    key(
        "data.layout",
        Kind::Choice(&["long", "wide"]),
        CSV,
        "long",
        "`long`: rows of OHLCV; `wide`: a time column, then a column of closes per symbol",
    ),
    key("data.time", Kind::Text, CSV, "ts", "the time column"),
    key(
        "data.time_format",
        Kind::Choice(&["unix", "unix_ms", "yyyymmdd", "iso"]),
        CSV,
        "unix",
        "how times are written",
    ),
    key(
        "data.timezone",
        Kind::Offset,
        CSV,
        "UTC",
        "the fixed offset of local times",
    ),
    key(
        "data.stamp",
        Kind::Text,
        CSV,
        "the `stamp` column if there is one, else `close`",
        "`open` or `close` (bars stamped at their start or end), or a column holding one of them per row",
    ),
    key(
        "data.interval",
        Kind::Choice(INTERVALS),
        CSV,
        "1d",
        "the bars' interval",
    ),
    key("data.columns.open", Kind::Text, CSV, "open", "the open column"),
    key("data.columns.high", Kind::Text, CSV, "high", "the high column"),
    key("data.columns.low", Kind::Text, CSV, "low", "the low column"),
    key("data.columns.close", Kind::Text, CSV, "close", "the close column"),
    key("data.columns.volume", Kind::Text, CSV, "volume", "the volume column"),
    key("data.store", Kind::Coordinates, STORE, "", "the series to read"),
    key(
        "data.store_root",
        Kind::Path,
        STORE,
        "required",
        "the store's directory",
    ),
    key(
        "data.as_of",
        Kind::Instant,
        STORE,
        "the latest",
        "read each bar's latest version fetched at or before this instant",
    ),
    key(
        "replay.from",
        Kind::Date,
        ANY,
        "the first bar",
        "the first date replayed (UTC)",
    ),
    key(
        "replay.to",
        Kind::Date,
        ANY,
        "the last bar",
        "the last date replayed, inclusive",
    ),
    key(
        "replay.market",
        Kind::Choice(&["us", "kr"]),
        ANY,
        "`kr` for a store coordinate whose source starts with `kis`, `kiwoom` or `toss` or contains `krx`, else `us`",
        "`us`: USD, any price; `kr`: KRW, prices on the KRX tick table",
    ),
    key(
        "replay.cash",
        POSITIVE,
        ANY,
        "100000 (`us`), 100000000 (`kr`)",
        "the opening cash",
    ),
    key(
        "replay.commission",
        RATE,
        ANY,
        "0",
        "the commission, a rate of each fill's value",
    ),
    key(
        "replay.min_commission",
        RATE,
        ANY,
        "0",
        "the least commission of a fill",
    ),
    key(
        "replay.sell_tax",
        RATE,
        ANY,
        "0",
        "the tax, a rate of each sell's value",
    ),
    key("replay.slippage_ticks", Kind::Count, ANY, "0", "slippage, in ticks"),
    key(
        "replay.fill",
        Kind::Choice(&["next-open", "open-prev-close", "legs"]),
        ANY,
        "next-open",
        "when an order fills",
    ),
    key(
        "replay.shorts",
        Kind::Bool,
        ANY,
        "false",
        "whether a position may go below zero (no borrow is modelled)",
    ),
    key(
        "replay.periods_per_year",
        POSITIVE,
        ANY,
        "252",
        "bars a year, for the annualized Sharpe ratio",
    ),
    key(
        "build.algotradelib",
        Kind::Path,
        ANY,
        "`CMAKE_PREFIX_PATH`, `algotradelib_DIR`, then CMake's search",
        "the installed algotradelib prefix",
    ),
    key(
        "build.duckdb",
        Kind::Path,
        ANY,
        "`DUCKDB_ROOT`, then the system's paths",
        "the directory of DuckDB's `duckdb.h` and shared library",
    ),
    key(
        "build.indicators",
        Kind::Choice(&["incremental", "naive", "exact"]),
        ANY,
        "incremental",
        "how windows are summed",
    ),
];

/// What the checks need from the program.
#[derive(Clone, Debug, Default)]
pub struct Program {
    /// Each parameter's name and type (`int`, `real` or `bool`).
    pub params: Vec<(String, ParamType)>,
    /// The names in `instruments {}`, if the program has the block.
    pub named: Option<Vec<String>>,
    /// Whether the program is a universe.
    pub universe: bool,
    /// Whether it has `on_open` or `on_close`.
    pub legs: bool,
}

/// A parameter's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType {
    Int,
    Real,
    Bool,
}

/// A spec checked against its program: the build settings, and the replay's arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub algotradelib: Option<PathBuf>,
    pub duckdb: Option<PathBuf>,
    pub indicators: Indicators,
    /// Whether the run reads a store, so needs DuckDB.
    pub store: bool,
    /// The replay driver's `key=value` arguments, one per key (the last setting of each), parameters as
    /// `param.NAME` and instruments as `instrument.NAME`.
    pub args: Vec<(String, String)>,
}

/// Reads a spec file: its entries (paths made absolute) and the program it names.
pub fn read(path: &Path) -> Result<(PathBuf, Vec<Entry>), Vec<String>> {
    let source = std::fs::read_to_string(path).map_err(|e| vec![format!("cannot read {}: {e}", path.display())])?;
    let mut entries = toml::parse(&source, path).map_err(|e| vec![e])?;
    let errors: Vec<String> = entries.iter_mut().filter_map(|e| check(e).err()).collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let Some(strategy) = entries.iter().find(|e| e.key == "strategy") else {
        return Err(vec![format!(
            "{}: no `strategy = \"FILE.atls\"`: a spec names the program it runs",
            path.display()
        )]);
    };
    let Value::Str(strategy) = &strategy.value else {
        unreachable!("checked as a path");
    };
    Ok((PathBuf::from(strategy), entries))
}

/// The settings of a program's `replay {}` (paths made absolute against the program's directory).
pub fn from_replay(
    settings: &[crate::frontend::ast::Setting],
    file: &Path,
    source: &str,
) -> Result<Vec<Entry>, Vec<String>> {
    use crate::frontend::ast::Lit;
    let mut out: Vec<Entry> = vec![];
    let mut errors = vec![];
    for s in settings {
        let line = source[..s.span.start.min(source.len())].matches('\n').count() + 1;
        let origin = Origin::File(file.to_path_buf(), line);
        if s.key == "strategy" || s.key.starts_with("params.") {
            errors.push(format!(
                "{origin}: `{}` cannot be set in `replay {{}}` (a parameter's default is in `params {{}}`)",
                s.key
            ));
            continue;
        }
        if let Some(first) = out.iter().find(|e| e.key == s.key) {
            errors.push(format!("{origin}: `{}` is already set on {}", s.key, first.origin));
            continue;
        }
        let value = match &s.value {
            Lit::Str(v) => Value::Str(v.clone()),
            Lit::Num(v) => Value::Num(format!("{v}"), *v),
            Lit::Bool(v) => Value::Bool(*v),
        };
        let mut e = Entry {
            key: s.key.clone(),
            value,
            origin,
        };
        match check(&mut e) {
            Ok(()) => out.push(e),
            Err(m) => errors.push(m),
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

/// `-P name=value`: a parameter from the command line.
pub fn from_command_line(assignment: &str) -> Result<Entry, String> {
    let Some((name, text)) = assignment.split_once('=') else {
        return Err(format!("-P takes name=value, not `{assignment}`"));
    };
    let value = match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        t => {
            let v: f64 = t
                .parse()
                .ok()
                .filter(|v: &f64| v.is_finite() && !t.contains(['i', 'n', 'I', 'N']))
                .ok_or_else(|| format!("-P {assignment}: `{t}` is not a number, `true` or `false`"))?;
            Value::Num(t.trim_start_matches('+').to_string(), v)
        }
    };
    Ok(Entry {
        key: format!("params.{name}"),
        value,
        origin: Origin::CommandLine,
    })
}

/// The documented key named `name`.
fn known(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.name == name)
}

/// Checks one entry's key and value, making a path absolute against its file's directory.
fn check(e: &mut Entry) -> Result<(), String> {
    let at = &e.origin;
    let kind = if let Some(name) = e
        .key
        .strip_prefix("instruments.")
        .or_else(|| e.key.strip_prefix("params."))
    {
        if name.contains('.') {
            return Err(format!("{at}: unknown setting `{}`", e.key));
        }
        if e.key.starts_with("params.") {
            return match e.value {
                Value::Num(..) | Value::Bool(_) => Ok(()),
                _ => Err(format!(
                    "{at}: `{}` is a number or a boolean, not {}",
                    e.key,
                    e.value.describe()
                )),
            };
        }
        Kind::Text
    } else {
        match known(&e.key) {
            Some(k) => k.kind,
            None => {
                let table = e.key.split_once('.').map(|(t, _)| t);
                let hint = match table {
                    Some("build") => " ([build] takes algotradelib, duckdb and indicators)",
                    Some("data" | "replay") | None => "",
                    Some(_) => " (tables: [data], [instruments], [replay], [params], [build])",
                };
                return Err(format!("{at}: unknown setting `{}`{hint}", e.key));
            }
        }
    };
    let bad = |what: &str| format!("{at}: `{}` is {what}, not {}", e.key, e.value.describe());
    let base = match at {
        Origin::File(path, _) => path.parent().map(Path::to_path_buf).unwrap_or_default(),
        Origin::CommandLine => PathBuf::new(),
    };
    let absolute = |s: &str| {
        let p = Path::new(s);
        if p.is_absolute() {
            s.to_string()
        } else {
            base.join(p).display().to_string()
        }
    };
    let strings = |v: &Value| -> Option<Vec<String>> {
        match v {
            Value::Str(s) => Some(vec![s.clone()]),
            Value::List(items) => items
                .iter()
                .map(|i| match i {
                    Value::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    };
    match kind {
        Kind::Path | Kind::Text => {
            let Value::Str(s) = &e.value else {
                return Err(bad("a string"));
            };
            if s.is_empty() {
                return Err(bad("a non-empty string"));
            }
            if kind == Kind::Path {
                e.value = Value::Str(absolute(s));
            }
        }
        Kind::Paths | Kind::Texts | Kind::Coordinates => {
            let Some(items) = strings(&e.value).filter(|v| !v.is_empty() && v.iter().all(|s| !s.is_empty())) else {
                return Err(bad("a string or a list of strings"));
            };
            if let Some(item) = items.iter().find(|s| s.contains(',')) {
                return Err(format!(
                    "{at}: `{}`: \"{item}\" holds a comma, which the replay cannot read",
                    e.key
                ));
            }
            if kind == Kind::Coordinates {
                for c in &items {
                    let parts: Vec<&str> = c.split('/').collect();
                    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
                        return Err(format!(
                            "{at}: `{}`: \"{c}\" is not a store coordinate, SOURCE/INTERVAL/SYMBOL (\"kis-krx/1d/005930\")",
                            e.key
                        ));
                    }
                    if !INTERVALS.contains(&parts[1]) {
                        return Err(format!(
                            "{at}: `{}`: \"{c}\": the interval is one of {}",
                            e.key,
                            INTERVALS.join(", ")
                        ));
                    }
                }
            }
            let items = if kind == Kind::Paths {
                items.iter().map(|s| absolute(s)).collect()
            } else {
                items
            };
            e.value = Value::List(items.into_iter().map(Value::Str).collect());
        }
        Kind::Choice(options) => match &e.value {
            Value::Str(s) if options.contains(&s.as_str()) => {}
            _ => {
                let list: Vec<String> = options.iter().map(|o| format!("\"{o}\"")).collect();
                return Err(bad(&format!("one of {}", list.join(", "))));
            }
        },
        Kind::Number { min, strict } => match e.value {
            Value::Num(_, v) if v > min || (!strict && v == min) => {}
            _ => {
                return Err(bad(if strict {
                    "a number above 0"
                } else {
                    "a number, 0 or more"
                }));
            }
        },
        Kind::Count => match e.value {
            Value::Num(_, v) if v >= 0.0 && v.fract() == 0.0 && v <= 1e9 => {
                e.value = Value::Num(format!("{v}"), v);
            }
            _ => return Err(bad("a whole number, 0 or more")),
        },
        Kind::Bool => {
            if !matches!(e.value, Value::Bool(_)) {
                return Err(bad("`true` or `false`"));
            }
        }
        Kind::Date => {
            let digits = match &e.value {
                Value::Num(t, _) => t.clone(),
                Value::Str(s) if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' => {
                    s.replace('-', "")
                }
                Value::Str(s) => s.clone(),
                _ => String::new(),
            };
            match date(&digits) {
                Some(d) => e.value = Value::Num(d.to_string(), d as f64),
                None => return Err(bad("a date, \"YYYY-MM-DD\" or yyyymmdd")),
            }
        }
        Kind::Instant => {
            let ok = match &e.value {
                Value::Str(s) => instant(s),
                _ => false,
            };
            if !ok {
                return Err(bad(
                    "an instant, \"YYYY-MM-DD\" or \"YYYY-MM-DDTHH:MM[:SS]\" with an optional offset",
                ));
            }
        }
        Kind::Offset => {
            let ok = match &e.value {
                Value::Str(s) => offset(s),
                _ => false,
            };
            if !ok {
                return Err(bad("a fixed offset, \"UTC\" or \"+HH:MM\""));
            }
        }
    }
    Ok(())
}

/// A yyyymmdd date that exists, as a number.
fn date(digits: &str) -> Option<u32> {
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let d: u32 = digits.parse().ok()?;
    let (y, m, day) = (d / 10000, d / 100 % 100, d % 100);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (y >= 1970 && (1..=days).contains(&day)).then_some(d)
}

/// `YYYY-MM-DD`, optionally `THH:MM[:SS]` (or a space for the `T`) and `Z` or `+HH:MM`.
fn instant(s: &str) -> bool {
    let (day, rest) = s.split_at(s.len().min(10));
    if day.len() != 10 || date(&day.replace('-', "")).is_none() || &day[4..5] != "-" || &day[7..8] != "-" {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    let Some(time) = rest.strip_prefix(['T', ' ']) else {
        return false;
    };
    let (clock, zone) = match time.find(['Z', '+', '-']) {
        Some(k) => time.split_at(k),
        None => (time, ""),
    };
    let fields: Vec<&str> = clock.split(':').collect();
    let two = |f: &str, max: u32| f.len() == 2 && f.parse::<u32>().is_ok_and(|v| v <= max);
    let clock_ok = matches!(fields.len(), 2 | 3)
        && two(fields[0], 23)
        && two(fields[1], 59)
        && fields.get(2).is_none_or(|f| two(f, 59));
    clock_ok && (zone.is_empty() || offset(zone))
}

/// `UTC`, `Z`, or `+HH:MM` / `-HH:MM`.
fn offset(s: &str) -> bool {
    if s == "UTC" || s == "Z" {
        return true;
    }
    let Some(rest) = s.strip_prefix(['+', '-']) else {
        return false;
    };
    let b = rest.as_bytes();
    rest.len() == 5
        && b[2] == b':'
        && rest[..2].parse::<u32>().is_ok_and(|h| h <= 14)
        && rest[3..].parse::<u32>().is_ok_and(|m| m <= 59)
        && b[..2].iter().chain(&b[3..]).all(u8::is_ascii_digit)
}

/// Merges the layers (later winning: the program's `replay {}`, the spec, the command line) and checks them
/// together against the program; every error found.
pub fn resolve(layers: &[Vec<Entry>], program: &Program) -> Result<Run, Vec<String>> {
    let mut merged: Vec<Entry> = vec![];
    for layer in layers {
        for e in layer {
            match merged.iter_mut().find(|m| m.key == e.key) {
                Some(m) => *m = e.clone(),
                None => merged.push(e.clone()),
            }
        }
    }
    let mut errors = vec![];
    let get = |key: &str| merged.iter().find(|e| e.key == key);
    let text = |key: &str| get(key).map(|e| e.value.text());

    // The data source, and the keys that belong to it.
    let source = text("data.source").unwrap_or_else(|| {
        if get("data.store").is_some() {
            "store".into()
        } else {
            "wide".into()
        }
    });
    for e in &merged {
        if let Some(k) = known(&e.key) {
            if !k.sources.is_empty() && !k.sources.contains(&source.as_str()) {
                errors.push(format!(
                    "{}: `{}` applies to data.source = {}, and this run reads \"{source}\"",
                    e.origin,
                    e.key,
                    k.sources
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ));
            }
        }
    }
    let both = |a: &str, b: &str, errors: &mut Vec<String>| {
        if let (Some(x), Some(y)) = (get(a), get(b)) {
            errors.push(format!(
                "{}: `{a}` and `{b}` ({}) name the same thing: give one",
                x.origin, y.origin
            ));
        }
    };
    match source.as_str() {
        "wide" => {
            both("data.file", "data.close", &mut errors);
            if get("data.file").is_none() && get("data.close").is_none() {
                errors.push(
                    "the run reads wide tables (data.source = \"wide\"): give the closes, `data.file` or `data.close`"
                        .into(),
                );
            }
        }
        "csv" => {
            both("data.file", "data.files", &mut errors);
            both("data.symbol", "data.symbols", &mut errors);
            if get("data.file").is_none() && get("data.files").is_none() {
                errors.push("the run reads CSV (data.source = \"csv\"): give `data.file` or `data.files`".into());
            }
            let count = |key: &str| match get(key).map(|e| &e.value) {
                Some(Value::List(items)) => items.len(),
                _ => 0,
            };
            let layout = text("data.layout").unwrap_or_else(|| "long".into());
            if layout == "wide" {
                for e in merged.iter().filter(|e| {
                    e.key.starts_with("data.columns.")
                        || matches!(e.key.as_str(), "data.symbol" | "data.symbols" | "data.symbol_column")
                }) {
                    errors.push(format!(
                        "{}: `{}` applies to data.layout = \"long\": a wide CSV names its symbols in its header",
                        e.origin, e.key
                    ));
                }
            } else if get("data.symbol_column").is_some() {
                for k in ["data.symbol", "data.symbols"] {
                    if let Some(e) = get(k) {
                        errors.push(format!(
                            "{}: `{k}` and `data.symbol_column` both name symbols: give one",
                            e.origin
                        ));
                    }
                }
            } else if get("data.files").is_some() {
                if count("data.symbols") != count("data.files") {
                    errors.push(format!(
                        "data.files names {} files and data.symbols {} symbols: give one symbol per file (or data.symbol_column)",
                        count("data.files"),
                        count("data.symbols")
                    ));
                }
            } else if get("data.file").is_some() && get("data.symbol").is_none() {
                errors.push(
                    "a long CSV names its symbol: give `data.symbol`, or `data.symbol_column` for a file of many symbols"
                        .into(),
                );
            }
        }
        _ => {
            for k in ["data.store", "data.store_root"] {
                if get(k).is_none() {
                    errors.push(format!("the run reads a store (data.source = \"store\"): give `{k}`"));
                }
            }
        }
    }

    // Fills.
    let fill = text("replay.fill").unwrap_or_else(|| "next-open".into());
    let at = |key: &str| get(key).map(|e| format!("{}: ", e.origin)).unwrap_or_default();
    if program.legs && fill != "legs" {
        errors.push(format!(
            "{}the program has on_open or on_close: set replay.fill = \"legs\"",
            at("replay.fill")
        ));
    }
    if !program.legs && fill == "legs" {
        errors.push(format!(
            "{}replay.fill = \"legs\" feeds each day as an open and a close, for a program with on_open or on_close",
            at("replay.fill")
        ));
    }
    if fill == "open-prev-close" && source != "wide" {
        errors.push(format!(
            "{}replay.fill = \"open-prev-close\" reads wide tables of closes (data.source = \"wide\")",
            at("replay.fill")
        ));
    }
    if fill == "legs" && program.legs && source == "wide" {
        for k in ["data.open", "data.high", "data.low"] {
            if get(k).is_none() {
                errors.push(format!("replay.fill = \"legs\" on wide tables needs `{k}`"));
            }
        }
    }
    if fill == "legs" && program.legs && source == "csv" && text("data.interval").is_some_and(|i| i != "1d") {
        errors.push(format!(
            "{}replay.fill = \"legs\" needs daily bars",
            at("data.interval")
        ));
    }
    if fill == "legs" && program.legs && source == "store" {
        if let Some(Value::List(items)) = get("data.store").map(|e| &e.value) {
            for c in items
                .iter()
                .map(Value::text)
                .filter(|c| c.split('/').nth(1) != Some("1d"))
            {
                errors.push(format!(
                    "{}replay.fill = \"legs\" needs daily bars, not \"{c}\"",
                    at("data.store")
                ));
            }
        }
    }
    if let (Some(from), Some(to)) = (get("replay.from"), get("replay.to")) {
        if from.value.text() > to.value.text() {
            errors.push(format!(
                "{}: replay.from is after replay.to ({})",
                from.origin, to.origin
            ));
        }
    }

    // Instruments.
    let instruments: Vec<&Entry> = merged.iter().filter(|e| e.key.starts_with("instruments.")).collect();
    match &program.named {
        Some(names) => {
            for n in names {
                if get(&format!("instruments.{n}")).is_none() {
                    errors.push(format!(
                        "the program names instrument `{n}`: map it to a symbol, `{n} = \"...\"` in [instruments]"
                    ));
                }
            }
            for e in &instruments {
                let n = &e.key["instruments.".len()..];
                if !names.iter().any(|m| m == n) {
                    errors.push(format!(
                        "{}: the program has no instrument `{n}` (it names {})",
                        e.origin,
                        names.join(", ")
                    ));
                }
            }
        }
        None if !program.universe && instruments.len() > 1 => {
            errors.push(format!(
                "{}: a program without instruments {{}} or universe {{}} trades one symbol: [instruments] holds at most one",
                instruments[1].origin
            ));
        }
        None => {}
    }

    // Parameters.
    let mut args: Vec<(String, String)> = vec![];
    for e in merged.iter().filter(|e| e.key.starts_with("params.")) {
        let name = &e.key["params.".len()..];
        let Some((_, ty)) = program.params.iter().find(|(p, _)| p == name) else {
            let list: Vec<&str> = program.params.iter().map(|(p, _)| p.as_str()).collect();
            errors.push(format!(
                "{}: the program has no parameter `{name}`{}",
                e.origin,
                if list.is_empty() {
                    String::from(" (it has none)")
                } else {
                    format!(" (it has {})", list.join(", "))
                }
            ));
            continue;
        };
        let value = match (ty, &e.value) {
            (ParamType::Bool, Value::Bool(b)) => (if *b { "1" } else { "0" }).to_string(),
            (ParamType::Int, Value::Num(_, v)) if v.fract() == 0.0 && v.abs() <= 9e15 => format!("{v}"),
            (ParamType::Real, Value::Num(t, _)) => t.clone(),
            (ParamType::Bool, _) => {
                errors.push(format!(
                    "{}: `{name}` is a bool parameter: give `true` or `false`",
                    e.origin
                ));
                continue;
            }
            (ParamType::Int, _) => {
                errors.push(format!(
                    "{}: `{name}` is an int parameter: give a whole number of magnitude at most 9e15, not {}",
                    e.origin,
                    e.value.describe()
                ));
                continue;
            }
            (ParamType::Real, _) => {
                errors.push(format!("{}: `{name}` is a real parameter: give a number", e.origin));
                continue;
            }
        };
        args.push((format!("param.{name}"), value));
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut run_args: Vec<(String, String)> = vec![("data.source".into(), source.clone())];
    for e in &merged {
        if e.key == "strategy" || e.key == "data.source" || e.key.starts_with("build.") || e.key.starts_with("params.")
        {
            continue;
        }
        let k = match e.key.strip_prefix("instruments.") {
            Some(n) => format!("instrument.{n}"),
            None => e.key.clone(),
        };
        run_args.push((k, e.value.text()));
    }
    run_args.extend(args);
    let path = |key: &str| text(key).map(PathBuf::from);
    Ok(Run {
        algotradelib: path("build.algotradelib"),
        duckdb: path("build.duckdb"),
        indicators: text("build.indicators")
            .and_then(|i| Indicators::from_name(&i))
            .unwrap_or(Indicators::Incremental),
        store: source == "store",
        args: run_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(src: &str) -> Result<Vec<Entry>, Vec<String>> {
        let mut entries = toml::parse(src, Path::new("/specs/s.toml")).map_err(|e| vec![e])?;
        let errors: Vec<String> = entries.iter_mut().filter_map(|e| check(e).err()).collect();
        if errors.is_empty() { Ok(entries) } else { Err(errors) }
    }

    fn one_error(src: &str) -> String {
        let e = spec(src).expect_err(src);
        assert_eq!(e.len(), 1, "{e:?}");
        e[0].clone()
    }

    fn single() -> Program {
        Program {
            params: vec![
                ("lookback".into(), ParamType::Int),
                ("entry".into(), ParamType::Real),
                ("flag".into(), ParamType::Bool),
            ],
            ..Program::default()
        }
    }

    fn run(src: &str, program: &Program) -> Result<Run, Vec<String>> {
        resolve(&[spec(src)?], program)
    }

    fn arg<'a>(r: &'a Run, key: &str) -> Option<&'a str> {
        r.args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    const WIDE_SPEC: &str = "strategy = \"m.atls\"\n[data]\nfile = \"closes.csv\"\n";

    #[test]
    fn values_are_checked_key_by_key() {
        for (src, says) in [
            ("stratgy = \"m.atls\"", "unknown setting `stratgy`"),
            ("[data]\nsorce = \"csv\"", "unknown setting `data.sorce`"),
            ("[build]\ncache = \"x\"", "unknown setting `build.cache`"),
            (
                "[build]\nindicators = \"pairwise\"",
                "`build.indicators` is one of \"incremental\", \"naive\", \"exact\"",
            ),
            (
                "[output]\nfills = \"f.csv\"",
                "tables: [data], [instruments], [replay], [params], [build]",
            ),
            ("[params.x]\ny = 1", "unknown setting `params.x.y`"),
            ("[params]\nlookback = \"20\"", "is a number or a boolean"),
            ("[instruments]\ny = 5930", "`instruments.y` is a string, not 5930"),
            ("[replay]\ncash = 0", "`replay.cash` is a number above 0, not 0"),
            ("[replay]\ncash = \"1000\"", "a number above 0"),
            ("[replay]\ncommission = -0.1", "a number, 0 or more"),
            ("[replay]\nslippage_ticks = 1.5", "a whole number"),
            ("[replay]\nshorts = \"yes\"", "`true` or `false`"),
            ("[replay]\nfill = \"close\"", "one of \"next-open\""),
            ("[replay]\nfrom = 20230230", "a date"),
            ("[replay]\nfrom = \"2023-13-01\"", "a date"),
            ("[replay]\nmarket = \"jp\"", "one of \"us\", \"kr\""),
            ("[data]\nstore = \"kis/1d\"", "is not a store coordinate"),
            ("[data]\nstore = \"kis/2d/005930\"", "the interval is one of"),
            ("[data]\nstore = [\"a/1d/x\", 3]", "a string or a list of strings"),
            ("[data]\nas_of = \"2026-10-01 25:00\"", "an instant"),
            ("[data]\ntimezone = \"Asia/Seoul\"", "a fixed offset"),
            ("[data]\ninterval = \"2h\"", "one of \"1m\""),
            ("[data]\nfiles = [\"a,b.csv\"]", "holds a comma"),
            ("[data]\nsymbol = \"\"", "a non-empty string"),
            ("strategy = 3", "`strategy` is a string, not 3"),
        ] {
            let e = one_error(src);
            assert!(e.contains(says), "{src}: {e}");
            assert!(e.starts_with("/specs/s.toml:"), "{e}");
        }
    }

    #[test]
    fn values_are_normalized() {
        let e = spec(
            "strategy = \"../m.atls\"\n[data]\nfiles = [\"a.csv\", \"/abs/b.csv\"]\nstore = \"x/1d/y\"\nas_of = \"2026-10-01T05:00:00Z\"\n\
             [replay]\nfrom = \"2020-01-02\"\nto = 20201231\nslippage_ticks = 2.0\n",
        )
        .unwrap();
        let v = |k: &str| e.iter().find(|x| x.key == k).unwrap().value.text();
        assert_eq!(
            v("strategy"),
            Path::new("/specs").join("../m.atls").display().to_string()
        );
        assert_eq!(
            v("data.files"),
            format!("{},/abs/b.csv", Path::new("/specs").join("a.csv").display())
        );
        assert_eq!(v("data.store"), "x/1d/y");
        assert_eq!(v("replay.from"), "20200102");
        assert_eq!(v("replay.to"), "20201231");
        assert_eq!(v("replay.slippage_ticks"), "2");
        assert!(instant("2026-10-01") && instant("2026-10-01 05:00") && instant("2026-10-01T05:00:00+09:00"));
        assert!(!instant("2026-10-01T5:00") && !instant("2026/10/01") && !instant("2026-10-01T05:00+9"));
    }

    #[test]
    fn a_spec_names_its_program() {
        let dir = std::env::temp_dir().join(format!("atlc-spec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.toml");
        std::fs::write(&path, "[replay]\ncash = 1\n").unwrap();
        assert!(read(&path).unwrap_err()[0].contains("no `strategy"));
        std::fs::write(&path, "strategy = 'm.atls'\n").unwrap();
        assert_eq!(read(&path).unwrap().0, dir.join("m.atls"));
        std::fs::write(&path, "strategy = 'm.atls'\nfoo = 1\nbar = 2\n").unwrap();
        assert_eq!(read(&path).unwrap_err().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(read(&path).unwrap_err()[0].contains("cannot read"));
    }

    #[test]
    fn layers_merge_later_winning() {
        let program_layer = vec![
            Entry {
                key: "replay.cash".into(),
                value: Value::Num("5".into(), 5.0),
                origin: Origin::File("p.atls".into(), 3),
            },
            Entry {
                key: "replay.shorts".into(),
                value: Value::Bool(true),
                origin: Origin::File("p.atls".into(), 4),
            },
        ];
        let spec_layer = spec(&format!("{WIDE_SPEC}[replay]\ncash = 7\n[params]\nlookback = 30\n")).unwrap();
        let cli = vec![from_command_line("lookback=40").unwrap()];
        let r = resolve(&[program_layer, spec_layer, cli], &single()).unwrap();
        assert_eq!(arg(&r, "replay.cash"), Some("7"));
        assert_eq!(arg(&r, "replay.shorts"), Some("true"));
        assert_eq!(arg(&r, "param.lookback"), Some("40"));
        assert_eq!(arg(&r, "data.source"), Some("wide"));
        assert_eq!(r.indicators, Indicators::Incremental);
        assert!(!r.store);
    }

    #[test]
    fn parameters_are_checked_against_the_program() {
        let p = single();
        let ok = run(
            &format!("{WIDE_SPEC}[params]\nlookback = 3e1\nentry = 1.50\nflag = true\n"),
            &p,
        )
        .unwrap();
        assert_eq!(arg(&ok, "param.lookback"), Some("30"));
        assert_eq!(arg(&ok, "param.entry"), Some("1.50"));
        assert_eq!(arg(&ok, "param.flag"), Some("1"));
        for (params, says) in [
            ("lokback = 3", "no parameter `lokback` (it has lookback, entry, flag)"),
            ("lookback = 2.5", "`lookback` is an int parameter"),
            ("lookback = 1e16", "`lookback` is an int parameter"),
            ("lookback = true", "`lookback` is an int parameter"),
            ("entry = false", "`entry` is a real parameter"),
            ("flag = 1", "`flag` is a bool parameter"),
        ] {
            let e = run(&format!("{WIDE_SPEC}[params]\n{params}\n"), &p).unwrap_err();
            assert!(e.len() == 1 && e[0].contains(says), "{params}: {e:?}");
        }
        assert!(from_command_line("lookback").unwrap_err().contains("name=value"));
        assert!(from_command_line("lookback=inf").unwrap_err().contains("not a number"));
        assert!(from_command_line("lookback=ten").unwrap_err().contains("not a number"));
        assert_eq!(from_command_line("x=+2").unwrap().value, Value::Num("2".into(), 2.0));
    }

    #[test]
    fn data_keys_belong_to_their_source() {
        let p = single();
        for (src, says) in [
            (
                "[data]\nfile = \"c.csv\"\ncolumns.open = \"Open\"",
                "`data.columns.open` applies to data.source = \"csv\"",
            ),
            (
                "[data]\nsource = \"csv\"\nfile = \"c.csv\"\nsymbol = \"A\"\nnames = \"n.csv\"",
                "`data.names` applies to data.source = \"wide\"",
            ),
            (
                "[data]\nstore = \"a/1d/b\"\nstore_root = \"/s\"\nfile = \"c.csv\"",
                "`data.file` applies to data.source = \"wide\" or \"csv\"",
            ),
            (
                "[data]\nsource = \"wide\"\nfile = \"c.csv\"\nstore = \"a/1d/b\"",
                "`data.store` applies to data.source = \"store\"",
            ),
            ("[data]\nnames = \"n.csv\"", "give the closes"),
            ("[data]\nfile = \"c.csv\"\nclose = \"c.csv\"", "name the same thing"),
            ("[data]\nsource = \"csv\"", "give `data.file` or `data.files`"),
            ("[data]\nsource = \"csv\"\nfile = \"c.csv\"", "give `data.symbol`"),
            (
                "[data]\nsource = \"csv\"\nfiles = [\"a\", \"b\"]\nsymbols = \"A\"",
                "2 files and data.symbols 1 symbols",
            ),
            (
                "[data]\nsource = \"csv\"\nfile = \"c\"\nsymbol = \"A\"\nsymbol_column = \"s\"",
                "both name symbols",
            ),
            (
                "[data]\nsource = \"csv\"\nlayout = \"wide\"\nfile = \"c\"\nsymbol = \"A\"",
                "applies to data.layout = \"long\"",
            ),
            ("[data]\nstore = \"a/1d/b\"", "give `data.store_root`"),
            ("[data]\nsource = \"store\"\nstore_root = \"/s\"", "give `data.store`"),
            (
                "[data]\nfile = \"c\"\n[replay]\nfrom = 20200102\nto = 20200101",
                "replay.from is after replay.to",
            ),
        ] {
            let e = run(&format!("strategy = \"m.atls\"\n{src}\n"), &p).unwrap_err();
            assert!(e.len() == 1 && e[0].contains(says), "{src}: {e:?}");
        }
        let store = run(
            "strategy = \"m.atls\"\n[data]\nstore = \"a/1d/b\"\nstore_root = \"/s\"\n",
            &p,
        )
        .unwrap();
        assert!(store.store);
        assert_eq!(arg(&store, "data.source"), Some("store"));
        let csv = run(
            "strategy = \"m.atls\"\n[data]\nsource = \"csv\"\nfiles = [\"a\", \"b\"]\nsymbols = [\"A\", \"B\"]\n",
            &p,
        )
        .unwrap();
        assert_eq!(arg(&csv, "data.symbols"), Some("A,B"));
    }

    #[test]
    fn fills_match_the_program_and_the_data() {
        let legs = Program {
            universe: true,
            legs: true,
            ..Program::default()
        };
        let single = single();
        for (src, program, says) in [
            ("[data]\nfile = \"c\"", &legs, "set replay.fill = \"legs\""),
            (
                "[data]\nfile = \"c\"\n[replay]\nfill = \"legs\"",
                &single,
                "for a program with on_open or on_close",
            ),
            (
                "[data]\nfile = \"c\"\nopen = \"o\"\nhigh = \"h\"\n[replay]\nfill = \"legs\"",
                &legs,
                "needs `data.low`",
            ),
            (
                "[data]\nsource = \"csv\"\nfile = \"c\"\nsymbol = \"A\"\n[replay]\nfill = \"open-prev-close\"",
                &single,
                "reads wide tables",
            ),
            (
                "[data]\nsource = \"csv\"\nfile = \"c\"\nsymbol = \"A\"\ninterval = \"1h\"\n[replay]\nfill = \"legs\"",
                &legs,
                "needs daily bars",
            ),
            (
                "[data]\nstore = [\"a/1d/b\", \"a/1m/c\"]\nstore_root = \"/s\"\n[replay]\nfill = \"legs\"",
                &legs,
                "not \"a/1m/c\"",
            ),
        ] {
            let e = run(&format!("strategy = \"m.atls\"\n{src}\n"), program).unwrap_err();
            assert!(e.len() == 1 && e[0].contains(says), "{src}: {e:?}");
        }
    }

    #[test]
    fn instruments_match_the_program() {
        let named = Program {
            named: Some(vec!["y".into(), "x".into()]),
            ..Program::default()
        };
        let ok = run(&format!("{WIDE_SPEC}[instruments]\ny = \"USO\"\nx = \"GLD\"\n"), &named).unwrap();
        assert_eq!(arg(&ok, "instrument.y"), Some("USO"));
        let e = run(&format!("{WIDE_SPEC}[instruments]\ny = \"USO\"\nz = \"GLD\"\n"), &named).unwrap_err();
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(e[0].contains("map it to a symbol, `x = \"...\"`"), "{e:?}");
        assert!(e[1].contains("no instrument `z` (it names y, x)"), "{e:?}");
        let e = run(&format!("{WIDE_SPEC}[instruments]\na = \"A\"\nb = \"B\"\n"), &single()).unwrap_err();
        assert!(e[0].contains("at most one"), "{e:?}");
        let universe = Program {
            universe: true,
            ..Program::default()
        };
        assert!(run(&format!("{WIDE_SPEC}[instruments]\na = \"A\"\nb = \"B\"\n"), &universe).is_ok());
    }

    #[test]
    fn replay_block_settings() {
        use crate::frontend::ast::Span;
        use crate::frontend::ast::{Lit, Setting};
        let source = "replay {\n  data.file = \"c.csv\";\n  replay.cash = 100;\n}\n";
        let at = |line_start: usize| Span {
            start: line_start,
            end: line_start + 1,
        };
        let s = |k: &str, v: Lit, start| Setting {
            key: k.into(),
            value: v,
            span: at(start),
        };
        let ok = from_replay(
            &[
                s("data.file", Lit::Str("c.csv".into()), 11),
                s("replay.cash", Lit::Num(1e5), 33),
            ],
            Path::new("/p/m.atls"),
            source,
        )
        .unwrap();
        assert_eq!(ok[0].value.text(), Path::new("/p").join("c.csv").display().to_string());
        assert_eq!(ok[0].origin.to_string(), "/p/m.atls:2");
        assert_eq!(ok[1].value.text(), "100000");
        let e = from_replay(
            &[
                s("params.lookback", Lit::Num(3.0), 11),
                s("replay.cash", Lit::Num(1.0), 11),
                s("replay.cash", Lit::Num(2.0), 33),
                s("replay.fill", Lit::Num(2.0), 33),
            ],
            Path::new("/p/m.atls"),
            source,
        )
        .unwrap_err();
        assert_eq!(e.len(), 3, "{e:?}");
        assert!(e[0].contains("cannot be set in `replay {}`"));
        assert!(e[1].contains("/p/m.atls:3: `replay.cash` is already set on /p/m.atls:2"));
        assert!(e[2].contains("`replay.fill` is one of"));
    }
}
