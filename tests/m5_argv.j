// Compiler test fixture — main(argv) form. argv[0] is the executable path.
// Run: jimc run tests/m5_argv.j --std tests/fake_std

#import <io>

function main(argv: Array<String>) -> Integer {
    print("args = " + argv.length().to_string());
    for (a: String in argv) {
        print("arg: " + a);
    }
    return 0;
}
