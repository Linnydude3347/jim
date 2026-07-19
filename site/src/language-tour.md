# Language Tour

A quick pass over the whole language. For runnable, fuller programs see
[Examples](examples.md).

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

## Functions

```jim
// A block of `//` comments above a declaration becomes its docstring.
function add(a: Integer, b: Integer) -> Integer {
    return a + b;
}
```

`main` returns `Integer` (the process exit code) and may omit its `return`.

## Operators are method calls

The heart of jim: `a + b` is sugar for `a.plus(b)`, `a < b` for
`a.less_than(b)`. Define those methods on your own types and the operators just
work.

```jim
class Vec2 {
    public x: Integer = 0;   // fields need a visibility and a default
    public y: Integer = 0;

    Vec2(x: Integer, y: Integer) {   // constructor: class name, no return type
        this.x = x;
        this.y = y;
    }

    public plus(other: Vec2) -> Vec2 {
        return Vec2(this.x + other.x, this.y + other.y);
    }
}

var sum: Vec2 = Vec2(1, 2) + Vec2(3, 4);   // calls Vec2.plus
```

## Control flow

```jim
for (var i: Integer = 0; i < 10; i = i + 1) {
    if (i div 2 == 0) { continue; }
    print(i.to_string());
}
```

jim has C-style `for`, plus `while`, `break`, and `continue`. `and`, `or`,
`not`, and `div` (integer division) are word operators.

## Optionals

A `T?` is either a `T` or `None`. Parsing and lookups return optionals.

```jim
var maybe: Integer? = "42".to_i64();
```

## Containers

```jim
var xs: Array<Integer> = Array(10);   // fixed size, context-typed
var v: Vector<Integer> = Vector();    // growable
```

## Errors: try / catch

```jim
try {
    risky();
} catch (e: Exception) {
    print(e.to_string());
}
```

Every panic is catchable — including `@panic` raised from standard-library
code. There is no `throw`.
