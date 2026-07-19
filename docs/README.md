# jim standard library

The std/ directory is **Ben's** — every `.j` file here is written in jim, by hand.
The compiler never generates or owns code in this directory. The full contracts
live in [docs/DESIGN.md](../docs/DESIGN.md) (desugaring §3, coercions §4,
value/reference classes §5, intrinsics §6, method inventory §6b, protocols §6a).

## Getting started

```
# build the compiler once
cargo build --release --manifest-path compiler/Cargo.toml

# write std/core.j, then from the repo root:
compiler/target/release/jimc.exe run yourtest.j
```

`std/` is found automatically from the repo root; `core.j` is auto-imported
into every program the moment the file exists. `#import <io>` loads `std/io.j`.
Intrinsics (`@...`) work in any file in the std/ directory and nowhere else.
Working examples of everything below exist as compiler test doubles in
[tests/fake_std/](../tests/fake_std/) — peek or ignore, your call.

## The rules of the game

1. **Value classes** (`Integer, Float, Bool, Char, String, Exception`):
   methods only — no fields, no constructor. `this` is the value itself.
   **Reference classes** (`Array<T>`, `Vector<T>`): fields (defaults
   mandatory, no `this` in them), one constructor, members via `this.` only.
2. Operator-backing methods must be **public**, and `equals`/`less_than` must
   return `Bool`. The six comparisons derive from those two alone.
3. Raise errors with `@panic("message")` — catchable by `try`/`catch`.
   Bounds checks belong in your `get`/`set`, not in the compiler.
4. Methods take no `function` keyword (`public plus(...) -> ...`);
   free functions do (`function print(...) -> ...`).
5. Need an intrinsic that doesn't exist (float math, input, ...)? Ask —
   the set grows on demand. Current gaps: no `floor`/`sqrt`/trig, no input.

## core.j — the required surface

```jim
class Integer {
    public plus(other: Integer) -> Integer { return @i64_add(this, other); }
    public minus(other: Integer) -> Integer { /* @i64_sub */ }
    public times(other: Integer) -> Integer { /* @i64_mul */ }
    public divide(other: Integer) -> Float { /* spec: 7 / 2 = 3.5 — go through @i64_to_f64 + @f64_div */ }
    public int_divide(other: Integer) -> Integer { /* @i64_divtrunc */ }
    public mod(other: Integer) -> Integer { /* @i64_mod */ }
    public negate() -> Integer { /* @i64_neg */ }
    public equals(other: Integer) -> Bool { /* @i64_eq */ }
    public less_than(other: Integer) -> Bool { /* @i64_lt */ }
    public to_float() -> Float { /* @i64_to_f64 — REQUIRED: mixed Integer/Float arithmetic inserts this */ }
    public to_string() -> String { /* @i64_to_string */ }
}

class Float {
    // plus/minus/times/divide(other: Float) -> Float   (@f64_*)
    // negate() -> Float, equals/less_than(other: Float) -> Bool
    // to_integer() -> Integer (@f64_to_i64), to_string() -> String
}

class Bool {
    // equals(other: Bool) -> Bool (@bool_eq) — backs == and !=
}

class Char {
    // equals/less_than(other: Char) -> Bool (@char_eq/@char_lt)
    // to_string() -> String (@char_to_string), to_integer() -> Integer (@char_to_i64)
}

class String {
    // plus(other: String) -> String (@str_concat) — backs "a" + "b"
    // equals/less_than(other: String) -> Bool (@str_eq/@str_lt)
    // length() -> Integer (@str_len, bytes)
}

class Exception {
    public msg() -> String { return @exc_msg(this); }
}

class Array<T> {
    // fields: a RawBuffer<T> + a count (defaults mandatory, e.g. = @buf_alloc(0) and = 0)
    // Array(len: Integer)                 <- REQUIRED: literals [a,b,c] and main(argv) call this
    // set(i: Integer, value: T) -> None   <- REQUIRED: literals, argv, a[i] = v
    // get(i: Integer) -> T                <- REQUIRED: a[i], for..in
    // length() -> Integer                 <- REQUIRED: for..in
    // bounds-check get/set yourself: @panic("Array index out of bounds")
}

class Vector<T> {
    // fields: RawBuffer<T> + count
    // Vector()                            <- literals call this; omitting the
    //                                        constructor also works (the auto
    //                                        default takes no arguments)
    // push(value: T) -> None              <- REQUIRED: literals; grow with a bigger @buf_alloc + copy loop
    // get/set/length                      <- as Array (indexing + for..in)
    // pop() -> T                          <- conventional
    // this.buf.capacity() tells you when to grow
}
```

`None` needs no class — it is a pure unit type.

## io.j

```jim
function print(s: String) -> None {
    @print_string(s);
}
// print takes String only (Ben's ruling): callers write print(n.to_string())
```

## math.j

Plain jim functions — `abs`, integer `pow`, `min`/`max` are all writable today
with operators and loops. Anything needing hardware math (`sqrt`, `floor`,
trig) needs a new intrinsic first — ask.
