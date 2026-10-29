//! The syntax tree, as parsed. Every node keeps its byte span for diagnostics.

/// A byte range in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Real,
    Bool,
}

impl Ty {
    pub fn name(self) -> &'static str {
        match self {
            Ty::Int => "int",
            Ty::Real => "real",
            Ty::Bool => "bool",
        }
    }
    pub fn numeric(self) -> bool {
        matches!(self, Ty::Int | Ty::Real)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// A number; `true` when written without a decimal point or exponent (an int literal).
    Num(f64, bool),
    Bool(bool),
    Name(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Ident, Vec<Expr>),
    /// `x[k]`: sugar for `lag(x, k)`.
    Index(Box<Expr>, Box<Expr>),
    /// `fill.price`
    Field(Ident, Ident),
    /// `name = value` as a call's argument: `sma(close, 20, missing = skip)`.
    Named(Ident, Box<Expr>),
    /// `mean_sd(close, 20).sd`: one value of a user indicator with several.
    Member(Box<Expr>, Ident),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamDecl {
    pub ty: Ty,
    pub ty_span: Span,
    pub lower: Option<Expr>,
    pub upper: Option<Expr>,
    pub name: Ident,
    pub default: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarDecl {
    pub ty: Ty,
    pub ty_span: Span,
    pub name: Ident,
    pub init: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Local(VarDecl),
    Assign {
        name: Ident,
        value: Expr,
        span: Span,
    },
    /// `target(x);`, or `target(y, x);` naming an instrument of `instruments {}`.
    Target {
        inst: Option<Ident>,
        value: Expr,
        span: Span,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        els: Option<Vec<Stmt>>,
        span: Span,
    },
    For {
        var: Ident,
        from: Expr,
        to: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// Parsed only to be refused with a clear message.
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `return x;`, in a function of `functions {}`.
    Return {
        value: Expr,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandlerKind {
    OnStart,
    OnBar,
    OnFill,
    OnStop,
    /// At the session's open: only today's `open` is known; series hold the last close's values.
    OnOpen,
    /// At the session's close, after the day's series; a target fills at the close.
    OnClose,
    /// The body of a function of `functions {}` or of a user indicator: sees only its own names.
    User,
}

impl HandlerKind {
    pub fn name(self) -> &'static str {
        match self {
            HandlerKind::OnStart => "on_start",
            HandlerKind::OnBar => "on_bar",
            HandlerKind::OnFill => "on_fill",
            HandlerKind::OnStop => "on_stop",
            HandlerKind::OnOpen => "on_open",
            HandlerKind::OnClose => "on_close",
            HandlerKind::User => "a function or indicator",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Params(Vec<ParamDecl>, Span),
    Series(Vec<VarDecl>, Span),
    State(Vec<VarDecl>, Span),
    Warmup(Expr, Span),
    Handler(HandlerKind, Vec<Stmt>, Span),
    /// `instruments { y; x; }`: named inputs, read as `y.close`.
    Instruments(Vec<Ident>, Span),
    /// `universe {}`: the same program for every instrument the run gives, with cross-sectional operators.
    Universe(Span),
    /// `replay { key = value; ... }`: run settings a run spec overrides.
    Replay(Vec<Setting>, Span),
    /// `options { missing = skip; }`: choices that are part of the strategy (they change decisions).
    Options(Vec<(Ident, Expr)>, Span),
    /// `functions { real clip(real z, real cap) { ... } }`: pure functions.
    Functions(Vec<FunctionDef>, Span),
    /// `indicator mean_sd(real x, int n) { ... }`: a streaming indicator for `series {}`.
    Indicator(IndicatorDef),
}

/// A typed argument of a function or indicator.
#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub ty: Ty,
    pub name: Ident,
}

/// `real clip(real z, real cap) { ... }`.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDef {
    pub ret: Ty,
    pub name: Ident,
    pub args: Vec<Arg>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// One part of an indicator's body.
#[derive(Clone, Debug, PartialEq)]
pub enum IndPart {
    /// `window w[n];`: the last n values of the indicator's real arguments, with the missing-value policy.
    Window { name: Ident, size: Expr, span: Span },
    /// `state real s = 0;`
    State(VarDecl),
    /// `update { ... }`: once a step, before the window takes the step's values.
    Update(Vec<Stmt>, Span),
    /// `enter(v) { ... }`: a value enters the window (never a missing one).
    Enter(Vec<Ident>, Vec<Stmt>, Span),
    /// `leave(v) { ... }`: a value leaves the window.
    Leave(Vec<Ident>, Vec<Stmt>, Span),
    /// `value real mean = ...;`: what the indicator reports after each step.
    Value(VarDecl),
    /// `warmup n;`: steps before the values are defined, for an indicator without a window.
    Warmup(Expr, Span),
}

/// `indicator NAME(real x, int n) { ... }`: real arguments are series, int arguments are fixed.
#[derive(Clone, Debug, PartialEq)]
pub struct IndicatorDef {
    pub name: Ident,
    pub args: Vec<Arg>,
    pub parts: Vec<IndPart>,
    pub span: Span,
}

/// A literal value in `replay {}` or a run spec.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Num(f64),
    Str(String),
    Bool(bool),
}

/// One `section.key = value;` of `replay {}`.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: String,
    pub value: Lit,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Program {
    pub items: Vec<Item>,
}
