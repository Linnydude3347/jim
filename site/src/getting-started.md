# Getting Started

jim compiles your program to C, then invokes a C compiler to produce a native
executable. So you need two things: the `jimc` compiler (built from Rust) and a
C compiler (gcc or clang).

## Requirements

- Rust (via [rustup](https://rustup.rs)) to build `jimc`. `jimc` has zero
  dependencies, so the build is quick.
- A C compiler for the second step.
  - Windows: install [MSYS2](https://www.msys2.org). gcc lives at
    `C:\msys64\ucrt64\bin\gcc.exe`. jim links with `-static` there.
  - Linux and macOS: install gcc or clang from your package manager.

## Build the compiler

From the repository root:

```
cargo build --release --manifest-path compiler/Cargo.toml
```

This produces `compiler/target/release/jimc` (or `jimc.exe` on Windows). Put it
on your PATH, or call it by path.

## Your first program

Create `hello.j`:

```jim
#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
```

Compile and run it:

```
jimc run hello.j
```

## The jimc subcommands

```
jimc run   hello.j                 compile and run
jimc build hello.j -o hello.exe    compile to a native binary
jimc check hello.j                 type-check only, no code generation
jimc run   hello.j --emit-c out.c  also write the generated C11 for inspection
```

`run` defaults to the debug profile, where panics print a full stack trace.
`build` defaults to release, which has no tracing overhead. Override either with
`--debug` or `--release`.

## Editor support

A VS Code extension lives in `editors/vscode-jim/`: syntax highlighting, a file
icon, a Monokai theme, and type-aware features. You get diagnostics on save (via
`jimc check`), completion, hover with signatures and doc comments, and parameter
help.
