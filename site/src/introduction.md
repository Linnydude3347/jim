# The jim Programming Language

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

## Why jim

- Operators desugar to method calls. `a + b` calls `plus`, `==` and `!=` call
  `equals`, `<` `>` `<=` `>=` all derive from `less_than`, and `[]` calls
  `get`/`set`. The standard library, not the compiler, decides what operators
  mean.
- Explicit by default. Every variable has a type and an initializer. Integer
  literals never silently become Floats. `print` takes a String, so you write
  `print(n.to_string())`.
- One C11 file out. The runtime is embedded in the generated C, so the output is
  a single self-contained file you can read and compile anywhere a C compiler
  runs.
- Arena memory. Allocation is a bump pointer; nothing is freed until the process
  exits. No manual free, no use-after-free.
- Value types by name. Integer, Float, Char, Bool, and None are value classes;
  inside their methods, `this` is the value itself.

## Where to go next

- [Getting Started](getting-started.md): install the toolchain and build your
  first program.
- [Language Tour](language-tour.md): the whole language in a few pages.
- [Examples](examples.md): complete, runnable programs.
- [Playground](playground.md): write jim in the browser, see the generated C,
  and run it.

jim is under active development. The language spec and the compiler evolve
together, and this book tracks the current, working behavior.
