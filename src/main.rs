//! The `atlc` command line.

use std::process::ExitCode;

use atlc::backend::{self, Indicators};
use atlc::frontend::diag::{self, Diag, json_string};
use atlc::ir::{Ir, Mode};

const USAGE: &str = "usage: atlc <command> [arguments]

commands:
  check FILE.atls [--json | --info]
                              parse and check a program; --info prints its facts as one JSON object
  build FILE.atls [-o OUT.hpp] [--indicators incremental|naive|exact] [--share on|off] [--history drop|keep]
                              write the C++ strategy (to standard output without -o)
  build --emit-runtime DIR    write the runtime headers the strategies include into DIR/atls/
  reference [CHAPTER]         print the language reference's index, or one chapter
  --version                   print the compiler's version";

/// The checking thread's stack. The parser refuses programs nested deeper than 256 levels; checking the
/// deepest program it accepts was measured to need up to 16 MB of stack in a debug build and 2 MB in a release
/// build, more than a main thread has on Windows (1 MB) or in a debug build elsewhere (8 MB).
const STACK_BYTES: usize = 64 << 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") if args.len() == 1 => {
            println!("atlc {}", atlc::VERSION);
            ExitCode::SUCCESS
        }
        Some(command @ ("check" | "build")) => {
            // The parser, the checker and the printer recurse over the syntax tree: give them room for the deepest
            // program the parser accepts, in any build (the stack is reserved, not committed).
            let rest = args[1..].to_vec();
            let run = if command == "check" { check } else { build };
            let worker = std::thread::Builder::new()
                .stack_size(STACK_BYTES)
                .spawn(move || run(&rest));
            match worker.map(|w| w.join()) {
                Ok(Ok(code)) => code,
                _ => {
                    eprintln!("atlc: internal error");
                    ExitCode::from(101)
                }
            }
        }
        Some("reference") => reference(&args[1..]),
        _ => usage(),
    }
}

/// `atlc reference [CHAPTER]`: exit 0 with the index or the chapter on standard output, 1 for a name that is not a
/// chapter (the chapters on standard error), 2 on a usage error.
fn reference(args: &[String]) -> ExitCode {
    match args {
        [] => {
            print!("{}", atlc::reference::INDEX);
            ExitCode::SUCCESS
        }
        [name] if !name.starts_with('-') => match atlc::reference::chapter(name) {
            Some(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            None => {
                let names: Vec<&str> = atlc::reference::CHAPTERS.iter().map(|(n, _)| *n).collect();
                eprintln!(
                    "atlc reference: no chapter `{name}`; the chapters are {}",
                    names.join(", ")
                );
                ExitCode::from(1)
            }
        },
        [_] => {
            eprintln!("atlc reference: unknown option `{}`", args[0]);
            usage()
        }
        _ => {
            eprintln!("atlc reference: expected at most one chapter, got {}", args.len());
            usage()
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

/// `atlc check FILE [--json]`: exit 0 when the program is valid (warnings allowed), 1 when it has errors,
/// 2 on a usage or file error.
fn check(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--info") {
        return info(args);
    }
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

/// `atlc check FILE --info`: exactly one JSON object on standard output, whatever happened, and nothing on standard
/// error. Exit 0 when the program is valid (warnings allowed), 1 when it has errors, 2 on a usage error, a file it
/// cannot read, or a file name no C++ struct can take.
fn info(args: &[String]) -> ExitCode {
    let mut seen = false;
    let mut files = vec![];
    let mut problem: Option<String> = None;
    for a in args {
        let wrong = match a.as_str() {
            "--info" if seen => Some("`--info` is given twice".to_string()),
            "--info" => {
                seen = true;
                None
            }
            "--json" => Some("`--info` and `--json` print different things: give one".to_string()),
            "-h" | "--help" => Some("`--help` and `--info` print different things: give one".to_string()),
            s if s.starts_with('-') && s.len() > 1 => Some(format!("unknown option `{s}`")),
            s => {
                files.push(s);
                None
            }
        };
        if wrong.is_some() {
            problem = wrong;
            break;
        }
    }
    let file = match (problem, &files[..]) {
        (None, [file]) => file,
        (problem, _) => {
            let message = problem.unwrap_or_else(|| format!("expected one file, got {}", files.len()));
            println!("{}", info_json(None, &[info_error("usage", &message, None)]));
            return ExitCode::from(2);
        }
    };
    let path = std::path::absolute(file).unwrap_or_else(|_| std::path::PathBuf::from(file));
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            let message = format!("cannot read {}: {e}", path.display());
            println!(
                "{}",
                info_json(None, &[info_error("unreadable", &message, Some(&path))])
            );
            return ExitCode::from(2);
        }
    };
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match backend::struct_name(&stem) {
        Ok(name) => name,
        Err(m) => {
            println!("{}", info_json(None, &[info_error("program-name", &m, Some(&path))]));
            return ExitCode::from(2);
        }
    };
    let shown = path.display().to_string();
    let flat =
        |diags: &[Diag]| -> Vec<String> { diags.iter().map(|d| diag::to_flat_json(d, &shown, &source)).collect() };
    match atlc::check(&source) {
        Ok(ir) => {
            let facts = program_json(&path, &name, &ir);
            println!("{}", info_json(Some(&facts), &flat(&ir.warnings)));
            ExitCode::SUCCESS
        }
        Err(diags) => {
            println!("{}", info_json(None, &flat(&diags)));
            ExitCode::from(1)
        }
    }
}

/// `atlc check --info`'s one object: the program's facts when it checked (else null), and the diagnostics, each
/// already a JSON object; the status is `error` exactly when one of them is an error.
fn info_json(program: Option<&str>, diagnostics: &[String]) -> String {
    let failed = diagnostics.iter().any(|d| d.starts_with("{\"severity\":\"error\""));
    format!(
        "{{\"atlc_info\":1,\"status\":\"{}\",\"atlc\":{},\"program\":{},\"diagnostics\":[{}]}}",
        if failed { "error" } else { "ok" },
        json_string(atlc::VERSION),
        program.unwrap_or("null"),
        diagnostics.join(",")
    )
}

/// The facts of a checked program at `path`, whose struct is `name`.
fn program_json(path: &std::path::Path, name: &str, ir: &Ir) -> String {
    let (mode, named) = match &ir.mode {
        Mode::Single => ("single", &[][..]),
        Mode::Named(names) => ("named", &names[..]),
        Mode::Universe => ("universe", &[][..]),
    };
    let instruments: Vec<String> = named.iter().map(|n| json_string(n)).collect();
    let params: Vec<String> = ir
        .params
        .iter()
        .map(|p| format!("{{\"name\":{},\"type\":\"{}\"}}", json_string(&p.name), p.ty.name()))
        .collect();
    format!(
        "{{\"path\":{},\"struct\":{},\"mode\":\"{mode}\",\"instruments\":[{}],\"session\":{},\"params\":[{}]}}",
        json_string(&path.display().to_string()),
        json_string(name),
        instruments.join(","),
        ir.session,
        params.join(",")
    )
}

/// An `atlc check --info` error that is not one of the program's own diagnostics: about the program's file at `file`
/// (`unreadable`, `program-name`), or about the command line when there is no file (`usage`).
fn info_error(code: &str, message: &str, file: Option<&std::path::Path>) -> String {
    let (file, origin) = match file {
        Some(p) => (json_string(&p.display().to_string()), "program"),
        None => ("null".to_string(), "command_line"),
    };
    format!(
        "{{\"severity\":\"error\",\"code\":{},\"message\":{},\"file\":{file},\"line\":null,\"origin\":\"{origin}\"}}",
        json_string(code),
        json_string(message)
    )
}

/// `atlc build FILE [-o OUT] [options]` and `atlc build --emit-runtime DIR`: exit 0 when the header (or the
/// runtime) was written, 1 when the program has errors, 2 on a usage or file error.
fn build(args: &[String]) -> ExitCode {
    let mut out: Option<&str> = None;
    let mut runtime: Option<&str> = None;
    let mut indicators = Indicators::Incremental;
    let mut share = true;
    let mut drop_history = true;
    let mut files = vec![];
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let takes_value = matches!(a, "-o" | "--emit-runtime" | "--indicators" | "--share" | "--history");
        let value = if takes_value {
            i += 1;
            match args.get(i) {
                Some(v) => v.as_str(),
                None => {
                    eprintln!("atlc build: `{a}` needs a value");
                    return usage();
                }
            }
        } else {
            ""
        };
        let ok = match a {
            "-o" => out.replace(value).is_none(),
            "--emit-runtime" => runtime.replace(value).is_none(),
            "--indicators" => Indicators::from_name(value).map(|m| indicators = m).is_some(),
            "--share" => on_off(value, "on", "off").map(|v| share = v).is_some(),
            "--history" => on_off(value, "drop", "keep").map(|v| drop_history = v).is_some(),
            "-h" | "--help" => return usage(),
            s if s.starts_with('-') && s.len() > 1 => {
                eprintln!("atlc build: unknown option `{s}`");
                return usage();
            }
            s => {
                files.push(s);
                true
            }
        };
        if !ok {
            eprintln!("atlc build: `{a} {value}` is not a valid setting, or is given twice");
            return usage();
        }
        i += 1;
    }
    if runtime.is_some() && files.is_empty() {
        return emit_runtime(runtime);
    }
    let [file] = files[..] else {
        eprintln!("atlc build: expected one file, got {}", files.len());
        return usage();
    };
    let path = std::path::Path::new(file);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match backend::struct_name(&stem) {
        Ok(name) => name,
        Err(m) => {
            eprintln!("atlc build: {m}");
            return ExitCode::from(2);
        }
    };
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("atlc build: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let options = backend::Options {
        name,
        source_name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        indicators,
        share_rings: share,
        drop_history,
    };
    let (header, diags) = match atlc::build(&source, &options) {
        Ok(built) => built,
        Err(errors) => {
            for d in &errors {
                eprintln!("{}", diag::render(d, file, &source));
            }
            return ExitCode::from(1);
        }
    };
    for d in &diags {
        eprintln!("{}", diag::render(d, file, &source));
    }
    // Only a program that checked writes anything, the runtime included.
    if emit_runtime(runtime) != ExitCode::SUCCESS {
        return ExitCode::from(2);
    }
    match out {
        Some(target) => {
            if let Err(e) = std::fs::write(target, header) {
                eprintln!("atlc build: cannot write {target}: {e}");
                return ExitCode::from(2);
            }
        }
        None => print!("{header}"),
    }
    ExitCode::SUCCESS
}

/// Writes the runtime into `dir`, if one is given.
fn emit_runtime(dir: Option<&str>) -> ExitCode {
    if let Some(dir) = dir
        && let Err(e) = backend::emit_runtime(std::path::Path::new(dir))
    {
        eprintln!("atlc build: cannot write the runtime into {dir}: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// `yes` as true, `no` as false, anything else as none.
fn on_off(value: &str, yes: &str, no: &str) -> Option<bool> {
    match value {
        v if v == yes => Some(true),
        v if v == no => Some(false),
        _ => None,
    }
}
