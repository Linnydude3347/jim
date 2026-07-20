# Examples

Complete, runnable programs. Try any of them in the [Playground](playground.md),
or from the repository root:

```
jimc run example.j
jimc run example.j --emit-c out.c   # inspect the generated C
```

## Hello, world

```jim
#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
```

## Operators on your own type

Reserved method names give your class real operators. `plus` backs `+`, `equals`
backs `==` and `!=`, and so on:

```jim
#import <io>

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
        print(c.to_string());                // (4, 6)
    }
    return 0;
}
```

## Custom iterables

`for..in` works on any class with `length() -> Integer` and
`get(i: Integer) -> T`, not just the built-in containers:

```jim
#import <io>

class Range {
    private lo: Integer = 0;
    private hi: Integer = 0;

    Range(lo: Integer, hi: Integer) {
        this.lo = lo;
        this.hi = hi;
    }

    public length() -> Integer { return this.hi - this.lo; }
    public get(i: Integer) -> Integer { return this.lo + i; }
}

function main() -> Integer {
    var total: Integer = 0;
    for (n: Integer in Range(3, 7)) {   // 3, 4, 5, 6
        total += n;
    }
    print(total.to_string());           // 18
    return 0;
}
```

## Errors: try / catch

```jim
#import <io>

function main() -> Integer {
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
overflow, out of bounds, and anything the std library raises.

## FizzBuzz

```jim
#import <io>

function label(n: Integer) -> String? {
    if (n % 15 == 0) { return "FizzBuzz"; }
    if (n % 3 == 0) { return "Fizz"; }
    if (n % 5 == 0) { return "Buzz"; }
    return None;
}

function main() -> Integer {
    var out: String = "";
    for (n: Integer = 1; n <= 15; n++) {
        var l: String? = label(n);
        if (l != None) {
            out = out + l + " ";
        } else {
            out = out + n.to_string() + " ";
        }
    }
    print(out);   // 1 2 Fizz 4 Buzz Fizz 7 8 Fizz Buzz 11 Fizz 13 14 FizzBuzz
    return 0;
}
```

## Prime sieve

Sized arrays, vectors, nested loops, and a `+=` step in a for loop:

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
    print(out);                                             // 2 3 5 7 11 ... 47
    print("count = " + primes.length().to_string());        // count = 15
    return 0;
}
```

## Generic functions

One definition that works for every sequence type:

```jim
#import <io>

function largest<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) { best = seq[i]; }
    }
    return best;
}

function main() -> Integer {
    var a: Array<Integer> = [3, 9, 4, 1];
    var big: Integer = largest(a);       // T = Integer from the expected type
    print(big.to_string());              // 9
    var c: Char = largest("hello");      // strings are sequences of Char
    print(c.to_string());                // o
    return 0;
}
```

The standard library ships `max`, `min`, and `sum` as generic functions built
exactly like this, so `max(a)` works out of the box.

## Reading the compiler's errors

`jimc` reports one error at a time, with the file, position, offending line, and
a caret. A tour of common ones:

| you wrote | the compiler says |
|---|---|
| `var x: Integer = "hi";` | type mismatch: `'x'` is declared Integer but initialized with String |
| `x /= 2;` on an Integer | operator `'/='`: result is Float but `'x'` is Integer |
| `day = 2;` on a const | cannot assign to constant `'day'` |
| a path with no `return` | function `'f'` may reach the end without returning Integer |
| `var x: Integer = None;` | `'None'` only fits optional types (T?), but Integer is expected |
| `width` instead of `this.width` | unknown variable `'width'`: member access must be `'this.width'` |
| `break;` outside a loop | `'break'` outside a loop |
| `@panic("x")` outside std | `'@panic'`: intrinsics are only allowed in the standard library |
| `+` on a class without `plus` | operator `'+'` needs `Shape.plus()`, but class `'Shape'` has no method `'plus'` |
