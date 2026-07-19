# Playground

> **Coming soon.** The jim playground will run entirely in your browser — no
> server, nothing to install.

Because `jimc` is written in zero-dependency Rust, it compiles cleanly to
**WebAssembly**. The first version of the playground (**"transpile mode"**) will
let you:

1. Write jim in an in-browser editor.
2. Compile it with `jimc` running as WebAssembly — right on this page.
3. See the **generated C11** side by side with your source.

That last part is the point. jim's whole design is *"operators desugar to method
calls, then transpile to one C file."* Watching your jim become C is the clearest
possible window into how the language works.

A later version may add in-browser **execution** (compiling and running the
generated C with a WebAssembly C toolchain), but transpile mode ships first.

*This page is a placeholder while the wasm build of `jimc` is wired up.*
