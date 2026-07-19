# jim by example

Every feature of the language, with runnable code. All examples in this file
have been compiled and executed. Compile and run any of them from the repo
root:

```powershell
compiler\target\release\jimc.exe run example.j       # compile + run
compiler\target\release\jimc.exe build example.j -o example.exe
compiler\target\release\jimc.exe check example.j     # type-check only
compiler\target\release\jimc.exe run example.j --emit-c out.c   # inspect the C
```

In VS Code (with the jim extension): errors appear on save, `Ctrl+Space`
completes, hovering shows signatures and `//`-docstrings, and typing `(`
shows parameter help.

The contracts behind these features — desugaring, coercions, protocols — live
in [DESIGN.md](DESIGN.md). The language spec is [jim.txt](../jim.txt).

---

## Hello, world

```jim
#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
```

Every program needs `main` returning `Integer` (the process exit code).
`main` may omit its `return` — the process exits 0. A second form receives
command-line arguments:

```jim
#import <io>

function main(argv: Array<String>) -> Integer {
    print("arguments: " + argv.length().to_string());
    for (a: String in argv) {
        print(a); // argv[0] is the executable path
    }
    return 0;
}
```

## Comments and docstrings

```jim
// This is a comment
// There are no multiline comments — stack single-line ones.
```

A block of `//` comments directly above a function, class, method, or field
becomes its **docstring**: VS Code shows it on hover, in completions, and in
parameter help.

```jim
// Returns every prime below `limit` (Sieve of Eratosthenes).
function primes_below(limit: Integer) -> Vector<Integer> { ... }
```

## Variables, constants, and types

```jim
var name: String = "Ben";
var age: Integer = 24;      // 64-bit signed
var pi: Float = 3.14;       // 64-bit double
var c: Char = 'c';          // one byte (0-255); literals must be ASCII
var is_alive: Bool = true;  // true/false, NOT True/False
const day: Integer = 1;     // cannot be reassigned
```

Every variable needs a type annotation and an initializer. `var f: Float = 3;`
is an error — write `3.0` (Integer literals never silently become Floats).
Variables shadow outer scopes but cannot be redeclared in the same scope.

## Operators

```jim
var a: Integer = 7 + 3 * 2;     // arithmetic: + - * (and unary -)
var q: Float = 7 / 2;           // / is TRUE division: 3.5 (always Float!)
var w: Integer = 7 div 2;       // integer division: 3
var r: Integer = 7 % 2;         // modulo: 1
var big: Bool = a > 10 or (a >= 5 and not (a == 13));
var diff: Bool = a != r;
a += 1;                          // also -= *= /=
a++;                             // ++/-- are statements only (no x = i++)
var m: Float = 1 + 2.5;          // Integer widens to Float in mixed arithmetic
```

Two consequences of `7 / 2 = 3.5`: assigning `7 / 2` to an `Integer` is a
compile error, and `x /= 2` on an Integer variable is a compile error (the
result would be Float). Use `div` for integer math.

Under the hood every operator is a method call on the left operand
(`a + b` is `a.plus(b)` — see DESIGN.md §3). That has a powerful consequence:
**operators work on any class implementing the right methods** — see
[Operators on your own classes](#operators-on-your-own-classes).

## Conversions

```jim
var f: Float = 3.to_float();
var i: Integer = 3.9.to_integer();   // truncates toward zero: 3
var s: String = 42.to_string();
var n: Integer = 'A'.to_integer();   // byte value: 65
```

Explicit, via methods — the only implicit conversion is Integer→Float in
mixed arithmetic.

## Strings

```jim
var s: String = "jim";
s = s + "!";                          // concatenation builds a NEW string
var n: Integer = s.length();          // bytes (UTF-8)
if (s == "jim!") { }                  // value comparison
var first: Char = s[0];               // 'j' — indexing yields the byte at i
```

Strings are **immutable**: `s[0] = 'J'` does not exist (there is no
`String.set`). Every "modification" builds a new string, like Python.
Because `String` has `length()` and `get()`, strings are iterable:

```jim
var spread: String = "";
for (c: Char in "jim") {
    spread = spread + c.to_string() + ".";
}
// spread == "j.i.m."
```

Escapes in string and char literals: `\n \t \r \0 \\ \" \'`.

## Control flow

```jim
if (condition) {
    // ...
} else if (other_condition) {
    // ...
} else {
    // ...
}

while (count < 3) {
    count++;
}

for (i: Integer = 0; i < 10; i++) {
    if (i == 2) continue;   // single-statement bodies may omit braces
    if (i == 7) break;
}
```

Conditions must be `Bool` — there is no truthiness. Loop variables in a
`for` header omit the `var` keyword. `break`/`continue` work in `while` and
both `for` forms.

## Arrays and Vectors

Two container types: `Array<T>` is **fixed-length**, `Vector<T>` **grows**.

### Creating them

```jim
// literals — the type comes from the declaration
var nums: Array<Integer> = [1, 2, 3, 4, 5];
var v: Vector<Integer> = [1, 2, 3];
var empty: Array<Float> = [];

// sized construction — n slots, no elements listed
var a: Array<Integer> = Array(5);
for (i: Integer = 0; i < a.length(); i++) {
    a[i] = i * i;          // fill before reading!
}

var fresh: Vector<String> = Vector();   // same as = []
```

Both literals and `Array(n)`/`Vector()` construction take their element type
from context (the declared type, a parameter type, or a return type) — a bare
`[1, 2]` or `Array(5)` with nothing to type it is a compile error.

**Important:** `Array(n)` gives you n *uninitialized* slots — reading before
writing yields garbage. Fill it first (literals and `main(argv)` always
arrive fully filled).

### Using them

```jim
var first: Integer = nums[0];   // 0-indexed; out of bounds panics
nums[0] = 6;
nums[1] += 10;                  // compound index assignment

v.push(4);
var last: Integer = v.pop();
var len: Integer = v.length();

var total: Integer = 0;
for (x: Integer in v) {         // for..in over any container
    total += x;
}
```

### As parameters and results

```jim
function zeros(n: Integer) -> Array<Integer> {
    var a: Array<Integer> = Array(n);
    for (i: Integer = 0; i < n; i++) {
        a[i] = 0;
    }
    return a;
}

function sum(nums: Array<Integer>) -> Integer {
    var total: Integer = 0;
    for (n: Integer in nums) {
        total += n;
    }
    return total;
}
```

### Nesting

Containers nest freely — each combination is stamped out at compile time
(monomorphization):

```jim
var grid: Vector<Vector<Integer>> = [];
grid.push([1, 2, 3]);
grid.push([4, 5, 6]);
var cell: Integer = grid[1][2];        // 6

var flat: Integer = 0;
for (row: Vector<Integer> in grid) {
    for (x: Integer in row) {
        flat += x;
    }
}
```

## Functions

```jim
function get_age() -> Integer {
    return 25;
}

function greet(name: String, excited: Bool) -> String {
    if (excited) {
        return "HELLO " + name;
    }
    return "hello " + name;
}
```

The return type is mandatory (`-> None` for no value). A non-None function
must return on every path — the compiler checks. There is no function
overloading: one name, one signature.

## Generic functions

One definition that works for every sequence type — `Array<T>`, `Vector<T>`,
even `String` (anything with `length()` and `get(i)`):

```jim
function max<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) { best = seq[i]; }
    }
    return best;
}

function sum<C>(seq: C) -> Integer {
    var total: Integer = 0;
    for (x: Integer in seq) {
        total += x;
    }
    return total;
}

function main() -> Integer {
    var a: Array<Integer> = [3, 9, 4, 1];
    var v: Vector<Float> = [2.5, 7.25, 1.0];

    var m1: Integer = max(a);   // C = Array<Integer> (from the argument),
                                // T = Integer (from the annotation)
    var m2: Float = max(v);     // C = Vector<Float>, T = Float
    var c: Char = max("hello"); // strings are sequences of Char

    print(sum(a).to_string()); // 17 — nothing to infer beyond the argument

    // no context to infer T from? pass the type arguments explicitly:
    print(max<Array<Integer>, Integer>(a).to_string());

    return 0;
}
```

How the compiler fills in the type parameters, in order:

1. **explicit type arguments** — `max<Array<Integer>, Integer>(a)`;
2. **argument types** — `seq: C` against an `Array<Integer>` binds `C`;
3. **the expected type** — `var m: Float = max(v);` binds `T = Float`, the
   same context-typing that gives `[1, 2, 3]` and `None` their types.

If a parameter is still unknown after all three, the compiler asks you to
annotate the target or write the explicit form.

Each distinct combination stamps out its own copy at compile time
(monomorphization — exactly how `Array<T>` itself works), so a generic call
costs the same as a hand-written one: direct calls, fully inlinable, nothing
at runtime. The body is checked per instantiation: calling `max` on a
`Vector<Bool>` errors because `Bool` has no `less_than` — the message ends
with `(in the instantiation 'max<Vector<Bool>,Bool>')`.

Building a result inside a generic function works through the usual
context-typing; return a concrete container:

```jim
function reversed<C, T>(seq: C) -> Array<T> {
    var out: Array<T> = Array(seq.length());
    for (i: Integer = 0; i < seq.length(); i++) {
        out[seq.length() - 1 - i] = seq[i];
    }
    return out;
}
```

Limits to know about: free functions only (no generic methods on classes),
no constraint syntax (the body *is* the constraint, checked per
instantiation), and a generic function nobody calls has an unchecked body —
`jimc check` can't guess what `C` might be.

## Optionals

`T?` is "a T or None" — jim's alternative to null.

```jim
function may_return_nothing(flag: Bool) -> Integer? {
    if (flag) {
        return 42;      // an Integer wraps into Integer? automatically
    }
    return None;
}

function falls_off() -> Integer? {
    // an optional function may fall off the end: implicit `return None`
}

function main() -> Integer {
    var a: Integer? = may_return_nothing(true);
    if (a != None) {
        // there is no flow-typing: a is still Integer?, but using it
        // where an Integer is needed unwraps it automatically
        var b: Integer = a + 1;
    }

    var d: Integer? = None;   // None takes its type from context
    d = 7;                    // wraps
    d += 1;                   // unwrap, add, re-wrap
    return 0;
}
```

Using a `None` where a value is needed is a **runtime panic** (catchable with
`try`/`catch`): `jim panic: used a None value where Integer was needed`.
That's the model: `T?` marks the possibility, the runtime enforces it.
Works for any payload: `Integer?`, `String?`, `Shape?`, `*Integer?`,
`Vector<Integer>?`. Nested optionals (`T??`) don't exist.

A common pattern — optional as a search result:

```jim
function find(v: Vector<Integer>, wanted: Integer) -> Integer? {
    for (i: Integer = 0; i < v.length(); i++) {
        if (v[i] == wanted) {
            return i;
        }
    }
    return None;
}
```

## Classes

```jim
class Shape {

    private width: Integer = 1;    // field defaults are mandatory
    private height: Integer = 1;

    Shape(width: Integer, height: Integer) {   // at most one constructor;
        this.width = width;                    // omit it for an auto default.
        this.height = height;                  // members ONLY via this.
    }

    public area() -> Integer {
        return this.width * this.height;
    }

    public grow(by: Integer) -> None {
        this.width += by;
        this.height += by;
    }

    private clamp() -> None {          // private: callable only inside Shape
        if (this.width < 0) { this.width = 0; }
    }
}

class Point {
    public x: Integer = 0;   // public fields are readable/writable outside
    public y: Integer = 0;
}

function main() -> Integer {
    var s: Shape = Shape(3, 4);        // instantiation = class-name call
    s.grow(1);
    print(s.area().to_string());       // 20

    var p: Point = Point();            // auto default constructor
    p.x = 7;
    p.x++;

    var maybe: Shape? = None;          // optional references work
    maybe = Shape(2, 2);
    var area: Integer = maybe.area();  // auto-unwrap (panics if None)
    return 0;
}
```

**Reference semantics**: assignment shares the object.

```jim
var a: Counter = Counter();
var b: Counter = a;    // same object!
b.bump();              // a sees the change
```

`const s: Shape = ...` prevents rebinding `s`, not mutating its fields
(shallow const). No inheritance; composition is the tool.

### Operators on your own classes

Implementing the reserved method names (DESIGN.md §3) gives your class real
operators — `plus` backs `+`, `equals` backs `==`/`!=`, `less_than` backs
`<`/`>`/`<=`/`>=`, `get`/`set` back indexing:

```jim
class Vec2 {
    public x: Integer = 0;
    public y: Integer = 0;

    Vec2(x: Integer, y: Integer) {
        this.x = x;
        this.y = y;
    }

    public plus(other: Vec2) -> Vec2 {
        return Vec2(this.x + other.x, this.y + other.y);
    }

    public equals(other: Vec2) -> Bool {
        return this.x == other.x and this.y == other.y;
    }

    public to_string() -> String {
        return "(" + this.x.to_string() + ", " + this.y.to_string() + ")";
    }
}

function main() -> Integer {
    var c: Vec2 = Vec2(1, 2) + Vec2(3, 4);   // plus() via +
    if (c == Vec2(4, 6)) {                   // equals() via ==
        print(c.to_string());
    }
    return 0;
}
```

### Custom iterables

`for..in` works on **any** class with `length() -> Integer` and
`get(i: Integer) -> T` — not just containers:

```jim
class Range {
    private lo: Integer = 0;
    private hi: Integer = 0;

    Range(lo: Integer, hi: Integer) {
        this.lo = lo;
        this.hi = hi;
    }

    public length() -> Integer {
        return this.hi - this.lo;
    }

    public get(i: Integer) -> Integer {
        return this.lo + i;
    }
}

function main() -> Integer {
    var total: Integer = 0;
    for (n: Integer in Range(3, 7)) {   // 3, 4, 5, 6
        total += n;
    }
    return 0;                            // total == 18
}
```

## Pointers

```jim
function bump(p: *Integer) -> None {
    *p += 1;                 // write through the pointer
}

function main() -> Integer {
    var age: Integer = 24;
    var age_ptr: *Integer = &age;
    *age_ptr = 35;           // age is now 35
    bump(&age);              // pass by reference: age is now 36

    var maybe: *Integer? = None;   // optional pointer
    maybe = &age;
    *maybe += 1;                   // unwraps, then writes

    return 0;
}
```

Safety rules (all compile errors): no pointer arithmetic; `&` only on
non-const variables; no `**T`; functions cannot **return** pointers; fields
cannot **hold** pointers; no containers of pointers; and `p = &y` is rejected
when `y` lives in an inner scope (it would dangle). Memory is arena-allocated
and freed at program exit — there is no `free()`.

## Errors: try / catch

```jim
#import <io>

function definitely_returns_none() -> Integer? {
    return None;
}

function main() -> Integer {
    try {
        var a: Integer? = definitely_returns_none();
        var b: Integer = a + 1;        // panics: a is None...
    } catch (e: Exception) {
        print("Something went wrong: " + e.msg());   // ...and lands here
    }

    try {
        var v: Vector<Integer> = [1];
        var x: Integer = v[5];         // bounds panic from the std library
    } catch (e: Exception) {
        print("caught: " + e.msg());   // "Vector index out of bounds"
    }
    return 0;
}
```

Everything that panics is catchable: None misuse, division by zero, integer
overflow, out-of-bounds, and anything the std library raises with `@panic`.
An uncaught panic prints the message and exits with code 1. `try` blocks
nest; a panic unwinds to the innermost handler. Current restrictions:
`return`/`break`/`continue` may not jump out of a `try` block, and user code
has no `throw` (raising is the std library's job).

## Modules

```jim
#import <io>            // std library: resolves to std/io.j
#import <math>          // std/math.j
#import "geometry.j"    // your own file, relative to the importing file
```

A multi-file program:

```jim
// geometry.j
function square_area(side: Integer) -> Integer {
    return side * side;
}
```

```jim
// program.j
#import <io>
#import "geometry.j"

function main() -> Integer {
    print(square_area(6).to_string());   // 36
    return 0;
}
```

Imports are idempotent (a file loads once no matter how often it's
imported, even through diamond-shaped import graphs). All top-level
functions and classes share one global namespace. `std/core.j` is the
prelude: imported into every program automatically — that's where Integer,
String, Vector and friends come from.

## Complete program: FizzBuzz

```jim
#import <io>

function label(n: Integer) -> String? {
    if (n % 15 == 0) { return "FizzBuzz"; }
    if (n % 3 == 0) { return "Fizz"; }
    if (n % 5 == 0) { return "Buzz"; }
    return None;
}

function main() -> Integer {
    var results: Vector<String> = [];
    for (n: Integer = 1; n <= 15; n++) {
        var l: String? = label(n);
        if (l != None) {
            results.push(l);
        } else {
            results.push(n.to_string());
        }
    }
    var out: String = "";
    for (r: String in results) {
        out = out + r + " ";
    }
    print(out);
    return 0;
}
```

Output: `1 2 Fizz 4 Buzz Fizz 7 8 Fizz Buzz 11 Fizz 13 14 FizzBuzz`

## Complete program: prime sieve

Sized arrays, vectors, nested loops, `+=` step in a for-loop:

```jim
#import <io>

// Returns every prime below `limit` (Sieve of Eratosthenes).
function primes_below(limit: Integer) -> Vector<Integer> {
    var is_composite: Array<Bool> = Array(limit);
    for (i: Integer = 0; i < limit; i++) {
        is_composite[i] = false;
    }
    var found: Vector<Integer> = [];
    for (n: Integer = 2; n < limit; n++) {
        if (not is_composite[n]) {
            found.push(n);
            for (m: Integer = n * n; m < limit; m += n) {
                is_composite[m] = true;
            }
        }
    }
    return found;
}

function main() -> Integer {
    var primes: Vector<Integer> = primes_below(50);
    var out: String = "";
    for (p: Integer in primes) {
        out = out + p.to_string() + " ";
    }
    print(out);
    print("count = " + primes.length().to_string());
    return 0;
}
```

Output: `2 3 5 7 11 13 17 19 23 29 31 37 41 43 47` / `count = 15`

## Reading the compiler's errors

`jimc` reports one error at a time, with the file, position, offending line,
and a caret. A tour of common ones:

| you wrote | the compiler says |
|---|---|
| `var x: Integer = "hi";` | `type mismatch: 'x' is declared Integer but initialized with String` |
| `x /= 2;` on an Integer | `operator '/=': result is Float but 'x' is Integer (the variable's type cannot change)` |
| `day = 2;` on a const | `cannot assign to constant 'day'` |
| a path with no `return` | `function 'f' may reach the end without returning Integer (every path must return)` |
| `var x: Integer = None;` | `'None' only fits optional types (T?), but Integer is expected here` |
| `x == None` on plain Integer | `only optional values (T?) can be compared with None — this is Integer` |
| `width` instead of `this.width` | `unknown variable 'width' — member access must be written 'this.width'` |
| `s.hidden()` on a private method | `method 'hidden' of class 'S' is private` |
| `break;` outside a loop | `'break' outside a loop` |
| `var x: Integer = [1, 2];` | `a container literal needs a declared type for context, ...` |
| `@panic("x")` outside std | `'@panic' — intrinsics are only allowed in the standard library ...` |
| `+` on a class without `plus` | `operator '+' needs Shape.plus(), but class 'Shape' has no method 'plus'` |

## Quick gotcha list

- Statements end with `;`. Conditions must be `Bool`. No truthiness.
- `7 / 2` is `3.5` — use `div` for integer division; `x /= 2` on an Integer
  is a compile error.
- `print` takes a `String` — write `print(n.to_string())`.
- `true`/`false` are lowercase; `None` is capitalized.
- Member access inside a class is **only** `this.field` — bare names error.
- `Char` is one byte; char literals must be ASCII; `s[i]` is the byte at i.
- Strings are immutable — build new ones instead.
- `Array(n)` slots are uninitialized until you fill them.
- Classes are references (assignment shares); `const` is shallow.
- No `throw`, no user-defined generic classes, no function overloading,
  no inheritance.
- `++`/`--` are statements, not expressions.
- Compound assignment through an index or field needs a simple receiver
  (`v[i] += x`, `this.n += 1` — not `f()[g()] += x`).
