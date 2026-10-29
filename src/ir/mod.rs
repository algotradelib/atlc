//! What the checker produces and the printer reads: parameters, the series DAG (evaluated every bar,
//! children before parents), state, and typed handler statements.

use crate::frontend::ast::{BinOp, HandlerKind, Span, Ty, UnOp};

/// An expression fixed before the run: numbers and params only.
#[derive(Clone, Debug, PartialEq)]
pub enum CExpr {
    /// A whole number: an `int` literal, or one the checker computes (window lengths, lookbacks).
    Num(f64),
    /// A `real` literal (written with `.` or an exponent), so the printer keeps its arithmetic in doubles.
    Real(f64),
    Param(usize),
    Neg(Box<CExpr>),
    Bin(BinOp, Box<CExpr>, Box<CExpr>),
    Max(Vec<CExpr>),
}

impl CExpr {
    pub fn key(&self) -> String {
        format!("{:?}", self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Open,
    High,
    Low,
    Close,
    Volume,
}

impl Field {
    pub fn name(self) -> &'static str {
        match self {
            Field::Open => "open",
            Field::High => "high",
            Field::Low => "low",
            Field::Close => "close",
            Field::Volume => "volume",
        }
    }
}

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

/// What an indicator does with an undefined (missing) input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Policy {
    /// The window starts again after the missing value (v0's rule).
    Restart,
    /// The missing value takes its slot in the window but is left out of the result: a mean over the
    /// values that exist, divided by their count.
    Skip,
    /// The indicator is undefined from the first missing value on, for good.
    Poison,
}

impl Policy {
    pub fn from_name(s: &str) -> Option<Policy> {
        Some(match s {
            "restart" => Policy::Restart,
            "skip" => Policy::Skip,
            "poison" => Policy::Poison,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Policy::Restart => "restart",
            Policy::Skip => "skip",
            Policy::Poison => "poison",
        }
    }
}

/// An indicator's missing-value policy, and for `skip` the fewest values that must exist in the window.
#[derive(Clone, Debug, PartialEq)]
pub struct Miss {
    pub policy: Policy,
    pub min: CExpr,
}

impl Default for Miss {
    fn default() -> Self {
        Miss {
            policy: Policy::Restart,
            min: CExpr::Num(1.0),
        }
    }
}

impl Ind {
    pub fn from_name(s: &str) -> Option<Ind> {
        Some(match s {
            "sma" => Ind::Sma,
            "ema" => Ind::Ema,
            "rsi" => Ind::Rsi,
            "lag" => Ind::Lag,
            "ts_max" => Ind::TsMax,
            "ts_min" => Ind::TsMin,
            "ts_sum" => Ind::TsSum,
            "stdev" => Ind::Stdev,
            "zscore" => Ind::Zscore,
            "stdev_pop" => Ind::StdevPop,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Ind::Sma => "sma",
            Ind::Ema => "ema",
            Ind::Rsi => "rsi",
            Ind::Lag => "lag",
            Ind::TsMax => "ts_max",
            Ind::TsMin => "ts_min",
            Ind::TsSum => "ts_sum",
            Ind::Stdev => "stdev",
            Ind::Zscore => "zscore",
            Ind::StdevPop => "stdev_pop",
        }
    }
    /// Steps back beyond the window: a lag of k and an RSI over k moves need k + 1 values.
    pub fn steps_back(self) -> bool {
        matches!(self, Ind::Lag | Ind::Rsi)
    }
}

/// When a series value is defined (not NaN).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// From the first bar: close, high, low, volume, constants, and arithmetic on them without `/`.
    Always,
    /// Once its lookback is reached, and from then on: an indicator over an always or eventually
    /// defined input.
    Eventually,
    /// May be undefined on any bar: `open`, a division by something that may be zero.
    Maybe,
}

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

/// Two-input window indicators: least squares of the first input on the second, with an intercept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PairInd {
    OlsSlope,
    OlsIntercept,
}

impl PairInd {
    pub fn name(self) -> &'static str {
        match self {
            PairInd::OlsSlope => "ols_slope",
            PairInd::OlsIntercept => "ols_intercept",
        }
    }
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

impl XsOp {
    pub fn name(self) -> &'static str {
        match self {
            XsOp::Mean => "xs_mean",
            XsOp::Sum => "xs_sum",
            XsOp::Count => "xs_count",
            XsOp::Rank => "xs_rank",
        }
    }
}

/// How the program sees instruments.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Mode {
    /// One instrument, fields read bare (`close`).
    #[default]
    Single,
    /// Named inputs from `instruments {}`, fields read qualified (`y.close`); series are one value a bar.
    Named(Vec<String>),
    /// The same program for every instrument the run gives: series per instrument, `xs_*` across them.
    Universe,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// A bar field of an instrument (its index in `instruments {}`, 0 otherwise).
    Field(usize, Field),
    Num(f64),
    Bool(bool),
    Param(usize),
    Unary(UnOp, usize),
    Binary(BinOp, usize, usize),
    Ternary(usize, usize, usize),
    Call(Builtin, Vec<usize>),
    /// An indicator over one input, its window and its missing-value policy.
    Ind(Ind, usize, CExpr, Miss),
    /// `ts_sum(x, n, min)`: defined once `min` values are in the window.
    Partial(usize, CExpr, CExpr),
    Pair(PairInd, usize, usize, CExpr),
    /// y, x, delta, ve, output.
    Kalman(usize, usize, CExpr, CExpr, KOut),
    /// A call of a function of `functions {}`.
    UCall(usize, Vec<usize>),
    /// One value of a user indicator: its definition, series inputs, fixed arguments, policy, value.
    UInd {
        def: usize,
        inputs: Vec<usize>,
        consts: Vec<CExpr>,
        miss: Miss,
        value: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub ty: Ty,
    pub class: Class,
    pub lookback: CExpr,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    pub default: f64,
    pub lower: Option<(CExpr, String)>,
    pub upper: Option<(CExpr, String)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StateVar {
    pub name: String,
    pub ty: Ty,
    pub init: CExpr,
    /// A real state may hold an undefined value when some assignment may be undefined.
    pub maybe: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Local {
    pub name: String,
    pub ty: Ty,
    pub maybe: bool,
    /// Declared at a universe handler's top level: one value per instrument, readable by later statements.
    pub top: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HKind {
    Num(f64),
    Bool(bool),
    Param(usize),
    Node(usize),
    State(usize),
    Local(usize),
    Held,
    /// `y.held` in a program with `instruments {}`.
    HeldOf(usize),
    /// Today's `open`, in `on_open`.
    OpenNow,
    /// In a universe handler: this instrument's bars so far.
    Bars,
    /// In a universe handler: the periods so far (the days, with legs).
    Periods,
    /// A cross-sectional value: the operator, its per-instrument argument, and its id.
    Xs(XsOp, Box<HExpr>, usize),
    Keep,
    FillPrice,
    FillQuantity,
    FillBuy,
    Unary(UnOp, Box<HExpr>),
    Binary(BinOp, Box<HExpr>, Box<HExpr>),
    Ternary(Box<HExpr>, Box<HExpr>, Box<HExpr>),
    Call(Builtin, Vec<HExpr>),
    /// A call of a function of `functions {}`.
    UserCall(usize, Vec<HExpr>),
    /// In a function or indicator: an argument.
    UArg(usize),
    /// In an indicator: a state.
    UState(usize),
    /// In an indicator's values: an earlier value.
    UValue(usize),
    /// In an indicator: how many values its window holds (missing ones left out).
    UCount,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HExpr {
    pub kind: HKind,
    pub ty: Ty,
    /// May be undefined at run time (NaN for a real, unknown for a bool).
    pub maybe: bool,
}

#[derive(Clone, Debug)]
pub enum HStmt {
    Local {
        id: usize,
        value: HExpr,
    },
    Assign {
        state: usize,
        value: HExpr,
    },
    /// The instrument (index in `instruments {}`, 0 otherwise) and the shares.
    Target(usize, HExpr),
    If {
        cond: HExpr,
        then: Vec<HStmt>,
        els: Vec<HStmt>,
    },
    For {
        var: usize,
        from: CExpr,
        to: CExpr,
        body: Vec<HStmt>,
    },
    /// In a function: `return x;`.
    Return(HExpr),
    /// In an indicator: assigns a state.
    UAssign {
        state: usize,
        value: HExpr,
    },
    /// In a function or indicator: a loop over bounds computed once (whole numbers, so it ends).
    UFor {
        var: usize,
        from: HExpr,
        to: HExpr,
        body: Vec<HStmt>,
    },
}

/// A function of `functions {}`, checked. It runs only on defined arguments: an undefined real or bool
/// argument makes the call undefined without running the body.
#[derive(Clone, Debug)]
pub struct FnIr {
    pub name: String,
    pub args: Vec<(String, Ty)>,
    pub ret: Ty,
    pub body: Vec<HStmt>,
    pub locals: Vec<Local>,
    /// The body may return an undefined value even on defined arguments.
    pub may_undef: bool,
}

/// A state of a user indicator.
#[derive(Clone, Debug)]
pub struct UStateIr {
    pub name: String,
    pub ty: Ty,
    pub init: HExpr,
    pub maybe: bool,
}

/// A value of a user indicator.
#[derive(Clone, Debug)]
pub struct UValueIr {
    pub name: String,
    pub ty: Ty,
    pub expr: HExpr,
}

/// A user indicator, checked. Real arguments are series (the window holds them), int arguments fixed.
#[derive(Clone, Debug)]
pub struct IndIr {
    pub name: String,
    pub args: Vec<(String, Ty)>,
    /// The window's name, its length as written (for call sites) and over the fixed arguments.
    pub window: Option<(String, crate::frontend::ast::Expr, HExpr)>,
    pub warmup: Option<(crate::frontend::ast::Expr, HExpr)>,
    pub states: Vec<UStateIr>,
    pub update: Vec<HStmt>,
    /// The locals naming each series argument, and the body.
    pub enter: Option<(Vec<usize>, Vec<HStmt>)>,
    pub leave: Option<(Vec<usize>, Vec<HStmt>)>,
    pub values: Vec<UValueIr>,
    pub locals: Vec<Local>,
    /// Int arguments known to be at least 1 at every call (the window's length when it is one argument).
    pub nonzero: Vec<usize>,
    /// Some part reads `w.count`.
    pub uses_count: bool,
}

impl IndIr {
    /// The positions of the series (real) arguments.
    pub fn series_args(&self) -> Vec<usize> {
        (0..self.args.len()).filter(|&i| self.args[i].1 == Ty::Real).collect()
    }
    /// The positions of the fixed (int) arguments.
    pub fn const_args(&self) -> Vec<usize> {
        (0..self.args.len()).filter(|&i| self.args[i].1 == Ty::Int).collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Handlers {
    pub on_start: Option<Vec<HStmt>>,
    pub on_bar: Option<Vec<HStmt>>,
    pub on_fill: Option<Vec<HStmt>>,
    pub on_stop: Option<Vec<HStmt>>,
    pub on_open: Option<Vec<HStmt>>,
    pub on_close: Option<Vec<HStmt>>,
}

impl Handlers {
    pub fn get(&self, k: HandlerKind) -> &Option<Vec<HStmt>> {
        match k {
            HandlerKind::OnStart => &self.on_start,
            HandlerKind::OnBar => &self.on_bar,
            HandlerKind::OnFill => &self.on_fill,
            HandlerKind::OnStop => &self.on_stop,
            HandlerKind::OnOpen => &self.on_open,
            HandlerKind::OnClose => &self.on_close,
            HandlerKind::User => &None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Ir {
    pub params: Vec<Param>,
    pub nodes: Vec<Node>,
    pub series: Vec<(String, usize)>,
    pub state: Vec<StateVar>,
    pub locals: Vec<Local>,
    pub warmup: Option<CExpr>,
    pub handlers: Handlers,
    pub uses_held: bool,
    /// Nodes read by a handler other than `on_bar`, kept in members.
    pub kept: Vec<usize>,
    /// Nodes any handler reads, and everything they depend on.
    pub live: Vec<bool>,
    pub warnings: Vec<crate::frontend::diag::Diag>,
    pub mode: Mode,
    /// `on_open`/`on_close` instead of `on_bar`: the run feeds each day as an open and a close.
    pub legs: bool,
    /// How many `xs_*` values the handlers compute.
    pub xs_count: usize,
    /// `replay {}`, as written.
    pub replay: Vec<crate::frontend::ast::Setting>,
    /// The program's missing-value policy (`options { missing = ...; }`), restart by default.
    pub missing: Policy,
    /// A universe whose policy is skip or poison: every instrument takes every period, and a period
    /// without its bar is a row of undefined values (restart keeps each instrument on its own bars).
    pub period_clock: bool,
    pub functions: Vec<FnIr>,
    pub indicators: Vec<IndIr>,
}

impl Default for Ir {
    fn default() -> Self {
        Ir {
            params: vec![],
            nodes: vec![],
            series: vec![],
            state: vec![],
            locals: vec![],
            warmup: None,
            handlers: Handlers::default(),
            uses_held: false,
            kept: vec![],
            live: vec![],
            warnings: vec![],
            mode: Mode::Single,
            legs: false,
            xs_count: 0,
            replay: vec![],
            missing: Policy::Restart,
            period_clock: false,
            functions: vec![],
            indicators: vec![],
        }
    }
}
