# Getting Started

jim compiles your program to C, then invokes a C compiler to produce a native
executable. So you need two things: the `jimc` compiler (built from Rust) and a
C compiler (`gcc`).

## Prerequisites

- **Rust** (via [rustup](https://rustup.rs)) — to build `jimc`. `jimc` has zero
  dependencies, so the build is quick.
- **gcc** — to compile the C that `jimc` emits.
  - **Windows:** [MSYS2](https://www.msys2.org) provides `gcc` at
    `C:\msys64\ucrt64\bin\gcc.exe`. jim links with `-static`.
  - **Linux/macOS:** install `gcc` (or `clang`) from your package manager.

## Build the compiler

From the repository root:

```powershell
cargo build --release --manifest-path compiler/Cargo.toml
```

This produces `compiler/target/release/jimc.exe` (or `jimc` on Unix).

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

```powershell
compiler\target\release\jimc.exe run hello.j
```

## The `jimc` subcommands

```powershell
jimc run   hello.j                 # compile + run (debug profile: stack traces on panic)
jimc build hello.j -o hello.exe    # compile to a native binary (release profile)
jimc check hello.j                 # type-check only, no code generation
jimc run   hello.j --emit-c out.c  # also write the generated C11 for inspection
```

`run` defaults to the **debug** profile (panics print a stack trace); `build`
defaults to **release** (zero-overhead, no traces). Override with `--debug` /
`--release`.

## Editor support

A **VS Code extension** lives in `editors/vscode-jim/`: syntax highlighting, a
"J" file icon, a Monokai theme, and type-aware features — diagnostics on save
(`jimc check`), completion (`Ctrl+Space`), hover with signatures and
`//`-docstrings, and parameter help.
