class Vector<T> {

    private buffer: RawBuffer<T> = @buf_alloc(0);
    private idx: Integer = 0;

    // Appends `value` to the end of the vector, growing the backing storage
    // (doubling its capacity) when it is full.
    public push(value: T) -> None {

        // If we have size=0 vector, expand to 1, assign, then return
        if (this.idx == 0) {
            this.idx++;
            this.buffer = @buf_alloc(this.idx);
            this.buffer[0] = value;
            return;
        }

        // If have a non zero sized vector but we are at capacity
        if (this.idx == this.buffer.capacity()) {

            // Copy the current buffer to a temporary buffer with double size
            var tmp: RawBuffer<T> = @buf_alloc(this.buffer.capacity() * 2);
            var i: Integer = 0;
            while (i < this.idx) {
                tmp[i] = this.buffer[i];
                i++;
            }
            tmp[i] = value;
            this.idx = i + 1;
            this.buffer = tmp;
            return;
        }

        // If we have enough room, add to buffer and return
        this.buffer[this.idx] = value;
        this.idx++;

    }

    // Returns the element at `index`. Backs indexing `v[i]`. Panics if the
    // index is out of bounds.
    public get(index: Integer) -> T {
        if (index < 0 or index >= this.idx) {
            @panic("Vector index out of bounds.");
        }
        return this.buffer[index];
    }

    // Replaces the element at `index` with `value`. Backs index assignment
    // `v[i] = v`. Panics if the index is out of bounds.
    public set(index: Integer, value: T) -> None {
        if (index < 0 or index >= this.idx) {
            @panic("Vector index out of bounds.");
        }
        this.buffer[index] = value;
    }

    // Returns the number of elements in the vector (not its capacity).
    public length() -> Integer { return this.idx; }

    // Returns the elements as a list literal, e.g. `[1, 2, 3]`. Requires the
    // payload type to provide `to_string`.
    public to_string() -> String {
        var result: String = "[";
        for (i: Integer = 0; i < this.idx; i++) {
            if (i == this.idx - 1) {
                result += this.buffer[i].to_string();
            } else {
                result += this.buffer[i].to_string() + ", ";
            }
        }
        result += "]";
        return result;
    }

    // Sets every existing element to `value`. The length is unchanged.
    public fill(value: T) -> None{}

    // Exchanges the elements at positions `a` and `b`. Panics if either
    // index is out of bounds.
    public swap(a: Integer, b: Integer) -> None{}

    // Return an independent copy of this vector: same length, same elements,
    // separate storage.
    public clone() -> Vector<T>?{}

    // Return a new vector containing the elements `[start, end]`. So,
    // `[10, 20, 30, 40].slice(1, 2)` => `[20, 30]`.
    public slice(start: Integer, end: Integer) -> Vector<T>?{}

    // Return a new vector holding this vector's elements followed by
    // `other`'s.
    public concat(other: Vector<T>) -> Vector<T>?{}

    // Return true if the vector has no elements.
    public is_empty() -> Bool?{}

    // Vectors are equal if they have the same length and equal elements at
    // every position (compared with the payload's `equals`). Capacity is
    // ignored.
    public equals(other: Vector<T>) -> Bool?{}

    // Sorts the vector in place into ascending order, as defined by the
    // payload's `less_than`.
    public sort() -> None{}

    // Reverses the order of the elements in place.
    public reverse() -> None{}

    // Return a fixed-length Array holding a copy of this vector's elements.
    public to_array() -> Array<T>?{}

    // Return the last element without removing it. Returns None when the
    // vector is empty.
    public peek() -> T?{}

    // Inserts `value` at position `index`, shifting that element and
    // everything after it one slot to the right. Panics unless `index` is in
    // `[0, length()]` (inserting at `length()` appends).
    public insert(index: Integer, value: T) -> None{}

    // Removes and returns the element at `index`, shifting everything after
    // it one slot to the left. Panics if the index is out of bounds.
    public remove(index: Integer) -> T?{}

    // Removes the first element equal to `value` (compared with the
    // payload's `equals`). Does nothing when no element matches.
    public remove_value(value: T) -> None{}

    // Removes every element. The capacity is kept for reuse.
    public clear() -> None{}

    // Appends every element of `other` to this vector, in order.
    public extend(other: Vector<T>) -> None{}

    // Return the number of elements the backing storage can hold before the
    // next `push` has to grow it.
    public capacity() -> Integer?{}

    // Grows the backing storage so it can hold at least `size` elements,
    // copying at most once. Call before pushing a known number of elements
    // to avoid repeated grow-and-copy cycles.
    public reserve(size: Integer) -> None{}

}
