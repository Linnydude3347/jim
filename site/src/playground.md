# Playground

Write jim on the left; watch it compile to **C11** on the right — entirely in
your browser. `jimc` is compiled to WebAssembly, so nothing is installed and
nothing is sent to a server. It compiles against the **real** standard library.

Hit **Run ▸** to compile that C to WebAssembly and execute it, right in the
page — you'll see your program's output. **Running requires the full-screen
playground** (the embedded preview below can't, for browser-isolation reasons),
so open it here:

<p style="margin:1rem 0;">
  <a href="playground/index.html" target="_blank" rel="noopener"><strong>Open the playground full-screen ↗</strong></a>
</p>

> **On running:** the first Run downloads a C toolchain (~30 MB, once), so give
> it a moment. And because the browser build compiles panics to a clean exit,
> `try`/`catch` **won't catch** in the playground — an error prints and stops the
> program (its normal uncaught behavior). Everything else runs faithfully.

<iframe
  src="playground/index.html"
  title="jim playground"
  loading="lazy"
  style="width:100%;height:80vh;border:1px solid #444;border-radius:6px;background:#1e1f1c;">
</iframe>

Why show the generated C at all? Because that *is* jim: operators desugar to
method calls (`a + b` → `a.plus(b)`) and the whole program becomes one C11 file.
Reading the C is the clearest window into how the language works.

> Compiling against the real `std/` is deliberate — the playground runs the same
> standard library the CLI does, so whatever breaks here breaks there.
