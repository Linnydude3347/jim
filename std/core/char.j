class Char {

    // Chars are equal if they hold the same byte value. Backs `==` and `!=`.
    public equals(other: Char) -> Bool { return @char_eq(this, other); }

    // Returns true if this char's byte value is strictly smaller than
    // `other`'s. Backs `<`, `<=`, `>` and `>=`.
    public less_than(other: Char) -> Bool { return @char_lt(this, other); }

    // Returns this char as a one-character String.
    public to_string() -> String { return @char_to_string(this); }

    // Returns this char's byte value as an Integer, e.g. `'A'` => 65.
    public to_integer() -> Integer { return @char_to_i64(this); }

    // Returns the upper-case form of this char, e.g. `'a'` => 'A'. Chars that
    // are not lower-case letters are returned unchanged.
    public to_upper() -> Char {
        if (this.to_integer() >= 97 and this.to_integer() <= 122) {
            return @i64_to_char(@char_to_i64(this) - 32);
        }
        return this;
    }

    // Returns the lower-case form of this char, e.g. `'A'` => 'a'. Chars that
    // are not upper-case letters are returned unchanged.
    public to_lower() -> Char {
        if (this.to_integer() >= 65 and this.to_integer() <= 90) {
            return @i64_to_char(@char_to_i64(this) + 32);
        }
        return this;
    }

    // Return true if this char is a letter, `[a-z]` or `[A-Z]`.
    public is_alpha() -> Bool?{}

    // Return true if this char is a decimal digit, `[0-9]`.
    public is_digit() -> Bool?{}

    // Return true if this char is a letter or a decimal digit.
    public is_alnum() -> Bool?{}

    // Return true if this char is an upper-case letter, `[A-Z]`.
    public is_upper() -> Bool?{}

    // Return true if this char is a lower-case letter, `[a-z]`.
    public is_lower() -> Bool?{}

    // Return true if this char is whitespace: a space, tab, newline, or
    // carriage return.
    public is_space() -> Bool?{}

    // Return true if this char is punctuation: printable, but not a letter,
    // digit, or space.
    public is_punct() -> Bool?{}

    // Return true if this char is a printable ASCII character, in `[32, 126]`.
    public is_ascii_printable() -> Bool?{}

    // Return the numeric value of this digit char, e.g. `'7'` => 7. Returns
    // None for chars that are not decimal digits.
    public digit_value() -> Integer?{}

}
