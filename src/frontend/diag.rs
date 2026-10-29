//! Diagnostics with source spans, rendered in the style of rustc (the line, a caret under the span, a
//! label, then notes) or as one JSON object per line. Every diagnostic has a stable code, listed in
//! `docs/diagnostics.md`.

use crate::frontend::ast::Span;

macro_rules! codes {
    ($($variant:ident = $text:literal,)*) => {
        /// A diagnostic's stable code.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Code {
            $($variant,)*
        }

        impl Code {
            /// Every code, in the order of `docs/diagnostics.md`.
            pub const ALL: &'static [Code] = &[$(Code::$variant,)*];

            /// The code as written in messages and in the JSON form (kebab case).
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Code::$variant => $text,)*
                }
            }
        }
    };
}

codes! {
    // Tokens.
    InvalidNumber = "invalid-number",
    UnclosedString = "unclosed-string",
    UnexpectedCharacter = "unexpected-character",
    // Syntax.
    ExpectedToken = "expected-token",
    ExpectedName = "expected-name",
    KeywordAsName = "keyword-as-name",
    ExpectedBlock = "expected-block",
    UnknownBlock = "unknown-block",
    V0Syntax = "v0-syntax",
    LiveBlock = "live-block",
    ExpectedSetting = "expected-setting",
    ExpectedLiteral = "expected-literal",
    ExpectedIndicatorPart = "expected-indicator-part",
    ExpectedType = "expected-type",
    UnknownType = "unknown-type",
    ExpectedBound = "expected-bound",
    ExpectedExpression = "expected-expression",
    NestingTooDeep = "nesting-too-deep",
    CStyleOperator = "c-style-operator",
    AssignInExpression = "assign-in-expression",
    ChainedComparison = "chained-comparison",
    // Names and blocks.
    BuiltInName = "built-in-name",
    DuplicateName = "duplicate-name",
    ShadowedName = "shadowed-name",
    UnknownName = "unknown-name",
    DuplicateBlock = "duplicate-block",
    MissingOnBar = "missing-on-bar",
    UnusedSeries = "unused-series",
    // Literals and types.
    IntLiteralTooLarge = "int-literal-too-large",
    TypeMismatch = "type-mismatch",
    OperandType = "operand-type",
    ConditionNotBool = "condition-not-bool",
    TernaryArms = "ternary-arms",
    ArgumentType = "argument-type",
    IntSeries = "int-series",
    // Parameters and constants.
    DefaultNotLiteral = "default-not-literal",
    IntDefaultNotWhole = "int-default-not-whole",
    BoolDefaultNotBool = "bool-default-not-bool",
    NumericDefaultIsBool = "numeric-default-is-bool",
    DefaultBelowBound = "default-below-bound",
    DefaultAboveBound = "default-above-bound",
    BoolBounds = "bool-bounds",
    NotConstant = "not-constant",
    NotWholeNumber = "not-whole-number",
    ConstParamNotInt = "const-param-not-int",
    CountDivision = "count-division",
    WarmupNotInt = "warmup-not-int",
    // Definedness.
    MaybeUndefined = "maybe-undefined",
    // Series.
    SeriesReadsState = "series-reads-state",
    SeriesReadsHeld = "series-reads-held",
    KeepInSeries = "keep-in-series",
    FunctionAsValue = "function-as-value",
    NamedArgumentOutsideCall = "named-argument-outside-call",
    // Built-in functions.
    UnknownFunction = "unknown-function",
    FunctionArity = "function-arity",
    TargetAsValue = "target-as-value",
    // Indicators, windows and lookback.
    IndicatorArity = "indicator-arity",
    SeriesNotNumeric = "series-not-numeric",
    LookAhead = "look-ahead",
    WindowBelowOne = "window-below-one",
    WindowBelowTwo = "window-below-two",
    FilterParameters = "filter-parameters",
    IndicatorInHandler = "indicator-in-handler",
    // Handlers and statements.
    AssignParam = "assign-param",
    AssignSeries = "assign-series",
    AssignInstrument = "assign-instrument",
    AssignFunction = "assign-function",
    AssignLocal = "assign-local",
    AssignReadOnly = "assign-read-only",
    TargetInHandler = "target-in-handler",
    TargetNotNumeric = "target-not-numeric",
    ReturnOutsideFunction = "return-outside-function",
    WhileLoop = "while-loop",
    ExpressionStatement = "expression-statement",
    NoBar = "no-bar",
    HeldInHandler = "held-in-handler",
    FillOutsideOnFill = "fill-outside-on-fill",
    FillWithoutField = "fill-without-field",
    UnknownField = "unknown-field",
    NoFields = "no-fields",
    FieldsInSeries = "fields-in-series",
    // Instruments and universes.
    EmptyInstruments = "empty-instruments",
    InstrumentsAndUniverse = "instruments-and-universe",
    InstrumentAsValue = "instrument-as-value",
    BareField = "bare-field",
    NotASeriesField = "not-a-series-field",
    HeldNeedsInstrument = "held-needs-instrument",
    NotAnInstrument = "not-an-instrument",
    TargetNeedsInstrument = "target-needs-instrument",
    TargetNamesInstrument = "target-names-instrument",
    BarsOutsideUniverse = "bars-outside-universe",
    XsInSeries = "xs-in-series",
    XsOutsideUniverse = "xs-outside-universe",
    XsNested = "xs-nested",
    BarAndLegs = "bar-and-legs",
    LegsOutsideUniverse = "legs-outside-universe",
    OpenReadsFuture = "open-reads-future",
    OnFillOutsideSingle = "on-fill-outside-single",
    UniverseLifecycleHandler = "universe-lifecycle-handler",
    // Missing-value policies.
    UnknownOption = "unknown-option",
    UnknownPolicy = "unknown-policy",
    UnknownNamedArgument = "unknown-named-argument",
    NamedArgumentNotAllowed = "named-argument-not-allowed",
    MinWithoutSkip = "min-without-skip",
    MinNotApplicable = "min-not-applicable",
    MinOutOfRange = "min-out-of-range",
    SkipUnsupported = "skip-unsupported",
    // Functions and user indicators.
    DuplicateArgument = "duplicate-argument",
    IntFunctionRealArguments = "int-function-real-arguments",
    MissingReturn = "missing-return",
    NotVisible = "not-visible",
    AssignInFunction = "assign-in-function",
    AssignStateOutsideStep = "assign-state-outside-step",
    LoopBoundsNotInt = "loop-bounds-not-int",
    CallInBody = "call-in-body",
    FieldInBody = "field-in-body",
    IndicatorBoolArgument = "indicator-bool-argument",
    IndicatorWithoutSeries = "indicator-without-series",
    DuplicateWindow = "duplicate-window",
    WindowNotInt = "window-not-int",
    WindowNotConstant = "window-not-constant",
    StepWithoutWindow = "step-without-window",
    StepArity = "step-arity",
    EnterLeavePair = "enter-leave-pair",
    ValueNotReal = "value-not-real",
    IndicatorWithoutValue = "indicator-without-value",
    UnknownValue = "unknown-value",
    AmbiguousValue = "ambiguous-value",
    MemberNotIndicator = "member-not-indicator",
    UserIndicatorInHandler = "user-indicator-in-handler",
}

/// Whether a diagnostic refuses the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    pub span: Span,
    pub label: String,
    pub secondary: Vec<(Span, String)>,
    pub notes: Vec<String>,
}

impl Diag {
    /// An error.
    pub fn new(code: Code, message: impl Into<String>, span: Span, label: impl Into<String>) -> Self {
        Diag {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
            label: label.into(),
            secondary: vec![],
            notes: vec![],
        }
    }
    /// A warning: reported, but the program is valid.
    pub fn warning(code: Code, message: impl Into<String>, span: Span, label: impl Into<String>) -> Self {
        Diag {
            severity: Severity::Warning,
            ..Diag::new(code, message, span, label)
        }
    }
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
    pub fn also(mut self, span: Span, label: impl Into<String>) -> Self {
        self.secondary.push((span, label.into()));
        self
    }
}

/// (line number from 1, column from 1 in characters, line start, line end)
fn line_col(source: &str, offset: usize) -> (usize, usize, usize, usize) {
    let offset = boundary(source, offset);
    let start = source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = source[offset..].find('\n').map(|i| offset + i).unwrap_or(source.len());
    let line = source[..start].matches('\n').count() + 1;
    let col = source[start..offset].chars().count() + 1;
    (line, col, start, end)
}

/// The largest character boundary at or below `i`.
fn boundary(source: &str, i: usize) -> usize {
    let mut i = i.min(source.len());
    while !source.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn marker_line(source: &str, start: usize, end: usize, span: Span, label: &str, marker: char) -> String {
    let from = boundary(source, span.start.clamp(start, end));
    let col = source[start..from].chars().count();
    let span_end = span.end.min(end).max(span.start.min(end) + 1);
    let to = boundary(source, span_end.min(end).max(from));
    let len = source[from..to].chars().count().max(1);
    format!("{}{} {}", " ".repeat(col), marker.to_string().repeat(len), label)
}

/// Renders one diagnostic against `source`, naming `file`, as rustc does: `error[code]: message`.
pub fn render(d: &Diag, file: &str, source: &str) -> String {
    let (line, col, _, _) = line_col(source, d.span.start);
    let mut lines: Vec<usize> = vec![line];
    for (s, _) in &d.secondary {
        lines.push(line_col(source, s.start).0);
    }
    let width = lines.iter().max().unwrap_or(&line).to_string().len();
    let pad = " ".repeat(width);
    let mut out = format!(
        "{}[{}]: {}\n{}--> {}:{}:{}\n{} |\n",
        d.severity.as_str(),
        d.code.as_str(),
        d.message,
        pad,
        file,
        line,
        col,
        pad
    );
    let mut parts: Vec<(Span, &str, char)> = vec![(d.span, d.label.as_str(), '^')];
    for (s, l) in &d.secondary {
        parts.push((*s, l.as_str(), '-'));
    }
    parts.sort_by_key(|p| p.0.start);
    // One source line, then one marker line per label on it (rightmost first, so labels do not collide).
    let mut k = 0;
    while k < parts.len() {
        let (line, _, start, end) = line_col(source, parts[k].0.start);
        let mut same: Vec<(Span, &str, char)> = vec![];
        while k < parts.len() && line_col(source, parts[k].0.start).0 == line {
            same.push(parts[k]);
            k += 1;
        }
        out.push_str(&format!("{:>w$} | {}\n", line, &source[start..end], w = width));
        same.reverse();
        for (s, l, m) in same {
            out.push_str(&format!("{} | {}\n", pad, marker_line(source, start, end, s, l, m)));
        }
    }
    for n in &d.notes {
        out.push_str(&format!("{pad} = {n}\n"));
    }
    out
}

/// A string as a JSON string literal.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_span(span: Span, source: &str) -> String {
    let (l1, c1, _, _) = line_col(source, span.start);
    let (l2, c2, _, _) = line_col(source, span.end.max(span.start));
    format!(
        "{{\"start\":{{\"line\":{l1},\"column\":{c1}}},\"end\":{{\"line\":{l2},\"column\":{c2}}},\"byte_start\":{},\"byte_end\":{}}}",
        span.start,
        span.end.max(span.start)
    )
}

/// One diagnostic as a single-line JSON object. Lines and columns count from 1, columns in characters;
/// `end` is the position just past the span.
pub fn to_json(d: &Diag, file: &str, source: &str) -> String {
    let secondary: Vec<String> = d
        .secondary
        .iter()
        .map(|(s, l)| format!("{{\"span\":{},\"label\":{}}}", json_span(*s, source), json_string(l)))
        .collect();
    let notes: Vec<String> = d.notes.iter().map(|n| json_string(n)).collect();
    format!(
        "{{\"severity\":{},\"code\":{},\"message\":{},\"file\":{},\"span\":{},\"label\":{},\"secondary\":[{}],\"notes\":[{}]}}",
        json_string(d.severity.as_str()),
        json_string(d.code.as_str()),
        json_string(&d.message),
        json_string(file),
        json_span(d.span, source),
        json_string(&d.label),
        secondary.join(","),
        notes.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_kebab_case() {
        let mut seen = std::collections::HashSet::new();
        for c in Code::ALL {
            let s = c.as_str();
            assert!(seen.insert(s), "{s} twice");
            assert!(
                !s.is_empty()
                    && s.chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
            );
            assert!(!s.starts_with('-') && !s.ends_with('-') && !s.contains("--"), "{s}");
        }
    }

    #[test]
    fn render_points_at_the_span() {
        let source = "on_bar {\n  target(x);\n}\n";
        let d = Diag::new(Code::UnknownName, "unknown name `x`", Span::new(18, 19), "not declared").note("a note");
        let text = render(&d, "a.atls", source);
        assert_eq!(
            text,
            "error[unknown-name]: unknown name `x`\n --> a.atls:2:10\n  |\n2 |   target(x);\n  |          ^ not declared\n  = a note\n"
        );
    }

    #[test]
    fn json_escapes_and_counts_characters() {
        let source = "# é\non_bar { \"x\" }";
        let d = Diag::warning(Code::UnusedSeries, "say \"hi\"\\", Span::new(15, 18), "l\tab");
        let json = to_json(&d, "dir/a b.atls", source);
        assert_eq!(
            json,
            "{\"severity\":\"warning\",\"code\":\"unused-series\",\"message\":\"say \\\"hi\\\"\\\\\",\"file\":\"dir/a b.atls\",\"span\":{\"start\":{\"line\":2,\"column\":11},\"end\":{\"line\":2,\"column\":14},\"byte_start\":15,\"byte_end\":18},\"label\":\"l\\tab\",\"secondary\":[],\"notes\":[]}"
        );
    }

    #[test]
    fn spans_past_the_end_or_inside_a_character_are_clamped() {
        let source = "é";
        assert_eq!(line_col(source, 1), (1, 1, 0, 2));
        assert_eq!(line_col(source, 99), (1, 2, 0, 2));
        let d = Diag::new(Code::MissingOnBar, "m", Span::new(0, 0), "l");
        assert!(render(&d, "f", "").contains("--> f:1:1"));
    }
}
