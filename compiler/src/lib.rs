//! The jim compiler as a library.
//!
//! The CLI (`src/main.rs`) is a thin wrapper over this crate. Embedders - most
//! notably the browser playground's WebAssembly build - call [`compile_to_c`]
//! to turn jim source into a C11 string entirely in memory, with no filesystem
//! access and no C compiler invocation.

pub mod ast;
pub mod codegen;
pub mod driver;
pub mod errors;
pub mod lexer;
pub mod loader;
pub mod parser;
pub mod sema;
pub mod token;

pub use driver::compile_to_c;
pub use loader::{Loader, MapLoader};
