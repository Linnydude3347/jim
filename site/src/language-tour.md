# Language Tour

The whole language in a few pages. Every snippet is valid jim. For complete
runnable programs see [Examples](examples.md), or try them in the
[Playground](playground.md).

## Variables, constants, and types

```jim
var name: String = "Ben";
var age: Integer = 24;      // 64-bit signed
var pi: Float = 3.14;       // 64-bit double
var c: Char = 'c';          // one byte (0-255); ASCII literals
var is_alive: Bool = true;  // true / false, lowercase
const day: Integer = 1;     // cannot be reassigned
```

Every variable needs a type annotation and an initializer. `var f: Float = 3;`
is an error; write `3.0` (Integer literals never silently become Floats).
Variables shadow outer scopes but cannot be redeclared in the same scope.

## Conversions

```jim
var f: Float = 3.to_float();
var i: Integer = 3.9.to_integer();   // truncates toward zero: 3
var s: String = 42.to_string();
var n: Integer = 'A'.to_integer();   // byte value: 65
```

Conversions are explicit, via methods. The only implicit one is Integer to Float
in mixed arithmetic.

## Operators are method calls

This is the core of jim. `a + b` is sugar for `a.plus(b)`, and `a < b` for
`a.less_than(b)`. Comparisons derive from `equals` and `less_than` alone. Define
those methods on your own types and the operators work (see
[Classes](#classes)).

`and`, `or`, `not`, and `div` (integer division) are word operators. Note that
`7 / 2` is `3.5`; use `7 div 2` for integer division.

## Strings

```jim
var s: String = "jim";
s = s + "!";                 // concatenation builds a NEW string
var n: Integer = s.length(); // bytes (UTF-8)
if (s == "jim!") { }         // value comparison
var first: Char = s[0];      // 'j': indexing yields the byte at i
```

Strings are immutable. There is no `String.set`, so every modification builds a
new string. Because String has `length()` and `get()`, it is iterable. Escapes:
`\n \t \r \0 \\ \" \'`.

## Control flow

```jim
if (condition) {
    // ...
} else if (other) {
    // ...
} else {
    // ...
}

while (count < 3) { count++; }

for (var i: Integer = 0; i < 10; i++) {
    if (i == 2) continue;   // single-statement bodies may omit braces
    if (i == 7) break;
}
```

Conditions must be Bool; there is no truthiness. Loop variables in a `for` header
omit `var`. `++` and `--` are statements, not expressions.

## Functions

```jim
function greet(name: String, excited: Bool) -> String {
    if (excited) { return "HELLO " + name; }
    return "hello " + name;
}
```

The return type is mandatory (`-> None` for no value). A non-None function must
return on every path; the compiler checks this. There is no overloading: one
name, one signature. `main` returns an Integer (the exit code) and may omit its
`return`.

## Arrays and Vectors

`Array<T>` is fixed length; `Vector<T>` grows.

```jim
var nums: Array<Integer> = [1, 2, 3, 4, 5];   // literal
var v: Vector<Integer> = [1, 2, 3];
var a: Array<Integer> = Array(5);             // 5 slots, uninitialized; fill first

var first: Integer = nums[0];   // 0-indexed; out of bounds panics
nums[1] += 10;                  // compound index assignment
v.push(4);
var last: Integer = v.pop();

for (x: Integer in v) { /* for..in over any container */ }
```

Literals and `Array(n)` / `Vector()` take their element type from context (the
declared type, a parameter, or a return type). A bare `[1, 2]` with nothing to
type it is a compile error. Containers nest freely, for example
`Vector<Vector<Integer>>`, with each combination stamped out at compile time.

## Optionals

`T?` is "a T or None", jim's alternative to null.

```jim
function find(v: Vector<Integer>, wanted: Integer) -> Integer? {
    for (var i: Integer = 0; i < v.length(); i++) {
        if (v[i] == wanted) { return i; }   // Integer wraps into Integer?
    }
    return None;                            // None takes its type from context
}
```

There is no flow typing: a `T?` stays a `T?`. But using it where a `T` is
required unwraps it automatically, and panics (catchably) if it was None. Nested
optionals (`T??`) do not exist.

## Classes

```jim
class Shape {
    private width: Integer = 1;    // field defaults are mandatory
    private height: Integer = 1;

    Shape(width: Integer, height: Integer) {   // at most one constructor;
        this.width = width;                    // omit it for an auto default
        this.height = height;
    }

    public area() -> Integer {
        return this.width * this.height;        // members only via this.
    }
}
```

Instantiation is a class-name call: `Shape(3, 4)`. Members are accessed only
through `this.` inside methods; bare names are an error. Classes are references,
so assignment shares the object, and `const` prevents rebinding but not mutation
(it is shallow). There is no inheritance; composition is the tool.

### Operators on your own classes

Implementing reserved method names gives your class real operators: `plus` backs
`+`, `equals` backs `==` and `!=`, `less_than` backs `<` `>` `<=` `>=`, and
`get`/`set` back indexing. A class with `length()` and `get(i)` is also iterable
with `for..in`.

## Generic functions

One definition for every sequence type: Array, Vector, even String.

```jim
function largest<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (var i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) { best = seq[i]; }
    }
    return best;
}

var a: Array<Integer> = [3, 9, 4, 1];
var big: Integer = largest(a);   // T = Integer (expected type), C = Array<Integer>
```

Type parameters are filled in this order: explicit arguments
(`largest<Array<Integer>, Integer>(a)`), then the argument types, then the
expected type. Each distinct combination is monomorphized, so a generic call
costs the same as a hand-written one. Limits: free functions only (no generic
methods), and no constraint syntax (the body is the constraint, checked per
instantiation). The standard library ships `max`, `min`, and `sum` built this
way.

## Pointers

```jim
function bump(p: *Integer) -> None {
    *p += 1;                 // write through the pointer
}

function main() -> Integer {
    var age: Integer = 24;
    bump(&age);              // pass by reference: age is now 25
    return 0;
}
```

Safety rules, all compile errors: no pointer arithmetic; `&` only on non-const
variables; no `**T`; functions cannot return pointers; fields cannot hold
pointers; no containers of pointers; and `p = &y` is rejected when `y` lives in
an inner scope. Memory is arena allocated and freed at exit, so there is no
`free()`.

## Errors: try / catch

```jim
try {
    var v: Vector<Integer> = [1];
    var x: Integer = v[5];         // bounds panic from the std library
} catch (e: Exception) {
    print("caught: " + e.msg());   // "Vector index out of bounds"
}
```

Everything that panics is catchable: None misuse, division by zero, integer
overflow, out of bounds, and anything the std library raises. An uncaught panic
prints its message and exits with code 1. There is no `throw`; raising is the
standard library's job.

## Modules

```jim
#import <io>            // std library: resolves to std/io.j
#import <math>          // std/math.j
#import "geometry.j"    // your own file, relative to the importing file
```

Imports are idempotent: a file loads once no matter how often it is imported. All
top-level functions and classes share one global namespace. `std/core.j` is the
prelude, imported into every program automatically, and is where Integer,
String, Vector, and friends come from.
