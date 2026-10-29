//! Tokens. Comments run from `#` or `//` to the end of the line.

use crate::frontend::ast::Span;
use crate::frontend::diag::{Code, Diag};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// A number and whether it was written as an integer.
    Num(f64, bool),
    Ident(String),
    /// A string, in `replay {}` only.
    Str(String),
    Punct(&'static str),
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

impl Token {
    pub fn describe(&self) -> String {
        match &self.tok {
            Tok::Num(_, _) => "a number".into(),
            Tok::Ident(s) => format!("`{}`", s),
            Tok::Str(_) => "a string".into(),
            Tok::Punct(p) => format!("`{}`", p),
            Tok::Eof => "the end of the file".into(),
        }
    }
}

const PUNCT: [&str; 25] = [
    "<=", ">=", "==", "!=", "&&", "||", "{", "}", "(", ")", "[", "]", "<", ">", "=", "+", "-", "*", "/", "?", ":", ";",
    ",", ".", "!",
];

pub fn lex(source: &str) -> Result<Vec<Token>, Diag> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'#' || (c == b'/' && bytes.get(i + 1) == Some(&b'/')) {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit())) {
            let start = i;
            let mut int = true;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                int = false;
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let save = i;
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                if i < bytes.len() && bytes[i].is_ascii_digit() {
                    int = false;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            let text = &source[start..i];
            let value: f64 = text.parse().map_err(|_| {
                Diag::new(
                    Code::InvalidNumber,
                    format!("`{}` is not a number", text),
                    Span::new(start, i),
                    "here",
                )
            })?;
            out.push(Token {
                tok: Tok::Num(value, int),
                span: Span::new(start, i),
            });
            continue;
        }
        if c == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'"' {
                return Err(Diag::new(
                    Code::UnclosedString,
                    "this string is never closed",
                    Span::new(start, i),
                    "add `\"`",
                ));
            }
            i += 1;
            out.push(Token {
                tok: Tok::Str(source[start + 1..i - 1].to_string()),
                span: Span::new(start, i),
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(source[start..i].to_string()),
                span: Span::new(start, i),
            });
            continue;
        }
        let mut matched = false;
        for p in PUNCT {
            if source[i..].starts_with(p) {
                out.push(Token {
                    tok: Tok::Punct(p),
                    span: Span::new(i, i + p.len()),
                });
                i += p.len();
                matched = true;
                break;
            }
        }
        if !matched {
            let ch = source[i..].chars().next().unwrap();
            return Err(Diag::new(
                Code::UnexpectedCharacter,
                format!("unexpected character `{}`", ch),
                Span::new(i, i + ch.len_utf8()),
                "not part of the language",
            ));
        }
    }
    out.push(Token {
        tok: Tok::Eof,
        span: Span::new(source.len(), source.len()),
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::diag::Code;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(
            toks("20 0.05 .5 1e-4 2E+2 1. 1e"),
            [
                Tok::Num(20.0, true),
                Tok::Num(0.05, false),
                Tok::Num(0.5, false),
                Tok::Num(1e-4, false),
                Tok::Num(200.0, false),
                Tok::Num(1.0, false),
                // `1e` with no digits after the `e` is the number 1 and the name `e`.
                Tok::Num(1.0, true),
                Tok::Ident("e".into()),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn comments_strings_and_punctuation() {
        assert_eq!(
            toks("a # x\n// y\n<= != && \"s\""),
            [
                Tok::Ident("a".into()),
                Tok::Punct("<="),
                Tok::Punct("!="),
                Tok::Punct("&&"),
                Tok::Str("s".into()),
                Tok::Eof,
            ]
        );
        assert_eq!(lex("\"open").unwrap_err().code, Code::UnclosedString);
        assert_eq!(lex("\"a\nb\"").unwrap_err().code, Code::UnclosedString);
        let e = lex("x = é;").unwrap_err();
        assert_eq!((e.code, e.span), (Code::UnexpectedCharacter, Span::new(4, 6)));
        assert_eq!(lex("").unwrap()[0].span, Span::new(0, 0));
    }
}
