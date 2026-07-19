class Bool {

    // Bools are equal if they hold the same truth value. Backs `==` and `!=`.
    public equals(other: Bool) -> Bool { return @bool_eq(this, other); }

    // Returns "true" or "false".
    public to_string() -> String { if (this) { return "true"; } return "false"; }

    // Returns 1 for true and 0 for false.
    public to_integer() -> Integer { if (this) { return 1; } return 0; }

}
