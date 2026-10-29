//! Names and blocks: declarations, reserved names, unknown names with suggestions, and each block kind at most once.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};

use super::*;

pub(super) fn is_reserved(name: &str) -> bool {
    [
        "open",
        "high",
        "low",
        "close",
        "volume",
        "held",
        "keep",
        "fill",
        "sma",
        "ema",
        "rsi",
        "lag",
        "ts_max",
        "ts_min",
        "ts_sum",
        "abs",
        "min",
        "max",
        "defined",
        "lower",
        "upper",
        "sign",
        "log",
        "sqrt",
        "round",
        "floor",
        "stdev",
        "zscore",
        "ols_slope",
        "ols_intercept",
        "kf_beta",
        "kf_error",
        "kf_sd",
        "xs_mean",
        "xs_sum",
        "xs_count",
        "xs_rank",
        "bars",
        "periods",
        "stdev_pop",
    ]
    .contains(&name)
}

pub(super) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1)
                .min(cur[j - 1] + 1)
                .min(prev[j - 1] + if a[i - 1] == b[j - 1] { 0 } else { 1 });
        }
        prev = cur;
    }
    prev[b.len()]
}

impl Checker<'_> {
    pub(super) fn declare(&mut self, ident: &Ident, name: Name) -> bool {
        if is_reserved(&ident.name) {
            self.err(Diag::new(
                Code::BuiltInName,
                format!("`{}` is a built-in name", ident.name),
                ident.span,
                "choose another name",
            ));
            return false;
        }
        if let Some((_, prev)) = self.names.get(&ident.name) {
            let prev = *prev;
            self.err(
                Diag::new(
                    Code::DuplicateName,
                    format!("`{}` is declared twice", ident.name),
                    ident.span,
                    "second declaration",
                )
                .also(prev, "first declared here"),
            );
            return false;
        }
        self.names.insert(ident.name.clone(), (name, ident.span));
        true
    }

    pub(super) fn unknown(&mut self, n: &str, span: Span) {
        let mut d = Diag::new(Code::UnknownName, format!("unknown name `{}`", n), span, "not declared");
        let mut best: Option<(&str, usize)> = None;
        let mut known: Vec<String> = self
            .names
            .keys()
            .cloned()
            .chain(
                ["open", "high", "low", "close", "volume", "held", "keep"]
                    .iter()
                    .map(|s| s.to_string()),
            )
            .collect();
        // Sorted, so that of several names at the same distance the suggestion is always the same one.
        known.sort();
        for k in &known {
            let dist = edit_distance(n, k);
            if dist <= 2 && best.is_none_or(|b| dist < b.1) {
                best = Some((k.as_str(), dist));
            }
        }
        if let Some((k, _)) = best {
            d = d.note(format!("did you mean `{}`?", k));
        }
        self.err(d);
    }

    /// Each block kind at most once (`indicator` excepted).
    pub(super) fn duplicate_blocks(&mut self, program: &Program) {
        let mut seen: HashMap<&'static str, Span> = HashMap::new();
        for item in program.items.iter() {
            let (key, span) = match item {
                Item::Params(_, s) => ("params", *s),
                Item::Series(_, s) => ("series", *s),
                Item::State(_, s) => ("state", *s),
                Item::Warmup(_, s) => ("warmup", *s),
                Item::Handler(k, _, s) => (k.name(), *s),
                Item::Instruments(_, s) => ("instruments", *s),
                Item::Universe(s) => ("universe", *s),
                Item::Replay(_, s) => ("replay", *s),
                Item::Options(_, s) => ("options", *s),
                Item::Functions(_, s) => ("functions", *s),
                Item::Indicator(_) => continue,
            };
            if let Some(prev) = seen.get(key) {
                self.err(
                    Diag::new(
                        Code::DuplicateBlock,
                        format!("a second `{}`", key),
                        Span::new(span.start, span.start + key.len()),
                        "second",
                    )
                    .also(Span::new(prev.start, prev.start + key.len()), "first here"),
                );
            }
            seen.insert(key, span);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{errors, message, ok};
    use super::edit_distance;
    use crate::frontend::diag::Code;

    #[test]
    fn declarations() {
        assert_eq!(
            errors("params { int close = 1; } on_bar { target(1); }"),
            [Code::BuiltInName]
        );
        assert_eq!(
            errors("params { int lower = 1; } on_bar { target(1); }"),
            [Code::BuiltInName]
        );
        assert_eq!(
            errors("params { int a = 1; } state { int a = 0; } on_bar { target(1); }"),
            [Code::DuplicateName]
        );
        assert_eq!(
            errors("on_bar { int a = 1; int a = 2; target(a); }"),
            [Code::ShadowedName]
        );
        // Contextual indicator words are ordinary names outside an indicator.
        ok("params { int<lower=1> window = 3; } series { real value = sma(close, window); } on_bar { target(value); }");
    }

    #[test]
    fn blocks_at_most_once() {
        assert_eq!(
            errors("on_bar { target(1); } on_bar { target(2); }"),
            [Code::DuplicateBlock]
        );
        assert_eq!(
            errors("series { } series { } on_bar { target(1); }"),
            [Code::DuplicateBlock]
        );
        ok(
            "indicator a(real x) { value real v = x; } indicator b(real x) { value real v = x; } \
            series { real p = a(close); real q = b(close); } on_bar { target(p + q); }",
        );
    }

    #[test]
    fn unknown_names_suggest_the_nearest_deterministically() {
        assert_eq!(errors("on_bar { target(clse); }"), [Code::UnknownName]);
        let suggest = |src: &str| crate::check(src).unwrap_err()[0].notes.clone();
        assert_eq!(suggest("on_bar { target(clse); }"), ["did you mean `close`?"]);
        // `ab` and `bb` are both one edit from `cb`: the first in order wins, every time.
        for _ in 0..20 {
            assert_eq!(
                suggest("params { int bb = 1; int ab = 1; } on_bar { target(cb); }"),
                ["did you mean `ab`?"]
            );
        }
        assert!(suggest("on_bar { target(zzzzzz); }").is_empty());
        assert_eq!(message("on_bar { target(zzzzzz); }"), "unknown name `zzzzzz`");
    }

    #[test]
    fn distance() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("same", "same"), 0);
    }
}
