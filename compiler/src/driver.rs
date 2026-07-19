use crate::ast::{ClassDecl, FunctionDecl, ImportKind, Program};
use crate::codegen;
use crate::errors;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::sema;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Options {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub emit_c: Option<PathBuf>,
    pub std_dir: Option<PathBuf>,
    pub cc: Option<String>,
    pub allow_intrinsics: bool,
    /// Emit shadow-stack maintenance so panics print full jim stack traces.
    /// `jimc run` defaults to true (the dev loop), `jimc build` to false.
    pub debug: bool,
}

/// Load the entry file plus everything it (transitively) imports, merged into
/// one Program, plus each file's display path (indexed by file_idx — codegen
/// bakes these into @panic locations). Errors come back fully rendered.
fn load_program(opts: &Options, require_main: bool) -> Result<(Program, Vec<String>), String> {
    let std_root = find_std_root(opts);
    let mut sources: Vec<(PathBuf, String)> = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut functions: Vec<FunctionDecl> = Vec::new();
    let mut classes: Vec<ClassDecl> = Vec::new();
    let mut queue: Vec<PathBuf> = vec![opts.input.clone()];

    // The prelude: std/core.j is auto-imported into every program (once it exists).
    if let Some(root) = &std_root {
        let core = root.join("core.j");
        if core.is_file() {
            queue.push(core);
        }
    }

    while let Some(path) = queue.pop() {
        let path = nice_path(&path);
        let canon = std::fs::canonicalize(&path)
            .map_err(|e| format!("jimc: cannot read '{}': {}", path.display(), e))?;
        if !visited.insert(canon.clone()) {
            continue; // idempotent imports
        }
        let src = std::fs::read_to_string(&canon)
            .map_err(|e| format!("jimc: cannot read '{}': {}", path.display(), e))?;
        let file_idx = sources.len();
        let from_std = std_root
            .as_ref()
            .map_or(false, |root| canon.starts_with(root));

        let toks = Lexer::new(&src)
            .tokenize()
            .map_err(|e| errors::render(&path, &src, &e))?;
        let module = Parser::new(toks)
            .parse_module(file_idx, from_std)
            .map_err(|e| errors::render(&path, &src, &e))?;

        for imp in &module.imports {
            match &imp.kind {
                ImportKind::Std(name) => {
                    let root = std_root.as_ref().ok_or_else(|| {
                        errors::render(
                            &path,
                            &src,
                            &crate::errors::JimError::new(
                                format!(
                                    "cannot resolve '#import <{}>': no std directory found (use --std <dir> or set JIM_STD)",
                                    name
                                ),
                                imp.line,
                                imp.col,
                            ),
                        )
                    })?;
                    // `<name>` resolves to <std>/name.j (user-facing libraries),
                    // falling back to <std>/core/name.j (the prelude's parts).
                    let direct = root.join(format!("{}.j", name));
                    let core_sub = root.join("core").join(format!("{}.j", name));
                    let target = if direct.is_file() {
                        direct
                    } else if core_sub.is_file() {
                        core_sub
                    } else {
                        return Err(errors::render(
                            &path,
                            &src,
                            &crate::errors::JimError::new(
                                format!(
                                    "standard library '<{}>' not found (looked for {} and {})",
                                    name,
                                    direct.display(),
                                    core_sub.display()
                                ),
                                imp.line,
                                imp.col,
                            ),
                        ));
                    };
                    queue.push(target);
                }
                ImportKind::Local(rel) => {
                    let base = canon.parent().unwrap_or_else(|| Path::new("."));
                    let target = base.join(rel);
                    if !target.is_file() {
                        return Err(errors::render(
                            &path,
                            &src,
                            &crate::errors::JimError::new(
                                format!("imported file \"{}\" not found (looked for {})", rel, target.display()),
                                imp.line,
                                imp.col,
                            ),
                        ));
                    }
                    queue.push(target);
                }
            }
        }

        sources.push((path.clone(), src));
        functions.extend(module.functions);
        classes.extend(module.classes);
    }

    let program = Program { functions, classes };
    let lowered = sema::check(program, opts.allow_intrinsics, require_main).map_err(|se| {
        let (path, src) = &sources[se.file_idx];
        errors::render(path, src, &se.err)
    })?;
    // Relative to the working directory when possible — panic locations
    // read better as "std/core/array.j" than "C:/Users/.../array.j".
    let cwd = std::env::current_dir().ok();
    let file_names = sources
        .iter()
        .map(|(p, _)| {
            cwd.as_ref()
                .and_then(|c| p.strip_prefix(c).ok())
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .collect();
    Ok((lowered, file_names))
}

/// Parse + type-check only (no C compilation). Library files without a
/// `main` are fine.
pub fn check(opts: &Options) -> Result<(), String> {
    load_program(opts, false)?;
    Ok(())
}

/// Locate the std root: --std flag, then $JIM_STD, then a `std/` directory
/// near the compiler binary or the current directory.
fn find_std_root(opts: &Options) -> Option<PathBuf> {
    if let Some(d) = &opts.std_dir {
        return std::fs::canonicalize(d).ok();
    }
    if let Ok(d) = std::env::var("JIM_STD") {
        return std::fs::canonicalize(d).ok();
    }
    let mut candidates: Vec<PathBuf> = vec![PathBuf::from("std")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("std"));
            candidates.push(dir.join("../std"));
            candidates.push(dir.join("../../std"));
            candidates.push(dir.join("../../../std")); // compiler/target/<profile>/jimc -> repo/std
        }
    }
    candidates
        .into_iter()
        .find(|c| c.is_dir())
        .and_then(|c| std::fs::canonicalize(c).ok())
}

/// Find a working C compiler: --cc / $JIM_CC override, then gcc, cc, clang, zig cc.
fn find_cc(pref: &Option<String>) -> Option<Vec<String>> {
    let mut candidates: Vec<Vec<String>> = Vec::new();
    if let Some(p) = pref {
        candidates.push(p.split_whitespace().map(String::from).collect());
    }
    if let Ok(p) = std::env::var("JIM_CC") {
        candidates.push(p.split_whitespace().map(String::from).collect());
    }
    candidates.push(vec!["gcc".into()]);
    candidates.push(vec!["cc".into()]);
    candidates.push(vec!["clang".into()]);
    candidates.push(vec!["zig".into(), "cc".into()]);

    candidates.into_iter().find(|c| {
        !c.is_empty()
            && Command::new(&c[0])
                .args(&c[1..])
                .arg("--version")
                .output()
                .map_or(false, |o| o.status.success())
    })
}

/// Strip Windows' verbatim prefix (\\?\C:\...) that canonicalize adds —
/// it's noise in diagnostics.
fn nice_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => p.to_path_buf(),
    }
}

fn temp_build_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("jimc");
    std::fs::create_dir_all(&dir).map_err(|e| format!("jimc: cannot create {}: {}", dir.display(), e))?;
    Ok(dir)
}

fn unique_stem(input: &Path) -> String {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}_{}_{}", stem, std::process::id(), nanos)
}

pub fn default_output(input: &Path) -> PathBuf {
    if cfg!(windows) {
        input.with_extension("exe")
    } else {
        input.with_extension("")
    }
}

/// Compile `opts.input` to a native executable. Returns the executable path.
pub fn build(opts: &Options) -> Result<PathBuf, String> {
    let (program, file_names) = load_program(opts, true)?;
    let c_source = codegen::generate(&program, &file_names, opts.debug);

    if let Some(c_path) = &opts.emit_c {
        std::fs::write(c_path, &c_source)
            .map_err(|e| format!("jimc: cannot write {}: {}", c_path.display(), e))?;
    }

    let gen_path = temp_build_dir()?.join(format!("{}.c", unique_stem(&opts.input)));
    std::fs::write(&gen_path, &c_source)
        .map_err(|e| format!("jimc: cannot write {}: {}", gen_path.display(), e))?;

    let cc = find_cc(&opts.cc).ok_or_else(|| {
        "jimc: no C compiler found (need gcc, clang, or zig on PATH; or set JIM_CC / --cc)".to_string()
    })?;

    let out_path = opts.output.clone().unwrap_or_else(|| default_output(&opts.input));

    let mut cmd = Command::new(&cc[0]);
    cmd.args(&cc[1..]);
    cmd.arg(&gen_path).arg("-o").arg(&out_path).arg("-w").arg("-O1");
    // libm for the float-math intrinsics (a no-op stub archive on MinGW)
    cmd.arg("-lm");
    // MSYS2 gcc: link statically so the exe runs outside the MSYS environment.
    if cfg!(windows) && cc[0].contains("gcc") {
        cmd.arg("-static");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("jimc: failed to run C compiler '{}': {}", cc[0], e))?;
    if !output.status.success() {
        return Err(format!(
            "jimc: internal error — generated C failed to compile (this is a jimc bug).\nGenerated file kept at: {}\n{}",
            gen_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let _ = std::fs::remove_file(&gen_path);
    Ok(out_path)
}

/// Compile to a temp location and run, forwarding the exit code.
pub fn run(opts: &Options) -> Result<i32, String> {
    let exe = temp_build_dir()?.join(format!(
        "{}{}",
        unique_stem(&opts.input),
        if cfg!(windows) { ".exe" } else { "" }
    ));
    let build_opts = Options { output: Some(exe.clone()), ..clone_opts(opts) };
    build(&build_opts)?;
    let status = Command::new(&exe)
        .status()
        .map_err(|e| format!("jimc: failed to run {}: {}", exe.display(), e))?;
    Ok(status.code().unwrap_or(1))
}

fn clone_opts(o: &Options) -> Options {
    Options {
        input: o.input.clone(),
        output: o.output.clone(),
        emit_c: o.emit_c.clone(),
        std_dir: o.std_dir.clone(),
        cc: o.cc.clone(),
        allow_intrinsics: o.allow_intrinsics,
        debug: o.debug,
    }
}
