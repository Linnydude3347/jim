// Compiler test fixture — milestone 4 smoke test (optionals).
// Exercises: T? returns with wrap coercion, explicit `return None`, implicit
// None on fall-off, == None / != None presence checks, auto-unwrap in
// operators / methods / compound assignment, None in var-init and assignment.
// Run: jimc run tests/m4_smoke.j --std tests/fake_std

#import <io>

function may_return_nothing(flag: Bool) -> Integer? {
    if (flag) {
        return 42; // Integer wraps into Integer? implicitly
    }
    return None;
}

function falls_off_the_end() -> Integer? {
    // no return at all: an optional function falling off the end returns None
}

function main() -> Integer {
    var a: Integer? = may_return_nothing(true);
    if (a != None) {
        print("got " + a.to_string()); // method on T? auto-unwraps
    }

    var b: Integer? = may_return_nothing(false);
    if (b == None) {
        print("b is None");
    }

    var c: Integer = a + 1; // T? in an operator auto-unwraps
    print("c = " + c.to_string());

    var d: Integer? = None; // None takes its type from the declaration
    d = 7;                  // assignment wraps
    d += 1;                 // compound assignment unwraps, adds, re-wraps
    print("d = " + d.to_string());

    var e: Integer? = falls_off_the_end();
    if (not (e != None)) {
        print("fall-off returns None");
    }

    var s: String? = None;
    s = "wrapped";
    print("s = " + s + "!"); // String? in String.plus auto-unwraps as argument
    return 0;
}
