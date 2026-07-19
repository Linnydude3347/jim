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
            @panic("Array index out of bounds");
        }
        return this.buffer[index];
    }

    // Replaces the element at `index` with `value`. Backs index assignment
    // `a[i] = v`. Panics if the index is out of bounds.
    public set(index: Integer, value: T) -> None {
        if (index < 0 or index >= this.size) {
            @panic("Array index out of bounds");
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
    public fill(value: T) -> None{}

    // Exchanges the elements at positions `a` and `b`. Panics if either
    // index is out of bounds.
    public swap(a: Integer, b: Integer) -> None{}

    // Return an independent copy of this array: same length, same elements,
    // separate storage.
    public clone() -> Array<T>?{}

    // Return a new array containing the elements `[start, end]`. So,
    // `[10, 20, 30, 40].slice(1, 2)` => `[20, 30]`.
    public slice(start: Integer, end: Integer) -> Array<T>?{}

    // Return a new array holding this array's elements followed by `other`'s.
    public concat(other: Array<T>) -> Array<T>?{}

    // Return true if the array has no elements.
    public is_empty() -> Bool?{}

    // Arrays are equal if they have the same length and equal elements at
    // every position (compared with the payload's `equals`).
    public equals(other: Array<T>) -> Bool?{}

    // Sorts the array in place into ascending order, as defined by the
    // payload's `less_than`.
    public sort() -> None{}

    // Reverses the order of the elements in place.
    public reverse() -> None{}

    // Return a growable Vector holding a copy of this array's elements.
    public to_vector() -> Vector<T>?{}

}
