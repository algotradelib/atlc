//! Instruments and universes: the program's mode, which handlers each mode allows, bare and qualified
//! fields, and cross-sectional operators.

use crate::frontend::ast::*;
use crate::frontend::diag::{Code, Diag};
use crate::ir::*;

use super::*;

impl Checker<'_> {
    /// Which handlers a program may have, by its mode.
    pub(super) fn handler_rules(&mut self, program: &Program) {
        let span_of = |kind: HandlerKind| {
            program.items.iter().find_map(|i| match i {
                Item::Handler(k, _, s) if *k == kind => Some(Span::new(s.start, s.start + kind.name().len())),
                _ => None,
            })
        };
        let legs = span_of(HandlerKind::OnOpen).or(span_of(HandlerKind::OnClose));
        if let Some(l) = legs {
            if let Some(b) = span_of(HandlerKind::OnBar) {
                self.err(Diag::new(Code::BarAndLegs, "`on_bar` and `on_open`/`on_close` in one program", b, "`on_bar`")
                    .also(l, "a session leg")
                    .note("a program sees every bar (`on_bar`) or each day as an open and a close (`on_open`, `on_close`), not both"));
            }
            if self.ir.mode != Mode::Universe {
                self.err(
                    Diag::new(
                        Code::LegsOutsideUniverse,
                        "`on_open`/`on_close` are supported in `universe {}` programs only",
                        l,
                        "not here yet",
                    )
                    .note("add `universe {}`; a single-instrument program with legs is not built yet"),
                );
            }
            self.ir.legs = true;
        } else if self.ir.handlers.on_bar.is_none() {
            self.err(
                Diag::new(
                    Code::MissingOnBar,
                    "the program has no `on_bar`",
                    Span::new(0, 0),
                    "nothing ever states a target",
                )
                .note("add `on_bar { target(...); }`"),
            );
        }
        if self.ir.mode != Mode::Single {
            if let Some(f) = span_of(HandlerKind::OnFill) {
                self.err(Diag::new(
                    Code::OnFillOutsideSingle,
                    "`on_fill` is supported in single-instrument programs only",
                    f,
                    "not here yet",
                ));
            }
        }
    }

    /// Bare `close` is refused when the program names its instruments.
    pub(super) fn bare_fields(&mut self, n: &str, span: Span) -> bool {
        if let Mode::Named(names) = &self.ir.mode {
            let first = names.first().cloned().unwrap_or_else(|| "y".into());
            self.err(
                Diag::new(
                    Code::BareField,
                    format!("which instrument's `{}`?", n),
                    span,
                    "the program names its instruments",
                )
                .note(format!("write `{}.{}`", first, n)),
            );
            return false;
        }
        true
    }

    /// `xs_mean(x)`, `xs_sum(x)`, `xs_count(x)`, `xs_rank(x)`: over every instrument of a universe whose
    /// `x` is defined, at this moment.
    pub(super) fn xs(&mut self, name: &Ident, args: &[Expr], span: Span, k: HandlerKind) -> Option<HExpr> {
        let op = match name.name.as_str() {
            "xs_mean" => XsOp::Mean,
            "xs_sum" => XsOp::Sum,
            "xs_count" => XsOp::Count,
            "xs_rank" => XsOp::Rank,
            _ => {
                self.err(
                    Diag::new(
                        Code::UnknownFunction,
                        format!("unknown function `{}`", name.name),
                        name.span,
                        "not a function",
                    )
                    .note("cross-sectional operators: xs_mean xs_sum xs_count xs_rank"),
                );
                return None;
            }
        };
        if self.ir.mode != Mode::Universe {
            self.err(
                Diag::new(
                    Code::XsOutsideUniverse,
                    format!("`{}` needs `universe {{}}`", name.name),
                    span,
                    "no cross-section here",
                )
                .note("a cross-sectional operator combines every instrument of a universe at one moment"),
            );
            return None;
        }
        if self.nest > 0 {
            self.err(
                Diag::new(
                    Code::XsNested,
                    format!("`{}` inside `if` or `for`", name.name),
                    span,
                    "must see every instrument",
                )
                .note(
                    "a cross-sectional value combines every instrument, so it cannot depend on one instrument's branch",
                )
                .note(format!(
                    "declare `real m = {};` at the top of `{}` and read `m` here",
                    crate::frontend::ast_text(&Expr {
                        kind: ExprKind::Call(name.clone(), args.to_vec()),
                        span
                    }),
                    k.name()
                )),
            );
            return None;
        }
        if args.len() != 1 {
            self.err(Diag::new(
                Code::FunctionArity,
                format!("`{}` takes one argument", name.name),
                span,
                format!("{} given", args.len()),
            ));
            return None;
        }
        let a = self.hexpr(&args[0], k)?;
        if !a.ty.numeric() {
            self.err(
                Diag::new(
                    Code::ArgumentType,
                    format!("`{}` needs a number", name.name),
                    args[0].span,
                    format!("this is `{}`", a.ty.name()),
                )
                .note("write `c ? 1 : 0` to count a condition"),
            );
            return None;
        }
        let id = self.ir.xs_count;
        self.ir.xs_count += 1;
        // A sum or count over no defined value is 0; a mean is undefined, and so is the rank of an undefined value.
        let maybe = matches!(op, XsOp::Mean | XsOp::Rank);
        Some(Self::h(HKind::Xs(op, Box::new(a), id), Ty::Real, maybe))
    }

    /// `instruments {}`, `universe {}`, `replay {}` and `options {}`: how the program sees instruments,
    /// and its missing-value policy.
    pub(super) fn modes(&mut self, program: &Program) {
        let mut named: Option<Span> = None;
        let mut universe: Option<Span> = None;
        for item in &program.items {
            match item {
                Item::Instruments(names, span) => {
                    named = Some(*span);
                    if names.is_empty() {
                        self.err(
                            Diag::new(
                                Code::EmptyInstruments,
                                "`instruments {}` names no instrument",
                                *span,
                                "empty",
                            )
                            .note("write `instruments { y; x; }`"),
                        );
                    }
                    let mut list = vec![];
                    for (i, n) in names.iter().enumerate() {
                        if self.declare(n, Name::Instrument(i)) {
                            list.push(n.name.clone());
                        }
                    }
                    self.ir.mode = Mode::Named(list);
                }
                Item::Universe(span) => {
                    universe = Some(*span);
                    self.ir.mode = Mode::Universe;
                }
                Item::Replay(settings, _) => self.ir.replay = settings.clone(),
                Item::Options(settings, _) => {
                    for (key, value) in settings {
                        self.option(key, value);
                    }
                }
                _ => {}
            }
        }
        self.ir.period_clock = self.ir.mode == Mode::Universe && self.ir.missing != Policy::Restart;
        if let (Some(a), Some(b)) = (named, universe) {
            self.err(
                Diag::new(
                    Code::InstrumentsAndUniverse,
                    "a program is either `instruments {}` or `universe {}`",
                    b,
                    "second",
                )
                .also(a, "first")
                .note("named instruments are read as `y.close`; a universe runs the same code for every instrument"),
            );
        }
    }
}
