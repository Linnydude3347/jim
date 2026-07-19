// ref: https://docs.python.org/3/library/math.html

// Number-theoretic functions //

// Return the number of ways to choose k items from n items without
// repetition and without order. Evaluates to `n! / (k! * (n - k)!)
// when `k <= n` and evaluates to zero when `k > n`
//
// Also called the binomial coefficient because it is equivalent
// to the coefficient of k-th term in polynomial expansion of `(1 + x)^n`
function comb(n: Integer, k: Integer) -> Integer {
    if (k > n) { return 0; }
    return @f64_to_i64(
        factorial(n) / (factorial(k) * factorial(n - k))
    );
}

// Return the factorial of the non-negative integer n. Does not accept negative
// integers. If a negative integer is passed, zero will be returned.
function factorial(n: Integer) -> Integer {
    if (n < 0) { return 0; }
    var result: Integer = 1;
    for (i: Integer = 1; i <= n; i++) {
        result *= i;
    }
    return result;
}

// Returns the factors of the non-negative integer n. Does not accept negative integers.
// If a negative integer is passed, an empty array is returned.
function factors(n: Integer) -> Array<Integer> {
    if (n <= 0) { return []; }
    var count: Integer = 0;
    var t: Array<Integer> = Array(n);
    var index: Integer = 0;
    for (i: Integer = 1; i <= n; i++) {
        if (n % i == 0) {
            t[index] = i;
            count++;
            index++;
        }
    }

    var r: Array<Integer> = Array(count);
    for (i : Integer = 0; i < count; i++) {
        r[i] = t[i];
    }

    return r;

}

// Return the greatest common divisor of the specified integer arguments. If any
// of the arguments is nonzero, then the returned value is the largest positive
// integer that is a divisor of all arguments. If all arguments are zero, then the
// returned value is zero.
function gcd(numbers: Array<Integer>) -> Integer? {}

// Return the integer square root of the nonnegative integer `n`. This is the floor
// of the exact square root of `n`, or equivalently, the greatest integer `a` such
// that `a^2 <= n`.
//
// For some applications, it may be more convenient to have the least integer `a`
// such that `n <= a^2`, or in other words the ceiling of the exact square root of `n`.
// For positive `n`, this can be computed using `a = 1 + isqrt(n - 1)`.
function isqrt(n: Integer) -> Integer? {}

// Return the least common multiple of the specified integer arguments. If all
// arguments are nonzero, then the returned value is the smallest positive integer
// that is a multiple of all arguments. If any of the arguments is zero, then the
// returned value is zero.
function lcm(numbers: Array<Integer>) -> Integer? {}

// Return the number of ways to choose `k` items from `n` items without repetition
// and with order. Evaluates to `n! / (n - k)!` when `k <= n` and evaluates to zero
// when `k > n`. If `k` is `None`, then `k` defaults to `n` and the function returns `n1!`
function perm(n: Integer, k: Integer?) -> Integer {

    if (k == None) { return factorial(n); }
    if (k > n) { return 0; }
    return @f64_to_i64(factorial(n) / factorial(n - k));

}

// Convert angle `x` from radians to degrees.
function degrees(x: Float) -> Float?{}

// Convert angle `x` from degrees to radians.
function radians(x: Float) -> Float?{}

// Return the Euclidean distance between two points `a` and `b`, each given
// as an array of coordinates. The two arrays must have the same length.
function dist(a: Array<Float>, b: Array<Float>) -> Float?{}

// Intrinsics needed for below functions

// Return the square root of the non-negative number `x`.
function sqrt(x: Float) -> Float?{}

// Return the cube root of `x`.
function cbrt(x: Float) -> Float?{}

// Return the length of the hypotenuse of a right triangle with legs `x` and
// `y`, i.e. `sqrt(x^2 + y^2)`.
function hypot(x: Float, y: Float) -> Float?{}

// Return `e` raised to the power `x`, where `e` is the base of natural
// logarithms.
function exp(x: Float) -> Float?{}

// Return the natural logarithm of `x` (base `e`). `x` must be positive.
function log(x: Float) -> Float?{}

// Return the base-2 logarithm of the positive number `x`.
function log2(x: Float) -> Float?{}

// Return the base-10 logarithm of the positive number `x`.
function log10(x: Float) -> Float?{}

// Return the sine of `x` radians.
function sin(x: Float) -> Float?{}

// Return the cosine of `x` radians.
function cos(x: Float) -> Float?{}

// Return the tangent of `x` radians.
function tan(x: Float) -> Float?{}

// Return the arc sine of `x`, in radians. The result is in `[-pi/2, pi/2]`;
// `x` must be in `[-1, 1]`.
function asin(x: Float) -> Float?{}

// Return the arc cosine of `x`, in radians. The result is in `[0, pi]`;
// `x` must be in `[-1, 1]`.
function acos(x: Float) -> Float?{}

// Return the arc tangent of `x`, in radians. The result is in `[-pi/2, pi/2]`.
function atan(x: Float) -> Float?{}

// Return `atan(y / x)`, in radians, using the signs of both arguments to pick
// the correct quadrant. The result is in `[-pi, pi]`. Note the argument
// order: `y` first, then `x`.
function atan2(y: Float, x: Float) -> Float?{}

// Return the floating-point remainder of `x / y`, with the sign of `x`.
function fmod(x: Float, y: Float) -> Float?{}


// Constants //

// The mathematical constant pi = 3.14159..., to available precision.
function pi() -> Float { return 3.141592653589793; }

// The mathematical constant e = 2.71828..., to available precision (the base
// of natural logarithms).
function e() -> Float { return 2.718281828459045; }

// The mathematical constant tau = 2 * pi = 6.28318..., to available precision.
function tau() -> Float { return 6.283185307179586; }