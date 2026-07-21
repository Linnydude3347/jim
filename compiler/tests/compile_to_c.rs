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

    let c = compile_to_c("main.j", files, Some("std".to_string()), false, false, false)
        .expect("in-memory compile should succeed against fake_std");

    // A jim `main` lowers to a C `main`, and the runtime string helper appears.
    assert!(c.contains("main"), "generated C should define main");
    assert!(!c.trim().is_empty(), "generated C should be non-empty");
    // Printing a literal needs strings but never allocates: no arena.
    assert!(c.contains("rt_str_lit"), "hello needs the string representation");
    assert!(!c.contains("rt_arena_alloc"), "hello should not carry the arena");
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

    let c = compile_to_c("main.j", files, Some("std".to_string()), false, false, false)
        .expect("in-memory compile of a user class with operators should succeed");
    assert!(c.contains("main"), "generated C should define main");
}

#[test]
fn panic_abort_mode_strips_setjmp() {
    // The browser playground's run path compiles with panic_abort = true so the
    // generated C has no setjmp/longjmp (unavailable on that wasm toolchain).
    let mut files = fake_std_map();
    files.insert(
        "main.j".to_string(),
        r#"function main() -> Integer {
    try {
        var x: Integer = 1;
    } catch (e: Exception) {
        var y: Integer = 2;
    }
    return 0;
}
"#
        .to_string(),
    );

    // Normal codegen emits the setjmp handler for try/catch, and jimc keeps the
    // setjmp machinery in the runtime it emits.
    let normal =
        compile_to_c("main.j", files.clone(), Some("std".to_string()), false, false, false)
            .expect("normal compile");
    assert!(normal.contains("setjmp(jim_h"), "normal build should emit a setjmp handler");
    assert!(
        normal.contains("jmp_buf") && normal.contains("rt_handler"),
        "normal build keeps the setjmp runtime"
    );

    // Abort mode: jimc filters the runtime and codegen omits the handler, so no
    // setjmp construct survives at all (only comments may mention the word).
    let abort = compile_to_c("main.j", files, Some("std".to_string()), false, false, true)
        .expect("panic=abort compile");
    for bad in ["#include <setjmp.h>", "jmp_buf", "setjmp(", "longjmp(", "rt_handler"] {
        assert!(!abort.contains(bad), "panic=abort build must not contain `{bad}`");
    }
}

#[test]
fn dce_prunes_unused_stdlib() {
    // Hello world touches only print + a String literal, so the generated C
    // must not carry unrelated stdlib methods (dead-code elimination).
    let mut files = fake_std_map();
    files.insert(
        "main.j".to_string(),
        "#import <io>\nfunction main() -> Integer { print(\"hi\"); return 0; }\n".to_string(),
    );
    let c = compile_to_c("main.j", files, Some("std".to_string()), false, false, false)
        .expect("hello compiles");
    assert!(!c.contains("jim_m_Float_plus"), "unused Float.plus should be pruned");
    assert!(!c.contains("jim_m_Integer_times"), "unused Integer.times should be pruned");

    // A program that adds Integers keeps Integer.plus (operators desugar to it).
    let mut files2 = fake_std_map();
    files2.insert(
        "main.j".to_string(),
        "function main() -> Integer { var x: Integer = 1 + 2; return x; }\n".to_string(),
    );
    let c2 = compile_to_c("main.j", files2, Some("std".to_string()), false, false, false)
        .expect("add compiles");
    assert!(c2.contains("jim_m_Integer_plus"), "used Integer.plus must be emitted");
}

#[test]
fn no_double_return_and_minimal_runtime() {
    let mut files = fake_std_map();
    files.insert(
        "main.j".to_string(),
        "function main() -> Integer { return 0; }\n".to_string(),
    );
    let c = compile_to_c("main.j", files, Some("std".to_string()), false, false, false)
        .expect("tiny compiles");

    // The explicit `return 0` is not duplicated by an implicit trailing return.
    assert_eq!(
        c.matches("return INT64_C(0)").count(),
        1,
        "expected exactly one return in jim_user_main"
    );

    // A do-nothing program pulls in none of the optional runtime families,
    // no allocator, no string machinery, and no container layouts.
    for absent in [
        "rt_panic(",
        "rt_opt_i64_some",
        "rt_f64_sqrt",
        "jmp_buf",
        "rt_i64_add",
        "rt_arena_alloc",
        "rt_init",
        "jim_str",
        "rt_frame_top",
        "JIM_DEFINE_BUF",
        "struct jim_c_",
    ] {
        assert!(!c.contains(absent), "tiny build should not contain `{absent}`");
    }
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

    let err = compile_to_c("main.j", files, Some("std".to_string()), false, false, false)
        .expect_err("returning a String from an Integer function should fail");
    assert!(
        err.contains("main.j"),
        "diagnostic should name the virtual file, got: {err}"
    );
}
