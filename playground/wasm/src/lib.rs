//! WebAssembly wrapper for the jim playground.
//!
//! Exposes [`compile`] to JavaScript: it embeds Ben's **real** standard library
//! at build time (via `include_str!`), compiles the user's program to C11
//! entirely in memory with [`jimc::compile_to_c`], and returns either the
//! generated C or a rendered diagnostic. No filesystem, no C compiler — it all
//! runs client-side in the browser.
//!
//! Compiling against the real `std/` is deliberate: the playground and the CLI
//! share the exact same standard library, so whatever breaks here breaks there.

use std::collections::HashMap;
use wasm_bindgen::prelude::*;

/// Pair a virtual std path with its embedded contents. `include_str!` paths are
/// relative to this file (`playground/wasm/src/lib.rs`), so `../../../std/...`
/// reaches the repository's `std/` directory.
macro_rules! std_file {
    ($path:literal) => {
        (
            concat!("std/", $path),
            include_str!(concat!("../../../std/", $path)),
        )
    };
}

/// Ben's real standard library, embedded at build time. This list mirrors
/// `std/core.j`'s import manifest plus the user-facing libraries; if Ben adds a
/// std file, add it here too.
const STD_FILES: &[(&str, &str)] = &[
    std_file!("core.j"),
    std_file!("core/integer.j"),
    std_file!("core/float.j"),
    std_file!("core/bool.j"),
    std_file!("core/char.j"),
    std_file!("core/string.j"),
    std_file!("core/exception.j"),
    std_file!("core/array.j"),
    std_file!("core/vector.j"),
    std_file!("core/generic.j"),
    std_file!("io.j"),
    std_file!("math.j"),
];

/// The outcome of a compile. On success `ok` is true, `c` holds the generated
/// C11, and `error` is empty; on failure `ok` is false, `error` holds the
/// rendered diagnostic (`main.j:line:col: error: ...`), and `c` is empty.
#[wasm_bindgen]
pub struct CompileResult {
    ok: bool,
    c: String,
    error: String,
}

#[wasm_bindgen]
impl CompileResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }
    #[wasm_bindgen(getter)]
    pub fn c(&self) -> String {
        self.c.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> String {
        self.error.clone()
    }
}

/// Compile a jim program to C11. `source` is the user's program (containing
/// `main`); it is compiled against the embedded real standard library. User
/// code may not use `@intrinsics` — only std files can.
#[wasm_bindgen]
pub fn compile(source: &str) -> CompileResult {
    let mut files: HashMap<String, String> = STD_FILES
        .iter()
        .map(|(path, src)| (path.to_string(), src.to_string()))
        .collect();
    files.insert("main.j".to_string(), source.to_string());

    match jimc::compile_to_c("main.j", files, Some("std".to_string()), false, false) {
        Ok(c) => CompileResult {
            ok: true,
            c,
            error: String::new(),
        },
        Err(error) => CompileResult {
            ok: false,
            c: String::new(),
            error,
        },
    }
}

/// The playground wrapper's version, for display in the UI.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
