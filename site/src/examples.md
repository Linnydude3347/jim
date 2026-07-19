# Examples

Runnable programs. Compile and run any of them from the repository root:

```powershell
jimc run example.j
jimc run example.j --emit-c out.c   # inspect the generated C
```

> The complete, continuously-tested example set lives in
> [`docs/EXAMPLES.md`](https://github.com/) in the repository — FizzBuzz, a
> prime sieve, custom iterables, nested containers, and the error-message
> table. A curated selection follows.

## Hello, world

```jim
#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
```

## Command-line arguments

```jim
#import <io>

function main(argv: Array<String>) -> Integer {
    print("arguments: " + argv.length().to_string());
    for (a: String in argv) {
        print(a);   // argv[0] is the executable path
    }
    return 0;
}
```

## FizzBuzz

```jim
#import <io>

function main() -> Integer {
    for (var i: Integer = 1; i <= 15; i = i + 1) {
        if (i div 15 == 0)     { print("FizzBuzz"); }
        else if (i div 3 == 0) { print("Fizz"); }
        else if (i div 5 == 0) { print("Buzz"); }
        else                   { print(i.to_string()); }
    }
    return 0;
}
```

## Operators on your own type

```jim
#import <io>

class Vec2 {
    var x: Integer;
    var y: Integer;

    public plus(other: Vec2) -> Vec2 {
        return Vec2(this.x + other.x, this.y + other.y);
    }

    public to_string() -> String {
        return "(" + this.x.to_string() + ", " + this.y.to_string() + ")";
    }
}

function main() -> Integer {
    var a: Vec2 = Vec2(1, 2);
    var b: Vec2 = Vec2(3, 4);
    print((a + b).to_string());   // (4, 6)
    return 0;
}
```
