//! The hand-written recursive-descent parser. On an error inside a block it records the diagnostic,
//! skips to the next `;` or `}` and goes on, so one run reports several mistakes.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::frontend::lexer::{Tok, Token, lex};

pub fn parse(source: &str) -> Result<Program, Vec<Diag>> {
    let tokens = lex(source).map_err(|d| vec![d])?;
    let mut p = Parser {
        tokens,
        at: 0,
        errors: vec![],
        depth: 0,
        height: 0,
    };
    let program = p.program();
    if p.errors.is_empty() {
        Ok(program)
    } else {
        Err(p.errors)
    }
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    errors: Vec<Diag>,
    depth: usize,
    /// The height of the expression the last expression function returned (a leaf is 1).
    height: usize,
}

/// The deepest a program may nest: blocks, `else if`, parentheses and every level of an expression's tree
/// (a chain of operators deepens its tree by one per operator) together. Deeper programs are refused before
/// their tree is built, rather than overflowing the stack of the parser, the checker or a tree's destructor.
pub const MAX_DEPTH: usize = 256;

type R<T> = Result<T, Diag>;

/// Words reserved everywhere ([syntax](docs/reference/syntax.md)).
pub const KEYWORDS: [&str; 31] = [
    "params",
    "series",
    "state",
    "warmup",
    "on_start",
    "on_bar",
    "on_fill",
    "on_stop",
    "int",
    "real",
    "bool",
    "if",
    "else",
    "for",
    "in",
    "while",
    "target",
    "and",
    "or",
    "not",
    "true",
    "false",
    "instruments",
    "universe",
    "replay",
    "on_open",
    "on_close",
    "options",
    "functions",
    "indicator",
    "return",
];

const BLOCKS: [&str; 16] = [
    "params",
    "series",
    "state",
    "warmup",
    "on_start",
    "on_bar",
    "on_fill",
    "on_stop",
    "on_open",
    "on_close",
    "instruments",
    "universe",
    "replay",
    "options",
    "functions",
    "indicator",
];

const BLOCKS_NOTE: &str = "blocks: instruments {}, universe {}, options {}, functions {}, indicator NAME(...) {}, params {}, series {}, state {}, warmup EXPR;, on_start {}, on_bar {}, on_open {}, on_close {}, on_fill {}, on_stop {}, replay {}";

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn peek_at(&self, k: usize) -> &Token {
        &self.tokens[(self.at + k).min(self.tokens.len() - 1)]
    }
    fn bump(&mut self) -> Token {
        let t = self.tokens[self.at].clone();
        if self.at + 1 < self.tokens.len() {
            self.at += 1;
        }
        t
    }
    fn is_punct(&self, p: &str) -> bool {
        matches!(&self.peek().tok, Tok::Punct(q) if *q == p)
    }
    fn is_word(&self, w: &str) -> bool {
        matches!(&self.peek().tok, Tok::Ident(s) if s == w)
    }
    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect_punct(&mut self, p: &str, context: &str) -> R<Span> {
        if self.is_punct(p) {
            return Ok(self.bump().span);
        }
        let t = self.peek().clone();
        let mut d = Diag::new(
            Code::ExpectedToken,
            format!("expected `{}` {}, found {}", p, context, t.describe()),
            t.span,
            format!("expected `{}`", p),
        );
        if p == ";" && self.at > 0 {
            // Point just past the previous token: that is where the semicolon goes.
            let prev = self.tokens[self.at - 1].span;
            d = Diag::new(
                Code::ExpectedToken,
                format!("expected `;` {}, found {}", context, t.describe()),
                // An insertion point: the empty range just past the previous token.
                Span::new(prev.end, prev.end),
                "add `;` here",
            );
        }
        Err(d)
    }
    fn ident(&mut self, what: &str) -> R<Ident> {
        let t = self.peek().clone();
        match &t.tok {
            Tok::Ident(s) if !KEYWORDS.contains(&s.as_str()) => {
                self.bump();
                Ok(Ident {
                    name: s.clone(),
                    span: t.span,
                })
            }
            Tok::Ident(s) => Err(Diag::new(
                Code::KeywordAsName,
                format!("`{}` is a keyword and cannot be used as {}", s, what),
                t.span,
                "reserved",
            )),
            _ => Err(Diag::new(
                Code::ExpectedName,
                format!("expected {}, found {}", what, t.describe()),
                t.span,
                format!("expected {}", what),
            )),
        }
    }

    /// Skips to just after the next `;` at this nesting, or to a `}` that closes it.
    fn recover(&mut self) {
        let mut depth = 0i32;
        loop {
            match &self.peek().tok {
                Tok::Eof => return,
                Tok::Punct(";") if depth == 0 => {
                    self.bump();
                    return;
                }
                Tok::Punct("{") => depth += 1,
                Tok::Punct("}") => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        return;
                    }
                }
                _ => {}
            }
            self.bump();
        }
    }

    /// Refuses a node of height `h` (its subtree's levels) built at the current depth, if it would be
    /// deeper than [`MAX_DEPTH`]: checked as each node is built, so no deeper tree ever exists.
    fn node(&self, h: usize, span: Span) -> R<()> {
        if self.depth + h > MAX_DEPTH {
            return Err(too_deep(span));
        }
        Ok(())
    }

    fn program(&mut self) -> Program {
        let mut items = vec![];
        while self.peek().tok != Tok::Eof {
            match self.item() {
                Ok(item) => items.push(item),
                Err(d) => {
                    self.errors.push(d);
                    // Skip to the next top-level keyword.
                    while self.peek().tok != Tok::Eof {
                        if let Tok::Ident(s) = &self.peek().tok {
                            if BLOCKS.contains(&s.as_str()) {
                                break;
                            }
                        }
                        self.bump();
                    }
                }
            }
        }
        Program { items }
    }

    fn item(&mut self) -> R<Item> {
        let t = self.peek().clone();
        let word = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => {
                return Err(Diag::new(
                    Code::ExpectedBlock,
                    format!("expected a block, found {}", t.describe()),
                    t.span,
                    "a program is made of blocks",
                )
                .note(BLOCKS_NOTE));
            }
        };
        match word.as_str() {
            "params" => {
                self.bump();
                self.expect_punct("{", "after `params`")?;
                let mut decls = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    match self.param() {
                        Ok(d) => decls.push(d),
                        Err(d) => {
                            self.errors.push(d);
                            self.recover();
                        }
                    }
                }
                let end = self.expect_punct("}", "to close `params`")?;
                Ok(Item::Params(decls, t.span.to(end)))
            }
            "series" | "state" => {
                self.bump();
                self.expect_punct("{", &format!("after `{}`", word))?;
                let mut decls = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    match self.var_decl() {
                        Ok(d) => decls.push(d),
                        Err(d) => {
                            self.errors.push(d);
                            self.recover();
                        }
                    }
                }
                let end = self.expect_punct("}", &format!("to close `{}`", word))?;
                Ok(if word == "series" { Item::Series(decls, t.span.to(end)) } else { Item::State(decls, t.span.to(end)) })
            }
            "warmup" => {
                self.bump();
                let e = self.expr()?;
                let end = self.expect_punct(";", "after the warm-up")?;
                Ok(Item::Warmup(e, t.span.to(end)))
            }
            "instruments" => {
                self.bump();
                self.expect_punct("{", "after `instruments`")?;
                let mut names = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    let name = self.ident("an instrument name")?;
                    self.expect_punct(";", "after the instrument")?;
                    names.push(name);
                }
                let end = self.expect_punct("}", "to close `instruments`")?;
                Ok(Item::Instruments(names, t.span.to(end)))
            }
            "universe" => {
                self.bump();
                self.expect_punct("{", "after `universe`")?;
                let end = self.expect_punct("}", "to close `universe` (it takes nothing yet: the run gives the instruments)")?;
                Ok(Item::Universe(t.span.to(end)))
            }
            "replay" => {
                self.bump();
                self.expect_punct("{", "after `replay`")?;
                let mut settings = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    match self.setting() {
                        Ok(st) => settings.push(st),
                        Err(d) => {
                            self.errors.push(d);
                            self.recover();
                        }
                    }
                }
                let end = self.expect_punct("}", "to close `replay`")?;
                Ok(Item::Replay(settings, t.span.to(end)))
            }
            "functions" => {
                self.bump();
                self.expect_punct("{", "after `functions`")?;
                let mut defs = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    let (ret, ret_span) = self.ty()?;
                    let name = self.ident("a function name")?;
                    let args = self.args()?;
                    let (body, span) = self.block()?;
                    defs.push(FunctionDef { ret, name, args, body, span: ret_span.to(span) });
                }
                let end = self.expect_punct("}", "to close `functions`")?;
                Ok(Item::Functions(defs, t.span.to(end)))
            }
            "indicator" => {
                self.bump();
                let name = self.ident("an indicator name")?;
                let args = self.args()?;
                self.expect_punct("{", "to open the indicator")?;
                let mut parts = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    match self.ind_part() {
                        Ok(p) => parts.push(p),
                        Err(d) => {
                            self.errors.push(d);
                            self.recover();
                        }
                    }
                }
                let end = self.expect_punct("}", "to close the indicator")?;
                Ok(Item::Indicator(IndicatorDef { name, args, parts, span: t.span.to(end) }))
            }
            "options" => {
                self.bump();
                self.expect_punct("{", "after `options`")?;
                let mut settings = vec![];
                while !self.is_punct("}") && self.peek().tok != Tok::Eof {
                    let key = match self.ident("an option name") {
                        Ok(k) => k,
                        Err(d) => {
                            self.errors.push(d);
                            self.recover();
                            continue;
                        }
                    };
                    self.expect_punct("=", &format!("after `{}`", key.name))?;
                    let value = self.expr()?;
                    self.expect_punct(";", "after the option")?;
                    settings.push((key, value));
                }
                let end = self.expect_punct("}", "to close `options`")?;
                Ok(Item::Options(settings, t.span.to(end)))
            }
            "on_start" | "on_bar" | "on_fill" | "on_stop" | "on_open" | "on_close" => {
                self.bump();
                let kind = match word.as_str() {
                    "on_start" => HandlerKind::OnStart,
                    "on_bar" => HandlerKind::OnBar,
                    "on_fill" => HandlerKind::OnFill,
                    "on_open" => HandlerKind::OnOpen,
                    "on_close" => HandlerKind::OnClose,
                    _ => HandlerKind::OnStop,
                };
                let (body, span) = self.block()?;
                Ok(Item::Handler(kind, body, t.span.to(span)))
            }
            "param" | "let" | "target" => Err(Diag::new(Code::V0Syntax,
                format!("`{}` is v0 syntax", word),
                t.span,
                "not a block",
            )
            .note("v0 `param`, `let` and `target =` became `params {}`, `series {}` and `on_bar { target(...); }`")),
            "live" => Err(Diag::new(Code::LiveBlock, "a program has no `live {}` block", t.span, "not a block")
                .note("live settings (broker, account, limits) belong to the run spec and the operator's configuration, never to the strategy")),
            _ => Err(Diag::new(Code::UnknownBlock, format!("unknown block `{}`", word), t.span, "not a block").note(BLOCKS_NOTE)),
        }
    }

    /// `section.key = value;` in `replay {}`.
    fn setting(&mut self) -> R<Setting> {
        let first = self.peek().clone();
        let mut key = match &first.tok {
            Tok::Ident(s) => s.clone(),
            _ => {
                return Err(Diag::new(
                    Code::ExpectedSetting,
                    format!("expected a setting name, found {}", first.describe()),
                    first.span,
                    "e.g. `cash = 100000;`",
                ));
            }
        };
        self.bump();
        let mut span = first.span;
        while self.eat_punct(".") {
            let t = self.peek().clone();
            match &t.tok {
                Tok::Ident(s) => {
                    key.push('.');
                    key.push_str(s);
                    span = span.to(t.span);
                    self.bump();
                }
                _ => {
                    return Err(Diag::new(
                        Code::ExpectedSetting,
                        format!("expected a name after `.`, found {}", t.describe()),
                        t.span,
                        "a dotted key",
                    ));
                }
            }
        }
        self.expect_punct("=", &format!("after `{}`", key))?;
        let t = self.peek().clone();
        let neg = self.eat_punct("-");
        let t2 = self.peek().clone();
        let value = match &t2.tok {
            Tok::Num(v, _) => Lit::Num(if neg { -*v } else { *v }),
            Tok::Str(s) if !neg => Lit::Str(s.clone()),
            Tok::Ident(s) if !neg && (s == "true" || s == "false") => Lit::Bool(s == "true"),
            _ => {
                return Err(Diag::new(
                    Code::ExpectedLiteral,
                    format!("expected a number, a string or a bool, found {}", t2.describe()),
                    t2.span,
                    "a setting's value is a literal",
                ));
            }
        };
        self.bump();
        let end = self.expect_punct(";", "after the setting")?;
        Ok(Setting {
            key,
            value,
            span: span.to(t.span).to(end),
        })
    }

    /// `(real x, int n)`.
    fn args(&mut self) -> R<Vec<Arg>> {
        self.expect_punct("(", "before the arguments")?;
        let mut args = vec![];
        if !self.is_punct(")") {
            loop {
                let (ty, _) = self.ty()?;
                let name = self.ident("an argument name")?;
                args.push(Arg { ty, name });
                if !self.eat_punct(",") {
                    break;
                }
            }
        }
        self.expect_punct(")", "after the arguments")?;
        Ok(args)
    }

    /// One part of an indicator: window, state, update, enter, leave, value, warmup.
    fn ind_part(&mut self) -> R<IndPart> {
        let t = self.peek().clone();
        let word = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => String::new(),
        };
        match word.as_str() {
            "window" => {
                self.bump();
                let name = self.ident("a window name")?;
                self.expect_punct("[", "before the window's length")?;
                let size = self.expr()?;
                self.expect_punct("]", "after the window's length")?;
                let end = self.expect_punct(";", "after the window")?;
                Ok(IndPart::Window { name, size, span: t.span.to(end) })
            }
            "state" => {
                self.bump();
                Ok(IndPart::State(self.var_decl()?))
            }
            "value" => {
                self.bump();
                Ok(IndPart::Value(self.var_decl()?))
            }
            "warmup" => {
                self.bump();
                let e = self.expr()?;
                let end = self.expect_punct(";", "after the warm-up")?;
                Ok(IndPart::Warmup(e, t.span.to(end)))
            }
            "update" => {
                self.bump();
                let (body, span) = self.block()?;
                Ok(IndPart::Update(body, t.span.to(span)))
            }
            "enter" | "leave" => {
                self.bump();
                self.expect_punct("(", &format!("after `{}`", word))?;
                let mut names = vec![];
                if !self.is_punct(")") {
                    loop {
                        names.push(self.ident("a name for the value")?);
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                }
                self.expect_punct(")", "after the names")?;
                let (body, span) = self.block()?;
                Ok(if word == "enter" { IndPart::Enter(names, body, t.span.to(span)) } else { IndPart::Leave(names, body, t.span.to(span)) })
            }
            _ => Err(Diag::new(Code::ExpectedIndicatorPart, format!("expected a part of an indicator, found {}", t.describe()), t.span, "not a part")
                .note("parts: window w[n];  state real s = 0;  update { }  enter(v) { }  leave(v) { }  value real v = ...;  warmup n;")),
        }
    }

    fn ty(&mut self) -> R<(Ty, Span)> {
        let t = self.peek().clone();
        let ty = match &t.tok {
            Tok::Ident(s) if s == "int" => Ty::Int,
            Tok::Ident(s) if s == "real" => Ty::Real,
            Tok::Ident(s) if s == "bool" => Ty::Bool,
            Tok::Ident(s) if s == "double" || s == "float" => {
                return Err(Diag::new(
                    Code::UnknownType,
                    format!("unknown type `{}`", s),
                    t.span,
                    "write `real`",
                ));
            }
            _ => {
                return Err(Diag::new(
                    Code::ExpectedType,
                    format!("expected a type (`int`, `real` or `bool`), found {}", t.describe()),
                    t.span,
                    "a declaration starts with its type",
                ));
            }
        };
        self.bump();
        Ok((ty, t.span))
    }

    fn param(&mut self) -> R<ParamDecl> {
        let (ty, ty_span) = self.ty()?;
        let (mut lower, mut upper) = (None, None);
        if self.eat_punct("<") {
            loop {
                let t = self.peek().clone();
                let which = match &t.tok {
                    Tok::Ident(s) if s == "lower" || s == "upper" => s.clone(),
                    _ => {
                        return Err(Diag::new(
                            Code::ExpectedBound,
                            format!("expected `lower` or `upper`, found {}", t.describe()),
                            t.span,
                            "a bound is `lower=EXPR` or `upper=EXPR`",
                        ));
                    }
                };
                self.bump();
                self.expect_punct("=", &format!("after `{}`", which))?;
                // A bound stops before comparisons, so the closing `>` is not read as one.
                let e = self.additive()?;
                if which == "lower" {
                    lower = Some(e);
                } else {
                    upper = Some(e);
                }
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(">", "to close the bounds")?;
        }
        let name = self.ident("a parameter name")?;
        self.expect_punct("=", "before the parameter's default")?;
        let default = self.expr()?;
        let end = self.expect_punct(";", "after the parameter")?;
        Ok(ParamDecl {
            ty,
            ty_span,
            lower,
            upper,
            name,
            default,
            span: ty_span.to(end),
        })
    }

    fn var_decl(&mut self) -> R<VarDecl> {
        let (ty, ty_span) = self.ty()?;
        let name = self.ident("a name")?;
        self.expect_punct("=", &format!("after `{}`", name.name))?;
        let init = self.expr()?;
        let end = self.expect_punct(";", "after the declaration")?;
        Ok(VarDecl {
            ty,
            ty_span,
            name,
            init,
            span: ty_span.to(end),
        })
    }

    fn block(&mut self) -> R<(Vec<Stmt>, Span)> {
        if self.depth >= MAX_DEPTH {
            let t = self.peek().clone();
            return Err(too_deep(t.span));
        }
        self.depth += 1;
        let b = self.block_inner();
        self.depth -= 1;
        b
    }

    fn block_inner(&mut self) -> R<(Vec<Stmt>, Span)> {
        let open = self.expect_punct("{", "to open the block")?;
        let mut body = vec![];
        while !self.is_punct("}") && self.peek().tok != Tok::Eof {
            match self.stmt() {
                Ok(s) => body.push(s),
                Err(d) => {
                    self.errors.push(d);
                    self.recover();
                }
            }
        }
        let close = self.expect_punct("}", "to close the block").map_err(|d| {
            if self.peek().tok == Tok::Eof {
                d.also(open, "this `{` is never closed")
            } else {
                d
            }
        })?;
        Ok((body, open.to(close)))
    }

    fn stmt(&mut self) -> R<Stmt> {
        let t = self.peek().clone();
        if let Tok::Ident(w) = &t.tok {
            match w.as_str() {
                "int" | "real" | "bool" => return Ok(Stmt::Local(self.var_decl()?)),
                "if" => return self.if_stmt(),
                "for" => {
                    self.bump();
                    self.expect_punct("(", "after `for`")?;
                    let var = self.ident("a loop variable")?;
                    if !self.is_word("in") {
                        let n = self.peek().clone();
                        return Err(Diag::new(
                            Code::ExpectedToken,
                            format!("expected `in`, found {}", n.describe()),
                            n.span,
                            "a loop is `for (i in FROM:TO)`",
                        ));
                    }
                    self.bump();
                    let from = self.additive()?;
                    self.expect_punct(":", "between the loop's bounds")?;
                    let to = self.additive()?;
                    self.expect_punct(")", "after the loop's range")?;
                    let (body, span) = self.block()?;
                    return Ok(Stmt::For {
                        var,
                        from,
                        to,
                        body,
                        span: t.span.to(span),
                    });
                }
                "return" => {
                    self.bump();
                    let value = self.expr()?;
                    let end = self.expect_punct(";", "after the returned value")?;
                    return Ok(Stmt::Return {
                        value,
                        span: t.span.to(end),
                    });
                }
                "while" => {
                    self.bump();
                    self.expect_punct("(", "after `while`")?;
                    let cond = self.expr()?;
                    self.expect_punct(")", "after the condition")?;
                    let (body, span) = self.block()?;
                    return Ok(Stmt::While {
                        cond,
                        body,
                        span: t.span.to(span),
                    });
                }
                "target" if matches!(self.peek_at(1).tok, Tok::Punct("(")) => {
                    self.bump();
                    self.bump();
                    // `target(y, x)`: an instrument name, then the shares.
                    let mut inst = None;
                    if matches!(self.peek().tok, Tok::Ident(_)) && matches!(self.peek_at(1).tok, Tok::Punct(",")) {
                        inst = Some(self.ident("an instrument")?);
                        self.bump();
                    }
                    let value = self.expr()?;
                    self.expect_punct(")", "after the target")?;
                    let end = self.expect_punct(";", "after `target(...)`")?;
                    return Ok(Stmt::Target {
                        inst,
                        value,
                        span: t.span.to(end),
                    });
                }
                "target" if matches!(self.peek_at(1).tok, Tok::Punct("=")) => {
                    return Err(Diag::new(
                        Code::V0Syntax,
                        "`target = ...` is v0 syntax",
                        t.span,
                        "write `target(...);`",
                    ));
                }
                _ => {}
            }
            if matches!(self.peek_at(1).tok, Tok::Punct("=")) {
                let name = self.ident("a name")?;
                self.bump();
                let value = self.expr()?;
                let end = self.expect_punct(";", "after the assignment")?;
                return Ok(Stmt::Assign {
                    span: name.span.to(end),
                    name,
                    value,
                });
            }
        }
        let expr = self.expr()?;
        let end = self.expect_punct(";", "after the expression")?;
        Ok(Stmt::Expr {
            span: expr.span.to(end),
            expr,
        })
    }

    fn if_stmt(&mut self) -> R<Stmt> {
        let start = self.bump().span;
        self.expect_punct("(", "after `if`")?;
        let cond = self.expr()?;
        self.expect_punct(")", "after the condition")?;
        let (then, mut span) = self.block()?;
        let mut els = None;
        if self.is_word("else") {
            self.bump();
            if self.is_word("if") {
                // `else if` nests an `if` in the `else`: one level deeper, like a block.
                if self.depth >= MAX_DEPTH {
                    let t = self.peek().clone();
                    return Err(too_deep(t.span));
                }
                self.depth += 1;
                let s = self.if_stmt();
                self.depth -= 1;
                let s = s?;
                span = span.to(stmt_span(&s));
                els = Some(vec![s]);
            } else {
                let (b, s) = self.block()?;
                span = span.to(s);
                els = Some(b);
            }
        }
        Ok(Stmt::If {
            cond,
            then,
            els,
            span: start.to(span),
        })
    }

    // Expressions, lowest to highest: ?:, or, and, not, comparison, + -, * /, unary -, postfix.
    pub fn expr(&mut self) -> R<Expr> {
        if self.depth >= MAX_DEPTH {
            let t = self.peek().clone();
            return Err(too_deep(t.span));
        }
        self.depth += 1;
        let e = self.expr_inner();
        self.depth -= 1;
        e
    }

    fn expr_inner(&mut self) -> R<Expr> {
        let cond = self.or()?;
        if self.eat_punct("?") {
            let hc = self.height;
            let a = self.expr()?;
            let ha = self.height;
            self.expect_punct(":", "in `?:`")?;
            let b = self.expr()?;
            let span = cond.span.to(b.span);
            let h = 1 + hc.max(ha).max(self.height);
            self.node(h, span)?;
            self.height = h;
            return Ok(Expr {
                kind: ExprKind::Ternary(Box::new(cond), Box::new(a), Box::new(b)),
                span,
            });
        }
        Ok(cond)
    }

    fn c_style(&self, wanted: &str) -> R<()> {
        let t = self.peek();
        let alt = match &t.tok {
            Tok::Punct("&&") => Some("and"),
            Tok::Punct("||") => Some("or"),
            Tok::Punct("!") => Some("not"),
            _ => None,
        };
        if let Some(a) = alt {
            if a == wanted {
                return Err(Diag::new(
                    Code::CStyleOperator,
                    format!("use `{}`", a),
                    t.span,
                    format!("write `{}`", a),
                ));
            }
        }
        Ok(())
    }

    fn or(&mut self) -> R<Expr> {
        let mut left = self.and()?;
        let mut h = self.height;
        loop {
            self.c_style("or")?;
            if !self.is_word("or") {
                self.height = h;
                return Ok(left);
            }
            self.bump();
            let right = self.and()?;
            let span = left.span.to(right.span);
            h = 1 + h.max(self.height);
            self.node(h, span)?;
            left = Expr {
                kind: ExprKind::Binary(BinOp::Or, Box::new(left), Box::new(right)),
                span,
            };
        }
    }

    fn and(&mut self) -> R<Expr> {
        let mut left = self.not()?;
        let mut h = self.height;
        loop {
            self.c_style("and")?;
            if !self.is_word("and") {
                self.height = h;
                return Ok(left);
            }
            self.bump();
            let right = self.not()?;
            let span = left.span.to(right.span);
            h = 1 + h.max(self.height);
            self.node(h, span)?;
            left = Expr {
                kind: ExprKind::Binary(BinOp::And, Box::new(left), Box::new(right)),
                span,
            };
        }
    }

    fn not(&mut self) -> R<Expr> {
        self.c_style("not")?;
        if self.is_word("not") {
            if self.depth >= MAX_DEPTH {
                let t = self.peek().clone();
                return Err(too_deep(t.span));
            }
            let start = self.bump().span;
            self.depth += 1;
            let inner = self.not();
            self.depth -= 1;
            let inner = inner?;
            let span = start.to(inner.span);
            self.height += 1;
            self.node(self.height, span)?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Not, Box::new(inner)),
                span,
            });
        }
        self.comparison()
    }

    fn comparison(&mut self) -> R<Expr> {
        let left = self.additive()?;
        let hl = self.height;
        let op = match &self.peek().tok {
            Tok::Punct("<") => BinOp::Lt,
            Tok::Punct("<=") => BinOp::Le,
            Tok::Punct(">") => BinOp::Gt,
            Tok::Punct(">=") => BinOp::Ge,
            Tok::Punct("==") => BinOp::Eq,
            Tok::Punct("!=") => BinOp::Ne,
            Tok::Punct("=") => {
                let t = self.peek().clone();
                return Err(Diag::new(
                    Code::AssignInExpression,
                    "`=` assigns; compare with `==`",
                    t.span,
                    "write `==`",
                ));
            }
            _ => return Ok(left),
        };
        self.bump();
        let right = self.additive()?;
        let span = left.span.to(right.span);
        let h = 1 + hl.max(self.height);
        if matches!(&self.peek().tok, Tok::Punct("<" | "<=" | ">" | ">=" | "==" | "!=")) {
            let t = self.peek().clone();
            return Err(Diag::new(
                Code::ChainedComparison,
                "comparisons do not chain",
                t.span,
                "a second comparison",
            )
            .also(span, "this is already a comparison")
            .note("write `a < b and b < c`"));
        }
        self.node(h, span)?;
        self.height = h;
        Ok(Expr {
            kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
            span,
        })
    }

    fn additive(&mut self) -> R<Expr> {
        let mut left = self.multiplicative()?;
        let mut h = self.height;
        loop {
            let op = match &self.peek().tok {
                Tok::Punct("+") => BinOp::Add,
                Tok::Punct("-") => BinOp::Sub,
                _ => {
                    self.height = h;
                    return Ok(left);
                }
            };
            self.bump();
            let right = self.multiplicative()?;
            let span = left.span.to(right.span);
            h = 1 + h.max(self.height);
            self.node(h, span)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                span,
            };
        }
    }

    fn multiplicative(&mut self) -> R<Expr> {
        let mut left = self.unary()?;
        let mut h = self.height;
        loop {
            let op = match &self.peek().tok {
                Tok::Punct("*") => BinOp::Mul,
                Tok::Punct("/") => BinOp::Div,
                _ => {
                    self.height = h;
                    return Ok(left);
                }
            };
            self.bump();
            let right = self.unary()?;
            let span = left.span.to(right.span);
            h = 1 + h.max(self.height);
            self.node(h, span)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                span,
            };
        }
    }

    fn unary(&mut self) -> R<Expr> {
        if self.is_punct("-") && self.depth >= MAX_DEPTH {
            let t = self.peek().clone();
            return Err(too_deep(t.span));
        }
        if self.is_punct("-") {
            let start = self.bump().span;
            self.depth += 1;
            let inner = self.unary();
            self.depth -= 1;
            let inner = inner?;
            let span = start.to(inner.span);
            self.height += 1;
            self.node(self.height, span)?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Neg, Box::new(inner)),
                span,
            });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> R<Expr> {
        let mut e = self.primary()?;
        let mut h = self.height;
        while self.is_punct("[") {
            self.bump();
            let k = self.expr()?;
            let end = self.expect_punct("]", "after the lag")?;
            let span = e.span.to(end);
            h = 1 + h.max(self.height);
            self.node(h, span)?;
            e = Expr {
                kind: ExprKind::Index(Box::new(e), Box::new(k)),
                span,
            };
        }
        self.height = h;
        Ok(e)
    }

    fn primary(&mut self) -> R<Expr> {
        let t = self.peek().clone();
        match &t.tok {
            Tok::Num(v, int) => {
                self.bump();
                self.height = 1;
                Ok(Expr {
                    kind: ExprKind::Num(*v, *int),
                    span: t.span,
                })
            }
            Tok::Punct("(") => {
                self.bump();
                let e = self.expr()?;
                let end = self.expect_punct(")", "to close the parenthesis")?;
                Ok(Expr {
                    kind: e.kind,
                    span: t.span.to(end),
                })
            }
            Tok::Ident(s) if s == "true" || s == "false" => {
                self.bump();
                self.height = 1;
                Ok(Expr {
                    kind: ExprKind::Bool(s == "true"),
                    span: t.span,
                })
            }
            Tok::Ident(s) if !KEYWORDS.contains(&s.as_str()) || s == "target" => {
                let name = Ident {
                    name: s.clone(),
                    span: t.span,
                };
                self.bump();
                if self.eat_punct("(") {
                    let mut args = vec![];
                    let mut h = 1;
                    if !self.is_punct(")") {
                        loop {
                            // `name = value`: a named argument (`missing = skip`).
                            let named = matches!(self.peek().tok, Tok::Ident(_))
                                && matches!(self.peek_at(1).tok, Tok::Punct("="));
                            if named {
                                let key = self.ident("an argument name")?;
                                self.bump();
                                let v = self.expr()?;
                                let span = key.span.to(v.span);
                                h = h.max(2 + self.height);
                                args.push(Expr {
                                    kind: ExprKind::Named(key, Box::new(v)),
                                    span,
                                });
                            } else {
                                args.push(self.expr()?);
                                h = h.max(1 + self.height);
                            }
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                    }
                    let end = self.expect_punct(")", &format!("to close `{}(`", name.name))?;
                    let span = t.span.to(end);
                    self.node(h, span)?;
                    let call = Expr {
                        span,
                        kind: ExprKind::Call(name, args),
                    };
                    self.height = h;
                    if self.is_punct(".") {
                        // `mean_sd(close, 20).sd`
                        self.bump();
                        let field = self.ident("a value name")?;
                        let span = call.span.to(field.span);
                        self.node(h + 1, span)?;
                        self.height = h + 1;
                        return Ok(Expr {
                            span,
                            kind: ExprKind::Member(Box::new(call), field),
                        });
                    }
                    return Ok(call);
                }
                if self.is_punct(".") {
                    self.bump();
                    let field = self.ident("a field name")?;
                    self.height = 1;
                    return Ok(Expr {
                        span: t.span.to(field.span),
                        kind: ExprKind::Field(name, field),
                    });
                }
                self.height = 1;
                Ok(Expr {
                    kind: ExprKind::Name(name.name),
                    span: t.span,
                })
            }
            _ => Err(Diag::new(
                Code::ExpectedExpression,
                format!("expected an expression, found {}", t.describe()),
                t.span,
                "expected an expression",
            )),
        }
    }
}

/// The refusal of a program nested deeper than [`MAX_DEPTH`].
fn too_deep(span: Span) -> Diag {
    Diag::new(Code::NestingTooDeep, format!("the program nests deeper than {MAX_DEPTH} levels here"), span, "too deep")
        .note("blocks, `else if`, parentheses and each operator of a chain count one level each; split the expression with locals or series")
}

pub fn stmt_span(s: &Stmt) -> Span {
    match s {
        Stmt::Local(d) => d.span,
        Stmt::Assign { span, .. }
        | Stmt::Target { span, .. }
        | Stmt::If { span, .. }
        | Stmt::For { span, .. }
        | Stmt::While { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Expr { span, .. } => *span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ast_text;
    use crate::frontend::diag::Code;

    fn expr(s: &str) -> Expr {
        let program = parse(&format!("on_bar {{ x = {s}; }}")).unwrap();
        let Item::Handler(_, body, _) = &program.items[0] else {
            panic!()
        };
        let Stmt::Assign { value, .. } = &body[0] else { panic!() };
        value.clone()
    }

    fn codes(s: &str) -> Vec<Code> {
        parse(s).unwrap_err().iter().map(|d| d.code).collect()
    }

    #[test]
    fn precedence() {
        let shown = |s: &str| ast_text(&expr(s));
        assert_eq!(shown("a + b * c - d"), "a + b * c - d");
        assert_eq!(shown("(a + b) * c"), "(a + b) * c");
        assert_eq!(shown("not a and b or c"), "not a and b or c");
        assert!(matches!(expr("a or b and c").kind, ExprKind::Binary(BinOp::Or, _, _)));
        assert!(
            matches!(expr("a ? b : c ? d : e").kind, ExprKind::Ternary(_, _, ref e) if matches!(e.kind, ExprKind::Ternary(..)))
        );
        assert!(
            matches!(expr("-a[1]").kind, ExprKind::Unary(UnOp::Neg, ref i) if matches!(i.kind, ExprKind::Index(..)))
        );
        assert!(matches!(expr("f(x).v").kind, ExprKind::Member(..)));
        assert!(matches!(expr("y.close").kind, ExprKind::Field(..)));
        assert!(
            matches!(expr("sma(x, 3, missing = skip)").kind, ExprKind::Call(_, ref a) if matches!(a[2].kind, ExprKind::Named(..)))
        );
        // Parentheses keep their span but leave no node.
        let e = expr("(1)");
        assert_eq!((e.kind, e.span), (ExprKind::Num(1.0, true), Span::new(13, 16)));
    }

    #[test]
    fn statements_and_blocks() {
        let p = parse("params { int<lower=1, upper=n * 2> n = 3; } replay { cash = -1; fill.mode = \"x\"; replay.on = true; } \
                       on_bar { target(y, 1); for (i in 1:n) { } while (true) { } f(1); if (a) { } else if (b) { } else { } return 1; }")
            .unwrap();
        let Item::Replay(settings, _) = &p.items[1] else {
            panic!()
        };
        assert_eq!(
            settings.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
            ["cash", "fill.mode", "replay.on"]
        );
        assert_eq!(settings[0].value, Lit::Num(-1.0));
        let Item::Handler(_, body, _) = &p.items[2] else {
            panic!()
        };
        assert!(matches!(&body[0], Stmt::Target { inst: Some(i), .. } if i.name == "y"));
        assert!(matches!(body[2], Stmt::While { .. }));
        assert!(matches!(body[3], Stmt::Expr { .. }));
        assert!(matches!(body[5], Stmt::Return { .. }));
    }

    #[test]
    fn errors_and_recovery() {
        assert_eq!(codes("on_bar { target(1) }"), [Code::ExpectedToken]);
        assert_eq!(
            codes("on_bar { a = 1 b = 2; c = ; }"),
            [Code::ExpectedToken, Code::ExpectedExpression]
        );
        assert_eq!(codes("on_bar { if (a && b) { } }"), [Code::CStyleOperator]);
        assert_eq!(codes("on_bar { if (a = b) { } }"), [Code::AssignInExpression]);
        assert_eq!(codes("on_bar { x = a < b < c; }"), [Code::ChainedComparison]);
        assert_eq!(codes("param n = 1;"), [Code::V0Syntax]);
        assert_eq!(codes("on_bar { target = 1; }"), [Code::V0Syntax]);
        assert_eq!(codes("live { }"), [Code::LiveBlock]);
        assert_eq!(codes("1"), [Code::ExpectedBlock]);
        assert_eq!(codes("params { int for = 1; }"), [Code::KeywordAsName]);
        assert_eq!(codes("on_bar {"), [Code::ExpectedToken]);
    }

    #[test]
    fn nesting_limit() {
        // Deep nesting needs more stack than a test thread has in a debug build.
        let run = |f: fn()| {
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn(f)
                .unwrap()
                .join()
                .unwrap()
        };
        run(|| {
            assert_eq!(
                codes(&format!("on_bar {{ x = {}1; }}", "-".repeat(300))),
                [Code::NestingTooDeep]
            )
        });
        run(|| {
            assert_eq!(
                codes(&format!("on_bar {{ x = {}a; }}", "not ".repeat(300))),
                [Code::NestingTooDeep]
            )
        });
        run(|| {
            assert_eq!(
                codes(&format!("on_bar {{ x = {}1{}; }}", "(".repeat(300), ")".repeat(300))),
                [Code::NestingTooDeep]
            )
        });
        // A chain of operators counts one level per operator.
        let chain = |n: usize| format!("on_bar {{ x = 1{}; }}", " + 1".repeat(n - 1));
        assert!(parse(&chain(254)).is_ok());
        assert_eq!(codes(&chain(255)), [Code::NestingTooDeep]);
        assert_eq!(codes(&chain(100_000)), [Code::NestingTooDeep]);
        // The deepest nesting accepted.
        run(|| drop(parse(&format!("on_bar {{ x = {}1{}; }}", "(".repeat(254), ")".repeat(254))).unwrap()));
    }
}
