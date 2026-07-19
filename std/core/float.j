class Float {

    // Returns the sum of this float and `other`. Backs operator `+`.
    public plus(other: Float) -> Float { return @f64_add(this, other); }

    // Returns the difference of this float and `other`. Backs operator `-`.
    public minus(other: Float) -> Float { return @f64_sub(this, other); }

    // Returns the product of this float and `other`. Backs operator `*`.
    public times(other: Float) -> Float { return @f64_mul(this, other); }

    // Returns the true division of this float by `other`. Backs operator `/`.
    // Division by zero yields IEEE infinity/nan.
    public divide(other: Float) -> Float { return @f64_div(this, other); }

    // Returns the negation of this float. Backs unary `-`.
    public negate() -> Float { return @f64_neg(this); }

    // Returns true if this float equals another float
    public equals(other: Float) -> Bool { return @f64_eq(this, other); }

    // Returns true if this float is strictly smaller than `other`. Backs
    // `<`, `<=`, `>` and `>=`.
    public less_than(other: Float) -> Bool { return @f64_lt(this, other); }

    // Returns this float truncated toward zero, e.g. `2.7` => 2, `-2.7` => -2.
    public to_integer() -> Integer { return @f64_to_i64(this); }

    // Returns the decimal representation of this float, e.g. `2.5` => "2.5".
    public to_string() -> String { return @f64_to_string(this); }

    // Return the absolute value of this float.
    public abs() -> Float?{}

    // Return -1.0 if this float is negative, 0.0 if it is zero, and 1.0 if
    // it is positive.
    public sign() -> Float?{}

    // Return the floor of this float, the largest integer less than or equal
    // to it, e.g. `2.7` => 2, `-2.1` => -3.
    public floor() -> Integer?{}

    // Return this float rounded to the nearest integer, e.g. `2.5` => 3,
    // `2.4` => 2.
    public round() -> Integer?{}

    // Return this float truncated toward zero, e.g. `2.7` => 2, `-2.7` => -2.
    public trunc() -> Integer?{}

    // Return the ceiling of this float, the smallest integer greater than or
    // equal to it, e.g. `2.1` => 3, `-2.7` => -2.
    //
    // Evaluates to `int(x + (1 - (x - int(x))))`
    public ceil() -> Integer {
        return @f64_to_i64(this + (1 - (this - @f64_to_i64(this))));
    }

    // Return the smaller of this float and `other`.
    public min(other: Float) -> Float?{}

    // Return the larger of this float and `other`.
    public max(other: Float) -> Float?{}

    // Return this float constrained to the range `[low, high]`: `low` when
    // below it, `high` when above it, the value itself otherwise.
    public clamp(low: Float, high: Float) -> Float?{}

    // Return true if this float and `other` differ by at most `epsilon`.
    // This is the right way to compare floats for near-equality, since
    // arithmetic accumulates rounding errors.
    public is_close_to(other: Float, epsilon: Float) -> Bool?{}

    // Return the fractional part of this float, e.g. `2.75` => 0.75. The
    // result carries the sign of this float, e.g. `-2.75` => -0.75.
    public fract() -> Float?{}

    // Intrinsics needed for below functions

    // Return the square root of this non-negative float.
    public sqrt() -> Float?{}

    // Return this float raised to the power `exp`.
    public pow(exp: Float) -> Float?{}

    // Return true if this float is IEEE nan (not a number, e.g. `0.0 / 0.0`).
    public is_nan() -> Bool?{}

    // Return true if this float is IEEE positive or negative infinity,
    // e.g. `1.0 / 0.0`.
    public is_infinite() -> Bool?{}

    // Return true if this float is neither infinite nor nan.
    public is_finite() -> Bool?{}

}
