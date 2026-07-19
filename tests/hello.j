// Compiler test fixture — exercises milestone 1 (functions, string literals,
// intrinsic calls, return). Not part of the jim standard library.
// Build: jimc run tests/hello.j --allow-intrinsics

function greet() -> None {
    @print_string("Hello, jim!");
}

function main() -> Integer {
    greet();
    return 0;
}
