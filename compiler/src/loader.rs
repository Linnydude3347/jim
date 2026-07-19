//! Where source files come from.
//!
//! The compiler front-end (import resolution + type-checking) is identical
//! whether jimc runs as a native CLI over a real filesystem or embedded in
//! WebAssembly over an in-memory map of virtual files. Both are expressed as a
//! [`Loader`]; the driver's `load_program` is generic over it.

use std::collections::HashMap;

/// Abstraction over a source of `.j` files. Paths are opaque strings whose only
/// meaning is what the implementor gives them.
pub trait Loader {
    /// The std-library root, if one is available, e.g. `"std"`. Used to resolve
    /// `#import <name>` and to decide which files may use `@intrinsics`.
    fn std_root(&self) -> Option<String>;
    /// Does a file exist at this (not-yet-canonical) path?
    fn exists(&self, path: &str) -> bool;
    /// Canonical identity key for a path — also the key [`read`](Loader::read)
    /// expects. Two paths naming the same file must share a key (so imports are
    /// idempotent). Errors if the file cannot be resolved.
    fn canonical(&self, path: &str) -> Result<String, String>;
    /// Read a file's contents by its canonical key.
    fn read(&self, canonical: &str) -> Result<String, String>;
    /// Is this canonical path inside the std root? (Gates `@intrinsics`.)
    fn is_under_std(&self, canonical: &str) -> bool;
    /// The directory containing this path (for resolving local imports).
    fn parent(&self, path: &str) -> String;
    /// Join a relative path onto a base directory.
    fn join(&self, base: &str, rel: &str) -> String;
    /// Human-readable name for diagnostics and baked-in `@panic` locations.
    fn display_name(&self, canonical: &str) -> String;
}

// ---------------------------------------------------------------------------
// In-memory loader (embedders: the wasm playground)
// ---------------------------------------------------------------------------

/// A [`Loader`] backed by a `path -> source` map. Paths are normalized to
/// forward-slashed, `.`/`..`-resolved keys, so `"std/./io.j"` and `"std/io.j"`
/// address the same entry.
pub struct MapLoader {
    files: HashMap<String, String>,
    std_root: Option<String>,
}

impl MapLoader {
    pub fn new(files: HashMap<String, String>, std_root: Option<String>) -> Self {
        let files = files
            .into_iter()
            .map(|(k, v)| (normalize(&k), v))
            .collect();
        MapLoader {
            files,
            std_root: std_root.map(|r| normalize(&r)),
        }
    }
}

/// Collapse `.`/`..` and duplicate/backslash separators into a clean
/// forward-slashed key. A leading `..` that would escape the root is dropped.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split(|c| c == '/' || c == '\\') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

impl Loader for MapLoader {
    fn std_root(&self) -> Option<String> {
        self.std_root.clone()
    }
    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(&normalize(path))
    }
    fn canonical(&self, path: &str) -> Result<String, String> {
        let n = normalize(path);
        if self.files.contains_key(&n) {
            Ok(n)
        } else {
            Err(format!("jimc: cannot read '{}': no such file", path))
        }
    }
    fn read(&self, canonical: &str) -> Result<String, String> {
        self.files
            .get(canonical)
            .cloned()
            .ok_or_else(|| format!("jimc: cannot read '{}': no such file", canonical))
    }
    fn is_under_std(&self, canonical: &str) -> bool {
        match &self.std_root {
            Some(root) => {
                let c = normalize(canonical);
                c == *root || c.starts_with(&format!("{}/", root))
            }
            None => false,
        }
    }
    fn parent(&self, path: &str) -> String {
        let n = normalize(path);
        match n.rfind('/') {
            Some(i) => n[..i].to_string(),
            None => String::new(),
        }
    }
    fn join(&self, base: &str, rel: &str) -> String {
        if base.is_empty() {
            normalize(rel)
        } else {
            normalize(&format!("{}/{}", base, rel))
        }
    }
    fn display_name(&self, canonical: &str) -> String {
        canonical.to_string()
    }
}

// ---------------------------------------------------------------------------
// Filesystem loader (the native CLI)
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub use fs_loader::FsLoader;

#[cfg(not(target_arch = "wasm32"))]
mod fs_loader {
    use super::Loader;
    use std::path::{Path, PathBuf};

    /// A [`Loader`] over the real filesystem, used by the `jimc` CLI.
    pub struct FsLoader {
        /// Canonical (verbatim-prefix-stripped) std root, if found.
        std_root: Option<String>,
        cwd: Option<PathBuf>,
    }

    impl FsLoader {
        /// `std_root` is the already-located std directory (see the driver's
        /// `find_std_root`), or `None` if there isn't one.
        pub fn new(std_root: Option<PathBuf>) -> Self {
            FsLoader {
                std_root: std_root.map(|p| nice_path(&p).to_string_lossy().into_owned()),
                cwd: std::env::current_dir().ok(),
            }
        }
    }

    impl Loader for FsLoader {
        fn std_root(&self) -> Option<String> {
            self.std_root.clone()
        }
        fn exists(&self, path: &str) -> bool {
            Path::new(path).is_file()
        }
        fn canonical(&self, path: &str) -> Result<String, String> {
            std::fs::canonicalize(path)
                .map(|p| nice_path(&p).to_string_lossy().into_owned())
                .map_err(|e| format!("jimc: cannot read '{}': {}", path, e))
        }
        fn read(&self, canonical: &str) -> Result<String, String> {
            std::fs::read_to_string(canonical)
                .map_err(|e| format!("jimc: cannot read '{}': {}", canonical, e))
        }
        fn is_under_std(&self, canonical: &str) -> bool {
            match &self.std_root {
                Some(root) => Path::new(canonical).starts_with(root),
                None => false,
            }
        }
        fn parent(&self, path: &str) -> String {
            Path::new(path)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_string_lossy()
                .into_owned()
        }
        fn join(&self, base: &str, rel: &str) -> String {
            Path::new(base).join(rel).to_string_lossy().into_owned()
        }
        fn display_name(&self, canonical: &str) -> String {
            // Panic locations read better as "std/core/array.j" than an absolute
            // path — strip the working directory when the file is under it.
            let p = nice_path(Path::new(canonical));
            match &self.cwd {
                Some(c) => p.strip_prefix(c).unwrap_or(&p).to_string_lossy().into_owned(),
                None => p.to_string_lossy().into_owned(),
            }
        }
    }

    /// Strip Windows' verbatim prefix (`\\?\C:\...`) that `canonicalize` adds —
    /// it's noise in diagnostics and breaks tidy `starts_with` comparisons.
    fn nice_path(p: &Path) -> PathBuf {
        let s = p.to_string_lossy();
        match s.strip_prefix(r"\\?\") {
            Some(stripped) => PathBuf::from(stripped),
            None => p.to_path_buf(),
        }
    }
}
