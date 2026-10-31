//! `atlc run`: a run spec checked against its program, the strategy generated and built once per program shape
//! into a cached CMake project linked to an installed algotradelib, and the replay run with the settings and
//! parameters as arguments, so that changing them never rebuilds.

pub mod cache;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::backend;
use crate::frontend::diag::{self, Diag};
use crate::ir::Ir;
use crate::spec::{self, At, Entry, Program};

/// Why a run stopped, with the exit code each gives.
#[derive(Debug)]
pub enum Failure {
    /// A usage, spec or file error (2): the errors.
    Spec(Vec<spec::Error>),
    /// The program has errors (1); they were printed.
    Program,
    /// The build failed (3): the message.
    Build(String),
    /// The replay failed (4); its error was printed.
    Replay,
}

impl Failure {
    /// The exit code.
    pub fn code(&self) -> u8 {
        match self {
            Failure::Program => 1,
            Failure::Spec(_) => 2,
            Failure::Build(_) => 3,
            Failure::Replay => 4,
        }
    }
}

/// A spec prepared for a run: its program checked, its layers merged and resolved.
pub struct Ready {
    /// The generated struct's name.
    pub name: String,
    /// The checked program.
    pub ir: Ir,
    /// The build settings and the replay's arguments.
    pub run: spec::Run,
}

/// Why preparing a spec stopped.
pub enum Stop {
    /// A usage, spec or file error.
    Spec(Vec<spec::Error>),
    /// The program's errors.
    Program(Vec<Diag>),
}

/// What preparing a spec found, for `atlc run` and `atlc spec` alike.
pub struct Preparation {
    /// The spec, made absolute.
    pub spec: PathBuf,
    /// The program's path and text, once read, to render its diagnostics.
    pub program: Option<(PathBuf, String)>,
    /// The program's warnings, once it checked.
    pub warnings: Vec<Diag>,
    /// The prepared run, or why there is none.
    pub outcome: Result<Ready, Stop>,
}

/// Prepares a spec as `atlc run` uses it, building nothing: the spec read (its path made absolute), the program read,
/// its struct name validated and the program checked, its `replay {}` merged under the spec and `cli` (`-P`), then
/// resolved, with the build paths CMake will be given checked.
pub fn prepare(spec_path: &Path, cli: Vec<Entry>) -> Preparation {
    let spec = std::path::absolute(spec_path).unwrap_or_else(|_| spec_path.to_path_buf());
    let mut p = Preparation {
        spec,
        program: None,
        warnings: vec![],
        outcome: Err(Stop::Spec(vec![])),
    };
    p.outcome = prepared(&mut p, cli);
    p
}

/// The steps of `prepare`, filling in what they found.
fn prepared(p: &mut Preparation, cli: Vec<Entry>) -> Result<Ready, Stop> {
    let (strategy, entries) = spec::read(&p.spec).map_err(Stop::Spec)?;
    let source = std::fs::read_to_string(&strategy).map_err(|e| {
        Stop::Spec(vec![spec::Error::unreadable(
            At::File(p.spec.clone()),
            format!("cannot read {}: {e}", strategy.display()),
        )])
    })?;
    let stem = strategy
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name =
        backend::struct_name(&stem).map_err(|m| Stop::Spec(vec![spec::Error::nowhere(m).with_code("program-name")]))?;
    p.program = Some((strategy.clone(), source.clone()));
    let ir = crate::check(&source).map_err(Stop::Program)?;
    p.warnings = ir.warnings.clone();
    let replay = spec::from_replay(&ir.replay, &strategy, &source).map_err(Stop::Spec)?;
    let run = spec::resolve(&[replay, entries, cli], &Program::of(&ir)).map_err(Stop::Spec)?;
    Ok(Ready { name, ir, run })
}

/// Runs a spec: `cli` holds `-P` settings, `fills` the file for every fill.
pub fn run(spec_path: &Path, cli: Vec<Entry>, fills: Option<PathBuf>) -> Result<(), Failure> {
    let p = prepare(spec_path, cli);
    let render = |d: &Diag| match &p.program {
        Some((path, source)) => diag::render(d, &path.display().to_string(), source),
        None => d.message.clone(),
    };
    if let Err(Stop::Program(errors)) = &p.outcome {
        for d in errors {
            eprintln!("{}", render(d));
        }
        return Err(Failure::Program);
    }
    for d in &p.warnings {
        eprintln!("{}", render(d));
    }
    let Ready {
        name,
        ir,
        run: resolved,
    } = match p.outcome {
        Ok(ready) => ready,
        Err(Stop::Spec(errors)) => return Err(Failure::Spec(errors)),
        Err(Stop::Program(_)) => unreachable!("returned above"),
    };
    let strategy = p.program.as_ref().map(|(path, _)| path.clone()).unwrap_or_default();
    let options = backend::Options {
        name: name.clone(),
        source_name: strategy
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        indicators: resolved.indicators,
        share_rings: true,
        drop_history: true,
    };
    let header = backend::print(&ir, &options);
    let project = Project::new(&options, header, &resolved)
        .map_err(|m| Failure::Spec(m.into_iter().map(spec::Error::nowhere).collect()))?;
    let exe = project.build().map_err(Failure::Build)?;
    let mut args: Vec<String> = resolved.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
    if let Some(f) = fills {
        let f = if f.is_absolute() {
            f
        } else {
            std::env::current_dir().unwrap_or_default().join(f)
        };
        args.push(format!("output.fills={}", f.display()));
    }
    let status = Command::new(&exe)
        .args(&args)
        .status()
        .map_err(|e| Failure::Build(format!("cannot run {}: {e}", exe.display())))?;
    if status.success() { Ok(()) } else { Err(Failure::Replay) }
}

/// The environment variables that change what CMake finds or how it compiles, in the key unless a spec setting
/// replaces them.
const LIBRARY_VARS: [&str; 4] = [
    "CMAKE_PREFIX_PATH",
    "algotradelib_DIR",
    "algotradelib_ROOT",
    "ALGOTRADELIB_ROOT",
];
const BUILD_VARS: [&str; 9] = [
    "CXXFLAGS",
    "CPPFLAGS",
    "LDFLAGS",
    "CMAKE_GENERATOR",
    "CMAKE_TOOLCHAIN_FILE",
    "OPENSSL_ROOT_DIR",
    "BOOST_ROOT",
    "Boost_ROOT",
    "Eigen3_ROOT",
];

/// The cache key of a project: atlc's version, every file written (the generated header, `main.cpp`,
/// `CMakeLists.txt` with the build settings, every runtime header), the compiler's identity and the environment
/// variables that change what CMake finds. Parameters and replay settings are arguments, so not in it.
fn key_of(files: &[(PathBuf, String)], compiler: &str, env: &[(String, String)]) -> u128 {
    let mut parts: Vec<Vec<u8>> = vec![format!("atlc {}", crate::VERSION).into_bytes()];
    for (path, text) in files {
        parts.push(path.display().to_string().into_bytes());
        parts.push(text.clone().into_bytes());
    }
    parts.push(compiler.as_bytes().to_vec());
    for (name, value) in env {
        parts.push(format!("{name}={value}").into_bytes());
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    cache::key(&refs)
}

/// One program shape's CMake project in the cache.
struct Project {
    dir: PathBuf,
    files: Vec<(PathBuf, String)>,
    compiler: Compiler,
    store: bool,
}

impl Project {
    /// The project of a generated header under the settings of `run`, in its cache entry.
    fn new(options: &backend::Options, header: String, run: &spec::Run) -> Result<Project, Vec<String>> {
        let main = format!(
            "// Generated by atlc {} from {} for `atlc run`. Do not edit.\n#include <atls/run.hpp>\n\n#include \"strategy.hpp\"\n\n\
             int main(int argc, char** argv) {{\n  return atls::run::main<atls::{}>(argc, argv);\n}}\n",
            crate::VERSION,
            options.source_name,
            options.name
        );
        let cmake = cmake_lists(run).map_err(|m| vec![m])?;
        let compiler = Compiler::probe().map_err(|m| vec![m])?;
        let mut files = vec![
            (PathBuf::from("CMakeLists.txt"), cmake),
            (PathBuf::from("main.cpp"), main),
            (PathBuf::from("include/strategy.hpp"), header),
        ];
        for (path, text) in backend::RUNTIME {
            files.push((Path::new("include").join(path), text.to_string()));
        }
        let mut vars: Vec<&str> = BUILD_VARS.to_vec();
        if run.algotradelib.is_none() {
            vars.extend(LIBRARY_VARS);
        }
        if run.store && run.duckdb.is_none() {
            vars.push("DUCKDB_ROOT");
        }
        let env: Vec<(String, String)> = vars
            .iter()
            .map(|v| {
                (
                    v.to_string(),
                    std::env::var_os(v).unwrap_or_default().to_string_lossy().into_owned(),
                )
            })
            .collect();
        let key = key_of(&files, &compiler.identity, &env);
        let root = cache::root().map_err(|m| vec![m])?;
        Ok(Project {
            dir: root.join(format!("{}-{key:032x}", options.name.to_ascii_lowercase())),
            files,
            compiler,
            store: run.store,
        })
    }

    /// The built executable: the cached one while what it was built from is unchanged, else built again.
    fn build(&self) -> Result<PathBuf, String> {
        let exe = self.dir.join("bin").join(if cfg!(windows) { "run.exe" } else { "run" });
        let stamp = self.dir.join("stamp");
        if let Some(cached) = std::fs::create_dir_all(self.dir.parent().unwrap_or(&self.dir))
            .ok()
            .and_then(|_| self.reusable(&stamp, &exe))
        {
            return Ok(cached);
        }
        let _lock = cache::Lock::take(&self.dir)?;
        // Another run may have built it while this one waited.
        if let Some(cached) = self.reusable(&stamp, &exe) {
            return Ok(cached);
        }
        let build = self.dir.join("build");
        let _ = std::fs::remove_file(&stamp);
        let _ = std::fs::remove_dir_all(&build);
        let _ = std::fs::remove_dir_all(self.dir.join("bin"));
        for (path, text) in &self.files {
            let target = self.dir.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
            std::fs::write(&target, text).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
        }
        eprintln!("atlc: building the strategy in {}", self.dir.display());
        let started = Instant::now();
        let log = self.dir.join("build.log");
        // CMake's file API lists every CMake file the configure reads (a toolchain file and what it includes, the
        // package files of algotradelib and its dependencies, CMake's own modules), for the fingerprint.
        let query = build.join(".cmake/api/v1/query");
        std::fs::create_dir_all(&query).map_err(|e| format!("cannot create {}: {e}", query.display()))?;
        std::fs::write(query.join("cmakeFiles-v1"), "")
            .map_err(|e| format!("cannot write {}: {e}", query.display()))?;
        let mut configure = Command::new("cmake");
        configure
            .arg("-S")
            .arg(&self.dir)
            .arg("-B")
            .arg(&build)
            .arg("-DCMAKE_BUILD_TYPE=Release");
        if let Some(path) = &self.compiler.path {
            configure.arg(format!("-DCMAKE_CXX_COMPILER={}", cmake_path(path)));
        }
        let mut text = String::new();
        let failed = run_logged(&mut configure, &mut text).or_else(|| {
            run_logged(
                Command::new("cmake")
                    .arg("--build")
                    .arg(&build)
                    .args(["--config", "Release"]),
                &mut text,
            )
        });
        let _ = std::fs::write(&log, &text);
        if let Some(what) = failed {
            return Err(build_error(&what, &text, &log, self.store));
        }
        if !exe.is_file() {
            return Err(format!(
                "the build made no {}; its log is {}",
                exe.display(),
                log.display()
            ));
        }
        let (roots, mut files) = found(&build.join("atlc-found.txt"));
        let inputs =
            cmake_inputs(&build.join(".cmake/api/v1/reply")).map_err(|m| format!("{m}, in {}", build.display()))?;
        files.extend(inputs);
        let join = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let stamp_text = format!(
            "{}\n--\n{}\n--\n{}",
            join(&roots),
            join(&files),
            cache::fingerprint(&roots, &files)
        );
        std::fs::write(&stamp, stamp_text).map_err(|e| format!("cannot write {}: {e}", stamp.display()))?;
        eprintln!("atlc: built in {:.1} s", started.elapsed().as_secs_f64());
        Ok(exe)
    }

    /// The executable, when the entry was built and what it used (the library package, DuckDB, the compiler) is
    /// unchanged since.
    fn reusable(&self, stamp: &Path, exe: &Path) -> Option<PathBuf> {
        let text = std::fs::read_to_string(stamp).ok()?;
        if !exe.is_file() {
            return None;
        }
        let mut parts = text.splitn(3, "\n--\n");
        let paths = |part: Option<&str>| -> Vec<PathBuf> {
            part.unwrap_or_default()
                .lines()
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect()
        };
        let roots = paths(parts.next());
        let files = paths(parts.next());
        let fingerprint = parts.next()?;
        if cache::fingerprint(&roots, &files) == fingerprint {
            Some(exe.to_path_buf())
        } else {
            eprintln!("atlc: what the cached build used has changed; building again");
            None
        }
    }
}

/// Runs a command, appending its output to `log`; what failed, if anything.
fn run_logged(command: &mut Command, log: &mut String) -> Option<String> {
    log.push_str(&format!("$ {command:?}\n"));
    match command.output() {
        Ok(out) => {
            log.push_str(&String::from_utf8_lossy(&out.stdout));
            log.push_str(&String::from_utf8_lossy(&out.stderr));
            (!out.status.success()).then(|| format!("{command:?} failed ({})", out.status))
        }
        Err(e) => Some(format!(
            "cannot run `{}`: {e} (atlc run needs CMake 3.20 or later on PATH)",
            command.get_program().to_string_lossy()
        )),
    }
}

/// The message of a failed build: the package or library not found, said plainly, or the log's end.
fn build_error(what: &str, log: &str, path: &Path, store: bool) -> String {
    // CMake wraps a message over indented lines, ending it with a blank one.
    let mut lines = log.lines().skip_while(|l| !l.contains("ATLC: "));
    if let Some(first) = lines.next() {
        let mut message = first[first.find("ATLC: ").unwrap_or(0) + 6..].trim().to_string();
        for l in lines.take_while(|l| !l.trim().is_empty()) {
            message.push(' ');
            message.push_str(l.trim());
        }
        return format!("{message}\n(the build's log is {})", path.display());
    }
    let lines: Vec<&str> = log.lines().collect();
    let tail = lines[lines.len().saturating_sub(40)..].join("\n");
    let store_note = if store { " or DuckDB" } else { "" };
    format!(
        "{what}\n{tail}\n(the C++ build of the strategy failed: check the compiler, algotradelib{store_note}; the whole log is {})",
        path.display()
    )
}

/// What CMake found, from the file the project writes: the directories (the package, the library's headers) and the
/// files (DuckDB, the compiler) whose change makes the build stale.
fn found(path: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut roots = vec![];
    let mut files = vec![];
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if v.is_empty() || v.contains("NOTFOUND") {
            continue;
        }
        match k {
            "package" => roots.push(PathBuf::from(v)),
            "include" => {
                for dir in v.split(';').filter(|d| !d.is_empty() && !d.contains("$<")) {
                    let headers = Path::new(dir).join("algotradelib");
                    if headers.is_dir() {
                        roots.push(headers);
                    }
                }
            }
            "file" => files.push(PathBuf::from(v)),
            _ => {}
        }
    }
    (roots, files)
}

/// The CMake files a configure read, from the file API's `cmakeFiles-v1` reply in `reply`: each input's absolute
/// path, without those CMake generated in the build tree (relative paths are the project's own, already in the key).
/// A reply that is missing or that cannot be read whole is an error, never a shorter list.
fn cmake_inputs(reply: &Path) -> Result<Vec<PathBuf>, String> {
    let text = std::fs::read_dir(reply)
        .ok()
        .and_then(|dir| {
            dir.flatten()
                .find(|e| e.file_name().to_string_lossy().starts_with("cmakeFiles-v1-"))
                .and_then(|e| std::fs::read_to_string(e.path()).ok())
        })
        .ok_or("CMake wrote no list of the files it read (its file API)")?;
    let unreadable = |what: &str| format!("cannot read CMake's list of the files it read: {what}");
    let start = text.find("\"inputs\"").ok_or_else(|| unreadable("no inputs"))?;
    let mut rest = &text[start..];
    let mut out = vec![];
    let mut seen = 0;
    // Each input is a flat object: `{ "path" : "...", "isGenerated" : true, ... }`.
    loop {
        let open = rest
            .find(['{', ']'])
            .ok_or_else(|| unreadable("the inputs never end"))?;
        if rest[open..].starts_with(']') {
            break;
        }
        let len = json_object_len(&rest[open..]).ok_or_else(|| unreadable("an input never ends"))?;
        let object = &rest[open..open + len];
        let generated = object
            .split_whitespace()
            .collect::<String>()
            .contains("\"isGenerated\":true");
        let path = json_string_after(object, "\"path\"").ok_or_else(|| unreadable(object))?;
        let path = PathBuf::from(path);
        if path.is_absolute() && !generated {
            out.push(path);
        }
        seen += 1;
        rest = &rest[open + len..];
    }
    if seen == 0 {
        return Err(unreadable("no inputs"));
    }
    Ok(out)
}

/// The length of the JSON object at the start of `text`, through its closing brace (braces in strings skipped).
fn json_object_len(text: &str) -> Option<usize> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (i, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The JSON string value after the key `key` in `object`, unescaped.
fn json_string_after(object: &str, key: &str) -> Option<String> {
    let after = &object[object.find(key)? + key.len()..];
    let after = after.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = after.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let high = hex4(&mut chars)?;
                    // A character past U+FFFF is two escapes, a UTF-16 surrogate pair.
                    let code = if (0xD800..0xDC00).contains(&high) {
                        if chars.next()? != '\\' || chars.next()? != 'u' {
                            return None;
                        }
                        let low = hex4(&mut chars)?;
                        if !(0xDC00..0xE000).contains(&low) {
                            return None;
                        }
                        0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                    } else {
                        high
                    };
                    out.push(char::from_u32(code)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// The four hex digits of a JSON `\u` escape, as a number.
fn hex4(chars: &mut std::str::Chars) -> Option<u32> {
    let hex: String = chars.by_ref().take(4).collect();
    if hex.len() != 4 {
        return None;
    }
    u32::from_str_radix(&hex, 16).ok()
}

/// A path as CMake reads it: forward slashes.
fn cmake_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A path for a quoted CMake argument, or why it cannot be one (which `spec::resolve` has already refused).
fn quoted_path(path: &Path, setting: &str) -> Result<String, String> {
    match spec::cmake_refuses(path, setting) {
        Some(m) => Err(m),
        None => Ok(cmake_path(path)),
    }
}

/// The project's `CMakeLists.txt`.
fn cmake_lists(run: &spec::Run) -> Result<String, String> {
    let mut s = format!(
        "# Generated by atlc {} for `atlc run`. Do not edit: atlc writes it again.\n\
         cmake_minimum_required(VERSION 3.20)\n\
         project(atlc_run LANGUAGES CXX)\n\n\
         set(CMAKE_CXX_STANDARD 17)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\nset(CMAKE_CXX_EXTENSIONS OFF)\n\n",
        crate::VERSION
    );
    match &run.algotradelib {
        Some(prefix) => {
            let p = quoted_path(prefix, "build.algotradelib")?;
            s.push_str(&format!(
                "# [build] algotradelib: only this prefix, which also comes first for the package's dependencies.\n\
                 list(PREPEND CMAKE_PREFIX_PATH \"{p}\")\n\
                 find_package(algotradelib CONFIG PATHS \"{p}\" NO_DEFAULT_PATH)\n\
                 if(NOT algotradelib_FOUND)\n  message(FATAL_ERROR \"ATLC: no algotradelib package in {p}, the spec's [build] algotradelib: \
                 name the prefix it was installed into\")\nendif()\n"
            ));
        }
        None => s.push_str(
            "find_package(algotradelib CONFIG)\n\
             if(NOT algotradelib_FOUND)\n  message(FATAL_ERROR \"ATLC: no algotradelib package found: set [build] algotradelib in the spec, \
             or CMAKE_PREFIX_PATH or algotradelib_DIR, to the prefix it was installed into\")\nendif()\n",
        ),
    }
    s.push_str(
        "\nadd_executable(run main.cpp)\n\
         target_include_directories(run PRIVATE include)\n\
         target_link_libraries(run PRIVATE algotradelib::algotradelib)\n\
         # Floating-point contraction off: Clang by the runtime's pragmas, GCC and MSVC by flags.\n\
         target_compile_options(run PRIVATE $<$<CXX_COMPILER_ID:GNU>:-ffp-contract=off> $<$<CXX_COMPILER_ID:MSVC>:/fp:precise /bigobj>)\n\
         set_target_properties(run PROPERTIES RUNTIME_OUTPUT_DIRECTORY \"${CMAKE_CURRENT_SOURCE_DIR}/bin\" \
         RUNTIME_OUTPUT_DIRECTORY_RELEASE \"${CMAKE_CURRENT_SOURCE_DIR}/bin\")\n",
    );
    s.push_str(
        "get_target_property(atlc_include algotradelib::algotradelib INTERFACE_INCLUDE_DIRECTORIES)\n\
         set(atlc_found \"package=${algotradelib_DIR}\\ninclude=${atlc_include}\\nfile=${CMAKE_CXX_COMPILER}\\n\")\n",
    );
    if run.store {
        let (hints, note) = match &run.duckdb {
            Some(dir) => {
                let d = quoted_path(dir, "build.duckdb")?;
                (
                    format!("PATHS \"{d}\" PATH_SUFFIXES include lib NO_DEFAULT_PATH"),
                    format!("in {d}, the spec's [build] duckdb"),
                )
            }
            None => (
                "HINTS ENV DUCKDB_ROOT PATH_SUFFIXES include lib".to_string(),
                "in DUCKDB_ROOT or the system's paths: set [build] duckdb in the spec, or DUCKDB_ROOT, to the directory of \
                 DuckDB's C library"
                    .to_string(),
            ),
        };
        s.push_str(&format!(
            "\n# A store run: DuckDB's prebuilt C library.\n\
             find_path(ATLC_DUCKDB_INCLUDE duckdb.h {hints})\n\
             find_library(ATLC_DUCKDB_LIBRARY duckdb {hints})\n\
             if(NOT ATLC_DUCKDB_INCLUDE OR NOT ATLC_DUCKDB_LIBRARY)\n  message(FATAL_ERROR \"ATLC: no DuckDB (duckdb.h and its library) {note}\")\nendif()\n\
             add_library(atlc_duckdb SHARED IMPORTED)\n\
             set_target_properties(atlc_duckdb PROPERTIES INTERFACE_INCLUDE_DIRECTORIES \"${{ATLC_DUCKDB_INCLUDE}}\")\n\
             if(WIN32)\n\
             \x20 get_filename_component(atlc_duckdb_dir \"${{ATLC_DUCKDB_LIBRARY}}\" DIRECTORY)\n\
             \x20 find_file(ATLC_DUCKDB_DLL duckdb.dll PATHS \"${{atlc_duckdb_dir}}\" \"${{atlc_duckdb_dir}}/../bin\" NO_DEFAULT_PATH)\n\
             \x20 if(NOT ATLC_DUCKDB_DLL)\n    message(FATAL_ERROR \"ATLC: no duckdb.dll beside ${{ATLC_DUCKDB_LIBRARY}}\")\n  endif()\n\
             \x20 set_target_properties(atlc_duckdb PROPERTIES IMPORTED_IMPLIB \"${{ATLC_DUCKDB_LIBRARY}}\" IMPORTED_LOCATION \"${{ATLC_DUCKDB_DLL}}\")\n\
             \x20 # The executable finds the DLL beside it.\n\
             \x20 add_custom_command(TARGET run POST_BUILD COMMAND \"${{CMAKE_COMMAND}}\" -E copy_if_different \"${{ATLC_DUCKDB_DLL}}\" \"$<TARGET_FILE_DIR:run>\")\n\
             \x20 string(APPEND atlc_found \"file=${{ATLC_DUCKDB_DLL}}\\n\")\n\
             else()\n\
             \x20 set_target_properties(atlc_duckdb PROPERTIES IMPORTED_LOCATION \"${{ATLC_DUCKDB_LIBRARY}}\")\n\
             \x20 # The executable finds the library by an rpath to its directory, wherever the cache is.\n\
             \x20 get_filename_component(atlc_duckdb_dir \"${{ATLC_DUCKDB_LIBRARY}}\" DIRECTORY)\n\
             \x20 set_target_properties(run PROPERTIES BUILD_RPATH \"${{atlc_duckdb_dir}}\")\n\
             endif()\n\
             target_link_libraries(run PRIVATE atlc_duckdb)\n\
             target_compile_definitions(run PRIVATE ATLS_DUCKDB=1)\n\
             string(APPEND atlc_found \"file=${{ATLC_DUCKDB_LIBRARY}}\\nfile=${{ATLC_DUCKDB_INCLUDE}}/duckdb.h\\n\")\n"
        ));
    }
    s.push_str(
        "\n# What was found, for atlc's cache.\nfile(WRITE \"${CMAKE_BINARY_DIR}/atlc-found.txt\" \"${atlc_found}\")\n",
    );
    Ok(s)
}

/// The C++ compiler a build uses, and what identifies it for the cache.
#[derive(Clone, Debug)]
struct Compiler {
    /// Passed to CMake as `CMAKE_CXX_COMPILER`; on Windows CMake's generator picks it.
    path: Option<PathBuf>,
    /// Its path, size, modification time and `--version`.
    identity: String,
}

impl Compiler {
    /// `CXX`, else the first of `c++`, `g++` and `clang++` on `PATH` (CMake's own order); on Windows, the compiler
    /// CMake's default generator picks, identified by `CXX`, the `cl.exe` on `PATH` and the latest Visual Studio.
    fn probe() -> Result<Compiler, String> {
        let cxx = std::env::var_os("CXX").filter(|v| !v.is_empty());
        let chosen = match &cxx {
            Some(name) => Some(
                find_program(Path::new(name))
                    .ok_or_else(|| format!("CXX names `{}`, which is not a file or on PATH", name.to_string_lossy()))?,
            ),
            None if cfg!(windows) => None,
            None => Some(
                ["c++", "g++", "clang++"]
                    .iter()
                    .find_map(|n| find_program(Path::new(n)))
                    .ok_or("no C++ compiler on PATH (c++, g++ or clang++): install one, or set CXX")?,
            ),
        };
        let mut identity = String::new();
        let describe = |path: &Path, args: &[&str]| {
            let version = Command::new(path)
                .args(args)
                .output()
                .map(|o| {
                    format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    )
                })
                .unwrap_or_default();
            format!("{} {}\n{version}", path.display(), cache::stat(path))
        };
        match &chosen {
            Some(path) => identity.push_str(&describe(path, &["--version"])),
            None => {
                if let Some(cl) = find_program(Path::new("cl")) {
                    identity.push_str(&describe(&cl, &[]));
                }
                let program_files = std::env::var_os("ProgramFiles(x86)")
                    .map(PathBuf::from)
                    .unwrap_or_default();
                let vswhere = program_files.join("Microsoft Visual Studio/Installer/vswhere.exe");
                if vswhere.is_file() {
                    identity.push_str(&describe(&vswhere, &["-latest", "-property", "installationVersion"]));
                }
            }
        }
        Ok(Compiler {
            path: if cfg!(windows) { None } else { chosen },
            identity,
        })
    }
}

/// A program by path, or by name on `PATH`, made absolute.
fn find_program(name: &Path) -> Option<PathBuf> {
    let exe = |p: &Path| -> Option<PathBuf> {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        let with = p.with_extension("exe");
        (cfg!(windows) && with.is_file()).then_some(with)
    };
    if name.components().count() > 1 || name.is_absolute() {
        return exe(name).map(|p| std::path::absolute(&p).unwrap_or(p));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| exe(&dir.join(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Indicators;

    fn spec_run(algotradelib: Option<&str>, store: bool, duckdb: Option<&str>) -> spec::Run {
        spec::Run {
            algotradelib: algotradelib.map(PathBuf::from),
            duckdb: duckdb.map(PathBuf::from),
            indicators: Indicators::Incremental,
            store,
            orders: spec::Orders {
                kind: "market".into(),
                offset: "0".into(),
            },
            args: vec![],
        }
    }

    #[test]
    fn the_cmake_project_follows_the_settings() {
        let plain = cmake_lists(&spec_run(None, false, None)).unwrap();
        assert!(plain.contains("find_package(algotradelib CONFIG)\n"));
        assert!(!plain.contains("duckdb"));
        assert!(plain.contains("-ffp-contract=off") && plain.contains("/fp:precise"));
        let explicit = cmake_lists(&spec_run(Some("/opt/my atl"), true, Some("/opt/duck db"))).unwrap();
        assert!(explicit.contains("find_package(algotradelib CONFIG PATHS \"/opt/my atl\" NO_DEFAULT_PATH)"));
        assert!(explicit.contains("PATHS \"/opt/duck db\" PATH_SUFFIXES include lib NO_DEFAULT_PATH"));
        assert!(explicit.contains("ATLS_DUCKDB=1") && explicit.contains("BUILD_RPATH"));
        let from_env = cmake_lists(&spec_run(None, true, None)).unwrap();
        assert!(from_env.contains("HINTS ENV DUCKDB_ROOT"));
        assert!(
            cmake_lists(&spec_run(Some("/a;b"), false, None))
                .unwrap_err()
                .contains("cannot be passed to CMake")
        );
        assert!(
            cmake_lists(&spec_run(None, true, Some("/a\"b")))
                .unwrap_err()
                .contains("build.duckdb")
        );
    }

    #[test]
    fn the_key_covers_every_input_but_the_run_settings() {
        let files = |runtime: &str| {
            vec![
                (PathBuf::from("CMakeLists.txt"), "cmake".to_string()),
                (PathBuf::from("include/strategy.hpp"), "strategy".to_string()),
                (PathBuf::from("include/atls/ops.hpp"), runtime.to_string()),
            ]
        };
        let env = vec![("CXXFLAGS".to_string(), String::new())];
        let base = key_of(&files("ops"), "c++ 1", &env);
        assert_eq!(base, key_of(&files("ops"), "c++ 1", &env));
        assert_ne!(base, key_of(&files("ops "), "c++ 1", &env), "a runtime header changed");
        assert_ne!(base, key_of(&files("ops"), "c++ 2", &env), "the compiler changed");
        let flags = vec![("CXXFLAGS".to_string(), "-O0".to_string())];
        assert_ne!(base, key_of(&files("ops"), "c++ 1", &flags), "the environment changed");
        // The embedded runtime is part of every key: each header, as compiled into atlc.
        let runtime: Vec<&str> = backend::RUNTIME.iter().map(|(p, _)| *p).collect();
        assert_eq!(
            runtime,
            ["atls/args.hpp", "atls/ops.hpp", "atls/run.hpp", "atls/universe.hpp"]
        );
    }

    #[test]
    fn found_reads_what_cmake_wrote() {
        let dir = std::env::temp_dir().join(format!("atlc-found-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("inc/algotradelib")).unwrap();
        let inc = dir.join("inc").display().to_string();
        std::fs::write(
            dir.join("found.txt"),
            format!("package=/p/lib/cmake/algotradelib\ninclude={inc};$<BUILD_INTERFACE:/x>;/none\nfile=/usr/bin/c++\nfile=X-NOTFOUND\n"),
        )
        .unwrap();
        let (roots, files) = found(&dir.join("found.txt"));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/p/lib/cmake/algotradelib"),
                dir.join("inc").join("algotradelib")
            ]
        );
        assert_eq!(files, vec![PathBuf::from("/usr/bin/c++")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_file_api_reply_gives_the_cmake_inputs() {
        let dir = std::env::temp_dir().join(format!("atlc-api-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(cmake_inputs(&dir).unwrap_err().contains("no list"));
        let reply = dir.join("cmakeFiles-v1-0a1b.json");
        // As CMake writes it: a character past U+FFFF (here U+1F600) is a surrogate pair.
        std::fs::write(
            &reply,
            r#"{
  "inputs" :
  [
    { "path" : "CMakeLists.txt" },
    { "isGenerated" : true, "path" : "/b/CMakeFiles/4.1/CMakeCXXCompiler.cmake" },
    { "isExternal" : true, "path" : "/t/my {tool}\"chain\\.cmake" },
    { "isCMake" : true, "isExternal" : true, "path" : "/c/Modules/Find\u0041.cmake" },
    { "isExternal" : true, "path" : "/t/flags-\ud83d\ude00.cmake" }
  ],
  "paths" : { "build" : "/b", "source" : "/s" }
}
"#,
        )
        .unwrap();
        // Windows reads `/t/...` as relative; the paths here are Unix ones.
        if !cfg!(windows) {
            assert_eq!(
                cmake_inputs(&dir).unwrap(),
                [
                    PathBuf::from("/t/my {tool}\"chain\\.cmake"),
                    PathBuf::from("/c/Modules/FindA.cmake"),
                    PathBuf::from("/t/flags-\u{1F600}.cmake"),
                ]
            );
        }
        // An input that cannot be decoded (a lone surrogate) fails the build rather than going unwatched.
        std::fs::write(
            &reply,
            r#"{ "inputs" : [ { "path" : "/t/a.cmake" }, { "path" : "/t/\ud83d.cmake" } ] }"#,
        )
        .unwrap();
        assert!(cmake_inputs(&dir).unwrap_err().contains("cannot read"));
        std::fs::write(&reply, r#"{ "inputs" : [ ] }"#).unwrap();
        assert!(cmake_inputs(&dir).unwrap_err().contains("no inputs"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_build_says_what_cmake_did_not_find() {
        let log = "CMake Error at CMakeLists.txt:9 (message):\n  ATLC: no algotradelib package in\n  /my prefix, the spec's\n\n-- Configuring incomplete\n";
        let m = build_error("cmake failed", log, Path::new("/c/build.log"), false);
        assert!(
            m.starts_with("no algotradelib package in /my prefix, the spec's\n"),
            "{m}"
        );
        assert!(m.contains("/c/build.log"));
        let m = build_error(
            "cmake failed",
            "error: x.hpp:1: oops\n",
            Path::new("/c/build.log"),
            true,
        );
        assert!(m.contains("oops") && m.contains("algotradelib or DuckDB"), "{m}");
    }
}
