# Playground

Write jim on the left; watch it compile to **C11** on the right — entirely in
your browser. `jimc` is compiled to WebAssembly, so nothing is installed and
nothing is sent to a server. It compiles against the **real** standard library.

<p style="margin:1rem 0;">
  <a href="playground/index.html" target="_blank" rel="noopener">Open the playground full-screen ↗</a>
</p>

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
