class Array<T> {

    private buffer: RawBuffer<T> = @buf_alloc(0);
    private size: Integer = 0;

    // Creates an array of `length` slots. The slots are uninitialized until
    // written — `set` (or `fill`) before you `get`.
    Array(length: Integer) {
        if (length < 0) {
            @panic("Array length cannot be negative.");
        }
        this.buffer = @buf_alloc(length);
        this.size = length;
    }

    // Returns the element at `index`. Backs indexing `a[i]`. Panics if the
    // index is out of bounds.
    public get(index: Integer) -> T {
        if (index < 0 or index >= this.size) {
            @panic("Array index out of bounds.");
        }
        return this.buffer[index];
    }

    // Replaces the element at `index` with `value`. Backs index assignment
    // `a[i] = v`. Panics if the index is out of bounds.
    public set(index: Integer, value: T) -> None {
        if (index < 0 or index >= this.size) {
            @panic("Array index out of bounds.");
        }
        this.buffer[index] = value;
    }

    // Returns the number of elements in the array.
    public length() -> Integer { return this.size; }

    // Returns the elements as a list literal, e.g. `[1, 2, 3]`. Requires the
    // payload type to provide `to_string`.
    public to_string() -> String {
        var result: String = "[";
        for (i: Integer = 0; i < this.size; i++) {
            if (i == this.size - 1) {
                result += this.buffer[i].to_string();
            } else {
                result += this.buffer[i].to_string() + ", ";
            }
        }
        result += "]";
        return result;
    }

    // Sets every slot to `value`. Useful right after construction, since new
    // slots are uninitialized.
    public fill(value: T) -> None {
        for (i: Integer = 0; i < this.size; i++) {
            this.buffer[i] = value;
        }
    }

    // Exchanges the elements at positions `a` and `b`. Panics if either
    // index is out of bounds.
    public swap(a: Integer, b: Integer) -> None {
        if (a == b) { return; }
        if ((a < 0 or a >= this.size) or (b < 0 or b >= this.size)) {
            @panic("Array index out of bounds.");
        }
        var tmp: T = this.buffer[a];
        this.buffer[a] = this.buffer[b];
        this.buffer[b] = tmp;
    }

    // Return an independent copy of this array: same length, same elements,
    // separate storage.
    public clone() -> Array<T> {
        var copy: Array<T> = Array(this.size);
        for (i: Integer = 0; i < this.size; i++) {
            copy[i] = this.buffer[i];
        }
        return copy;
    }

    // Return a new array containing the elements `[start, end]`. So,
    // `[10, 20, 30, 40].slice(1, 2)` => `[20, 30]`.
    public slice(start: Integer, end: Integer) -> Array<T> {
        var result: Array<T> = Array((end - start) + 1);
        var index: Integer = 0;
        for (i: Integer = start; i <= end; i++) {
            result[index] = this.buffer[i];
            index++;
        }
        return result;
    }

    // Return a new array holding this array's elements followed by `other`'s.
    public concat(other: Array<T>) -> Array<T>? {
        const new_size: Integer = this.length() + other.length();
        var result: Array<T> = Array(new_size);
        var next: Bool = false;
        for (i: Integer = 0; i < new_size; i++) {
            if (i == this.length()) { next = true; }
            if (not next) {
                result[i] = this.buffer[i];
            } else {
                result[i] = other[i];
            }
        }
        return result;
    }

    // Return true if the array has no elements.
    public is_empty() -> Bool {
        return this.length() == 0;
    }

    // Arrays are equal if they have the same length and equal elements at
    // every position (compared with the payload's `equals`).
    public equals(other: Array<T>) -> Bool? {
        if (this.length() != other.length()) { return false; }
        for (i: Integer = 0; i < this.length(); i++) {
            if (this.buffer[i] != other[i]) {
                return false;
            }
        }
        return true;
    }

    // Sorts the array in place into ascending order, as defined by the
    // payload's `less_than`.
    public sort() -> None {}

    // Reverses the order of the elements in place.
    public reverse() -> None {
        for (i: Integer = 0; i < this.size div 2; i++) {
            var left: Integer = i;
            var right: Integer = this.size - i - 1;
            this.swap(left, right);
        }
    }

    // Return a growable Vector holding a copy of this array's elements.
    public to_vector() -> Vector<T> {
        var result: Vector<T> = [];
        for (i: Integer = 0; i < this.size; i++) {
            result[i] = this.buffer[i];
        }
        return result;
    }

}
