// COMPILER TEST DOUBLE — not the real jim standard library.
// The real core.j is Ben's and lives in /std. This minimal version exists so
// the compiler test suite can exercise desugaring without depending on it.
// Used via: jimc run tests/m2_smoke.j --std tests/fake_std

class Integer {
    public plus(other: Integer) -> Integer { return @i64_add(this, other); }
    public minus(other: Integer) -> Integer { return @i64_sub(this, other); }
    public times(other: Integer) -> Integer { return @i64_mul(this, other); }
    public divide(other: Integer) -> Float {
        return @f64_div(@i64_to_f64(this), @i64_to_f64(other));
    }
    public int_divide(other: Integer) -> Integer { return @i64_divtrunc(this, other); }
    public mod(other: Integer) -> Integer { return @i64_mod(this, other); }
    public negate() -> Integer { return @i64_neg(this); }
    public equals(other: Integer) -> Bool { return @i64_eq(this, other); }
    public less_than(other: Integer) -> Bool { return @i64_lt(this, other); }
    public to_float() -> Float { return @i64_to_f64(this); }
    public to_string() -> String { return @i64_to_string(this); }

    // exercises private methods + this-dispatch
    private double_it() -> Integer { return @i64_add(this, this); }
    public doubled() -> Integer { return this.double_it(); }
}

class Float {
    public plus(other: Float) -> Float { return @f64_add(this, other); }
    public minus(other: Float) -> Float { return @f64_sub(this, other); }
    public times(other: Float) -> Float { return @f64_mul(this, other); }
    public divide(other: Float) -> Float { return @f64_div(this, other); }
    public negate() -> Float { return @f64_neg(this); }
    public equals(other: Float) -> Bool { return @f64_eq(this, other); }
    public less_than(other: Float) -> Bool { return @f64_lt(this, other); }
    public to_integer() -> Integer { return @f64_to_i64(this); }
    public to_string() -> String { return @f64_to_string(this); }
}

class Bool {
    public equals(other: Bool) -> Bool { return @bool_eq(this, other); }
}

class Char {
    public equals(other: Char) -> Bool { return @char_eq(this, other); }
    public less_than(other: Char) -> Bool { return @char_lt(this, other); }
    public to_integer() -> Integer { return @char_to_i64(this); }
    public to_string() -> String { return @char_to_string(this); }
}

class String {
    public plus(other: String) -> String { return @str_concat(this, other); }
    public equals(other: String) -> Bool { return @str_eq(this, other); }
    public less_than(other: String) -> Bool { return @str_lt(this, other); }
    public length() -> Integer { return @str_len(this); }
    public get(index: Integer) -> Char {
        if (index < 0 or index >= this.length()) {
            @panic("String index out of bounds");
        }
        return @str_byte(this, index);
    }
}

// An Exception is delivered to catch blocks by the runtime; its
// representation is its message.
class Exception {
    public msg() -> String { return @exc_msg(this); }
}

// Fixed-length array over raw storage; safety logic lives here, in jim.
class Array<T> {
    private buf: RawBuffer<T> = @buf_alloc(0);
    private count: Integer = 0;

    Array(len: Integer) {
        if (len < 0) {
            @panic("Array length cannot be negative");
        }
        this.buf = @buf_alloc(len);
        this.count = len;
    }

    public length() -> Integer {
        return this.count;
    }

    public get(i: Integer) -> T {
        if (i < 0 or i >= this.count) {
            @panic("Array index out of bounds");
        }
        return this.buf.get(i);
    }

    public set(i: Integer, value: T) -> None {
        if (i < 0 or i >= this.count) {
            @panic("Array index out of bounds");
        }
        this.buf.set(i, value);
    }
}

// Growable vector: geometric growth, all in jim.
class Vector<T> {
    private buf: RawBuffer<T> = @buf_alloc(4);
    private count: Integer = 0;

    Vector() {
    }

    public length() -> Integer {
        return this.count;
    }

    public get(i: Integer) -> T {
        if (i < 0 or i >= this.count) {
            @panic("Vector index out of bounds");
        }
        return this.buf.get(i);
    }

    public set(i: Integer, value: T) -> None {
        if (i < 0 or i >= this.count) {
            @panic("Vector index out of bounds");
        }
        this.buf.set(i, value);
    }

    public push(value: T) -> None {
        if (this.count == this.buf.capacity()) {
            this.grow();
        }
        this.buf.set(this.count, value);
        this.count += 1;
    }

    public pop() -> T {
        if (this.count == 0) {
            @panic("pop from an empty Vector");
        }
        this.count -= 1;
        return this.buf.get(this.count);
    }

    private grow() -> None {
        var bigger: RawBuffer<T> = @buf_alloc(this.buf.capacity() * 2);
        for (i: Integer = 0; i < this.count; i++) {
            bigger.set(i, this.buf.get(i));
        }
        this.buf = bigger;
    }
}
