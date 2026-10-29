//! The parser's corpus: programs it must accept (`tests/parse-ok/`, the prototype's corpus, whatever the
//! checker then says), and the stage at which each UI program is refused.

use std::path::{Path, PathBuf};

fn files(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "atls"))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no program in {}", dir.display());
    out
}

fn parses(path: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(path).unwrap();
    let name = path.display().to_string();
    // The deepest expression the parser accepts needs more than a test thread's stack in a debug build.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            atlc::frontend::parse(&source)
                .map(|_| ())
                .map_err(|e| format!("{name}: {:?}", e[0].message))
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn accepted() {
    for p in files("tests/parse-ok") {
        parses(&p).unwrap();
    }
}

#[test]
fn refused_by_the_parser() {
    for p in files("tests/ui/syntax") {
        assert!(parses(&p).is_err(), "{} parses", p.display());
    }
}

#[test]
fn refused_by_the_checker_only() {
    for p in files("tests/ui/check") {
        parses(&p).unwrap();
    }
}
