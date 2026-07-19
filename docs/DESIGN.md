# jim compiler — design & contracts

This document records how `jimc` (the jim compiler) is built and, more importantly,
the **contracts** between the compiler and the jim standard library. The language
specification itself lives in [jim.txt](../jim.txt). The standard library
(`/std/*.j`) is written in jim by Ben; the compiler only provides the scaffolding
(intrinsics, desugaring, code generation) described here.

---

## 1. Division of labor

**The compiler owns (unavoidable magic):**
- Memory representation of built-in types (i64, f64, byte for Char, `{ptr, len}` for String, raw buffers)
- Literal syntax: `24` → Integer, `3.14` → Float, `"..."` → String, `'c'` → Char,
  `true`/`false` → Bool, `[a, b, c]` → Array/Vector (context-typed from the annotation)
- `and`, `or`, `not` — short-circuit evaluation cannot be a method call
- `=` assignment, `if`/`while`/`for` consuming Bool, `&`/`*` pointer operations
- Optional presence tests: `x == None` / `x != None` on a `T?` check the presence tag
- Numeric widening and optional wrap/unwrap (§4)
- Raising and unwinding (`@panic` → nearest `try`); exception delivery to `catch`

**The standard library owns (written in jim over intrinsics):**
- The classes `Integer, Float, Bool, Char, String, Exception, Array<T>, Vector<T>`
  (`None` is a pure unit type — there is nothing to implement for it)
- **All operator behavior** via desugared method calls (§3)
- All named methods (`length()`, `push()`, `to_string()`, `msg()`, ...)
- Bounds checking, growth logic, formatting — safety logic lives in jim, not in C
- `<io>`, `<math>`, and any other library modules (plain jim functions)

## 2. Architecture

```
file.j ──lexer──► tokens ──parser──► AST ──sema──► typed AST ──lower──► desugared AST ──codegen──► C ──gcc──► native exe
```

- **Host language:** Rust (zero external crates)
- **Backend:** transpile to a single C11 translation unit (runtime embedded), compiled
  by the first C compiler found: `$JIM_CC` → `gcc` → `cc` → `clang` → `zig cc`
- **CLI:** `jimc build file.j [-o out.exe]`, `jimc run file.j`, `--emit-c <path>`,
  `--std <dir>`, `--allow-intrinsics` (for compiler test fixtures only)
- Names are mangled `jim_user_<name>` in C so jim identifiers can never collide with libc.

## 3. Operator desugaring contract

Every operator (except the native ones in §1) is rewritten by the compiler into a
method call on the **left operand**. These method names are therefore **reserved**:
implementing them in a class is what makes the operator work for that type.

| jim source        | desugars to                     |
|-------------------|---------------------------------|
| `a + b`           | `a.plus(b)`                     |
| `a - b`           | `a.minus(b)`                    |
| `a * b`           | `a.times(b)`                    |
| `a / b`           | `a.divide(b)`                   |
| `a div b`         | `a.int_divide(b)`               |
| `a % b`           | `a.mod(b)`                      |
| `-a` (unary)      | `a.negate()`                    |
| `a == b`          | `a.equals(b)`                   |
| `a != b`          | `not a.equals(b)`               |
| `a < b`           | `a.less_than(b)`                |
| `a > b`           | `b.less_than(a)`                |
| `a <= b`          | `not b.less_than(a)`            |
| `a >= b`          | `not a.less_than(b)`            |
| `a += b`          | `a = a.plus(b)` (likewise `-=` `*=` `/=`) |
| `a++` / `a--`     | `a = a.plus(1)` / `a = a.minus(1)` |
| `a[i]`            | `a.get(i)`                      |
| `a[i] = v`        | `a.set(i, v)`                   |
| `a[i] += v`       | `a.set(i, a.get(i).plus(v))`    |

Consequences worth knowing:
- Operator-backing methods must be **public** — a private `plus` makes `+`
  fail everywhere outside the class.
- A comparable class implements exactly **two** methods (`equals`, `less_than`);
  all six comparison operators derive from them and can never disagree.
  Both must return `Bool` (enforced).
- `a > b` compiles as `b.less_than(a)` — with side-effecting operands, evaluation
  order follows C argument rules (currently unspecified; to be pinned left-to-right
  in a later milestone).
- `7 / 2 = 3.5` (per spec) means `Integer.divide(other: Integer) -> Float`.
  Therefore `x /= 2` on an `x: Integer` is a **compile error** (it would change x's type).
- `String + String` works because `String` implements `plus`.
- `Integer.divide` needs no division intrinsic of its own:
  `return @f64_div(@i64_to_f64(this), @i64_to_f64(other));`

## 4. Implicit coercions (exactly three)

1. **Numeric widening.** If a binary operation mixes `Integer` and `Float`, the
   compiler inserts `.to_float()` on the Integer side **before** desugaring:
   `3 + 2.5` → `(3.to_float()).plus(2.5)`. `to_float()` is therefore a reserved
   name on `Integer`. `var f: Float = 3;` is still a compile error (write `3.0`),
   and Float→Integer is always an explicit `.to_integer()`.
2. **Wrap `T -> T?`** — always safe. `return 42;` from a `-> Integer?` function,
   `var a: Integer? = 5;`, passing an Integer to an `Integer?` parameter.
3. **Unwrap `T? -> T`** — **runtime-checked**: the compiler inserts an
   unwrap-or-panic wherever a `T?` is used as a value (operator operand, method
   receiver or argument, condition, assignment into a `T`). If it's None at
   runtime: `jim panic: used a None value where Integer was needed` — exactly
   the spec's `a + 1` example, catchable with `try`/`catch`.

Optional semantics: `x == None` / `x != None` are native presence tests
(never method calls); `None` takes its type from context (declaration,
assignment, return, function or method argument) and is an error where no
optional is expected; optional-returning functions may fall off the end
(implicit `return None`); nested optionals (`T??`) are rejected, as are
`Exception?` and `RawBuffer<T>?`. Representation: core values use a tagged
C struct `{ bool has; T value; }`; classes, containers, and pointers use a
nullable pointer.

## 5. Value classes vs reference classes

Declared as classes, but special-cased **by name** in the compiler:

| kind        | types                                    | semantics                                   |
|-------------|------------------------------------------|---------------------------------------------|
| value       | `Integer, Float, Char, Bool, String, Exception` | copied on assignment; **methods only — no fields, no constructor**; inside their class, `this` **is** the value and usable as an expression (e.g. `@i64_add(this, other)`) |
| reference   | user classes, `Array<T>`, `Vector<T>`    | arena-allocated; assignment copies the reference; may have fields and one constructor |

Notes for std authoring:
- `String` and `Exception` are **value** classes despite being heap-backed —
  their C representation is an immutable `{ptr, len}` pair copied by value.
  That is why `String` cannot have fields and why concatenation is the
  `@str_concat` intrinsic rather than buffer manipulation.
- `None` is a pure unit type — no class to write.
- `Char` is a single byte (0–255); char literals must be ASCII or an escape;
  Unicode text lives in `String` (UTF-8 bytes, `@str_len` counts bytes).
- Reference-class rules (M7): field defaults are mandatory and cannot use
  `this`; at most one constructor (auto default otherwise); members are
  accessed only via `this.` inside the class; `const` variables prevent
  rebinding, not field mutation.

## 6. Intrinsics (v0)

`@name(args)` is only legal inside files imported from the std root (`/std`), or
when compiling with `--allow-intrinsics` (compiler fixtures). The initial set —
grows as the stdlib needs it, shrinks never:

| intrinsic | signature | notes |
|---|---|---|
| `@i64_add` `@i64_sub` `@i64_mul` | `(Integer, Integer) -> Integer` | wrapping is a panic (overflow-checked) |
| `@i64_divtrunc` `@i64_mod` | `(Integer, Integer) -> Integer` | **panics** on zero divisor |
| `@i64_neg` | `(Integer) -> Integer` | |
| `@i64_eq` `@i64_lt` | `(Integer, Integer) -> Bool` | |
| `@i64_to_f64` | `(Integer) -> Float` | |
| `@i64_to_string` | `(Integer) -> String` | |
| `@i64_to_char` | `(Integer) -> Char` | panics unless 0–255 (Char is a byte) |
| `@f64_add` `@f64_sub` `@f64_mul` `@f64_div` | `(Float, Float) -> Float` | float div-by-zero follows IEEE (inf/nan) |
| `@f64_neg` | `(Float) -> Float` | |
| `@f64_eq` `@f64_lt` | `(Float, Float) -> Bool` | |
| `@f64_to_i64` | `(Float) -> Integer` | truncates toward zero |
| `@f64_to_string` | `(Float) -> String` | |
| `@bool_eq` | `(Bool, Bool) -> Bool` | |
| `@char_eq` `@char_lt` | `(Char, Char) -> Bool` | |
| `@char_to_i64` | `(Char) -> Integer` | byte value 0–255 |
| `@char_to_string` | `(Char) -> String` | one-byte String |
| `@str_len` | `(String) -> Integer` | length in bytes (UTF-8) |
| `@str_byte` | `(String, Integer) -> Char` | **unchecked** byte read — String.get owns the bounds check |
| `@str_concat` | `(String, String) -> String` | permanent — String is a value class with no fields, so building strings stays intrinsic |
| `@str_eq` `@str_lt` | `(String, String) -> Bool` | byte-wise / lexicographic |
| `@str_slice` | `(String, Integer, Integer) -> String` | `(s, start, len)` — **zero-copy** view into the original bytes (safe: strings are immutable). **Unchecked** — the std owns the bounds checks |
| `@str_from_buf` | `(RawBuffer<Char>, Integer) -> String` | `(buf, len)` — copies `len` bytes out into a fresh string; the string-builder finish |
| `@str_to_i64` | `(String) -> Integer?` | strict decimal parse (optional sign + digits); None if invalid or overflowing |
| `@str_to_f64` | `(String) -> Float?` | strict decimal/scientific parse (`"2.5"`, `"-1e9"`); None if invalid |
| `@f64_sqrt` `@f64_cbrt` `@f64_exp` `@f64_log` `@f64_log2` `@f64_log10` | `(Float) -> Float` | libm; IEEE-permissive — domain errors yield nan/±inf (None-vs-panic policy belongs in jim code) |
| `@f64_sin` `@f64_cos` `@f64_tan` `@f64_asin` `@f64_acos` `@f64_atan` | `(Float) -> Float` | radians; asin/acos give nan outside `[-1, 1]` |
| `@f64_atan2` | `(Float, Float) -> Float` | `(y, x)` — note the order |
| `@f64_hypot` | `(Float, Float) -> Float` | `sqrt(x² + y²)` without intermediate overflow |
| `@f64_fmod` | `(Float, Float) -> Float` | remainder of `x / y`, sign of `x` |
| `@f64_pow` | `(Float, Float) -> Float` | `x^y` |
| `@f64_is_nan` `@f64_is_inf` `@f64_is_finite` | `(Float) -> Bool` | IEEE state tests |
| `@i64_and` `@i64_or` `@i64_xor` | `(Integer, Integer) -> Integer` | bitwise |
| `@i64_not` | `(Integer) -> Integer` | bitwise complement |
| `@i64_shl` `@i64_shr` | `(Integer, Integer) -> Integer` | **panics** unless the shift amount is in 0–63; `shl` drops overflow bits, `shr` is arithmetic (sign-preserving) |
| `@print_string` | `(String) -> None` | writes to stdout + newline |
| `@print_err` | `(String) -> None` | writes to stderr + newline |
| `@read_line` | `() -> String?` | one line from stdin without the newline (CRLF handled); None at EOF |
| `@read_file` | `(String) -> String?` | whole file as one string; None if unreadable |
| `@write_file` `@append_file` | `(String, String) -> Integer?` | `(path, content)` — bytes written; None on failure; write replaces, append extends; both create the file |
| `@file_exists` | `(String) -> Bool` | |
| `@panic` | `(String) -> None` | raises: caught by the nearest `try`, else prints to stderr **with its file:line and function** and exits 1 |
| `@exc_msg` | `(Exception) -> String` | the message of a caught exception |
| `@buf_alloc` | `(Integer) -> RawBuffer<T>` | context-typed; std-only (see §6a) |

**`RawBuffer<T>`** (shipped with M5): a compiler-provided raw-storage value type
for the std library — `@buf_alloc(n)` creates one (std-only, element type taken
from context: `var b: RawBuffer<T> = @buf_alloc(n);`), with **unchecked**
`get(i)`, `set(i, v)`, and `capacity()`. Bounds checks and growth logic belong
in the jim code built on top of it. `RawBuffer` cannot be optional or hold
pointers; new `Array(len)` storage is uninitialized until `set` (literals and
argv always fill every slot).

The 2026-07-19 batch (float math, bit ops, parsing, `@str_slice`/`@str_from_buf`,
console/file I/O) closes every "Intrinsics needed" marker in the std. Note the
two string primitives exist for performance: `@str_slice` makes `substr` (and
everything built on it) O(1) instead of quadratic, and `@str_from_buf` is the
fill-a-buffer-then-finish pattern that makes `upper`/`lower`/`split` O(n).

## 6b. Method inventory — what each std class must provide

"Required" means a language feature desugars to it; leaving it out is not an
error by itself, but any program using the feature gets a compile error naming
the missing method. All of these must be `public`. Signatures are exact.

| class | required (feature → method) | conventional |
|---|---|---|
| `Integer` | `+ - * / div %` → `plus/minus/times/divide/int_divide/mod(other: Integer)` (note `divide -> Float` per spec); unary `-` → `negate() -> Integer`; `== <` etc. → `equals/less_than(other: Integer) -> Bool`; **widening** → `to_float() -> Float` | `to_string() -> String` |
| `Float` | `plus/minus/times/divide(other: Float) -> Float`; `negate() -> Float`; `equals/less_than(other: Float) -> Bool` | `to_integer() -> Integer`, `to_string() -> String` |
| `Bool` | `equals(other: Bool) -> Bool` (for `==`/`!=`) | `to_string() -> String` |
| `Char` | `equals/less_than(other: Char) -> Bool` | `to_string() -> String`, `to_integer() -> Integer` |
| `String` | `+` → `plus(other: String) -> String`; `equals/less_than(other: String) -> Bool`; indexing `s[i]` → `get(i: Integer) -> Char` over `@str_byte` | `length() -> Integer` |
| `Exception` | *(nothing — the runtime delivers it)* | `msg() -> String` via `@exc_msg(this)` |
| `Array<T>` | literals & argv → constructor `Array(len: Integer)` and `set(i: Integer, value: T) -> None`; indexing & `for..in` → `get(i: Integer) -> T`, `length() -> Integer` | — |
| `Vector<T>` | literals → a no-argument constructor (the auto default qualifies) and `push(value: T) -> None`; indexing & `for..in` → `get/set/length` as Array | `pop() -> T` |
| `<io>` (module) | — | `function print(s: String) -> None` over `@print_string` |

To raise an error from std code: `@panic("message")` — it unwinds to the
nearest `try` or aborts the program. User code has no `throw`; if you want
users to raise, export a std function that wraps `@panic`.

## 6a. Container & iteration protocols (M5)

Generics exist only as the std `class Array<T>` / `class Vector<T>` (single
type parameter, monomorphized per instantiation — no user generics). The
compiler builds three features through **protocols your classes must satisfy**:

- **Literals.** `[a, b, c]` takes its type from context. For `Array<T>` it
  compiles to `Array(3)` + `set(i, v)` per element — so Array needs a
  constructor `Array(len: Integer)` and `set(i: Integer, value: T)`. For
  `Vector<T>` it compiles to `Vector()` + `push(v)` — so Vector needs a
  no-argument constructor and `push(value: T)`.
- **Sized construction.** `var a: Array<Integer> = Array(10);` — a call named
  like the expected container's base constructs it, with the type argument
  taken from context (declaration, parameter, or return type). Same for
  `Vector()`. `Array(n)` slots are uninitialized until `set`.
- **Indexing.** `a[i]` → `a.get(i)`; `a[i] = v` → `a.set(i, v)`;
  `a[i] += v` → `a.set(i, a.get(i).plus(v))` (compound needs a simple receiver
  and index, since both evaluate twice).
- **`for (x: T in c)`** desugars to an index loop over `c.length()` and
  `c.get(i)` — any class implementing those two methods is iterable, not just
  the containers.
- **`main(argv: Array<String>)`** — the compiler builds argv through the same
  Array constructor + `set` protocol at startup.

## 6c. Generic functions (M10)

Free functions can be generic — declared like the container templates, with
one or more type parameters:

```jim
// works on Array<T>, Vector<T>, String — anything with length() and get()
function max<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) { best = seq[i]; }
    }
    return best;
}
```

They are **templates, monomorphized per call** (the C++/Rust model, same
machinery as `Array<T>`): each distinct binding stamps a concrete copy named
by its key (`max<Vector<Float>,Float>`), cached and emitted as a plain C
function. Inside a stamped copy every call is direct and inlinable — **zero
runtime cost**, no vtables, no boxing.

**How type parameters bind** (in order; first source that binds a parameter wins):

1. **Explicit type arguments**: `max<Array<Integer>, Integer>(a)` — all
   parameters, in declared order. This is the escape hatch when nothing below
   applies (e.g. chaining: `max<...>(a).to_string()`).
2. **Argument types**, by structural unification: parameter `seq: C` against
   an argument of type `Vector<Float>` binds `C`; a parameter `seq: Array<T>`
   binds `T` directly.
3. **The expected type** at the call site, unified with the declared return
   type — the same context that types literals and `None`:
   `var m: Float = max(v);` binds `T = Float`. Works in `var` initializers,
   assignments, `return`, and argument positions.

Anything still unbound is a compile error naming the parameter.

**Checking model.** Bodies are duck-typed per instantiation, like the
container templates: the body must make sense for the actual bindings, and a
violation errors at instantiation with a `(in the instantiation 'max<...>')`
note. Signatures are typo-checked up front, but a generic function that is
**never called has an unchecked body** (`jimc check` cannot probe an
unconstrained `C` — unlike class templates, which are probed with `Integer`).

**Rules and limits (v1):**

- Type parameter names must not shadow real types (`function f<Integer>(...)`
  is an error). `main` cannot be generic. No overloading, as everywhere.
- Generic **methods** on classes are not supported — free functions only.
  User generic *classes* remain unsupported (std `Array`/`Vector` only).
- No constraint syntax yet (`<C: Sequence>` is future work); constraints are
  implicit in what the body does.
- A generic function can call other generic functions, including with
  explicit arguments built from its own parameters (`pick<C, T>(seq, i)`).
  Runaway chains (`f<T>` calling `f<Array<T>>`) stop at 1000 instantiations.
- Construction inside a generic body works through context-typing after
  substitution: `var out: Array<T> = Array(n);` is fine. There is no way to
  construct "a `C`" generically — functions that build a result should return
  a concrete container (conventionally `Array<T>`).
- **Parsing ambiguity note:** `f(a < b, c > (v))` parses as the generic call
  `f(a<b, c>(v))` when the angle brackets read as a well-formed type list
  followed by `(`. If you meant two comparisons, parenthesize:
  `f((a < b), (c > (v)))`. In practice this only bites expressions comparing
  against a parenthesized value inside an argument list.

## 7. Modules & prelude

- `#import <name>` resolves to `<std root>/name.j`, falling back to
  `<std root>/core/name.j` — user-facing libraries live at the std root, and
  the prelude's per-datatype files live in `std/core/` (core.j is an
  import-only manifest of them). `#import "path.j"` resolves relative to the
  importing file. Imports are idempotent (visited-set on canonical paths).
- Std root: `--std <dir>` flag → `JIM_STD` env var → `std/` found near the
  compiler binary or the current directory. Working in this repo, plain
  `jimc run program.j` from the repo root picks up `std/` automatically.
- Once `std/core.j` exists, it is **auto-imported into every program** (the prelude).
  `<io>`, `<math>` etc. remain explicit imports.
- Intrinsics (`@...`) are legal in any file under the std root — core.j, io.j,
  math.j alike — and nowhere else.

## 8. Memory model

Arena allocation, exactly as the spec promises: all heap allocations come from a
bump arena owned by the runtime, freed in one sweep at program exit. No `free()`,
no use-after-free, no double-free.

**Pointer rules (M6, all enforced):** no pointer arithmetic; no null pointers —
`*T?` is a maybe-pointer (nullable at the C level, panics on None use); `&x`
only on non-const variables; no `**T`. Dangling is prevented by construction:
functions/methods **cannot return pointers**, fields **cannot hold pointers**,
containers/buffers **of pointers are rejected**, and `p = &y` is rejected when
`y` lives in an inner scope relative to `p`. Known soundness gap (accepted for
v1, revisit in M9): pointer-to-pointer-variable aliasing (`p = q`) is not
scope-tracked; the blocked channels above make this hard to abuse.

## 8a. Panics & stack traces

Two build profiles, cargo-style:

- **`jimc run`** (and `jimc build --debug`): every panic — `@panic` from std
  code *and* the built-in runtime panics (overflow, division by zero, None
  misuse) — prints a full jim stack trace with a file:line per frame:

  ```
  jim panic: Array index out of bounds
  stack trace (most recent call first):
    at Array<Integer>.get (std/core/array.j:20)
    at inner (program.j:2)
    at main (program.j:15)
  ```

  Mechanism: a shadow stack — a frame push/pop per jim function plus one line
  store per call site. Cost: a few stores per call; fine for development,
  wrong for benchmarks.
- **`jimc build`** (and `jimc run --release`): none of that instrumentation
  is emitted — zero runtime cost. Uncaught `@panic` still shows its exact
  file:line and function (baked in at compile time, free); the built-in
  runtime panics print only their message.

Caught panics are identical in both modes: the exception carries the bare
message, `try`/`catch` restores the shadow stack to its depth at try entry.
The trace records at most 256 frames (deeper recursion is counted, not stored).

## 9. Milestones

| # | scope | status |
|---|---|---|
| 1 | lexer, parser, hello world end-to-end (functions, literals, intrinsic calls, `return`, module imports, `if`/`while`, definite-return analysis) | **done 2026-07-18** |
| 2 | operator desugaring + widening, core classes incl. `String` (methods, `this`, public/private), C-style `for` + `break`/`continue` (pulled forward from M3), `core.j` prelude auto-import → **`std/core.j`, `io.j`, `math.j` now writable** | **done 2026-07-18** |
| 3 | *(folded into 2 and 5)* `for..in` ships with Array/Vector | — |
| 4 | optionals (`T?`, `None` checks, runtime panics) | **done 2026-07-18** |
| 5 | `Array`/`Vector` + generics (monomorphization) + `RawBuffer<T>` + `for..in` + indexing + `main(argv)` | **done 2026-07-18** |
| 6 | pointers & references with safety rules | **done 2026-07-18** |
| 7 | user classes: fields, constructors, `this.` enforcement *(pulled before M5 — fields are a prerequisite for jim-implemented containers)* | **done 2026-07-18** |
| 8 | `try`/`catch` exceptions (panics become catchable) | **done 2026-07-18** |
| 9 | polish: pinned evaluation order, `return` inside try, full pointer escape analysis, cross-compilation | *editor tooling done: VS Code extension in `editors/vscode-jim/` (grammar + "Jim Monokai" theme)* |
| 10 | monomorphized generic functions (`function max<C, T>(seq: C) -> T`, §6c) | **done 2026-07-19** |

## 10. Decisions log

- **2026-07-19** (Ben) **Generic functions over inheritance/overloading** for
  container-polymorphic std functions (`max`, `min`, `reversed`, ...). A
  common base class was rejected (vtables = indirect calls, no inlining, per-
  object header — against the performance goal); type-based overloading was
  rejected (ambiguity with implicit coercions and context-typing; "one name,
  one meaning"). Instead: monomorphized generic functions over the existing
  structural protocol (§6c) — zero runtime cost, same machinery as `Array<T>`.
- **2026-07-18** Context-typed container construction added:
  `var a: Array<Integer> = Array(10);` (Ben hit the gap — literals were the
  only way to create containers, so sized-but-empty arrays were impossible).
  Consistent with how literals and `@buf_alloc` take their types from context.
- **2026-07-18** (Ben) **String indexing is back in** — supersedes the earlier
  "out" ruling. `s[i]` works like any indexing: it desugars to `String.get(i)`,
  which the std implements over the new `@str_byte` intrinsic (unchecked;
  bounds panic lives in jim). `s[i]` yields the **byte** at `i` (Char is a
  byte; multi-byte UTF-8 text indexes per byte, not per glyph).
- **2026-07-18** M8 shipped: `try { } catch (e: Exception) { }` via a
  setjmp/longjmp handler stack — **every** panic is catchable (None misuse,
  div-by-zero, overflow, and `@panic` calls from std code, e.g. bounds checks).
  `Exception` is a std value class whose representation is its message;
  `msg()` goes through `@exc_msg`. There is deliberately no `throw` in user
  code (the spec has none) — raising is std territory via `@panic`.
  V1 restrictions, lifted in M9: `return`/`break`/`continue` may not jump out
  of a try block; locals mutated inside `try` and read in `catch` may be stale
  under optimization (classic setjmp caveat — avoid that pattern for now).
- **2026-07-18** M6 shipped: pointers per §8. `*p += v` desugars through the
  operator machinery like any target; optional pointers share the
  nullable-pointer representation with class references.
- **2026-07-18** M5 shipped: monomorphized generics (std Array/Vector only),
  RawBuffer<T> + `@buf_alloc`, container/iteration protocols per §6a, indexing
  desugar, `for..in`, `main(argv)`. Generated C uses GNU statement expressions
  for literals (gcc/clang/zig only — fine, they're the supported backends).
  Lexer now skips a UTF-8 BOM (Windows editors add them). `x++` is now
  literally `x += 1` internally, so it works on variables, fields, and indexes.
- **2026-07-18** M7 shipped (before M5 — `Array`/`Vector` need fields). Semantics
  fixed: field defaults are **mandatory** (`private w: Integer = 1;`), so instances
  are always fully initialized; at most **one constructor** (auto default when
  absent); instantiation is `ClassName(args)`; instances are arena-allocated
  **references** (assignment shares the object); `const` on a variable prevents
  rebinding, not field mutation (shallow const); optional references (`Shape?`)
  compile to nullable pointers; bare member names are rejected with a hint —
  the spec's "only via `this.`" rule; compound field assignment (`o.x += v`)
  requires a simple receiver (`this` or a variable) since the receiver evaluates
  twice; field defaults cannot reference `this`.
- **2026-07-18** M4 shipped: optionals per §4 — context-typed `None`, native
  presence tests, implicit wrap/unwrap with runtime panic on None misuse,
  implicit None on fall-off. Ben is building the full compiler before writing
  any jim code himself.
- **2026-07-18** M2 shipped: desugaring per §3, Integer→Float widening, core classes
  (Integer/Float/Bool/Char/String) with public/private and `this`, C-style `for` +
  `break`/`continue` pulled forward, prelude auto-import of `std/core.j`.
  Compiler fixtures live in `tests/fake_std/` (test doubles, not the real library).
- **2026-07-18** Operators desugar to methods (Ben's call — maximum stdlib control). Compiler-magic fallback rejected.
- **2026-07-18** Host = Rust (zero deps); backend = C via system `gcc` (present on this machine); `zig cc` optional later for cross-compiling.
- **2026-07-18** Comparison operators derive from `equals` + `less_than` only.
- **2026-07-18** Integer div/mod by zero panics; float division follows IEEE 754.
- **2026-07-18** `++`/`--` are statements only (no `x = i++`).
- **2026-07-18** `main` may omit `return` (implicit 0); all other non-None functions must return on every path.
- **2026-07-18** (Ben) `print` takes a String only — callers write `print(number.to_string())`.
  The spec's `print(number)` example changes accordingly.
- **2026-07-18** (Ben) Instantiation is a constructor call by class name: `var s: Shape = Shape(1, 2);`.
- **2026-07-18** (Ben) String indexing (`s[0]`) is **out** — no `get` on String.
- **2026-07-18** (Ben) `Char` is a **byte** (0–255), not a Unicode scalar. Char literals must be ASCII or an escape.
- Earlier rounds: `div` keyword replaces `//` (comment collision); Arrays fixed / Vectors growable; mandatory `->` on functions; `name: Type` declaration style; exceptions deferred.