# The jim Programming Language

**jim** is a small, deliberately explicit programming language. Its defining
idea: **operators are not compiler magic** — `a + b` desugars to `a.plus(b)`, so
the behavior of every operator lives in ordinary library code you can read. The
compiler, `jimc`, transpiles your program to a single **C11** file and hands it
to a C compiler, producing a native executable.

```jim
#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
```

## What makes jim jim

- **Operators desugar to method calls.** `a + b` → `a.plus(b)`, `a < b` →
  `a.less_than(b)`. Comparisons derive from `equals` and `less_than` alone. The
  standard library, not the compiler, decides what operators mean.
- **Explicit by default.** Every variable has a type annotation and an
  initializer. Integer literals never silently become Floats. `print` takes a
  `String`, so you write `print(n.to_string())`.
- **Transpiles to one C11 file.** The runtime is embedded; the output is a
  single self-contained `.c` you can read, and compile anywhere a C compiler
  runs.
- **Arena memory.** Allocation is bump-fast; nothing is freed until the process
  exits. No manual `free`, no use-after-free.
- **Value types by name.** `Integer`, `Float`, `Char`, `Bool`, `None` are value
  classes; inside their methods, `this` *is* the value.

## Where to go next

- [Getting Started](getting-started.md) — install the toolchain and build your
  first program.
- [Language Tour](language-tour.md) — the whole language in a few pages.
- [Examples](examples.md) — runnable programs, from FizzBuzz to a prime sieve.
- [Playground](playground.md) — write jim in the browser and watch it become C.

> jim is under active development. The language spec and the compiler design
> contracts evolve together; this book tracks the current, working behavior.
