// Compiler test fixture — milestone 8 (try/catch).
// Exercises: catching None-misuse (the spec's own example), division by zero,
// bounds panics raised from jim std code, nested try with propagation to the
// outer handler, and normal execution continuing after a catch.
// Run: jimc run tests/m8_smoke.j --std tests/fake_std

#import <io>

function definitely_returns_none() -> Integer? {
    return None;
}

function main() -> Integer {
    // the spec's error-handling example, verbatim
    try {
        var a: Integer? = definitely_returns_none();
        var b: Integer = a + 1;
        print("not reached: " + b.to_string());
    } catch (e: Exception) {
        print("Something went wrong: " + e.msg());
    }

    try {
        var x: Integer = 1 div 0;
        print("not reached: " + x.to_string());
    } catch (e: Exception) {
        print("caught: " + e.msg());
    }

    // this panic is raised by jim code (Vector.get in the std fixture)
    try {
        var v: Vector<Integer> = [1];
        print("not reached: " + v[5].to_string());
    } catch (e: Exception) {
        print("caught: " + e.msg());
    }

    // nested: inner succeeds; a later panic reaches the outer handler
    try {
        try {
            var ok: Integer = 2 + 2;
            print("inner ok = " + ok.to_string());
        } catch (e: Exception) {
            print("not reached (inner catch)");
        }
        var z: Integer? = None;
        print("not reached: " + z.to_string());
    } catch (e: Exception) {
        print("outer caught: " + e.msg());
    }

    print("done");
    return 0;
}
