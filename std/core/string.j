// NOTE: All string methods return new values. They do not change the original string.
#import <io>
class String {

    // Appends `other` string to this string and returns the result
    public plus(other: String) -> String { return @str_concat(this, other); }

    // Strings are equal if they have the same size and contain the same characters
    public equals(other: String) -> Bool { return @str_eq(this, other); }

    // Less than is determined byte-wise, length as tiebreaker
    public less_than(other: String) -> Bool { return @str_lt(this, other); }

    // Returns the number of characters in the string
    public length() -> Integer { return @str_len(this); }

    // Needed for print function, simply returns this object
    public to_string() -> String { return this; }

    // Returns the character at the specified index
    public get(index: Integer) -> Char {
        if (index < 0 or index >= this.length()) {
            @panic("String index out of range.");
        }
        return @str_byte(this, index);
    }

    // Returns True if the passed character exists in the string, false otherwise
    public contains(value: Char) -> Bool {
        for (c: Char in this) {
            if (c == value) {
                return true;
            }
        }
        return false;
    }

    // Returns a substring containing `[start, end]` characters. So, `"abcdefg".substr(1, 2) => "bc"`.
    public substr(start: Integer, end: Integer) -> String {
        var result: String = "";
        for (i: Integer = start; i <= end; i++) {
            result += this[i].to_string();
        }
        return result;
    }

    // Converts the first character to upper case
    public capitalize() -> String {
        return this[0].to_upper().to_string() + this.substr(1, this.length() - 1);
    }

    // Converts string into upper case
    public upper() -> String {
        var result: String = "";
        for (c: Char in this) {
            result += c.to_upper().to_string();
        }
        return result;
    }

    // Converts string into lower case
    public lower() -> String {
        var result: String = "";
        for (c: Char in this) {
            result += c.to_lower().to_string();
        }
        return result;
    }

    // Returns the number of times a character occurs in a string
    public count(value: Char) -> Integer {
        var result: Integer = 0;
        for (c: Char in this) {
            if (c == value) {
                result += 1;
            }
        }
        return result;
    }

    // Returns true if the string starts with a specified value
    public startsWith(value: String) -> Bool {
        return this.substr(0, value.length() - 1) == value;
    }

    // Returns true if the string ends with a specified value
    public endsWith(value: String) -> Bool {
        return this.substr(this.length() - value.length(), this.length() - 1) == value;
    }

    // Searches the string for a specified value and returns the position of where it was found (-1 if not found)
    public index(value: String) -> Integer {
        if (value.length() > this.length()) { return -1; }
        for (i: Integer = 0; i <= this.length() - value.length(); i++) {
            if (this.substr(i, i + value.length() - 1) == value) {
                return i;
            }
        }
        return -1;
    }

    // Returns true if all characters in the string are alphanumeric
    public isAlnum() -> Bool {
        for (c: Char in this) {
            if (not c.is_alnum()) { return false; }
        }
        return true;
    }

    // Returns true if all characters in the string are in the alphabet
    public isAlpha() -> Bool {
        for (c: Char in this) {
            if (not c.is_alpha()) { return false; }
        }
        return true;
    }

    // Returns true if all characters in the string are ascii characters [32, 127]
    public isAscii() -> Bool {
        for (c: Char in this) {
            if (not c.is_ascii_printable()) { return false; }
        }
        return true;
    }

    // Returns true if all characters in the string are decimals
    public isDecimal() -> Bool {
        for (c: Char in this) {
            if (not c.is_digit() or c != '.') { return false; }
        }
        return true;
    }

    // Returns true if all characters in the string are lower case
    public isLower() -> Bool {
        for (c: Char in this) {
            if (c != c.to_lower()) { return false; }
        }
        return true;
    }

    // Returns true if all characters in the string are whitespaces
    public isSpace() -> Bool {
        for (c: Char in this) {
            if (not c.is_space()) { return false; }
        }
        return true;
    }

    // Returns true if all characters in the string are upper case
    public isUpper() -> Bool {
        for (c: Char in this) {
            if (c != c.to_upper()) { return false; }
        }
        return true;
    }

    // Splits the string at the specified separator and returns a list
    public split(delim: Char) -> Array<String> {

        if (this == "") return [];

        // Determine length of result array. We cannot initialize by count(delim) + 1, as
        // strings can be either "1,2,3" or "1,2,". Avoid over-allocating memory whenever possible.
        // Assume "1,2," first, then if it doesn't end with delim, add 1
        //
        // We do not use endsWith (allocate + n^4 builds). Instead, a simply length
        // check and last char check is all we need.
        var result_size: Integer = this.count(delim);
        if (this.length() > 0 and this[this.length() - 1] != delim) {
            result_size += 1;
        }

        var result: Array<String> = Array(result_size);
        var temp: String = "";
        var index: Integer = 0;

        for (c: Char in this) {
            if (c == delim) {
                result[index] = temp;
                index++;
                temp = "";
            } else {
                temp += c.to_string();
            }
        }

        if (temp != "") {
            result[index] = temp;
        }

        return result;
        
    }

    // Returns this string with leading and trailing whitespace removed. `'  abc  ' => 'abc'`
    public trim() -> String {
        return this.rtrim().ltrim();
    }

    // Returns this string with leading whitespace removed. `'  abc  ' => 'abc  '`
    public ltrim() -> String {
        var index: Integer = 0;
        while (this[index].is_space()) { index++; }
        return this.substr(index, this.length() - 1);
    }

    // Returns this string with trailing whitespace removed. `'  abc  ' => '  abc'`
    public rtrim() -> String {
        var index: Integer = this.length() - 1;
        while (this[index].is_space()) { index--; }
        return this.substr(0, index);
    }

    // Returns this string with every occurrence of `target` replaced by
    // `new`. Occurrences are found left to right and do not overlap. This
    // function is specifically for replacing characters, not strings.
    public replace_char(target: Char, new: Char) -> String {
        var r: String = "";
        for (c: Char in this) {
            if (c == target) { r += new.to_string(); }
            else { r += c.to_string(); }
        }
        return r;
    }

    // Returns this string with every occurrence of `target` replaced by
    // `new`. Occurrences are found left to right and do not overlap. This
    // function is specifically for replacing strings, not characters.
    //
    // If the target string is longer than the source string, None is returned.
    public replace(target: String, new: String) -> String? {
        if (target.length() > this.length()) { return None; }
        var r: String = "";
        var end: Integer = this.length() - target.length() + 1;
        var i: Integer = 0;

        // NOTE: This works for replacing strings at the start or end of the string
        // TODO: Does not currently work for replacing segments in the middle of the string,
        // or replacing multiple segments in one string
        while (i <= end) {
            var s: String = this.substr(i, i + target.length() - 1);
            print(s);
            if (i == end - 1) {
                if (s != target) {
                    r += s;
                } else {
                    r += new;
                }
                return r;
            }
            if (s != target) {
                r += this[i].to_string();
                i++;
            } else {
                r += new;
                i += new.length() + 1;
            }
        }
        return r;
    }

    // Returns this string repeated `n` times, e.g. `"ab".repeat(3)` =>
    // "ababab". Does not accept negative counts. If a negative count is
    // passed, an empty string will be returned.
    public repeat(n: Integer) -> String {
        var r: String = "";
        for (i: Integer = 0; i < n; i++) {
            r += this;
        }
        return r;
    }

    // Returns this string with its characters in reverse order.
    public reverse() -> String {
        var r: String = "";
        for (i: Integer = this.length() - 1; i >= 0; i--) {
            r += this[i].to_string();
        }
        return r;
    }

    // Searches the string backwards for `value` and returns the position of
    // the last occurrence (-1 if not found).
    public lastIndex(value: String) -> Integer?{}

    // Splits this string at newlines and returns the lines. The line breaks
    // themselves are not included in the results.
    public lines() -> Array<String> { return this.split('\n'); }

    // Joins `values` into one string with this string between each pair,
    // e.g. `", ".join(["a", "b", "c"])` => `"a, b, c"`.
    public join(values: Array<String>) -> String?{}

    // Returns this string padded on the left with `c` until it is `n`
    // characters long. Strings already `n` or more characters long are
    // returned unchanged.
    public padLeft(c: Char, n: Integer) -> String?{}

    // Returns this string padded on the right with `c` until it is `n`
    // characters long. Strings already `n` or more characters long are
    // returned unchanged.
    public padRight(c: Char, n: Integer) -> String?{}

    // Returns this string without `prefix` if it starts with it, otherwise
    // returns the string unchanged.
    public removePrefix(prefix: String) -> String?{}

    // Returns this string without `suffix` if it ends with it, otherwise
    // returns the string unchanged.
    public removeSuffix(suffix: String) -> String?{}

    // Returns this string with the case of every letter flipped: upper case
    // becomes lower case and vice versa.
    public swapCase() -> String?{}

    // Returns this string in title case: the first letter of every word
    // upper case, the rest lower case. Words are separated by whitespace.
    public title() -> String?{}

    // Returns true if the string is in title case (see `title`).
    public isTitle() -> Bool?{}

    // Strings are compared as if both were lower case, so `"JIM"` equals
    // `"jim"`.
    public equalsIgnoreCase(other: String) -> Bool { return this.lower() == other.lower(); }

    // Intrinsics needed for below functions

    // Parses this string as a decimal integer, e.g. `"-42"` => -42. Returns
    // None if the string is not a valid integer.
    public to_integer() -> Integer { return @str_to_i64(this); }

    // Parses this string as a decimal number, e.g. `"2.5"` => 2.5. Returns
    // None if the string is not a valid number.
    public to_float() -> Float { return @str_to_f64(this); }

}