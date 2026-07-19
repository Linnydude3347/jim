// The jim playground: compile jim to C11 entirely in the browser.
//
// `jim-playground` is jimc compiled to WebAssembly (see playground/wasm). Its
// `compile(source)` runs the real compiler front-end against the real standard
// library and returns the generated C — or a rendered diagnostic.
import init, { compile, version } from "./pkg/jim_playground.js";

const DEFAULT_PROGRAM = `#import <io>

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
`;

// A CodeMirror "simple mode" for jim — mirrors the VS Code TextMate grammar's
// token classes closely enough for the Monokai theme to color them.
CodeMirror.defineSimpleMode("jim", {
  start: [
    { regex: /\/\/.*/, token: "comment" },
    { regex: /"(?:[^\\"]|\\.)*"/, token: "string" },
    { regex: /'(?:[^\\']|\\.)'/, token: "string" },
    { regex: /#import\b/, token: "keyword" },
    { regex: /@[A-Za-z_]\w*/, token: "meta" }, // intrinsics
    {
      regex:
        /\b(?:if|else|for|while|in|break|continue|return|try|catch|var|const|function|class|public|private|and|or|not|div)\b/,
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
    { regex: /[A-Z][A-Za-z0-9_]*/, token: "def" }, // types / constructors
  ],
  meta: { lineComment: "//" },
});

const editor = CodeMirror(document.getElementById("editor"), {
  value: DEFAULT_PROGRAM,
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
