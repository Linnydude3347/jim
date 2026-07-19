// Compiler test fixture — milestone 5 smoke test (generics + containers).
// Exercises: Array/Vector literals (three payload types), indexing read/write,
// compound index assignment, for..in over both containers, push/pop with
// growth past the initial capacity, containers as function parameters,
// empty literals, monomorphization of multiple instantiations.
// Run: jimc run tests/m5_smoke.j --std tests/fake_std

#import <io>

function sum(nums: Array<Integer>) -> Integer {
    var total: Integer = 0;
    for (n: Integer in nums) {
        total += n;
    }
    return total;
}

function main() -> Integer {
    var nums: Array<Integer> = [1, 2, 3, 4, 5];
    print("len = " + nums.length().to_string());
    print("first = " + nums[0].to_string());
    nums[0] = 6;
    print("changed = " + nums[0].to_string());
    nums[1] += 10;
    print("compound = " + nums[1].to_string());
    print("sum = " + sum(nums).to_string());

    var v: Vector<Integer> = [1, 2, 3];
    v.push(4);
    v.push(5);
    v.push(6); // grows past the initial capacity of 4
    print("vlen = " + v.length().to_string());
    var total: Integer = 0;
    for (x: Integer in v) {
        total += x;
    }
    print("vsum = " + total.to_string());
    print("popped = " + v.pop().to_string());
    print("vlen2 = " + v.length().to_string());

    var words: Vector<String> = ["a", "b"];
    words.push("c");
    var joined: String = "";
    for (w: String in words) {
        joined += w;
    }
    print("joined = " + joined);

    var empty: Array<Float> = [];
    print("empty = " + empty.length().to_string());
    return 0;
}
