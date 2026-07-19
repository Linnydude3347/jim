// Prints the string to stdout.
function print(s: String) -> None {
    @print_string(s);
}

// Intrinsics needed for below functions

// Return one line read from stdin, without the trailing newline. Returns
// None when the end of input has been reached.
function input() -> String?{}

// Prints the string to stderr instead of stdout. Use for errors and
// warnings so they can be separated from program output.
function print_err(msg: String) -> None{}

// Return the entire contents of the file at path `file` as one string.
// Returns None if the file does not exist or cannot be read.
function read_file(file: String) -> String?{}

// Write `content` to the file at path `file`, replacing whatever the file
// held before (the file is created if it does not exist). Returns the number
// of bytes written, or None if the file could not be written.
function write_file(file: String, content: String) -> Integer?{}

// Append `content` to the end of the file at path `file` (the file is
// created if it does not exist). Returns the number of bytes written, or
// None if the file could not be written.
function append_file(file: String, content: String) -> Integer?{}

// Return true if a file exists at path `file`, false otherwise.
function file_exists(file: String) -> Bool?{}
