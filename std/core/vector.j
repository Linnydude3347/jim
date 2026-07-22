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
    public fill(value: T) -> None {
        for (i: Integer = 0; i < this.length(); i++) {
            this.buffer[i] = value;
        }
    }

    // Exchanges the elements at positions `a` and `b`. Panics if either
    // index is out of bounds.
    public swap(a: Integer, b: Integer) -> None {
        if (a < 0 or a >= this.length()) { @panic("Vector index out of bounds."); }
        if (b < 0 or b >= this.length()) { @panic("Vector index out of bounds."); }
        var tmp: T = this.buffer[a];
        this.buffer[a] = this.buffer[b];
        this.buffer[b] = tmp;
    }

    // Return an independent copy of this vector: same length, same elements,
    // separate storage.
    public clone() -> Vector<T> {
        var r: Vector<T> = Vector();
        for (i: Integer = 0; i < this.length(); i++) {
            r.push(this.buffer[i]);
        }
        return r;
    }

    // Return a new vector containing the elements `[start, end]`. So,
    // `[10, 20, 30, 40].slice(1, 2)` => `[20, 30]`.
    public slice(start: Integer, end: Integer) -> Vector<T> {
        if (start < 0 or start >= this.length()) { @panic("Vector index out of bounds."); }
        if (end < 0 or end >= this.length()) { @panic("Vector index out of bounds."); }
        if (start > end) { @panic("Start index cannot be greater than end index."); }
        var r: Vector<T> = Vector();
        for (i: Integer = start; i <= end; i++) {
            r.push(this.buffer[i]);
        }
        return r;
    }

    // Return a new vector holding this vector's elements followed by `other`'s.
    public concat(other: Vector<T>) -> Vector<T> {
        var r: Vector<T> = this.clone();
        for (i: Integer = 0; i < other.length(); i++) {
            r.push(other[i]);
        }
        return r;
    }

    // Return true if the vector has no elements.
    public is_empty() -> Bool { return this.length() == 0; }

    // Vectors are equal if they have the same length and equal elements at
    // every position (compared with the payload's `equals`). Capacity is
    // ignored.
    public equals(other: Vector<T>) -> Bool {
        if (this.length() != other.length()) { return false; }
        for (i: Integer = 0; i < this.length(); i++) {
            if (this.buffer[i] != other[i]) { return false; }
        }
        return true;
    }

    // Sorts the vector in place into ascending order, as defined by the
    // payload's `less_than`.
    public sort() -> None {
        for (i: Integer = 0; i < this.length(); i++) {
            var insert_index: Integer = i;
            var current_value: T = this.buffer[i];
            for (j: Integer = i - 1; j >= 0; j--) {
                if (this.buffer[j] > current_value) {
                    this.buffer[j + 1] = this.buffer[j];
                    insert_index = j;
                } else {
                    break;
                }
            }
            this.buffer[insert_index] = current_value;
        }
    }

    // Reverses the order of the elements in place.
    public reverse() -> None {
        for (i: Integer = 0; i < this.length() div 2; i++) {
            var left: Integer = i;
            var right: Integer = this.length() - i - 1;
            this.swap(left, right);
        }
    }

    // Return a fixed-length Array holding a copy of this vector's elements.
    public to_array() -> Array<T> {
        var r: Array<T> = Array(this.length());
        for (i: Integer = 0; i < this.length(); i++) {
            r[i] = this.buffer[i];
        }
        return r;
    }

    // Return the last element without removing it. Returns None when the
    // vector is empty.
    public peek() -> T? {
        if (this.is_empty()) { return None; }
        return this.buffer[this.length() - 1];
    }

    // Inserts `value` at position `index`, shifting that element and
    // everything after it one slot to the right. Panics unless `index` is in
    // `[0, length()]` (inserting at `length()` appends).
    public insert(index: Integer, value: T) -> None {
        if (index < 0 or index > this.length()) { @panic("Vector index out of bounds."); }
        if (index == this.length()) { this.push(value); return; }
        
    }

    // Removes and returns the element at `index`, shifting everything after
    // it one slot to the left. Panics if the index is out of bounds.
    public remove(index: Integer) -> T? {}

    // Removes the first element equal to `value` (compared with the
    // payload's `equals`). Does nothing when no element matches.
    public remove_value(value: T) -> None {}

    // Removes every element. The capacity is kept for reuse.
    public clear() -> None {}

    // Appends every element of `other` to this vector, in order.
    public extend(other: Vector<T>) -> None {
        for (v: T in other) {
            this.push(v);
        }
    }

    // Return the number of elements the backing storage can hold before the
    // next `push` has to grow it.
    public capacity() -> Integer { return this.buffer.capacity(); }

    // Grows the backing storage so it can hold at least `size` elements,
    // copying at most once. Call before pushing a known number of elements
    // to avoid repeated grow-and-copy cycles.
    public reserve(size: Integer) -> None {}

}
