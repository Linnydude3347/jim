use jimc::driver;
use std::path::PathBuf;

const USAGE: &str = "\
jimc — compiler for the jim programming language

USAGE:
    jimc build <file.j> [options]     compile to a native executable
    jimc run   <file.j> [options]     compile and run
    jimc check <file.j> [options]     parse + type-check only (library files
                                      without a main are accepted)

OPTIONS:
    -o <path>            output executable (default: next to the input)
    --emit-c <path>      also write the generated C to <path>
    --std <dir>          standard library directory (default: auto-detect / $JIM_STD)
    --cc <cmd>           C compiler to use (default: auto-detect / $JIM_CC)
    --allow-intrinsics   permit @intrinsics outside the std library (test fixtures)
    --debug              panics print full jim stack traces (default for `run`)
    --release            no panic traces, zero tracing overhead (default for `build`)
    -h, --help           show this help
    --version            show version
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(real_main(args));
}

fn real_main(args: Vec<String>) -> i32 {
    if args.iter().any(|a| a == "-h" || a == "--help") || args.is_empty() {
        print!("{}", USAGE);
        return if args.is_empty() { 2 } else { 0 };
    }
    if args.iter().any(|a| a == "--version") {
        println!("jimc {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }

    let command = args[0].clone();
    if command != "build" && command != "run" && command != "check" {
        eprintln!("jimc: unknown command '{}'\n\n{}", command, USAGE);
        return 2;
    }

    let mut input: Option<PathBuf> = None;
    // traces on for the dev loop, off for shipped binaries; flags override
    let mut debug_flag: Option<bool> = None;
    let mut opts = driver::Options {
        input: PathBuf::new(),
        output: None,
        emit_c: None,
        std_dir: None,
        cc: None,
        allow_intrinsics: false,
        debug: false,
    };

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        let take_value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("jimc: flag '{}' needs a value", a))
        };
        match a.as_str() {
            "-o" => match take_value(&mut i) {
                Ok(v) => opts.output = Some(PathBuf::from(v)),
                Err(e) => return usage_error(&e),
            },
            "--emit-c" => match take_value(&mut i) {
                Ok(v) => opts.emit_c = Some(PathBuf::from(v)),
                Err(e) => return usage_error(&e),
            },
            "--std" => match take_value(&mut i) {
                Ok(v) => opts.std_dir = Some(PathBuf::from(v)),
                Err(e) => return usage_error(&e),
            },
            "--cc" => match take_value(&mut i) {
                Ok(v) => opts.cc = Some(v),
                Err(e) => return usage_error(&e),
            },
            "--allow-intrinsics" => opts.allow_intrinsics = true,
            "--debug" => debug_flag = Some(true),
            "--release" => debug_flag = Some(false),
            other if other.starts_with('-') => {
                return usage_error(&format!("jimc: unknown flag '{}'", other));
            }
            _ => {
                if input.is_some() {
                    return usage_error("jimc: more than one input file given");
                }
                input = Some(PathBuf::from(a));
            }
        }
        i += 1;
    }

    let input = match input {
        Some(p) => p,
        None => return usage_error("jimc: no input file given"),
    };
    opts.input = input;
    opts.debug = debug_flag.unwrap_or(command == "run");

    match command.as_str() {
        "build" => match driver::build(&opts) {
            Ok(out) => {
                println!("jimc: built {}", out.display());
                0
            }
            Err(e) => {
                eprintln!("{}", e.trim_end());
                1
            }
        },
        "run" => match driver::run(&opts) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("{}", e.trim_end());
                1
            }
        },
        "check" => match driver::check(&opts) {
            Ok(()) => {
                println!("jimc: no errors");
                0
            }
            Err(e) => {
                eprintln!("{}", e.trim_end());
                1
            }
        },
        _ => unreachable!(),
    }
}

fn usage_error(msg: &str) -> i32 {
    eprintln!("{}\n\n{}", msg, USAGE);
    2
}
