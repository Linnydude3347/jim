class Exception {

    // Returns the message this exception was raised with, e.g. an exception
    // from an out-of-range index carries "Array index out of bounds".
    public msg() -> String { return @exc_msg(this); }

}
