# jim

jim is a small, explicit programming language. Its one big idea: operators are
not compiler magic. `a + b` is sugar for `a.plus(b)`, so the behavior of every
operator lives in ordinary library code you can read and change. The compiler,
`jimc`, type-checks your program, lowers it to a single C11 source file, and
hands that to a C compiler to produce a native executable.

```jim
#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
```

- Docs: https://linnydude3347.github.io/jim/
- Playground (write and run jim in your browser): https://linnydude3347.github.io/jim/playground/

## Why jim

- Operators desugar to method calls. `a + b` calls `plus`, `==` and `!=` call
  `equals`, `<` `>` `<=` `>=` all derive from `less_than`, and `[]` calls
  `get`/`set`. Implement those methods on your own type and the operators work.
- Explicit by default. Every variable has a type and an initializer. Integer
  literals never silently become Floats. `print` takes a String, so you write
  `print(n.to_string())`.
- One C11 file out. The runtime is embedded in the generated C, so the output
  is a single self-contained file you can read and compile anywhere.
- Arena memory. Allocation is a bump pointer; nothing is freed until the process
  exits. No manual free, no use-after-free.

## Requirements

To build and run jim programs you need two things:

1. Rust (via https://rustup.rs) to build the `jimc` compiler. `jimc` has zero
   dependencies, so the build is quick.
2. A C compiler for `jimc` to hand its generated C to.
   - Windows: install MSYS2 (https://www.msys2.org). gcc lives at
     `C:\msys64\ucrt64\bin\gcc.exe`. jim links with `-static` there.
   - Linux and macOS: install gcc or clang from your package manager.

## Build the compiler

From the repository root:

```
cargo build --release --manifest-path compiler/Cargo.toml
```

This produces the compiler at `compiler/target/release/jimc` (or `jimc.exe` on
Windows). Put it on your PATH, or call it by path in the examples below.

## Run your first program

Save this as `hello.j`:

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

## Using jimc

```
jimc run   file.j                 compile and run
jimc build file.j -o file.exe     compile to a native binary
jimc check file.j                 type-check only, no code generation
jimc run   file.j --emit-c out.c  also write the generated C11 for inspection
```

Useful flags:

- `-o PATH` set the output binary path.
- `--emit-c PATH` write the generated C alongside building.
- `--std DIR` point at a standard library directory (auto-detected by default).
- `--cc CMD` choose the C compiler (gcc, clang, and others are auto-detected).
- `--debug` / `--release` control panic stack traces. `run` defaults to debug
  (traces on), `build` defaults to release (no tracing overhead).
- `--panic-abort` compile panics to print-and-exit with no try/catch unwinding.
  This emits C without setjmp, which is what the browser playground uses.

## Language basics

This is the short version. The full tour and more examples are in the docs at
https://linnydude3347.github.io/jim/ and in the `docs/` directory.

### Variables and types

```jim
var name: String = "Ben";
var age: Integer = 24;      // 64-bit signed
var pi: Float = 3.14;       // 64-bit double
var c: Char = 'c';          // one byte (0-255); ASCII literals
var ok: Bool = true;        // true / false, lowercase
const day: Integer = 1;     // cannot be reassigned
```

Every variable needs a type annotation and an initializer. `var f: Float = 3;`
is an error; write `3.0`. Note that `7 / 2` is `3.5`. Use `div` for integer
division.

### Functions

```jim
function add(a: Integer, b: Integer) -> Integer {
    return a + b;
}
```

The return type is mandatory (`-> None` for no value). Every path in a non-None
function must return. There is no overloading: one name, one signature. `main`
returns an Integer, which is the process exit code.

### Operators are method calls

```jim
class Vec2 {
    public x: Integer = 0;   // fields need a visibility and a default
    public y: Integer = 0;

    Vec2(x: Integer, y: Integer) {   // constructor: class name, no return type
        this.x = x;
        this.y = y;
    }

    public plus(other: Vec2) -> Vec2 {
        return Vec2(this.x + other.x, this.y + other.y);
    }

    public to_string() -> String {
        return "(" + this.x.to_string() + ", " + this.y.to_string() + ")";
    }
}

function main() -> Integer {
    var sum: Vec2 = Vec2(1, 2) + Vec2(3, 4);   // calls Vec2.plus
    print(sum.to_string());                    // (4, 6)
    return 0;
}
```

Members are accessed only through `this.` inside methods. Classes are references
(assignment shares the object), and there is no inheritance.

### Control flow

```jim
for (var i: Integer = 0; i < 10; i = i + 1) {
    if (i div 2 == 0) { continue; }
    print(i.to_string());
}
```

jim has C-style `for`, plus `while`, `break`, and `continue`. `and`, `or`,
`not`, and `div` are word operators. Conditions must be Bool; there is no
truthiness.

### Containers

```jim
var xs: Array<Integer> = [1, 2, 3];   // fixed length
var v: Vector<Integer> = [1, 2, 3];   // growable
v.push(4);
var first: Integer = xs[0];           // out of bounds panics

for (x: Integer in v) {
    print(x.to_string());
}
```

`Array<T>` is fixed length, `Vector<T>` grows. Any class with `length()` and
`get(i)` works with `for..in`, so your own types can be iterable too.

### Optionals

```jim
function find(v: Vector<Integer>, wanted: Integer) -> Integer? {
    for (var i: Integer = 0; i < v.length(); i = i + 1) {
        if (v[i] == wanted) { return i; }
    }
    return None;
}
```

`T?` is "a T or None". Using a `T?` where a `T` is required unwraps it
automatically and panics if it was None.

### Generic functions

```jim
function largest<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (var i: Integer = 1; i < seq.length(); i = i + 1) {
        if (seq[i] > best) { best = seq[i]; }
    }
    return best;
}
```

One definition works for every sequence type (Array, Vector, even String). Each
combination is stamped out at compile time, so a generic call costs the same as
a hand-written one. The standard library ships `max`, `min`, and `sum` built
this way.

### Errors

```jim
try {
    var v: Vector<Integer> = [1];
    var x: Integer = v[5];          // out of bounds panic
} catch (e: Exception) {
    print("caught: " + e.msg());
}
```

Everything that panics is catchable: None misuse, division by zero, integer
overflow, out of bounds, and anything the standard library raises. An uncaught
panic prints its message and exits with code 1. There is no `throw`.

### Modules

```jim
#import <io>            // standard library: resolves to std/io.j
#import <math>          // std/math.j
#import "geometry.j"    // your own file, relative to the importing file
```

Imports are idempotent. `std/core.j` is the prelude and is imported into every
program automatically; that is where Integer, String, Vector, and friends come
from.

## Editor support

A VS Code extension lives in `editors/vscode-jim/`: syntax highlighting, a file
icon, a Monokai theme, and type-aware features (diagnostics on save via
`jimc check`, completion, hover with signatures and doc comments, and parameter
help).

## Repository layout

```
compiler/   jimc: the Rust to C11 compiler (zero dependencies)
std/        the jim standard library (core, io, math)
editors/    the VS Code extension
docs/       language spec, design notes, and examples
site/       the mdBook documentation site (deployed to GitHub Pages)
playground/ the WebAssembly wrapper that powers the in-browser playground
tests/      compiler test fixtures
```

## How it works

`jimc` lexes, parses, and type-checks your program, lowering operators to method
calls as it goes. It then generates one C11 translation unit with the runtime
embedded at the top and invokes a C compiler to produce a native binary. Memory
is arena allocated and released in a single sweep when the program exits.

The compiler is also compiled to WebAssembly for the playground, where it
produces the generated C in the browser. A C toolchain compiled to WebAssembly
then compiles and runs that C client-side, so the playground needs no server.

## Status

jim is under active development. The language spec and the compiler evolve
together, and this documentation tracks the current, working behavior.
