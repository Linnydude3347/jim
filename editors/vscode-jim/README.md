# jim language support for VS Code

Syntax highlighting, a "J" file icon, a Monokai-style color theme
("Jim Monokai"), **inline error checking**, **auto-complete**, and
**hover documentation** for `.j` files.

## Language smarts

- **Diagnostics**: on save, `jimc check` runs and errors appear as red
  squiggles (Problems panel too). Configure with `jim.checkOnSave`,
  `jim.compilerPath`, `jim.stdPath`.
- **Completions**: keywords, built-in types, and every function/class in the
  workspace; method and field names after `.`; intrinsics with signatures
  after `@`.
- **Signature help**: typing `(` after a function, method, constructor, or
  `@intrinsic` (and `,` between arguments) pops up its parameter list with the
  current argument highlighted, plus the docstring.
- **Hover docs**: hovering a function, class, method, field, or `@intrinsic`
  shows its signature - plus its **docstring**: any block of `//` comments
  directly above a declaration becomes its hover documentation. Document
  your std that way and it shows up everywhere.

```jim
// Returns the area of this shape.
// Grows with grow().
public area() -> Integer { ... }
```

Completions are declaration-based (a lexical scan), not type-inferred: after
`.` you're offered all known public members, not just the receiver's. The
diagnostics, however, are the real compiler - exact errors, one at a time.

The file icon is a per-language icon: the built-in Seti and Minimal icon
themes show it automatically for `.j` files (they have no icon of their own).
Third-party icon themes may override it with their generic icon.

The grammar uses standard TextMate scopes, so `.j` files highlight correctly
under **any** theme - including VS Code's built-in Monokai. The bundled
"Jim Monokai" theme adds jim-specific touches (intrinsics in orange, `this`
italic orange).

## Install (local)

Copy this folder into your VS Code extensions directory:

```powershell
Copy-Item -Recurse editors\vscode-jim "$env:USERPROFILE\.vscode\extensions\jim-lang-0.1.0"
```

Then reload VS Code (`Developer: Reload Window`). `.j` files are recognized
automatically. To use the theme: `Ctrl+K Ctrl+T` -> "Jim Monokai".

## What gets highlighted

- keywords (`if while for return try catch ...`), declarations (`var const function class public private`) - Monokai pink
- word operators (`and or not div`) and symbols (`-> == += ...`) - pink
- types (`Integer`, `Vector`, user classes) - cyan italic; class declarations green underline
- function/method names and calls - green
- strings/chars - yellow; escapes, numbers, `true`/`false`/`None` - purple
- `this` and `@intrinsics` - orange
- comments - gray
