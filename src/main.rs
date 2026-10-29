//! The `atlc` command line.

use std::process::ExitCode;

const USAGE: &str = "usage: atlc <command> [arguments]

commands:
  check FILE.atls             parse and check a program
  build FILE.atls -o OUT.hpp  write the C++ strategy
  run SPEC.toml [-P k=v]      replay a strategy from a run spec and print the report as JSON
  --version                   print the compiler's version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("atlc {}", atlc::VERSION);
            ExitCode::SUCCESS
        }
        Some("check" | "build" | "run") => {
            eprintln!("atlc: `{}` is not implemented yet", args[0]);
            ExitCode::from(2)
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
