//! The `atlc` command line.

use std::process::ExitCode;

use atlc::frontend::diag::{self, Diag};

const USAGE: &str = "usage: atlc <command> [arguments]

commands:
  check FILE.atls [--json]    parse and check a program
  build FILE.atls -o OUT.hpp  write the C++ strategy (not implemented yet)
  run SPEC.toml [-P k=v]      replay a strategy from a run spec and print the report as JSON (not implemented yet)
  --version                   print the compiler's version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") if args.len() == 1 => {
            println!("atlc {}", atlc::VERSION);
            ExitCode::SUCCESS
        }
        Some("check") => check(&args[1..]),
        Some("build" | "run") => {
            eprintln!("atlc: `{}` is not implemented yet", args[0]);
            ExitCode::from(2)
        }
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

/// `atlc check FILE [--json]`: exit 0 when the program is valid (warnings allowed), 1 when it has errors,
/// 2 on a usage or file error.
fn check(args: &[String]) -> ExitCode {
    let mut json = false;
    let mut files = vec![];
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            "-h" | "--help" => return usage(),
            s if s.starts_with('-') && s.len() > 1 => {
                eprintln!("atlc check: unknown option `{s}`");
                return usage();
            }
            s => files.push(s),
        }
    }
    let [file] = files[..] else {
        eprintln!("atlc check: expected one file, got {}", files.len());
        return usage();
    };
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("atlc check: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let (diags, failed): (Vec<Diag>, bool) = match atlc::check(&source) {
        Ok(ir) => (ir.warnings, false),
        Err(errors) => (errors, true),
    };
    for d in &diags {
        if json {
            println!("{}", diag::to_json(d, file, &source));
        } else {
            eprintln!("{}", diag::render(d, file, &source));
        }
    }
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
