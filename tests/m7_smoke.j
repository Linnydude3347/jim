// Compiler test fixture — user classes (spec's Shape example and beyond).
// Exercises: fields with defaults, constructor, auto default constructor,
// this.field read/write/compound/++, methods, public fields from outside,
// reference semantics, optional references, instantiation chaining.
// Run: jimc run tests/m7_smoke.j --std tests/fake_std

#import <io>

class Shape {

    private width: Integer = 1;
    private height: Integer = 1;

    Shape(width: Integer, height: Integer) {
        this.width = width;
        this.height = height;
    }

    public getWidth() -> Integer {
        return this.width;
    }

    public area() -> Integer {
        return this.width * this.height;
    }

    public grow(by: Integer) -> None {
        this.width += by;
        this.height += by;
    }
}

class Counter {
    private n: Integer = 0;

    // no constructor: the auto-generated default is used

    public bump() -> None {
        this.n++;
    }

    public value() -> Integer {
        return this.n;
    }
}

class Point {
    public x: Integer = 0;
    public y: Integer = 0;
}

function main() -> Integer {
    var s: Shape = Shape(3, 4);
    print("area = " + s.area().to_string());
    s.grow(1);
    print("grown area = " + s.area().to_string());
    print("width = " + s.getWidth().to_string());

    // reference semantics: b and a are the same object
    var a: Counter = Counter();
    var b: Counter = a;
    b.bump();
    b.bump();
    print("shared count = " + a.value().to_string());

    // public fields, from outside the class
    var p: Point = Point();
    p.x = 7;
    p.y = p.x + 1;
    p.x++;
    print("point = " + p.x.to_string() + "," + p.y.to_string());

    // optional references
    var maybe: Shape? = None;
    if (maybe == None) {
        print("no shape yet");
    }
    maybe = Shape(2, 2);
    print("maybe area = " + maybe.area().to_string());

    // instantiation chains like any expression
    print("chained = " + Shape(5, 6).area().to_string());
    return 0;
}
