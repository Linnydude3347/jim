class Integer {

    // Returns the sum of this integer and `other`. Backs operator `+`.
    // Panics on overflow.
    public plus(other: Integer) -> Integer { return @i64_add(this, other); }

    // Returns the difference of this integer and `other`. Backs operator `-`.
    // Panics on overflow.
    public minus(other: Integer) -> Integer { return @i64_sub(this, other); }

    // Returns the product of this integer and `other`. Backs operator `*`.
    // Panics on overflow.
    public times(other: Integer) -> Integer { return @i64_mul(this, other); }

    // Returns the true division of this integer by `other`, as a Float.
    // Backs operator `/`. Division by zero yields IEEE infinity/nan.
    public divide(other: Integer) -> Float { return @f64_div(@i64_to_f64(this), @i64_to_f64(other)); }

    // Returns this integer divided by `other`, truncated toward zero, e.g.
    // `7 div 2` => 3. Backs operator `div`. Panics when `other` is zero.
    public int_divide(other: Integer) -> Integer { return @i64_divtrunc(this, other); }

    // Returns the remainder of `this div other`, e.g. `7 % 2` => 1. Backs
    // operator `%`. Panics when `other` is zero.
    public mod(other: Integer) -> Integer { return @i64_mod(this, other); }

    // Returns the negation of this integer. Backs unary `-`. Panics on
    // overflow (the minimum Integer has no positive counterpart).
    public negate() -> Integer { return @i64_neg(this); }

    // Integers are equal if they hold the same value. Backs `==` and `!=`.
    public equals(other: Integer) -> Bool { return @i64_eq(this, other); }

    // Returns true if this integer is strictly smaller than `other`. Backs
    // `<`, `<=`, `>` and `>=`.
    public less_than(other: Integer) -> Bool { return @i64_lt(this, other); }

    // Returns this integer as a Float. Also inserted automatically when
    // Integer and Float mix in arithmetic.
    public to_float() -> Float { return @i64_to_f64(this); }

    // Returns the decimal representation of this integer, e.g. `-42` => "-42".
    public to_string() -> String { return @i64_to_string(this); }

    // Return the absolute value of this integer.
    public abs() -> Integer {
        if (this < 0) { return -this; }
        return this;
    }

    // Return -1 if this integer is negative, 0 if it is zero, and 1 if it
    // is positive.
    public sign() -> Integer {
        if (this < 0) { return -1; }
        if (this == 0) { return 0; }
        return 1;
    }

    // Return this integer raised to the power `exp`. Does not accept negative
    // exponents. If a negative exponent is passed, zero will be returned.
    public pow(exp: Integer) -> Integer {
        if (exp < 0) return 0;
        var r: Integer = this;
        for (i: Integer = 1; i <= exp; i++) {
            r *= exp;
        }
        return r;
    }

    // Return the smaller of this integer and `other`.
    public min(other: Integer) -> Integer {
        if (this < other) { return this; }
        return other;
    }

    // Return the larger of this integer and `other`.
    public max(other: Integer) -> Integer {
        if (this > other) { return this; }
        return other;
    }

    // Return this integer constrained to the range `[low, high]`: `low` when
    // below it, `high` when above it, the value itself otherwise.
    public clamp(low: Integer, high: Integer) -> Integer {
        if (this < low) { return low; }
        if (this > high) { return high; }
        return this;
    }

    // Return true if this integer is divisible by two.
    public is_even() -> Bool {
        return this % 2 == 0;
    }

    // Return true if this integer is not divisible by two.
    public is_odd() -> Bool {
        return this % 2 != 0;
    }

    // Return the Char with this byte value. Returns None unless this integer
    // is in `[0, 255]`.
    public to_char() -> Char { return @i64_to_char(this); }

    // Return the number of decimal digits in the absolute value of this
    // integer, e.g. `-1234` => 4. Zero has one digit.
    public digit_count() -> Integer {
        var s: String = this.to_string();
        var count: Integer = 0;
        for (c: Char in s) {
            if (c.is_digit()) {
                count++;
            }
        }
        return count;
    }

    // Intrinsics needed for below functions

    // Return the bitwise AND of this integer and `other`.
    public bit_and(other: Integer) -> Integer { return @i64_and(this, other); }

    // Return the bitwise OR of this integer and `other`.
    public bit_or(other: Integer) -> Integer { return @i64_or(this, other); }

    // Return the bitwise XOR of this integer and `other`.
    public bit_xor(other: Integer) -> Integer { return @i64_xor(this, other); }

    // Return the bitwise complement of this integer, with every bit flipped.
    public bit_not() -> Integer { return @i64_not(this); }

    // Return this integer shifted left by `bits` bit positions. Bits shifted
    // past the top are lost; zeros come in from the right.
    public shift_left(bits: Integer) -> Integer { return @i64_shl(this, bits); }

    // Return this integer shifted right by `bits` bit positions. The shift is
    // arithmetic: the sign bit is preserved.
    public shift_right(bits: Integer) -> Integer { return @i64_shr(this, bits); }

}
