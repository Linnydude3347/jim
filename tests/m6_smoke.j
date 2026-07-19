// Compiler test fixture — milestone 6 (pointers & references).
// Exercises: the spec's own example, pass-by-reference mutation, compound
// writes through pointers, optional pointers (*T?), pointers to Strings and
// to class-reference variables.
// Run: jimc run tests/m6_smoke.j --std tests/fake_std

#import <io>

function set_to(p: *Integer, v: Integer) -> None {
    *p = v;
}

function bump(p: *Integer) -> None {
    *p += 1;
}

class Box {
    public v: Integer = 0;
}

function rebind(p: *Box, replacement: Box) -> None {
    *p = replacement; // rebinding the caller's variable through the pointer
}

function main() -> Integer {
    // the spec's example, verbatim
    var age: Integer = 24;
    var age_ptr: *Integer = &age;
    *age_ptr = 35;
    print("age = " + age.to_string());

    set_to(age_ptr, 40);
    print("set_to = " + age.to_string());
    bump(&age);
    print("bump = " + age.to_string());

    // optional pointers are nullable
    var maybe: *Integer? = None;
    if (maybe == None) {
        print("no pointer yet");
    }
    maybe = &age;
    *maybe += 1; // unwraps (runtime-checked), then writes through
    print("maybe = " + age.to_string());

    // pointers to Strings
    var s: String = "hi";
    var sp: *String = &s;
    *sp = *sp + "!";
    *sp += "?";
    print("s = " + s);

    // pointer to a class-reference variable
    var a: Box = Box();
    var b: Box = Box();
    b.v = 9;
    rebind(&a, b);
    print("rebound = " + a.v.to_string());
    return 0;
}
