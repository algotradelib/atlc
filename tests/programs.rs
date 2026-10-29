//! Valid programs: one of each kind (`tests/programs/`: one instrument, named instruments, a universe with
//! session legs, state with `on_fill`, functions and a user indicator) and the code examples of the language
//! reference and the README must check without a diagnostic.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            out.push(p);
        }
    }
}

fn check_clean(name: &str, source: &str) {
    match atlc::check(source) {
        Ok(ir) => {
            let warnings: Vec<_> = ir.warnings.iter().map(|w| w.message.clone()).collect();
            assert!(warnings.is_empty(), "{name}: {warnings:?}");
        }
        Err(e) => {
            let text: Vec<String> = e
                .iter()
                .map(|d| atlc::frontend::diag::render(d, name, source))
                .collect();
            panic!("{}", text.join("\n"));
        }
    }
}

#[test]
fn valid_programs() {
    let mut files = vec![];
    walk(&root().join("tests/programs"), "atls", &mut files);
    files.sort();
    assert_eq!(files.len(), 5);
    for p in files {
        check_clean(&p.display().to_string(), &std::fs::read_to_string(&p).unwrap());
    }
}

/// The ```atls blocks of a Markdown file, with their line numbers.
fn atls_blocks(text: &str) -> Vec<(usize, String)> {
    let mut out = vec![];
    let mut current: Option<(usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        match (&mut current, line.trim_end()) {
            (None, "```atls") => current = Some((i + 1, String::new())),
            (Some(_), "```") => out.push(current.take().unwrap()),
            (Some((_, body)), _) => {
                body.push_str(line);
                body.push('\n');
            }
            _ => {}
        }
    }
    assert!(current.is_none(), "an unclosed code block");
    out
}

#[test]
fn reference_examples() {
    let mut files = vec![root().join("README.md")];
    walk(&root().join("docs/reference"), "md", &mut files);
    files.sort();
    let mut count = 0;
    for p in files {
        for (line, block) in atls_blocks(&std::fs::read_to_string(&p).unwrap()) {
            let name = format!("{}:{line}", p.display());
            // A block that shows one part of a program (no handler) is checked inside a minimal program.
            let handlers = ["on_bar", "on_open", "on_close"];
            let complete = handlers.iter().any(|h| block.contains(h));
            let source = if complete {
                block
            } else {
                format!("{block}on_bar {{ target(0); }}\n")
            };
            match atlc::check(&source) {
                Ok(ir) if complete => assert!(ir.warnings.is_empty(), "{name}: {:?}", ir.warnings[0].message),
                Ok(_) => {}
                Err(e) => panic!("{name}: {}", atlc::frontend::diag::render(&e[0], &name, &source)),
            }
            count += 1;
        }
    }
    assert!(count >= 4, "found {count} examples");
}
