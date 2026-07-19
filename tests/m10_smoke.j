// M10 smoke — monomorphized generic functions.
// Run: jimc run tests/m10_smoke.j --std tests/fake_std

#import <io>

// single type param, arg-only inference (return type is concrete)
function sum<C>(seq: C) -> Integer {
    var total: Integer = 0;
    for (x: Integer in seq) {
        total += x;
    }
    return total;
}

// container + element params: C from the argument, T from context
function max<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) {
            best = seq[i];
        }
    }
    return best;
}

// T inferred straight from a parameterized argument type
function head<T>(seq: Array<T>) -> T {
    return seq[0];
}

// generic function building a result container
function reversed<C, T>(seq: C) -> Array<T> {
    var out: Array<T> = Array(seq.length());
    for (i: Integer = 0; i < seq.length(); i++) {
        out[seq.length() - 1 - i] = seq[i];
    }
    return out;
}

function pick<C, T>(seq: C, i: Integer) -> T {
    return seq[i];
}

// a generic calling another generic with explicit args made of its own params
function last<C, T>(seq: C) -> T {
    return pick<C, T>(seq, seq.length() - 1);
}

// zero-argument: type parameter comes purely from the expected type
function empty_vec<T>() -> Vector<T> {
    return [];
}

function main() -> Integer {
    var a: Array<Integer> = [3, 9, 4, 1];
    var v: Vector<Float> = [2.5, 7.25, 1.0];

    // C from the argument, T from the annotated target
    var m1: Integer = max(a);
    print(m1.to_string()); // 9
    var m2: Float = max(v);
    print(m2.to_string()); // 7.25

    // arg-only inference, chained with no context at all
    print(sum(a).to_string());  // 17
    print(head(a).to_string()); // 3

    // String is a sequence too (length() + get())
    var big: Char = max("jimlang");
    print(big.to_string()); // n

    // explicit type arguments — no context anywhere
    print(max<Array<Integer>, Integer>(a).to_string()); // 9

    // building a result container
    var r: Array<Integer> = reversed(a);
    print(head(r).to_string()); // 1

    // generic-to-generic call
    var l: Float = last(v);
    print(l.to_string()); // 1

    // inferred purely from context
    var e: Vector<Integer> = empty_vec();
    print(e.length().to_string()); // 0

    return 0;
}
