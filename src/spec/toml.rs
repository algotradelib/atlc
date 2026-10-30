//! The TOML subset a run spec is written in, read by hand: tables, dotted bare keys, strings, numbers,
//! booleans and one-line arrays of strings or numbers, flattened to `table.key` entries in file order.
//!
//! Every file it accepts is valid TOML with the same meaning; what TOML has beyond the subset (escapes, quoted
//! keys, inline tables, multi-line arrays and strings, dates, arrays of tables) is refused with a message, never
//! read differently.

use std::path::Path;

use super::{At, Entry, Error, Origin, Value};

/// Reads a run spec into entries, in file order; the first error found, at its line.
pub fn parse(source: &str, file: &Path) -> Result<Vec<Entry>, Error> {
    let mut out: Vec<Entry> = vec![];
    let mut table = String::new();
    let mut tables: Vec<(String, usize)> = vec![];
    // Tables a dotted key creates (`a.b = 1` creates `a`); TOML refuses a header for one of them.
    let mut dotted: Vec<(String, usize)> = vec![];
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let err = |m: String| Error::new(At::Line(file.to_path_buf(), line), m);
        characters(raw).map_err(err)?;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }
        if let Some(rest) = text.strip_prefix('[') {
            if rest.starts_with('[') {
                return Err(err("arrays of tables (`[[...]]`) are not part of the run spec".into()));
            }
            let Some(name) = rest.strip_suffix(']') else {
                return Err(err("a table header is `[name]`".into()));
            };
            let name = dotted_key(name).map_err(err)?;
            if let Some((_, first)) = tables.iter().find(|(t, _)| *t == name) {
                return Err(err(format!("table `[{name}]` is already defined on line {first}")));
            }
            tables.push((name.clone(), line));
            table = name;
            continue;
        }
        let Some(eq) = text.find('=') else {
            return Err(err("expected `key = value` or `[table]`".into()));
        };
        let key = dotted_key(&text[..eq]).map_err(err)?;
        let value = value(text[eq + 1..].trim()).map_err(err)?;
        let full = if table.is_empty() {
            key
        } else {
            format!("{table}.{key}")
        };
        let depth = table.matches('.').count() + usize::from(!table.is_empty());
        let parts: Vec<&str> = full.split('.').collect();
        for k in depth + 1..parts.len() {
            dotted.push((parts[..k].join("."), line));
        }
        if let Some(first) = out.iter().find(|e| e.key == full) {
            return Err(err(format!("`{full}` is already set on {}", first.origin)));
        }
        out.push(Entry {
            key: full,
            value,
            origin: Origin::File(file.to_path_buf(), line),
        });
    }
    // A table is made by a header or by dotted keys, not both (`a.b = 1` with `[a]`, or `[a.b]` with `b.c = 1`
    // under `[a]`); refusing a header that only lies under a dotted table is stricter than TOML.
    for (d, dline) in &dotted {
        let under = format!("{d}.");
        if let Some((t, tline)) = tables.iter().find(|(t, _)| t == d || t.starts_with(&under)) {
            return Err(Error::new(
                At::Line(file.to_path_buf(), *tline),
                format!("table `[{t}]` and the dotted key on line {dline} both define `{d}`; use one or the other"),
            ));
        }
    }
    // A name is a value or a table, not both (`data = "x"` with `[data]`, or `a.b = 1` with `a.b.c = 2`).
    for e in &out {
        let prefix = format!("{}.", e.key);
        if let Some(other) = out.iter().find(|o| o.key.starts_with(&prefix)) {
            return Err(Error::at(
                &e.origin,
                format!("`{}` is a value here and a table on {}", e.key, other.origin),
            ));
        }
        if let Some((t, line)) = tables.iter().find(|(t, _)| *t == e.key || t.starts_with(&prefix)) {
            return Err(Error::at(
                &e.origin,
                format!(
                    "`{}` is a value here and a table on {}:{line} (`[{t}]`)",
                    e.key,
                    file.display()
                ),
            ));
        }
    }
    Ok(out)
}

/// Refuses what TOML forbids in a line: control characters other than tab anywhere, and whitespace other than spaces
/// and tabs outside strings.
fn characters(raw: &str) -> Result<(), String> {
    let mut quote: Option<char> = None;
    let mut comment = false;
    for c in raw.chars() {
        if (c < ' ' && c != '\t') || c == '\u{7f}' {
            return Err(format!("a control character (U+{:04X}) is not allowed", c as u32));
        }
        match (quote, c) {
            _ if comment => {}
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') => comment = true,
            (None, c) if c.is_whitespace() && c != ' ' && c != '\t' => {
                return Err(format!(
                    "U+{:04X} is whitespace TOML does not allow outside a string: use spaces or tabs",
                    c as u32
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The line without its comment: a `#` outside a string starts one.
fn strip_comment(raw: &str) -> &str {
    let mut quote: Option<char> = None;
    for (k, c) in raw.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') => return &raw[..k],
            _ => {}
        }
    }
    raw
}

/// A dotted bare key (`a.b`, spaces allowed around the dots), with its parts joined by single dots.
fn dotted_key(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.starts_with('"') || text.starts_with('\'') {
        return Err(format!(
            "quoted keys (`{text}`) are not part of the run spec: keys are bare (letters, digits, `_`, `-`)"
        ));
    }
    let mut parts = vec![];
    for part in text.split('.') {
        let part = part.trim();
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(format!(
                "`{text}` is not a key: bare names (letters, digits, `_`, `-`) joined by dots"
            ));
        }
        parts.push(part);
    }
    Ok(parts.join("."))
}

/// One value: a string, a number, a boolean, or an array of strings or numbers on one line.
fn value(text: &str) -> Result<Value, String> {
    if text.is_empty() {
        return Err("expected a value after `=`".into());
    }
    if let Some(inner) = text.strip_prefix('[') {
        let Some(inner) = inner.strip_suffix(']') else {
            return Err("an array is written on one line: `[\"a\", \"b\"]`".into());
        };
        let mut items = vec![];
        let mut rest = inner.trim();
        while !rest.is_empty() {
            let (item, after) = scalar_prefix(rest)?;
            match item {
                Value::Str(_) | Value::Num(..) => items.push(item),
                _ => return Err("an array holds strings or numbers".into()),
            }
            let after = after.trim_start();
            rest = match after.strip_prefix(',') {
                Some(r) => r.trim_start(),
                None if after.is_empty() => after,
                None => return Err(format!("expected `,` or `]` before `{after}`")),
            };
        }
        return Ok(Value::List(items));
    }
    if text.starts_with('{') {
        return Err("inline tables (`{ ... }`) are not part of the run spec: write a `[table]`".into());
    }
    let (item, after) = scalar_prefix(text)?;
    if !after.trim().is_empty() {
        return Err(format!("unexpected `{}` after the value", after.trim()));
    }
    Ok(item)
}

/// The scalar at the start of `text`, and what follows it.
fn scalar_prefix(text: &str) -> Result<(Value, &str), String> {
    if text.starts_with("\"\"\"") || text.starts_with("'''") {
        return Err("multi-line strings are not part of the run spec".into());
    }
    if let Some(rest) = text.strip_prefix('"') {
        let Some(end) = rest.find('"') else {
            return Err("this string is never closed".into());
        };
        let s = &rest[..end];
        if s.contains('\\') {
            return Err(format!(
                "\"{s}\": a backslash in a double-quoted string is an escape, which the run spec does not read; write \
                 the path in single quotes ('{s}') or with forward slashes"
            ));
        }
        return Ok((Value::Str(s.to_string()), &rest[end + 1..]));
    }
    if let Some(rest) = text.strip_prefix('\'') {
        let Some(end) = rest.find('\'') else {
            return Err("this string is never closed".into());
        };
        return Ok((Value::Str(rest[..end].to_string()), &rest[end + 1..]));
    }
    let end = text
        .find(|c: char| c == ',' || c == ']' || c.is_whitespace())
        .unwrap_or(text.len());
    let word = &text[..end];
    let rest = &text[end..];
    match word {
        "true" => return Ok((Value::Bool(true), rest)),
        "false" => return Ok((Value::Bool(false), rest)),
        _ => {}
    }
    if let Some(n) = number(word) {
        return Ok((n, rest));
    }
    let date = word.len() >= 10
        && word.as_bytes()[4] == b'-'
        && word.as_bytes()[7] == b'-'
        && word[..4].bytes().all(|b| b.is_ascii_digit());
    if date {
        return Err(format!("`{word}`: write a date as a string (\"{word}\")"));
    }
    if matches!(word.trim_start_matches(['+', '-']), "inf" | "nan") {
        return Err(format!("`{word}` is not a finite number"));
    }
    Err(format!(
        "`{word}` is not a string, number, boolean or array (strings take quotes)"
    ))
}

/// A TOML decimal integer or float (underscores between digits, no leading zeros), as its text without
/// underscores or a leading `+`, and its value.
fn number(word: &str) -> Option<Value> {
    let body = word.strip_prefix(['+', '-']).unwrap_or(word);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(k) => (&body[..k], Some(&body[k + 1..])),
        None => (body, None),
    };
    let (int, frac) = match mantissa.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (mantissa, None),
    };
    let digits = |s: &str| {
        !s.is_empty()
            && !s.starts_with('_')
            && !s.ends_with('_')
            && !s.contains("__")
            && s.chars().all(|c| c.is_ascii_digit() || c == '_')
    };
    if !digits(int) || (int.len() > 1 && int.starts_with('0')) {
        return None;
    }
    if frac.is_some_and(|f| !digits(f)) {
        return None;
    }
    if let Some(e) = exponent
        && !digits(e.strip_prefix(['+', '-']).unwrap_or(e))
    {
        return None;
    }
    let text: String = word.trim_start_matches('+').chars().filter(|c| *c != '_').collect();
    let v: f64 = text.parse().ok()?;
    v.is_finite().then_some(Value::Num(text, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Vec<(String, Value)> {
        parse(src, Path::new("s.toml"))
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .map(|e| (e.key, e.value))
            .collect()
    }

    fn bad(src: &str) -> String {
        parse(src, Path::new("s.toml")).expect_err(src).to_string()
    }

    fn s(v: &str) -> Value {
        Value::Str(v.into())
    }

    #[test]
    fn tables_dotted_keys_and_values() {
        let got = ok(
            "strategy = \"a.atls\"  # comment\n\n[data]\ncolumns . open = 'Open#1'\n[replay]\ncash = 1_000_000\n\
                      shorts = true\ncommission = 1.5e-4\n[data.x]\nfiles = [\"a,b.csv\", 'c.csv',]\n",
        );
        assert_eq!(
            got,
            vec![
                ("strategy".into(), s("a.atls")),
                ("data.columns.open".into(), s("Open#1")),
                ("replay.cash".into(), Value::Num("1000000".into(), 1e6)),
                ("replay.shorts".into(), Value::Bool(true)),
                ("replay.commission".into(), Value::Num("1.5e-4".into(), 1.5e-4)),
                ("data.x.files".into(), Value::List(vec![s("a,b.csv"), s("c.csv")])),
            ]
        );
        assert_eq!(ok("a = []"), vec![("a".into(), Value::List(vec![]))]);
        assert_eq!(ok("a = -0.5"), vec![("a".into(), Value::Num("-0.5".into(), -0.5))]);
        assert_eq!(ok("a = +3"), vec![("a".into(), Value::Num("3".into(), 3.0))]);
        assert_eq!(ok("a = 'C:\\data'"), vec![("a".into(), s("C:\\data"))]);
    }

    #[test]
    fn duplicates_are_errors() {
        assert!(bad("a = 1\na = 2").contains("s.toml:2: `a` is already set on s.toml:1"));
        assert!(bad("[data]\nfile = \"x\"\n[data]\n").contains("table `[data]` is already defined on line 1"));
        assert!(bad("[data]\ncolumns.open = \"o\"\n[data.columns]\nopen = \"p\"").contains("already set"));
        assert!(bad("data = \"x\"\n[data]\nfile = \"y\"").contains("`data` is a value here and a table"));
        assert!(bad("a.b = 1\na.b.c = 2").contains("`a.b` is a value here and a table"));
        // TOML refuses a header for a table that dotted keys made, in either order.
        assert!(bad("data.file = \"x\"\n[data]\nsymbol = \"A\"").contains("both define `data`"));
        assert!(
            bad("[data]\ncolumns.open = \"o\"\n[data.columns]\nhigh = \"h\"").contains("both define `data.columns`")
        );
        assert!(
            bad("[data.columns]\nopen = \"o\"\n[data]\ncolumns.high = \"h\"").contains("both define `data.columns`")
        );
        ok("[data]\ncolumns.open = \"o\"\n[replay]\ncash = 1");
        ok("[a.b]\nx = 1\n[a]\nc.d = 1");
        // A header under a value, in either order.
        assert!(bad("[data]\nfile = \"x\"\n[data.file.child]\n").contains("`data.file` is a value here and a table"));
        assert!(bad("[a]\nb = 1\n[a.b.c]\n").contains("`a.b` is a value here and a table"));
    }

    #[test]
    fn characters_toml_forbids_are_refused() {
        assert!(bad("[data]\nsymbol\u{a0}= \"A\"").contains("U+00A0 is whitespace"));
        assert!(bad("[data]\nsymbol = \"A\u{1}\"").contains("control character (U+0001)"));
        assert!(bad("a = 1 # \u{7f}").contains("control character (U+007F)"));
        assert!(bad("a = 1\r").contains("control character (U+000D)"));
        assert_eq!(ok("a = \"x\u{a0}y\"\t# a\u{3000}comment\r\nb = 2\n").len(), 2);
    }

    #[test]
    fn what_toml_has_beyond_the_subset_is_refused() {
        for (src, says) in [
            ("[[a]]", "arrays of tables"),
            ("[a", "a table header"),
            ("\"a\" = 1", "quoted keys"),
            ("a b = 1", "is not a key"),
            ("a..b = 1", "is not a key"),
            ("a", "expected `key = value`"),
            ("a =", "expected a value"),
            ("a = \"C:\\data\"", "single quotes"),
            ("a = \"open", "never closed"),
            ("a = \"\"\"x\"\"\"", "multi-line"),
            ("a = { b = 1 }", "inline tables"),
            ("a = [1,\n", "on one line"),
            ("a = [true]", "strings or numbers"),
            ("a = [[1]]", "not a string"),
            ("a = [1 2]", "expected `,`"),
            ("a = 2024-01-02", "write a date as a string"),
            ("a = inf", "not a finite number"),
            ("a = nan", "not a finite number"),
            ("a = 0x10", "not a string"),
            ("a = 012", "not a string"),
            ("a = 1__0", "not a string"),
            ("a = 1.", "not a string"),
            ("a = 1e400", "not a string"),
            ("a = yes", "strings take quotes"),
            ("a = \"x\" y", "unexpected `y`"),
        ] {
            let e = bad(src);
            assert!(e.contains(says), "{src}: {e}");
            assert!(e.starts_with("s.toml:"), "{e}");
        }
    }
}
