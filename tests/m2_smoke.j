// Compiler test fixture — milestone 2 smoke test.
// Exercises: operator desugaring, derived comparisons, widening, unary minus,
// compound assignment, ++/--, C-style for, break/continue, and/or/not,
// method calls, private methods via this, prelude classes.
// Run: jimc run tests/m2_smoke.j --std tests/fake_std

#import <io>

function factorial(n: Integer) -> Integer {
    var r: Integer = 1;
    for (i: Integer = 2; i <= n; i++) {
        r *= i;
    }
    return r;
}

function main() -> Integer {
    print("2 + 3 = " + (2 + 3).to_string());
    print("7 / 2 = " + (7 / 2).to_string());
    print("7 div 2 = " + (7 div 2).to_string());
    print("7 % 2 = " + (7 % 2).to_string());
    print("-(5) via negate = " + (-factorial(0) - 4).to_string());
    print("1 + 2.5 = " + (1 + 2.5).to_string());
    print("10! = " + factorial(10).to_string());

    var s: String = "jim";
    if (s == "jim" and 3 < 4 or false) {
        print("logic works");
    }
    if (not (1 > 2) and 2 >= 2 and 1 != 2) {
        print("derived comparisons work");
    }

    var f: Float = 1.0;
    f /= 4.0;
    f += 1;
    print("f = " + f.to_string());

    var count: Integer = 0;
    while (true) {
        count++;
        if (count < 3) continue;
        break;
    }
    print("count = " + count.to_string());
    print("len = " + s.length().to_string());
    print("21 doubled = " + 21.doubled().to_string());
    return 0;
}
