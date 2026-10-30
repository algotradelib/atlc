//! The `atlc` command line.

use std::process::ExitCode;

use atlc::backend::{self, Indicators};
use atlc::frontend::diag::{self, Diag, Severity, json_string};
use atlc::runner::{Ready, Stop};
use atlc::spec::{self as runspec, Entry};

const USAGE: &str = "usage: atlc <command> [arguments]

commands:
  check FILE.atls [--json]    parse and check a program
  build FILE.atls [-o OUT.hpp] [--indicators incremental|naive|exact] [--share on|off] [--history drop|keep]
                              write the C++ strategy (to standard output without -o)
  build --emit-runtime DIR    write the runtime headers the strategies include into DIR/atls/
  run SPEC.toml [-P name=value]... [--fills FILE]
                              replay a strategy from a run spec and print the report as JSON
  spec SPEC.toml [-P name=value]... [--json]
                              check a run spec as `run` would, build nothing, and print its resolved settings
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
        Some(command @ ("check" | "build" | "run" | "spec")) => {
            // The parser, the checker and the printer recurse over the syntax tree: give them room for the deepest
            // program the parser accepts, in any build (the stack is reserved, not committed).
            let rest = args[1..].to_vec();
            let run = match command {
                "check" => check,
                "build" => build,
                "spec" => spec,
                _ => run,
            };
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

/// `atlc run SPEC [-P name=value]... [--fills FILE]`: exit 0 after printing the report, 1 when the program has
/// errors, 2 on a usage, spec or file error, 3 when the build fails, 4 when the replay fails.
fn run(args: &[String]) -> ExitCode {
    let mut spec: Option<&str> = None;
    let mut fills: Option<&str> = None;
    let mut cli = vec![];
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if matches!(a, "-P" | "--fills") {
            i += 1;
            let Some(value) = args.get(i) else {
                eprintln!("atlc run: `{a}` needs a value");
                return usage();
            };
            if a == "--fills" {
                if fills.replace(value).is_some() {
                    eprintln!("atlc run: `--fills` is given twice");
                    return usage();
                }
            } else if let Err(m) = assignment(value, &mut cli) {
                eprintln!("atlc run: {m}");
                return usage();
            }
        } else if matches!(a, "-h" | "--help") {
            return usage();
        } else if a.starts_with('-') && a.len() > 1 {
            eprintln!("atlc run: unknown option `{a}`");
            return usage();
        } else if spec.replace(a).is_some() {
            eprintln!("atlc run: expected one run spec");
            return usage();
        }
        i += 1;
    }
    let Some(spec) = spec else {
        eprintln!("atlc run: expected a run spec");
        return usage();
    };
    match atlc::runner::run(std::path::Path::new(spec), cli, fills.map(std::path::PathBuf::from)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            match &f {
                atlc::runner::Failure::Spec(messages) => {
                    for m in messages {
                        eprintln!("atlc run: {m}");
                    }
                }
                atlc::runner::Failure::Build(m) => eprintln!("atlc run: {m}"),
                atlc::runner::Failure::Program | atlc::runner::Failure::Replay => {}
            }
            ExitCode::from(f.code())
        }
    }
}

/// Adds `-P value` to `cli`; a malformed or repeated assignment is a usage error.
fn assignment(value: &str, cli: &mut Vec<Entry>) -> Result<(), runspec::Error> {
    let e = runspec::from_command_line(value)?;
    if cli.iter().any(|c| c.key == e.key) {
        return Err(runspec::Error::usage(format!("`{}` is given twice", e.key)));
    }
    cli.push(e);
    Ok(())
}

/// `atlc spec SPEC [-P name=value]... [--json]`: the spec read and checked as `atlc run` prepares it, nothing built.
/// Exit 0 when it is valid (warnings allowed), 1 when the program has errors, 2 on a usage, file or spec error.
/// With `--json`, always exactly one object on standard output.
fn spec(args: &[String]) -> ExitCode {
    let json = spec_wants_json(args);
    let mut spec: Option<&str> = None;
    let mut cli = vec![];
    let mut problem: Option<runspec::Error> = None;
    let mut i = 0;
    while i < args.len() && problem.is_none() {
        let a = args[i].as_str();
        match a {
            "--json" => {}
            "-P" => {
                i += 1;
                problem = match args.get(i) {
                    Some(value) => assignment(value, &mut cli).err(),
                    None => Some(runspec::Error::usage("`-P` needs a value")),
                };
            }
            "-h" | "--help" => problem = Some(runspec::Error::usage(USAGE)),
            s if s.starts_with('-') && s.len() > 1 => {
                problem = Some(runspec::Error::usage(format!("unknown option `{s}`")));
            }
            s if spec.replace(s).is_some() => problem = Some(runspec::Error::usage("expected one run spec")),
            _ => {}
        }
        i += 1;
    }
    if problem.is_none() && spec.is_none() {
        problem = Some(runspec::Error::usage("expected a run spec"));
    }
    if let Some(e) = problem {
        if json {
            println!("{}", spec_json(&e));
        } else if e.message == USAGE {
            eprintln!("{USAGE}");
        } else {
            eprintln!("atlc spec: {e}");
            eprintln!("{USAGE}");
        }
        return ExitCode::from(2);
    }
    let p = atlc::runner::prepare(std::path::Path::new(spec.unwrap_or_default()), cli);
    let (file, source) = match &p.program {
        Some((path, source)) => (path.display().to_string(), source.as_str()),
        None => (String::new(), ""),
    };
    let mut warnings: Vec<&Diag> = p.warnings.iter().collect();
    let mut program_errors: Vec<&Diag> = vec![];
    let mut spec_errors: &[runspec::Error] = &[];
    let code = match &p.outcome {
        Ok(_) => 0,
        Err(Stop::Program(diags)) => {
            for d in diags {
                match d.severity {
                    Severity::Error => program_errors.push(d),
                    Severity::Warning => warnings.push(d),
                }
            }
            1
        }
        Err(Stop::Spec(errors)) => {
            spec_errors = errors;
            2
        }
    };
    if json {
        let mut errors: Vec<String> = program_errors
            .iter()
            .map(|d| diag::to_flat_json(d, &file, source))
            .collect();
        errors.extend(spec_errors.iter().map(runspec::Error::to_json));
        let warnings: Vec<String> = warnings.iter().map(|d| diag::to_flat_json(d, &file, source)).collect();
        let ready = p
            .outcome
            .as_ref()
            .ok()
            .map(|r| (p.program.as_ref().map(|(path, _)| path.as_path()), r));
        println!("{}", spec_json_lines(ready, &warnings, &errors));
    } else {
        for d in warnings.iter().chain(&program_errors) {
            eprintln!("{}", diag::render(d, &file, source));
        }
        for e in spec_errors {
            eprintln!("atlc spec: {e}");
        }
        if let (Ok(ready), Some((path, _))) = (&p.outcome, &p.program) {
            print!("{}", spec_text(path, ready));
        }
    }
    ExitCode::from(code)
}

/// Whether `atlc spec`'s arguments ask for JSON: `--json` anywhere but as the value of `-P`.
fn spec_wants_json(args: &[String]) -> bool {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => return true,
            "-P" => i += 2,
            _ => i += 1,
        }
    }
    false
}

/// `atlc spec --json`'s object for a usage error, before any spec is read.
fn spec_json(error: &runspec::Error) -> String {
    spec_json_lines(None, &[], &[error.to_json()])
}

/// `atlc spec --json`'s one object: the resolved settings when `ready` holds them (else null), the warnings and the
/// errors, each already a JSON object.
fn spec_json_lines(
    ready: Option<(Option<&std::path::Path>, &Ready)>,
    warnings: &[String],
    errors: &[String],
) -> String {
    let null = || "null".to_string();
    let path = |p: &Option<std::path::PathBuf>| p.as_ref().map_or_else(null, |p| json_string(&p.display().to_string()));
    let (program, build, store, orders, args) = match ready {
        Some((file, r)) => {
            let file = file.map_or_else(null, |f| json_string(&f.display().to_string()));
            let args: Vec<String> = r
                .run
                .args
                .iter()
                .map(|(k, v)| format!("[{},{}]", json_string(k), json_string(v)))
                .collect();
            (
                format!("{{\"path\":{file},\"struct\":{}}}", json_string(&r.name)),
                format!(
                    "{{\"algotradelib\":{},\"duckdb\":{},\"indicators\":{}}}",
                    path(&r.run.algotradelib),
                    path(&r.run.duckdb),
                    json_string(r.run.indicators.name())
                ),
                r.run.store.to_string(),
                format!(
                    "{{\"type\":{},\"offset\":{}}}",
                    json_string(&r.run.orders.kind),
                    json_string(&r.run.orders.offset)
                ),
                format!("[{}]", args.join(",")),
            )
        }
        None => (null(), null(), null(), null(), null()),
    };
    format!(
        "{{\"atlc_spec\":1,\"status\":\"{}\",\"atlc\":{},\"program\":{program},\"build\":{build},\"store\":{store},\"orders\":{orders},\"args\":{args},\"warnings\":[{}],\"errors\":[{}]}}",
        if errors.is_empty() { "ok" } else { "error" },
        json_string(atlc::VERSION),
        warnings.join(","),
        errors.join(",")
    )
}

/// `atlc spec`'s text: the same facts as its JSON, for a person.
fn spec_text(program: &std::path::Path, r: &Ready) -> String {
    let path = |p: &Option<std::path::PathBuf>, otherwise: &str| {
        p.as_ref()
            .map_or_else(|| otherwise.to_string(), |p| p.display().to_string())
    };
    let mut out = format!(
        "program: {} (atls::{})
",
        program.display(),
        r.name
    );
    out += &format!(
        "algotradelib: {}
",
        path(
            &r.run.algotradelib,
            "CMAKE_PREFIX_PATH or algotradelib_DIR, then CMake's search"
        )
    );
    out += &format!(
        "duckdb: {}
",
        if r.run.store {
            path(&r.run.duckdb, "DUCKDB_ROOT, then the system's paths")
        } else {
            "not needed (no store)".to_string()
        }
    );
    out += &format!(
        "indicators: {}
",
        r.run.indicators.name()
    );
    out += &match r.run.orders.kind.as_str() {
        "limit" => format!(
            "orders: limit, offset {}
",
            r.run.orders.offset
        ),
        kind => format!(
            "orders: {kind}
"
        ),
    };
    out += "arguments:\n";
    for (k, v) in &r.run.args {
        out += &format!("  {k}={v}\n");
    }
    out
}
