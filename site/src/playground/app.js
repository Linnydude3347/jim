// The jim playground: compile jim to C11 entirely in the browser.
//
// `jim-playground` is jimc compiled to WebAssembly (see playground/wasm). Its
// `compile(source)` runs the real compiler front-end against the real standard
// library and returns the generated C — or a rendered diagnostic.
import init, { compile, version } from "./pkg/jim_playground.js";

// A menu of ready-to-run programs. Keys match the <option> values in index.html.
const EXAMPLES = {
  vec2: `#import <io>

// In jim, operators desugar to method calls: a + b becomes a.plus(b).
// Edit the code and watch the generated C11 update on the right.
class Vec2 {
    public x: Integer = 0;
    public y: Integer = 0;

    Vec2(x: Integer, y: Integer) {
        this.x = x;
        this.y = y;
    }

    public plus(other: Vec2) -> Vec2 {
        return Vec2(this.x + other.x, this.y + other.y);
    }

    public to_string() -> String {
        return "(" + this.x.to_string() + ", " + this.y.to_string() + ")";
    }
}

function main() -> Integer {
    var sum: Vec2 = Vec2(1, 2) + Vec2(3, 4);
    print(sum.to_string());
    return 0;
}
`,

  hello: `#import <io>

function main() -> Integer {
    print("Hello, jim!");
    return 0;
}
`,

  fizzbuzz: `#import <io>

function label(n: Integer) -> String? {
    if (n % 15 == 0) { return "FizzBuzz"; }
    if (n % 3 == 0) { return "Fizz"; }
    if (n % 5 == 0) { return "Buzz"; }
    return None;
}

function main() -> Integer {
    var out: String = "";
    for (n: Integer = 1; n <= 15; n++) {
        var l: String? = label(n);
        if (l != None) {
            out = out + l + " ";
        } else {
            out = out + n.to_string() + " ";
        }
    }
    print(out);
    return 0;
}
`,

  primes: `#import <io>

// Returns every prime below \`limit\` (Sieve of Eratosthenes).
function primes_below(limit: Integer) -> Vector<Integer> {
    var is_composite: Array<Bool> = Array(limit);
    for (i: Integer = 0; i < limit; i++) {
        is_composite[i] = false;
    }
    var found: Vector<Integer> = [];
    for (n: Integer = 2; n < limit; n++) {
        if (not is_composite[n]) {
            found.push(n);
            for (m: Integer = n * n; m < limit; m += n) {
                is_composite[m] = true;
            }
        }
    }
    return found;
}

function main() -> Integer {
    var primes: Vector<Integer> = primes_below(50);
    var out: String = "";
    for (p: Integer in primes) {
        out = out + p.to_string() + " ";
    }
    print(out);
    print("count = " + primes.length().to_string());
    return 0;
}
`,

  optionals: `#import <io>

function find(v: Vector<Integer>, wanted: Integer) -> Integer? {
    for (i: Integer = 0; i < v.length(); i++) {
        if (v[i] == wanted) { return i; }
    }
    return None;
}

function main() -> Integer {
    var v: Vector<Integer> = [10, 20, 30];
    var idx: Integer? = find(v, 20);
    if (idx != None) {
        print("found at index " + idx.to_string());
    } else {
        print("not found");
    }
    return 0;
}
`,

  errors: `#import <io>

function main() -> Integer {
    try {
        var v: Vector<Integer> = [1];
        var x: Integer = v[5];         // bounds panic from the std library
    } catch (e: Exception) {
        print("caught: " + e.msg());
    }
    return 0;
}
`,

  generics: `#import <io>

function largest<C, T>(seq: C) -> T {
    var best: T = seq[0];
    for (i: Integer = 1; i < seq.length(); i++) {
        if (seq[i] > best) { best = seq[i]; }
    }
    return best;
}

function main() -> Integer {
    var a: Array<Integer> = [3, 9, 4, 1];
    var big: Integer = largest(a);   // T = Integer from the expected type
    print(big.to_string());          // 9
    var c: Char = largest("hello");  // strings are sequences of Char
    print(c.to_string());            // o
    return 0;
}
`,
};

// A CodeMirror "simple mode" for jim. Declaration names (after `function` /
// `class`) get the "def" token so they render green, matching the VS Code
// theme; type names render as "variable-2" (blue).
CodeMirror.defineSimpleMode("jim", {
  start: [
    { regex: /\/\/.*/, token: "comment" },
    { regex: /"(?:[^\\"]|\\.)*"/, token: "string" },
    { regex: /'(?:[^\\']|\\.)'/, token: "string" },
    { regex: /#import\b/, token: "keyword" },
    { regex: /@[A-Za-z_]\w*/, token: "meta" }, // intrinsics
    { regex: /\b(?:function|class)\b/, token: "keyword", next: "declName" },
    {
      regex:
        /\b(?:if|else|for|while|in|break|continue|return|try|catch|var|const|public|private|and|or|not|div)\b/,
      token: "keyword",
    },
    { regex: /\b(?:true|false|None)\b/, token: "atom" },
    {
      regex: /\b(?:Integer|Float|Bool|Char|String|Exception|Array|Vector|RawBuffer)\b/,
      token: "variable-2",
    },
    { regex: /\bthis\b/, token: "variable-3" },
    { regex: /\b\d+\.\d+\b/, token: "number" },
    { regex: /\b\d+\b/, token: "number" },
    { regex: /[A-Z][A-Za-z0-9_]*/, token: "variable-2" }, // user types / constructors
  ],
  // The identifier immediately after `function`/`class` is the declared name.
  declName: [
    { regex: /\s+/ },
    { regex: /[A-Za-z_]\w*/, token: "def", next: "start" },
    { regex: /./, next: "start" },
  ],
  meta: { lineComment: "//" },
});

const editor = CodeMirror(document.getElementById("editor"), {
  value: EXAMPLES.vec2,
  mode: "jim",
  theme: "monokai",
  lineNumbers: true,
  indentUnit: 4,
  tabSize: 4,
  matchBrackets: true,
});

const output = CodeMirror(document.getElementById("output"), {
  value: "",
  mode: "text/x-csrc",
  theme: "monokai",
  lineNumbers: true,
  readOnly: true,
});

const statusEl = document.getElementById("status");
const runBtn = document.getElementById("run");
const errorEl = document.getElementById("error");
const outputEl = document.getElementById("output");
const examplesEl = document.getElementById("examples");

let ready = false;

function setStatus(text, kind) {
  statusEl.textContent = text;
  statusEl.className = "status" + (kind ? " " + kind : "");
}

function doCompile() {
  if (!ready) return;
  const res = compile(editor.getValue());
  if (res.ok) {
    errorEl.hidden = true;
    outputEl.style.display = "";
    output.setValue(res.c);
    output.refresh();
    setStatus("compiled", "ok");
  } else {
    outputEl.style.display = "none";
    errorEl.hidden = false;
    errorEl.textContent = res.error;
    setStatus("error", "err");
  }
}

// Debounced auto-compile as you type, plus the explicit button.
let timer;
editor.on("change", () => {
  clearTimeout(timer);
  timer = setTimeout(doCompile, 400);
});
runBtn.addEventListener("click", doCompile);

examplesEl.addEventListener("change", () => {
  const src = EXAMPLES[examplesEl.value];
  if (src) {
    editor.setValue(src);
    doCompile();
  }
});

init()
  .then(() => {
    ready = true;
    runBtn.disabled = false;
    document.getElementById("version").textContent = "· wasm v" + version();
    setStatus("ready", "ok");
    doCompile();
  })
  .catch((err) => {
    setStatus("failed to load", "err");
    errorEl.hidden = false;
    outputEl.style.display = "none";
    errorEl.textContent = "Could not load the compiler:\n" + err;
  });
