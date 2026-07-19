// Returns the highest value of the passed container.
function max<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) {
            best = seq[i];
        }
    }
    return best;
}

// Returns the lowest value of the passed container
function min<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] < best) {
            best = seq[i];
        }
    }
    return best;
}

// Returns the sum of a sequence
function sum<C, T>(seq: C) -> T {
    var total: T = 0;
    for (e: T in seq) {
        total += e;
    }
    return total;
}

function product<C, T>(seq: C) -> T?{}
function mean<C, T>(seq: C) -> T?{}
function reversed<C, T>(seq: C) -> Array<T>?{}
function sorted<C, T>(seq: C) -> Array<T>?{}
function all<C, T>(seq: C) -> Bool?{}
function any<C, T>(seq: C) -> Bool?{}