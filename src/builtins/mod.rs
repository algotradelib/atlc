//! Every built-in of atls, with what the checker and the printer need to know of it: one table per kind of call
//! (a function; a window over one series; a window over a pair; an output of a filter; a cross-section; a reduction
//! over the equity curve).
//! The rest of the compiler names no built-in: it asks these tables.
//! The enums identify a built-in in the IR; the tables are private, behind the functions below.

use crate::backend::Indicators;
use crate::frontend::ast::{Expr, Ty};
use crate::frontend::diag::{Code, Diag};
use crate::ir::Policy;

/// The built-in functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    Abs,
    Min,
    Max,
    Defined,
    /// -1, 0 or 1.
    Sign,
    /// Natural logarithm; undefined at or below 0.
    Log,
    /// Square root; undefined below 0.
    Sqrt,
    /// To the nearest whole number, halves away from zero (as `std::llround`).
    Round,
    /// Down to a whole number.
    Floor,
}

/// The built-in indicators over one series and a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ind {
    Sma,
    Ema,
    Rsi,
    Lag,
    TsMax,
    TsMin,
    TsSum,
    /// Sample standard deviation over the window (n - 1 in the divisor).
    Stdev,
    /// `(x - mean) / stdev` over the window, undefined when the deviation is 0.
    Zscore,
    /// Population standard deviation over the window (the count in the divisor).
    StdevPop,
}

/// Two-input window indicators: least squares of the first input on the second, with an intercept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PairInd {
    OlsSlope,
    OlsIntercept,
}

/// What a Kalman-filter hedge reports (one filter per (y, x, delta, ve)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KOut {
    /// The slope estimate after this bar's correction.
    Beta,
    /// The forecast error before the correction.
    Error,
    /// The forecast error's standard deviation.
    Sd,
}

/// Cross-sectional operators, over every instrument of a universe at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XsOp {
    Mean,
    Sum,
    Count,
    /// 1 for the smallest defined value, then 2, ...; ties broken by instrument order.
    Rank,
}

/// Reductions over the finished replay's equity curve, in `measures {}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CumOp {
    Sum,
    Mean,
    /// The sample standard deviation (n - 1 in the divisor).
    Stdev,
    Min,
    Max,
    First,
    Last,
    /// How many points have a defined true value.
    Count,
}

// ---------------------------------------------------------------------------------------------------------------
// Functions

/// When a function's value is undefined.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Definedness {
    /// When an argument is.
    Propagates,
    /// Never, whatever its arguments.
    Always,
    /// It may be, even on defined arguments.
    Maybe,
}

struct FunctionEntry {
    id: Builtin,
    name: &'static str,
    /// The argument types it takes, and the result's type for each: one row per overload, all of one length.
    signatures: &'static [(&'static [Ty], Ty)],
    /// The message for an argument of a type no row takes.
    wrong_type: &'static str,
    defined: Definedness,
    /// Where it is true, its first argument is defined (`if (defined(x))`).
    proves_argument: bool,
    /// Its `int` result may throw (an overflow), so the printer evaluates what is around it lazily.
    int_may_throw: bool,
    cpp: FunctionCpp,
}

/// How a call of a function is written in C++.
enum FunctionCpp {
    /// `(!std::isnan(x))`, a plain `bool`.
    NotNan,
    /// `int(x)` on a plain `int`, else `real(x)`.
    ByType { int: &'static str, real: &'static str },
    /// `atls::ops::NAME_of(x)`, three-valued.
    OpsOf(&'static str),
    /// `std::NAME<T>(a, b)` on plain values, `atls::ops::NAME3(a, b)` on three-valued ones.
    Pairwise(&'static str),
}

const ANY_TO_BOOL: &[(&[Ty], Ty)] = &[(&[Ty::Int], Ty::Bool), (&[Ty::Real], Ty::Bool), (&[Ty::Bool], Ty::Bool)];
const NUMBER_TO_SAME: &[(&[Ty], Ty)] = &[(&[Ty::Int], Ty::Int), (&[Ty::Real], Ty::Real)];
const NUMBER_TO_REAL: &[(&[Ty], Ty)] = &[(&[Ty::Int], Ty::Real), (&[Ty::Real], Ty::Real)];
const NUMBERS_JOINED: &[(&[Ty], Ty)] = &[
    (&[Ty::Int, Ty::Int], Ty::Int),
    (&[Ty::Int, Ty::Real], Ty::Real),
    (&[Ty::Real, Ty::Int], Ty::Real),
    (&[Ty::Real, Ty::Real], Ty::Real),
];

/// In the order the unknown-function note lists them.
static FUNCTIONS: [FunctionEntry; 9] = [
    FunctionEntry {
        id: Builtin::Abs,
        name: "abs",
        signatures: NUMBER_TO_SAME,
        wrong_type: "`abs` needs a number",
        defined: Definedness::Propagates,
        proves_argument: false,
        int_may_throw: true,
        cpp: FunctionCpp::ByType {
            int: "atls::ops::abs_int",
            real: "std::fabs",
        },
    },
    FunctionEntry {
        id: Builtin::Min,
        name: "min",
        signatures: NUMBERS_JOINED,
        wrong_type: "`min`/`max` need numbers",
        defined: Definedness::Propagates,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::Pairwise("min"),
    },
    FunctionEntry {
        id: Builtin::Max,
        name: "max",
        signatures: NUMBERS_JOINED,
        wrong_type: "`min`/`max` need numbers",
        defined: Definedness::Propagates,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::Pairwise("max"),
    },
    FunctionEntry {
        id: Builtin::Defined,
        name: "defined",
        signatures: ANY_TO_BOOL,
        wrong_type: "",
        defined: Definedness::Always,
        proves_argument: true,
        int_may_throw: false,
        cpp: FunctionCpp::NotNan,
    },
    FunctionEntry {
        id: Builtin::Sign,
        name: "sign",
        signatures: NUMBER_TO_REAL,
        wrong_type: "this function needs a number",
        defined: Definedness::Propagates,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::OpsOf("sign"),
    },
    FunctionEntry {
        id: Builtin::Log,
        name: "log",
        signatures: NUMBER_TO_REAL,
        wrong_type: "this function needs a number",
        defined: Definedness::Maybe,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::OpsOf("log"),
    },
    FunctionEntry {
        id: Builtin::Sqrt,
        name: "sqrt",
        signatures: NUMBER_TO_REAL,
        wrong_type: "this function needs a number",
        defined: Definedness::Maybe,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::OpsOf("sqrt"),
    },
    FunctionEntry {
        id: Builtin::Round,
        name: "round",
        signatures: NUMBER_TO_REAL,
        wrong_type: "this function needs a number",
        defined: Definedness::Propagates,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::OpsOf("round"),
    },
    FunctionEntry {
        id: Builtin::Floor,
        name: "floor",
        signatures: NUMBER_TO_REAL,
        wrong_type: "this function needs a number",
        defined: Definedness::Propagates,
        proves_argument: false,
        int_may_throw: false,
        cpp: FunctionCpp::OpsOf("floor"),
    },
];

fn function_entry(b: Builtin) -> &'static FunctionEntry {
    FUNCTIONS
        .iter()
        .find(|e| e.id == b)
        .expect("every function has an entry")
}

/// The function of this name.
pub(crate) fn function(name: &str) -> Option<Builtin> {
    FUNCTIONS.iter().find(|e| e.name == name).map(|e| e.id)
}

/// How many arguments a function takes.
pub(crate) fn function_arity(b: Builtin) -> usize {
    function_entry(b).signatures[0].0.len()
}

/// A function's result type over arguments of these types (as many as it takes), or the first argument no
/// signature takes and the message for it.
pub(crate) fn function_result(b: Builtin, tys: &[Ty]) -> Result<Ty, (usize, &'static str)> {
    let e = function_entry(b);
    if let Some((_, ret)) = e.signatures.iter().find(|(args, _)| *args == tys) {
        return Ok(*ret);
    }
    let bad = (0..tys.len())
        .find(|&i| !e.signatures.iter().any(|(args, _)| args[i] == tys[i]))
        .unwrap_or(0);
    Err((bad, e.wrong_type))
}

pub(crate) fn function_definedness(b: Builtin) -> Definedness {
    function_entry(b).defined
}

/// Where a call of `b` is true, its first argument is defined.
pub(crate) fn proves_argument_defined(b: Builtin) -> bool {
    function_entry(b).proves_argument
}

/// A call of `b` with an `int` result may throw.
pub(crate) fn int_result_may_throw(b: Builtin) -> bool {
    function_entry(b).int_may_throw
}

/// An argument of a function call, written as a plain value and as a three-valued one.
pub(crate) struct CppArg {
    pub plain: String,
    pub checked: String,
}

/// A call of `b` in C++, with the result's type `ty`, three-valued if `checked` asks: its code, type and whether
/// it is three-valued.
pub(crate) fn function_cpp(b: Builtin, args: &[CppArg], ty: Ty, checked: bool) -> (String, Ty, bool) {
    match function_entry(b).cpp {
        FunctionCpp::NotNan => (format!("(!std::isnan({}))", args[0].checked), Ty::Bool, false),
        FunctionCpp::ByType { int, real } => {
            let code = match (checked, ty) {
                (false, Ty::Int) => format!("{}({})", int, args[0].plain),
                (false, _) => format!("{}({})", real, args[0].plain),
                (true, _) => format!("{}({})", real, args[0].checked),
            };
            (code, ty, checked)
        }
        FunctionCpp::OpsOf(f) => (format!("atls::ops::{}_of({})", f, args[0].checked), ty, checked),
        FunctionCpp::Pairwise(f) => {
            let code = match (checked, ty) {
                (false, Ty::Int) => {
                    format!("std::{}<std::int64_t>({}, {})", f, args[0].plain, args[1].plain)
                }
                (false, _) => format!("std::{}<double>({}, {})", f, args[0].plain, args[1].plain),
                (true, _) => format!("atls::ops::{}3({}, {})", f, args[0].checked, args[1].checked),
            };
            (code, ty, checked)
        }
    }
}

/// The note on a call of an unknown function. It leaves out `stdev_pop`, as it always has.
pub(crate) fn unknown_function_note() -> String {
    let functions: Vec<&str> = FUNCTIONS.iter().map(|e| e.name).collect();
    let indicators: Vec<&str> = WINDOWS
        .iter()
        .filter(|e| e.in_function_note)
        .map(|e| e.name)
        .chain(PAIRS.iter().map(|e| e.name))
        .chain(FILTERS.iter().map(|e| e.name))
        .collect();
    format!(
        "functions: {}; indicators (series only): {}; cross-sectional (universe handlers): {}",
        functions.join(" "),
        indicators.join(" "),
        cross_section_names()
    )
}

/// The advice on a value that may be undefined where a defined one is needed.
pub(crate) fn guard_note() -> String {
    let d = FUNCTIONS.iter().find(|e| e.proves_argument).expect("a guard").name;
    format!("test it first: `{d}(x) ? x > 1 : false`, or assign inside `if ({d}(x)) {{ ... }}`")
}

/// The advice on a function returning `int` over real arguments.
pub(crate) const WHOLE_NUMBER_NOTE: &str =
    "an undefined real argument would make an undefined `int`; return `real` and use `round` or `floor`";

// ---------------------------------------------------------------------------------------------------------------
// Windows over one series

/// When a window needs at least two values.
#[derive(Clone, Copy, PartialEq)]
enum TwoValues {
    Never,
    Always,
    /// Under `restart` only.
    UnderRestart,
}

struct WindowEntry {
    id: Ind,
    name: &'static str,
    /// Listed in the unknown-function note (`stdev_pop` is not, as it never was).
    in_function_note: bool,
    /// `x[k]` is this indicator.
    indexed: bool,
    /// A literal window at or below 0 reads a future bar, and is refused with its own diagnostic (`look_ahead`).
    reads_ahead: bool,
    /// Over k slots it needs k + 1 values (a lag of k, an RSI over k moves).
    steps_back: bool,
    /// Under `skip` its value is the one in an exact slot: its lookback stays that slot's.
    exact_slot: bool,
    two_values: TwoValues,
    /// Undefined on some bars whatever its input (a z-score over equal values).
    may_stay_undefined: bool,
    /// Why it takes no `min =`, when it takes none.
    min_refused: Option<&'static str>,
    /// Under `skip`, a missing input leaves its value as it was (it has no ring then: see `slots`).
    skip_keeps_value: bool,
    class: Classes,
    slots: Slots,
}

/// An indicator's C++ class.
struct Classes {
    /// Under `restart` or `poison`, incremental or naive (which calls `recompute`).
    incremental: &'static str,
    /// Under `restart` or `poison` with exact arithmetic, where it has a class of its own.
    exact: Option<&'static str>,
    /// Under `skip`, where it has a rule: a window without one refuses `skip`.
    skip: Option<&'static str>,
}

/// The slots beyond its window an indicator needs of its input's ring, by arithmetic and policy; `None`: no ring,
/// it keeps its own state.
struct Slots {
    incremental: Option<i64>,
    naive: Option<i64>,
    exact: Option<i64>,
    /// Under `skip`, incremental or exact.
    skip: Option<i64>,
    skip_naive: Option<i64>,
}

/// In the order the notes list them.
static WINDOWS: [WindowEntry; 10] = [
    WindowEntry {
        id: Ind::Sma,
        name: "sma",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::Sma",
            exact: Some("atls::ops::SmaExact"),
            skip: Some("atls::ops::SmaSkip"),
        },
        slots: Slots {
            incremental: Some(1),
            naive: Some(0),
            exact: None,
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::Ema,
        name: "ema",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: Some("under `skip`, an EMA ignores a missing value and keeps its previous output"),
        skip_keeps_value: true,
        class: Classes {
            incremental: "atls::ops::Ema",
            exact: None,
            skip: Some("atls::ops::Ema"),
        },
        slots: Slots {
            incremental: None,
            naive: None,
            exact: None,
            skip: None,
            skip_naive: None,
        },
    },
    WindowEntry {
        id: Ind::Rsi,
        name: "rsi",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: true,
        exact_slot: false,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::Rsi",
            exact: Some("atls::ops::RsiExact"),
            skip: None,
        },
        slots: Slots {
            incremental: Some(2),
            naive: Some(1),
            exact: None,
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::Lag,
        name: "lag",
        in_function_note: true,
        indexed: true,
        reads_ahead: true,
        steps_back: true,
        exact_slot: true,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: Some("under `skip`, a lag reads the value in that exact slot, which may be missing"),
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::Lag",
            exact: None,
            skip: Some("atls::ops::Lag"),
        },
        slots: Slots {
            incremental: Some(1),
            naive: Some(1),
            exact: Some(1),
            skip: Some(1),
            skip_naive: Some(1),
        },
    },
    WindowEntry {
        id: Ind::TsMax,
        name: "ts_max",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::TsMax",
            exact: None,
            skip: None,
        },
        slots: Slots {
            incremental: None,
            naive: Some(0),
            exact: None,
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::TsMin,
        name: "ts_min",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::TsMin",
            exact: None,
            skip: None,
        },
        slots: Slots {
            incremental: None,
            naive: Some(0),
            exact: None,
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::TsSum,
        name: "ts_sum",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Never,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::TsSum",
            exact: Some("atls::ops::TsSumExact"),
            skip: Some("atls::ops::TsSumSkip"),
        },
        slots: Slots {
            incremental: Some(1),
            naive: Some(0),
            exact: None,
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::Stdev,
        name: "stdev",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Always,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::Stdev",
            exact: None,
            skip: Some("atls::ops::StdevSkip"),
        },
        slots: Slots {
            incremental: Some(1),
            naive: Some(0),
            exact: Some(1),
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::StdevPop,
        name: "stdev_pop",
        in_function_note: false,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::UnderRestart,
        may_stay_undefined: false,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::StdevPop",
            exact: None,
            skip: Some("atls::ops::StdevPopSkip"),
        },
        slots: Slots {
            incremental: Some(1),
            naive: Some(0),
            exact: Some(1),
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
    WindowEntry {
        id: Ind::Zscore,
        name: "zscore",
        in_function_note: true,
        indexed: false,
        reads_ahead: false,
        steps_back: false,
        exact_slot: false,
        two_values: TwoValues::Always,
        may_stay_undefined: true,
        min_refused: None,
        skip_keeps_value: false,
        class: Classes {
            incremental: "atls::ops::Zscore",
            exact: None,
            skip: Some("atls::ops::ZscoreSkip"),
        },
        slots: Slots {
            incremental: Some(1),
            naive: Some(0),
            exact: Some(1),
            skip: Some(1),
            skip_naive: Some(0),
        },
    },
];

fn window_entry(ind: Ind) -> &'static WindowEntry {
    WINDOWS.iter().find(|e| e.id == ind).expect("every window has an entry")
}

impl Ind {
    pub fn from_name(s: &str) -> Option<Ind> {
        WINDOWS.iter().find(|e| e.name == s).map(|e| e.id)
    }
    pub fn name(self) -> &'static str {
        window_entry(self).name
    }
    /// Steps back beyond the window: a lag of k and an RSI over k moves need k + 1 values.
    pub fn steps_back(self) -> bool {
        window_entry(self).steps_back
    }
}

/// The indicator `x[k]` is.
pub(crate) fn indexed_window() -> Ind {
    WINDOWS.iter().find(|e| e.indexed).expect("an indexed window").id
}

/// The one partial form, `NAME(x, n, m)`: a window of n over one series, defined once m values are in it.
/// Its IR node does not name the window, so there is one such form, of one window.
struct Partial {
    window: Ind,
    /// Its C++ class.
    class: &'static str,
}

static PARTIAL: Partial = Partial {
    window: Ind::TsSum,
    class: "atls::ops::TsSumMin",
};

/// `NAME(x, n, m)` is a form of this indicator.
pub(crate) fn has_partial_form(ind: Ind) -> bool {
    PARTIAL.window == ind
}

/// The C++ class of the partial form (`NAME(x, n, m)`).
pub(crate) fn partial_class() -> &'static str {
    PARTIAL.class
}

/// A literal window at or below 0 reads a future bar (see [`look_ahead`]).
pub(crate) fn reads_ahead(ind: Ind) -> bool {
    window_entry(ind).reads_ahead
}

/// The refusal of a literal window `value` at or below 0 of an indicator that [`reads_ahead`], over input `x`
/// and window `w`, written as an index or as a call.
pub(crate) fn look_ahead(ind: Ind, x: &Expr, w: &Expr, indexed: bool, value: f64) -> Diag {
    let shown = if indexed {
        format!("{}[{}]", crate::frontend::ast_text(x), crate::frontend::ast_text(w))
    } else {
        format!(
            "{}({}, {})",
            window_entry(ind).name,
            crate::frontend::ast_text(x),
            crate::frontend::ast_text(w)
        )
    };
    Diag::new(
        Code::LookAhead,
        format!("`{}` would read a future bar", shown),
        w.span,
        if value == 0.0 {
            "a lag of 0 is the current bar: write the series itself"
        } else {
            "a negative lag looks ahead"
        },
    )
    .note("a strategy only sees bars up to the current one; a lag is at least 1")
}

/// Under `skip`, the indicator's lookback is its exact slot's, not its window's.
pub(crate) fn exact_slot(ind: Ind) -> bool {
    window_entry(ind).exact_slot
}

/// The window needs at least two values under this policy.
pub(crate) fn needs_two_values(ind: Ind, policy: Policy) -> bool {
    match window_entry(ind).two_values {
        TwoValues::Never => false,
        TwoValues::Always => true,
        TwoValues::UnderRestart => policy == Policy::Restart,
    }
}

/// The indicator may be undefined on a bar whatever its input.
pub(crate) fn may_stay_undefined(ind: Ind) -> bool {
    window_entry(ind).may_stay_undefined
}

/// The indicator has a `skip` rule.
pub(crate) fn takes_skip(ind: Ind) -> bool {
    window_entry(ind).class.skip.is_some()
}

/// Why the indicator takes no `min =`, if it takes none.
pub(crate) fn min_refused(ind: Ind) -> Option<&'static str> {
    window_entry(ind).min_refused
}

/// Under `skip`, a missing input leaves the indicator's value as it was.
pub(crate) fn skip_keeps_value(ind: Ind) -> bool {
    window_entry(ind).skip_keeps_value
}

/// The indicator's C++ class under this arithmetic and policy.
pub(crate) fn window_class(ind: Ind, mode: Indicators, policy: Policy) -> &'static str {
    let class = &window_entry(ind).class;
    if policy == Policy::Skip {
        return class.skip.unwrap_or_else(|| unreachable!("refused by the checker"));
    }
    match class.exact {
        Some(exact) if mode == Indicators::Exact => exact,
        _ => class.incremental,
    }
}

/// The slots beyond its window the indicator needs of its input's ring, or `None` if it keeps its own state.
pub(crate) fn ring_slots(ind: Ind, mode: Indicators, policy: Policy) -> Option<i64> {
    let slots = &window_entry(ind).slots;
    match (policy == Policy::Skip, mode) {
        (true, Indicators::Naive) => slots.skip_naive,
        (true, _) => slots.skip,
        (false, Indicators::Incremental) => slots.incremental,
        (false, Indicators::Naive) => slots.naive,
        (false, Indicators::Exact) => slots.exact,
    }
}

/// The note on a named argument where none is taken.
pub(crate) fn named_argument_note() -> String {
    let names = |skip: bool| -> String {
        WINDOWS
            .iter()
            .filter(|e| e.class.skip.is_some() == skip)
            .map(|e| e.name)
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "only indicators over one series take `missing = ...` ({}, and {} with restart or poison)",
        names(true),
        names(false)
    )
}

// ---------------------------------------------------------------------------------------------------------------
// Windows over a pair

/// Names beginning so are indicators over a pair, known or not.
const PAIR_PREFIX: &str = "ols_";

struct PairEntry {
    id: PairInd,
    name: &'static str,
    /// The C++ class, with running sums or exact arithmetic.
    class: &'static str,
    /// The C++ class with naive arithmetic.
    naive_class: &'static str,
}

static PAIRS: [PairEntry; 2] = [
    PairEntry {
        id: PairInd::OlsSlope,
        name: "ols_slope",
        class: "atls::ops::OlsSlope",
        naive_class: "atls::ops::OlsLlt<true>",
    },
    PairEntry {
        id: PairInd::OlsIntercept,
        name: "ols_intercept",
        class: "atls::ops::OlsIntercept",
        naive_class: "atls::ops::OlsLlt<false>",
    },
];

fn pair_entry(p: PairInd) -> &'static PairEntry {
    PAIRS
        .iter()
        .find(|e| e.id == p)
        .expect("every pair indicator has an entry")
}

impl PairInd {
    pub fn name(self) -> &'static str {
        pair_entry(self).name
    }
}

/// The indicator over a pair of this name.
pub(crate) fn pair(name: &str) -> Option<PairInd> {
    PAIRS.iter().find(|e| e.name == name).map(|e| e.id)
}

/// The C++ class of an indicator over a pair, with naive arithmetic or not.
pub(crate) fn pair_class(p: PairInd, naive: bool) -> &'static str {
    let e = pair_entry(p);
    if naive { e.naive_class } else { e.class }
}

// ---------------------------------------------------------------------------------------------------------------
// Outputs of a filter

/// The C++ class of the filter all outputs share.
const FILTER_CLASS: &str = "atls::ops::KalmanHedge";

struct FilterEntry {
    id: KOut,
    name: &'static str,
    /// The filter's accessor that reads this output.
    accessor: &'static str,
}

static FILTERS: [FilterEntry; 3] = [
    FilterEntry {
        id: KOut::Beta,
        name: "kf_beta",
        accessor: "beta",
    },
    FilterEntry {
        id: KOut::Error,
        name: "kf_error",
        accessor: "error",
    },
    FilterEntry {
        id: KOut::Sd,
        name: "kf_sd",
        accessor: "sd",
    },
];

/// The filter output of this name.
pub(crate) fn filter(name: &str) -> Option<KOut> {
    FILTERS.iter().find(|e| e.name == name).map(|e| e.id)
}

/// The filter's accessor that reads this output.
pub(crate) fn filter_accessor(out: KOut) -> &'static str {
    FILTERS
        .iter()
        .find(|e| e.id == out)
        .expect("every filter output has an entry")
        .accessor
}

/// The C++ class of the filter.
pub(crate) fn filter_class() -> &'static str {
    FILTER_CLASS
}

// ---------------------------------------------------------------------------------------------------------------
// Cross-sections

/// Names beginning so are cross-sectional, known or not.
const CROSS_SECTION_PREFIX: &str = "xs_";

struct CrossSectionEntry {
    id: XsOp,
    name: &'static str,
    /// Undefined over no defined value, or where the instrument's own value is.
    may_be_undefined: bool,
    reduce: Reduce,
}

/// How a cross-section is computed over the active instruments' defined values.
pub(crate) enum Reduce {
    /// Accumulates their sum, their count, or both; the value is `result`, where `{i}` stands for the value's id.
    Accumulate {
        sum: bool,
        count: bool,
        result: &'static str,
    },
    /// Each instrument's position (from 1) among them in ascending order, ties by instrument order.
    Order,
}

static CROSS_SECTIONS: [CrossSectionEntry; 4] = [
    CrossSectionEntry {
        id: XsOp::Mean,
        name: "xs_mean",
        may_be_undefined: true,
        reduce: Reduce::Accumulate {
            sum: true,
            count: true,
            result: "(xs{i}_n > 0 ? xs{i}_sum / static_cast<double>(xs{i}_n) : atls::ops::NaN)",
        },
    },
    CrossSectionEntry {
        id: XsOp::Sum,
        name: "xs_sum",
        may_be_undefined: false,
        reduce: Reduce::Accumulate {
            sum: true,
            count: false,
            result: "xs{i}_sum",
        },
    },
    CrossSectionEntry {
        id: XsOp::Count,
        name: "xs_count",
        may_be_undefined: false,
        reduce: Reduce::Accumulate {
            sum: false,
            count: true,
            result: "static_cast<double>(xs{i}_n)",
        },
    },
    CrossSectionEntry {
        id: XsOp::Rank,
        name: "xs_rank",
        may_be_undefined: true,
        reduce: Reduce::Order,
    },
];

fn cross_section_entry(op: XsOp) -> &'static CrossSectionEntry {
    CROSS_SECTIONS
        .iter()
        .find(|e| e.id == op)
        .expect("every cross-section has an entry")
}

impl XsOp {
    pub fn name(self) -> &'static str {
        cross_section_entry(self).name
    }
}

/// The cross-section of this name.
pub(crate) fn cross_section(name: &str) -> Option<XsOp> {
    CROSS_SECTIONS.iter().find(|e| e.name == name).map(|e| e.id)
}

/// The cross-section may be undefined.
pub(crate) fn cross_section_maybe(op: XsOp) -> bool {
    cross_section_entry(op).may_be_undefined
}

/// How the cross-section is computed.
pub(crate) fn cross_section_reduce(op: XsOp) -> &'static Reduce {
    &cross_section_entry(op).reduce
}

fn cross_section_names() -> String {
    CROSS_SECTIONS.iter().map(|e| e.name).collect::<Vec<_>>().join(" ")
}

/// The note on an unknown name of the cross-sectional family.
pub(crate) fn unknown_cross_section_note() -> String {
    format!("cross-sectional operators: {}", cross_section_names())
}

// ---------------------------------------------------------------------------------------------------------------
// Reductions

/// Names beginning so are reductions over the equity curve, known or not.
const REDUCTION_PREFIX: &str = "cum_";

struct ReductionEntry {
    id: CumOp,
    name: &'static str,
    /// Takes a `bool` (it counts where it is true) instead of a number.
    takes_bool: bool,
}

/// In the order the reference lists them.
static REDUCTIONS: [ReductionEntry; 8] = [
    ReductionEntry {
        id: CumOp::Sum,
        name: "cum_sum",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::Mean,
        name: "cum_mean",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::Stdev,
        name: "cum_stdev",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::Min,
        name: "cum_min",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::Max,
        name: "cum_max",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::First,
        name: "cum_first",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::Last,
        name: "cum_last",
        takes_bool: false,
    },
    ReductionEntry {
        id: CumOp::Count,
        name: "cum_count",
        takes_bool: true,
    },
];

fn reduction_entry(op: CumOp) -> &'static ReductionEntry {
    REDUCTIONS
        .iter()
        .find(|e| e.id == op)
        .expect("every reduction has an entry")
}

impl CumOp {
    pub fn name(self) -> &'static str {
        reduction_entry(self).name
    }
}

/// The reduction of this name.
pub(crate) fn reduction(name: &str) -> Option<CumOp> {
    REDUCTIONS.iter().find(|e| e.name == name).map(|e| e.id)
}

/// The reduction takes a `bool`, not a number.
pub(crate) fn reduction_takes_bool(op: CumOp) -> bool {
    reduction_entry(op).takes_bool
}

/// The runtime function computing the reduction, `atls::measures::NAME`: its name without the family's prefix.
pub(crate) fn reduction_cpp(op: CumOp) -> &'static str {
    &reduction_entry(op).name[REDUCTION_PREFIX.len()..]
}

/// The note on an unknown name of the reduction family.
pub(crate) fn unknown_reduction_note() -> String {
    let names: Vec<&str> = REDUCTIONS.iter().map(|e| e.name).collect();
    format!("reductions (measures {{}} only): {}", names.join(" "))
}

// ---------------------------------------------------------------------------------------------------------------
// Names

/// A name of the reduction family (`cum_...`), a built-in or not.
pub(crate) fn in_reduction_family(name: &str) -> bool {
    name.starts_with(REDUCTION_PREFIX)
}

/// A name of the cross-sectional family (`xs_...`), a built-in or not.
pub(crate) fn in_cross_section_family(name: &str) -> bool {
    name.starts_with(CROSS_SECTION_PREFIX)
}

/// A name a handler refuses as an indicator: a window over one series or a pair, a filter output, or a name of the
/// pair family (`ols_...`), a built-in or not.
pub(crate) fn is_indicator_name(name: &str) -> bool {
    Ind::from_name(name).is_some() || pair(name).is_some() || filter(name).is_some() || name.starts_with(PAIR_PREFIX)
}

/// The name of a built-in of any kind.
pub(crate) fn is_builtin_name(name: &str) -> bool {
    function(name).is_some()
        || Ind::from_name(name).is_some()
        || pair(name).is_some()
        || filter(name).is_some()
        || cross_section(name).is_some()
        || reduction(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The members of each enum. A variant added without being listed here fails to compile at the `match` beside
    // its list.
    const FUNCTION_IDS: [Builtin; 9] = [
        Builtin::Abs,
        Builtin::Min,
        Builtin::Max,
        Builtin::Defined,
        Builtin::Sign,
        Builtin::Log,
        Builtin::Sqrt,
        Builtin::Round,
        Builtin::Floor,
    ];
    fn _function_ids(b: Builtin) {
        match b {
            Builtin::Abs
            | Builtin::Min
            | Builtin::Max
            | Builtin::Defined
            | Builtin::Sign
            | Builtin::Log
            | Builtin::Sqrt
            | Builtin::Round
            | Builtin::Floor => {}
        }
    }
    const WINDOW_IDS: [Ind; 10] = [
        Ind::Sma,
        Ind::Ema,
        Ind::Rsi,
        Ind::Lag,
        Ind::TsMax,
        Ind::TsMin,
        Ind::TsSum,
        Ind::Stdev,
        Ind::Zscore,
        Ind::StdevPop,
    ];
    fn _window_ids(i: Ind) {
        match i {
            Ind::Sma
            | Ind::Ema
            | Ind::Rsi
            | Ind::Lag
            | Ind::TsMax
            | Ind::TsMin
            | Ind::TsSum
            | Ind::Stdev
            | Ind::Zscore
            | Ind::StdevPop => {}
        }
    }
    const PAIR_IDS: [PairInd; 2] = [PairInd::OlsSlope, PairInd::OlsIntercept];
    fn _pair_ids(p: PairInd) {
        match p {
            PairInd::OlsSlope | PairInd::OlsIntercept => {}
        }
    }
    const FILTER_IDS: [KOut; 3] = [KOut::Beta, KOut::Error, KOut::Sd];
    fn _filter_ids(k: KOut) {
        match k {
            KOut::Beta | KOut::Error | KOut::Sd => {}
        }
    }
    const CROSS_SECTION_IDS: [XsOp; 4] = [XsOp::Mean, XsOp::Sum, XsOp::Count, XsOp::Rank];
    fn _cross_section_ids(x: XsOp) {
        match x {
            XsOp::Mean | XsOp::Sum | XsOp::Count | XsOp::Rank => {}
        }
    }

    const REDUCTION_IDS: [CumOp; 8] = [
        CumOp::Sum,
        CumOp::Mean,
        CumOp::Stdev,
        CumOp::Min,
        CumOp::Max,
        CumOp::First,
        CumOp::Last,
        CumOp::Count,
    ];
    fn _reduction_ids(c: CumOp) {
        match c {
            CumOp::Sum
            | CumOp::Mean
            | CumOp::Stdev
            | CumOp::Min
            | CumOp::Max
            | CumOp::First
            | CumOp::Last
            | CumOp::Count => {}
        }
    }

    /// Each member has exactly one entry, and each entry is a member's.
    fn one_entry_each<T: PartialEq + std::fmt::Debug>(members: &[T], ids: &[T]) {
        assert_eq!(members.len(), ids.len());
        for m in members {
            assert_eq!(ids.iter().filter(|id| *id == m).count(), 1, "{m:?}");
        }
    }

    #[test]
    fn every_member_has_one_entry() {
        one_entry_each(&FUNCTION_IDS, &FUNCTIONS.iter().map(|e| e.id).collect::<Vec<_>>());
        one_entry_each(&WINDOW_IDS, &WINDOWS.iter().map(|e| e.id).collect::<Vec<_>>());
        one_entry_each(&PAIR_IDS, &PAIRS.iter().map(|e| e.id).collect::<Vec<_>>());
        one_entry_each(&FILTER_IDS, &FILTERS.iter().map(|e| e.id).collect::<Vec<_>>());
        one_entry_each(
            &CROSS_SECTION_IDS,
            &CROSS_SECTIONS.iter().map(|e| e.id).collect::<Vec<_>>(),
        );
        one_entry_each(&REDUCTION_IDS, &REDUCTIONS.iter().map(|e| e.id).collect::<Vec<_>>());
    }

    #[test]
    fn names_are_unique() {
        let names: Vec<&str> = FUNCTIONS
            .iter()
            .map(|e| e.name)
            .chain(WINDOWS.iter().map(|e| e.name))
            .chain(PAIRS.iter().map(|e| e.name))
            .chain(FILTERS.iter().map(|e| e.name))
            .chain(CROSS_SECTIONS.iter().map(|e| e.name))
            .chain(REDUCTIONS.iter().map(|e| e.name))
            .collect();
        for n in &names {
            assert_eq!(names.iter().filter(|m| *m == n).count(), 1, "{n}");
        }
    }

    #[test]
    fn entries_are_consistent() {
        for e in &FUNCTIONS {
            assert!(!e.signatures.is_empty(), "{}", e.name);
            assert!(
                e.signatures
                    .iter()
                    .all(|(args, _)| args.len() == e.signatures[0].0.len()),
                "{}",
                e.name
            );
        }
        assert_eq!(WINDOWS.iter().filter(|e| e.indexed).count(), 1);
        assert_eq!(FUNCTIONS.iter().filter(|e| e.proves_argument).count(), 1);
        assert!(REDUCTIONS.iter().all(|e| in_reduction_family(e.name)));
        for e in &WINDOWS {
            for mode in [Indicators::Incremental, Indicators::Naive, Indicators::Exact] {
                for policy in [Policy::Restart, Policy::Skip, Policy::Poison] {
                    if policy == Policy::Skip && !takes_skip(e.id) {
                        continue;
                    }
                    window_class(e.id, mode, policy);
                    ring_slots(e.id, mode, policy);
                }
            }
        }
    }
}
