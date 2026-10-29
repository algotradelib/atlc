//! The grammar and atlc's hand-written parser accept and refuse the same programs at the syntax stage, and
//! build the same tree, spans included. Programs: every `.atls` of the main package's tests and of
//! `grammar/tests/corpus/`, and token-level mutants of them (a token deleted, doubled, swapped with the
//! next one, or replaced), which reach the refusals no file shows.
//!
//! Two exceptions, both stated in `docs/reference/syntax.md`:
//! - the forms the parser accepts only so that the checker can explain them (an expression used as a
//!   statement, `while`, `target` used as a value) are not in the grammar;
//! - the parser refuses expressions nested more than 256 levels deep, an implementation limit.

use std::path::{Path, PathBuf};

use atlc::frontend::ast::*;
use atlc::frontend::diag::Code;
use atlc::frontend::lexer::lex;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "atls") {
            out.push(p);
        }
    }
}

fn corpus() -> Vec<(String, String)> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![];
    walk(&here.join("../tests"), &mut files);
    walk(&here.join("tests/corpus"), &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| (p.display().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect()
}

/// Whether a parsed program uses a form the parser accepts only to report it later.
fn diagnostic_only(p: &Program) -> bool {
    fn expr(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Name(n) => n == "target",
            ExprKind::Call(f, args) => f.name == "target" || args.iter().any(expr),
            ExprKind::Field(o, _) => o.name == "target",
            ExprKind::Num(..) | ExprKind::Bool(_) => false,
            ExprKind::Unary(_, a) | ExprKind::Named(_, a) | ExprKind::Member(a, _) => expr(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => expr(a) || expr(b),
            ExprKind::Ternary(c, a, b) => expr(c) || expr(a) || expr(b),
        }
    }
    fn stmts(body: &[Stmt]) -> bool {
        body.iter().any(|s| match s {
            Stmt::While { .. } | Stmt::Expr { .. } => true,
            Stmt::Local(d) => expr(&d.init),
            Stmt::Assign { value, .. } | Stmt::Target { value, .. } | Stmt::Return { value, .. } => expr(value),
            Stmt::If { cond, then, els, .. } => expr(cond) || stmts(then) || els.as_deref().is_some_and(stmts),
            Stmt::For { from, to, body, .. } => expr(from) || expr(to) || stmts(body),
        })
    }
    p.items.iter().any(|item| match item {
        Item::Params(d, _) => d
            .iter()
            .any(|d| expr(&d.default) || d.lower.iter().chain(&d.upper).any(expr)),
        Item::Series(d, _) | Item::State(d, _) => d.iter().any(|d| expr(&d.init)),
        Item::Warmup(e, _) => expr(e),
        Item::Handler(_, body, _) => stmts(body),
        Item::Options(o, _) => o.iter().any(|(_, e)| expr(e)),
        Item::Functions(f, _) => f.iter().any(|f| stmts(&f.body)),
        Item::Indicator(d) => d.parts.iter().any(|part| match part {
            IndPart::Window { size: e, .. } | IndPart::Warmup(e, _) => expr(e),
            IndPart::State(v) | IndPart::Value(v) => expr(&v.init),
            IndPart::Update(b, _) | IndPart::Enter(_, b, _) | IndPart::Leave(_, b, _) => stmts(b),
        }),
        Item::Instruments(..) | Item::Universe(_) | Item::Replay(..) => false,
    })
}

#[derive(Debug, PartialEq)]
enum Outcome {
    /// Both accept, with the same tree.
    Same,
    /// Both refuse.
    Refused,
    /// The parser accepts a diagnostic-only form, which the grammar refuses.
    DiagnosticOnly,
    /// The parser's nesting limit.
    TooDeep,
}

/// Compares the two parsers on one program, or says how they differ.
fn compare(name: &str, source: &str) -> Result<Outcome, String> {
    let hand = atlc::frontend::parse(source);
    let generated = atlc_grammar::parse(source);
    match (hand, generated) {
        (Ok(a), Some(b)) if diagnostic_only(&a) => {
            Err(format!("{name}: the grammar accepts a diagnostic-only form: {b:?}"))
        }
        (Ok(a), Some(b)) if a == b => Ok(Outcome::Same),
        (Ok(a), Some(b)) => Err(format!("{name}: different trees\n  parser:  {a:?}\n  grammar: {b:?}")),
        (Ok(a), None) if diagnostic_only(&a) => Ok(Outcome::DiagnosticOnly),
        (Ok(_), None) => Err(format!("{name}: the parser accepts it, the grammar refuses it")),
        (Err(d), _) if d.iter().any(|d| d.code == Code::NestingTooDeep) => Ok(Outcome::TooDeep),
        (Err(_), None) => Ok(Outcome::Refused),
        (Err(d), Some(_)) => Err(format!(
            "{name}: the grammar accepts it, the parser refuses it: {}",
            d[0].message
        )),
    }
}

/// Runs `f` with room for the deepest expression the parser accepts.
fn deep<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(f: F) -> R {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn the_corpus() {
    let counts = deep(|| {
        let mut counts = [0usize; 4];
        let mut failures = vec![];
        for (name, source) in corpus() {
            match compare(&name, &source) {
                Ok(o) => counts[o as usize] += 1,
                Err(e) => failures.push(e),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        counts
    });
    let [same, refused, diagnostic_only, too_deep] = counts;
    // The prototype's programs, the parse corpus and every UI program refused by the checker parse alike.
    assert!(same >= 140, "{counts:?}");
    assert!(refused >= 20, "{counts:?}");
    assert!(diagnostic_only >= 4, "{counts:?}");
    assert_eq!(too_deep, 1, "{counts:?}");
}

/// Replacement tokens for the mutants: every kind of token the grammar knows, and some it does not.
const REPLACEMENTS: [&str; 24] = [
    "x", "1", "1.5", "\"s\"", "true", "(", ")", "{", "}", "[", "]", ",", ";", "=", "==", "<", ">", "+", "-", "?", ":",
    ".", "target", "window",
];

#[test]
fn mutants() {
    let (count, outcomes) = deep(|| {
        let mut failures = vec![];
        let mut count = 0;
        let mut outcomes = [0usize; 4];
        for (name, source) in corpus() {
            // The large stress file and the nesting limit add nothing but time here.
            if source.len() > 20_000 || name.contains("nesting-too-deep") {
                continue;
            }
            let Ok(tokens) = lex(&source) else { continue };
            let spans: Vec<Span> = tokens.iter().map(|t| t.span).filter(|s| s.end > s.start).collect();
            let text = |s: Span| &source[s.start..s.end];
            let mut mutants = vec![];
            for (i, s) in spans.iter().enumerate() {
                mutants.push(format!("{} {}", &source[..s.start], &source[s.end..]));
                mutants.push(format!("{}{} {}", &source[..s.end], " ", &source[s.start..]));
                if let Some(n) = spans.get(i + 1) {
                    mutants.push(format!(
                        "{}{} {}{}",
                        &source[..s.start],
                        text(*n),
                        text(*s),
                        &source[n.end..]
                    ));
                }
                let r = REPLACEMENTS[(i * 7 + source.len()) % REPLACEMENTS.len()];
                mutants.push(format!("{} {} {}", &source[..s.start], r, &source[s.end..]));
            }
            for (k, m) in mutants.iter().enumerate() {
                count += 1;
                match compare(&format!("{name} (mutant {k})"), m) {
                    Ok(o) => outcomes[o as usize] += 1,
                    Err(e) => failures.push(format!("{e}\n--- mutant ---\n{m}")),
                }
                if failures.len() >= 5 {
                    break;
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
        (count, outcomes)
    });
    assert!(count > 10_000, "only {count} mutants");
    // Both verdicts occur often, so the comparison is not vacuous.
    assert!(
        outcomes[Outcome::Same as usize] > 1_000 && outcomes[Outcome::Refused as usize] > 1_000,
        "{outcomes:?}"
    );
}
