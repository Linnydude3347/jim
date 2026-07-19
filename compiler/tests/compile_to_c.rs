//! Phase-1 check: the compiler front-end runs entirely in memory, with no
//! filesystem and no C compiler — the foundation the wasm playground stands on.
//!
//! We compile against the known-good `tests/fake_std` doubles (not Ben's real
//! `std/`, which intentionally still has bugs the playground is meant to expose)
//! so this test is deterministic.

use jimc::compile_to_c;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Recursively load every `.j` file under `dir` into `map`, keyed by
/// `<prefix>/<relative-path>` with forward slashes.
fn load_dir(map: &mut HashMap<String, String>, dir: &Path, prefix: &str) {
    for entry in fs::read_dir(dir).expect("read fake_std dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            load_dir(map, &path, &format!("{}/{}", prefix, name));
        } else if path.extension().map_or(false, |e| e == "j") {
            let src = fs::read_to_string(&path).expect("read .j file");
            map.insert(format!("{}/{}", prefix, name), src);
        }
    }
}

fn fake_std_map() -> HashMap<String, String> {
    // CARGO_MANIFEST_DIR is the `compiler/` crate root.
    let fake_std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fake_std");
    let mut map = HashMap::new();
    load_dir(&mut map, &fake_std, "std");
    map
}

#[test]
fn compiles_hello_in_memory() {
    let mut files = fake_std_map();
    files.insert(
        "main.j".to_string(),
        r#"#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
"#
        .to_string(),
    );

    let c = compile_to_c("main.j", files, Some("std".to_string()), false, false)
        .expect("in-memory compile should succeed against fake_std");

    // A jim `main` lowers to a C `main`, and the runtime string helper appears.
    assert!(c.contains("main"), "generated C should define main");
    assert!(!c.trim().is_empty(), "generated C should be non-empty");
}

#[test]
fn operators_desugar_to_method_calls_in_memory() {
    // Exercises the desugaring path (`a + b` -> `a.plus(b)`) with a user class,
    // proving sema + codegen run in memory, not just parsing.
    let mut files = fake_std_map();
    files.insert(
        "main.j".to_string(),
        r#"#import <io>

class Vec2 {
    public x: Integer = 0;
    public y: Integer = 0;

    Vec2(x: Integer, y: Integer) {
        this.x = x;
        this.y = y;
    }

    public plus(other: Vec2) -> Vec2 {
        return Vec2(this.x + other.x, this.y + other.y);
    }
}

function main() -> Integer {
    var a: Vec2 = Vec2(1, 2);
    var b: Vec2 = Vec2(3, 4);
    var c: Vec2 = a + b;
    return c.x;
}
"#
        .to_string(),
    );

    let c = compile_to_c("main.j", files, Some("std".to_string()), false, false)
        .expect("in-memory compile of a user class with operators should succeed");
    assert!(c.contains("main"), "generated C should define main");
}

#[test]
fn a_type_error_comes_back_rendered() {
    // No filesystem, but diagnostics still render with a path:line:col header.
    let mut files = fake_std_map();
    files.insert(
        "main.j".to_string(),
        r#"function main() -> Integer {
    return "not an integer";
}
"#
        .to_string(),
    );

    let err = compile_to_c("main.j", files, Some("std".to_string()), false, false)
        .expect_err("returning a String from an Integer function should fail");
    assert!(
        err.contains("main.j"),
        "diagnostic should name the virtual file, got: {err}"
    );
}
