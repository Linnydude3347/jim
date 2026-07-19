// jim language support: diagnostics (via `jimc check`), type-aware
// completions/hovers/signature help, import hovers, and go-to-definition.
// Symbol data comes from a lightweight regex scan of the workspace's .j files
// (see jim-analysis.js); consecutive `//` lines directly above a declaration
// are its docstring, and a leading `//` block is a file's module docstring.
const vscode = require('vscode');
const cp = require('child_process');
const path = require('path');
const fs = require('fs');
const A = require('./jim-analysis');

const KEYWORDS = [
    'var', 'const', 'function', 'class', 'public', 'private',
    'if', 'else', 'for', 'while', 'in', 'break', 'continue', 'return',
    'try', 'catch', 'and', 'or', 'not', 'div', 'true', 'false', 'None', 'this',
];

const BUILTIN_TYPES = [
    'Integer', 'Float', 'Bool', 'Char', 'String', 'Exception',
    'Array', 'Vector', 'RawBuffer',
];

// @intrinsics — mirrors docs/DESIGN.md §6 (std-only).
const INTRINSICS = {
    i64_add: '(Integer, Integer) -> Integer — overflow-checked add',
    i64_sub: '(Integer, Integer) -> Integer — overflow-checked subtract',
    i64_mul: '(Integer, Integer) -> Integer — overflow-checked multiply',
    i64_divtrunc: '(Integer, Integer) -> Integer — truncating division, panics on 0',
    i64_mod: '(Integer, Integer) -> Integer — panics on 0',
    i64_neg: '(Integer) -> Integer',
    i64_eq: '(Integer, Integer) -> Bool',
    i64_lt: '(Integer, Integer) -> Bool',
    i64_to_f64: '(Integer) -> Float',
    i64_to_string: '(Integer) -> String',
    i64_to_char: '(Integer) -> Char — panics unless 0-255',
    f64_add: '(Float, Float) -> Float',
    f64_sub: '(Float, Float) -> Float',
    f64_mul: '(Float, Float) -> Float',
    f64_div: '(Float, Float) -> Float — IEEE inf/nan on /0',
    f64_neg: '(Float) -> Float',
    f64_eq: '(Float, Float) -> Bool',
    f64_lt: '(Float, Float) -> Bool',
    f64_to_i64: '(Float) -> Integer — truncates toward zero',
    f64_to_string: '(Float) -> String',
    bool_eq: '(Bool, Bool) -> Bool',
    char_eq: '(Char, Char) -> Bool',
    char_lt: '(Char, Char) -> Bool',
    char_to_i64: '(Char) -> Integer',
    char_to_string: '(Char) -> String',
    str_len: '(String) -> Integer — bytes',
    str_byte: '(String, Integer) -> Char — unchecked byte read',
    str_concat: '(String, String) -> String',
    str_eq: '(String, String) -> Bool',
    str_lt: '(String, String) -> Bool',
    str_slice: '(String, Integer, Integer) -> String — (s, start, len) zero-copy view, unchecked',
    str_from_buf: '(RawBuffer<Char>, Integer) -> String — copies len bytes out (string builder)',
    str_to_i64: '(String) -> Integer? — strict decimal parse, None if invalid',
    str_to_f64: '(String) -> Float? — strict decimal/scientific parse, None if invalid',
    f64_sqrt: '(Float) -> Float — IEEE: nan for negatives',
    f64_cbrt: '(Float) -> Float',
    f64_hypot: '(Float, Float) -> Float — sqrt(x^2 + y^2)',
    f64_exp: '(Float) -> Float — e^x',
    f64_log: '(Float) -> Float — natural log; IEEE: nan/-inf outside domain',
    f64_log2: '(Float) -> Float',
    f64_log10: '(Float) -> Float',
    f64_sin: '(Float) -> Float — radians',
    f64_cos: '(Float) -> Float — radians',
    f64_tan: '(Float) -> Float — radians',
    f64_asin: '(Float) -> Float — radians; IEEE: nan outside [-1, 1]',
    f64_acos: '(Float) -> Float — radians; IEEE: nan outside [-1, 1]',
    f64_atan: '(Float) -> Float — radians',
    f64_atan2: '(Float, Float) -> Float — (y, x), radians',
    f64_fmod: '(Float, Float) -> Float — remainder of x/y, sign of x',
    f64_pow: '(Float, Float) -> Float — x^y',
    f64_is_nan: '(Float) -> Bool',
    f64_is_inf: '(Float) -> Bool',
    f64_is_finite: '(Float) -> Bool',
    i64_and: '(Integer, Integer) -> Integer — bitwise AND',
    i64_or: '(Integer, Integer) -> Integer — bitwise OR',
    i64_xor: '(Integer, Integer) -> Integer — bitwise XOR',
    i64_not: '(Integer) -> Integer — bitwise complement',
    i64_shl: '(Integer, Integer) -> Integer — shift left; panics outside 0-63',
    i64_shr: '(Integer, Integer) -> Integer — arithmetic shift right; panics outside 0-63',
    exc_msg: '(Exception) -> String',
    print_string: '(String) -> None',
    print_err: '(String) -> None — writes to stderr',
    read_line: '() -> String? — one stdin line without the newline; None at EOF',
    read_file: '(String) -> String? — whole file; None if unreadable',
    write_file: '(String, String) -> Integer? — (path, content) bytes written; None on failure',
    append_file: '(String, String) -> Integer? — (path, content) bytes written; None on failure',
    file_exists: '(String) -> Bool',
    panic: '(String) -> None — raises; caught by the nearest try',
    buf_alloc: '(Integer) -> RawBuffer<T> — element type from context',
};

let diagnostics;
// file path -> scan result ({ functions, classes, moduleDoc, file })
const fileSymbols = new Map();

// ---------- symbol access ----------

async function scanWorkspace() {
    const files = await vscode.workspace.findFiles('**/*.j', '**/target/**');
    for (const uri of files) {
        try {
            const bytes = await vscode.workspace.fs.readFile(uri);
            fileSymbols.set(uri.fsPath, A.scanText(Buffer.from(bytes).toString('utf8'), uri.fsPath));
        } catch (_) { /* unreadable file — skip */ }
    }
}

const normPath = (p) => (p ? path.resolve(p).toLowerCase() : null);

/// The std root a file belongs to, if it is part of one: its own directory
/// (std/, tests/fake_std/, ...) or its parent (std/core/*). Null for user code.
const stdRootCache = new Map();
function stdRootOfFile(fsPath) {
    if (stdRootCache.has(fsPath)) return stdRootCache.get(fsPath);
    const dir = path.dirname(fsPath);
    let root = null;
    if (fs.existsSync(path.join(dir, 'core.j'))) root = dir;
    else if (fs.existsSync(path.join(path.dirname(dir), 'core.j'))) root = path.dirname(dir);
    const n = normPath(root);
    stdRootCache.set(fsPath, n);
    return n;
}

/// Symbols visible from `docPath`. The workspace can hold several std
/// universes (the real std/ plus test doubles like tests/fake_std/) that
/// define the same classes — only the std root governing this document
/// contributes; other roots' symbols are filtered out.
function allSymbols(docPath) {
    const activeRoot = docPath ? normPath(findStdRootFor(docPath)) : null;
    const functions = [], classes = [];
    for (const syms of fileSymbols.values()) {
        const root = syms.file ? stdRootOfFile(syms.file) : null;
        if (root !== null && root !== activeRoot) continue;
        functions.push(...syms.functions);
        classes.push(...syms.classes);
    }
    return { functions, classes };
}

// ---------- type resolution glue ----------

function docLines(doc) {
    return doc.getText().split(/\r?\n/);
}

/// Type of the expression ending just before the '.' at (line, dotCol).
function resolveReceiver(doc, line, dotCol) {
    const lines = docLines(doc);
    const { functions, classes } = allSymbols(doc.uri.fsPath);
    const ctx = {
        classes, functions,
        enclosingClass: A.enclosingClassAt(lines, line, classes),
        localType: (n) => (A.localDeclAt(lines, line, n) || {}).type || null,
    };
    return A.resolveTypeAt(lines, line, dotCol, ctx);
}

/// The class symbol + payload substitution for a resolved type string.
function classFor(typeStr, docPath) {
    const pt = typeStr && A.parseTypeStr(typeStr);
    if (!pt) return null;
    const cls = allSymbols(docPath).classes.find((c) => c.name === pt.base);
    if (!cls) return null;
    const sub = (x) => (cls.typeParam && pt.payload ? A.substWord(x, cls.typeParam, pt.payload) : x);
    const display = pt.payload ? `${pt.base}<${pt.payload}>` : pt.base;
    return { cls, sub, display };
}

function enclosingClassName(doc, line) {
    const c = A.enclosingClassAt(docLines(doc), line, allSymbols(doc.uri.fsPath).classes);
    return c ? c.name : null;
}

// ---------- std root & import resolution ----------

function findStdRootFor(docPath) {
    const cfg = vscode.workspace.getConfiguration('jim').get('stdPath');
    const wsRoot = (vscode.workspace.workspaceFolders || [])[0]?.uri.fsPath;
    if (cfg) return path.isAbsolute(cfg) ? cfg : path.resolve(wsRoot || '.', cfg);
    const dir = path.dirname(docPath);
    if (fs.existsSync(path.join(dir, 'core.j'))) return dir;
    if (fs.existsSync(path.join(path.dirname(dir), 'core.j'))) return path.dirname(dir);
    for (const f of vscode.workspace.workspaceFolders || []) {
        const p = path.join(f.uri.fsPath, 'std');
        if (fs.existsSync(p)) return p;
    }
    return null;
}

/// Mirror of the compiler's resolution: <name> -> std/name.j, then
/// std/core/name.j; "path.j" -> relative to the importing file.
function resolveImportTarget(docPath, imp) {
    if (imp.kind === 'local') {
        const p = path.resolve(path.dirname(docPath), imp.name);
        return fs.existsSync(p) ? p : null;
    }
    const root = findStdRootFor(docPath);
    if (!root) return null;
    const direct = path.join(root, `${imp.name}.j`);
    if (fs.existsSync(direct)) return direct;
    const coreSub = path.join(root, 'core', `${imp.name}.j`);
    if (fs.existsSync(coreSub)) return coreSub;
    return null;
}

function symbolsForFile(file) {
    let syms = fileSymbols.get(file);
    if (!syms) {
        try {
            syms = A.scanText(fs.readFileSync(file, 'utf8'), file);
            fileSymbols.set(file, syms);
        } catch (_) {
            return null;
        }
    }
    return syms;
}

// ---------- diagnostics via jimc check ----------

function findJimc() {
    const cfg = vscode.workspace.getConfiguration('jim').get('compilerPath');
    if (cfg) return cfg;
    const exe = process.platform === 'win32' ? 'jimc.exe' : 'jimc';
    for (const f of vscode.workspace.workspaceFolders || []) {
        const p = path.join(f.uri.fsPath, 'compiler', 'target', 'release', exe);
        if (fs.existsSync(p)) return p;
    }
    return 'jimc'; // hope it's on PATH
}

function runCheck(doc) {
    if (doc.languageId !== 'jim') return;
    if (!vscode.workspace.getConfiguration('jim').get('checkOnSave')) return;
    const jimc = findJimc();
    const args = ['check', doc.uri.fsPath];
    let stdPath = vscode.workspace.getConfiguration('jim').get('stdPath');
    if (!stdPath) {
        // a file sitting next to a core.j belongs to that std root
        // (e.g. std/ itself, or test-double directories like tests/fake_std);
        // files in std/core/ belong to the parent root
        const dir = path.dirname(doc.uri.fsPath);
        if (fs.existsSync(path.join(dir, 'core.j'))) stdPath = dir;
        else if (fs.existsSync(path.join(path.dirname(dir), 'core.j'))) stdPath = path.dirname(dir);
    }
    if (stdPath) args.push('--std', stdPath);
    const cwd = (vscode.workspace.workspaceFolders || [])[0]?.uri.fsPath;
    cp.execFile(jimc, args, { cwd }, (err, _stdout, stderr) => {
        diagnostics.clear();
        if (!err) return;
        if (err.code === 'ENOENT') {
            vscode.window.showWarningMessage(
                `jim: compiler not found ('${jimc}'). Set "jim.compilerPath" in settings.`);
            return;
        }
        // jimc reports: path:line:col: error: message
        const m = String(stderr).match(/^(.*?):(\d+):(\d+): error: (.*)$/m);
        if (!m) return;
        const [, file, lineS, colS, message] = m;
        const line = Math.max(0, parseInt(lineS, 10) - 1);
        const col = Math.max(0, parseInt(colS, 10) - 1);
        const range = new vscode.Range(line, col, line, col + 1);
        const diag = new vscode.Diagnostic(range, message, vscode.DiagnosticSeverity.Error);
        diag.source = 'jimc';
        diagnostics.set(vscode.Uri.file(path.resolve(cwd || '.', file)), [diag]);
    });
}

// ---------- completion ----------

function memberItems(doc, pos, prefix) {
    const dotCol = prefix.lastIndexOf('.');
    const t = resolveReceiver(doc, pos.line, dotCol);
    const resolved = t && classFor(t, doc.uri.fsPath);
    const items = [];

    if (resolved) {
        // type-aware: only this class's members, payload substituted
        const { cls, sub, display } = resolved;
        const inSelf = enclosingClassName(doc, pos.line) === cls.name;
        for (const m of cls.methods) {
            if (m.vis === 'private' && !inSelf) continue;
            const it = new vscode.CompletionItem(
                { label: m.name, detail: `(${sub(m.params)})` },
                vscode.CompletionItemKind.Method
            );
            it.detail = `${display} — ${sub(m.sig)}`;
            if (m.doc) it.documentation = new vscode.MarkdownString(m.doc);
            items.push(it);
        }
        for (const f of cls.fields) {
            if (f.vis === 'private' && !inSelf) continue;
            const it = new vscode.CompletionItem(
                { label: f.name, detail: `: ${sub(f.type)}` },
                vscode.CompletionItemKind.Field
            );
            it.detail = `${display} — ${sub(f.sig)}`;
            if (f.doc) it.documentation = new vscode.MarkdownString(f.doc);
            items.push(it);
        }
        return items;
    }

    // fallback: type unknown — offer every public member
    for (const c of allSymbols(doc.uri.fsPath).classes) {
        for (const mth of c.methods) {
            if (mth.vis === 'private') continue;
            const it = new vscode.CompletionItem(
                { label: mth.name, detail: `(${mth.params})` },
                vscode.CompletionItemKind.Method
            );
            it.detail = `${c.name}${c.generic} — ${mth.sig}`;
            if (mth.doc) it.documentation = new vscode.MarkdownString(mth.doc);
            items.push(it);
        }
        for (const f of c.fields) {
            if (f.vis === 'private') continue;
            const it = new vscode.CompletionItem(
                { label: f.name, detail: `: ${f.type}` },
                vscode.CompletionItemKind.Field
            );
            it.detail = `${c.name}${c.generic} — ${f.sig}`;
            if (f.doc) it.documentation = new vscode.MarkdownString(f.doc);
            items.push(it);
        }
    }
    return items;
}

function completionItems(doc, pos) {
    const prefix = doc.lineAt(pos).text.slice(0, pos.character);
    const items = [];

    if (/@\w*$/.test(prefix)) {
        // Pin the replace range to cover the '@' and everything typed after
        // it — otherwise VS Code fuzzy-filters the typed word against labels
        // that start with '@' and quietly drops most of the list.
        const atCol = prefix.lastIndexOf('@');
        const range = new vscode.Range(pos.line, atCol, pos.line, pos.character);
        for (const [name, sig] of Object.entries(INTRINSICS)) {
            const it = new vscode.CompletionItem(`@${name}`, vscode.CompletionItemKind.Function);
            it.range = range;
            it.insertText = `@${name}`;
            it.filterText = `@${name}`;
            it.sortText = name;
            it.detail = `@${name}${sig}`;
            it.documentation = 'Intrinsic — only usable inside the standard library.';
            items.push(it);
        }
        return items;
    }

    if (/\.\w*$/.test(prefix)) {
        return memberItems(doc, pos, prefix);
    }

    const { functions, classes } = allSymbols(doc.uri.fsPath);
    for (const kw of KEYWORDS) {
        items.push(new vscode.CompletionItem(kw, vscode.CompletionItemKind.Keyword));
    }
    for (const t of BUILTIN_TYPES) {
        const it = new vscode.CompletionItem(t, vscode.CompletionItemKind.Class);
        it.detail = 'built-in type';
        items.push(it);
    }
    for (const fn of functions) {
        const it = new vscode.CompletionItem(
            { label: fn.name, detail: `${fn.generics || ''}(${fn.params})` },
            vscode.CompletionItemKind.Function
        );
        it.detail = fn.sig;
        if (fn.doc) it.documentation = new vscode.MarkdownString(fn.doc);
        items.push(it);
    }
    for (const c of classes) {
        const it = new vscode.CompletionItem(c.name, vscode.CompletionItemKind.Class);
        it.detail = `class ${c.name}${c.generic}`;
        if (c.doc) it.documentation = new vscode.MarkdownString(c.doc);
        items.push(it);
    }
    return items;
}

// ---------- signature help ----------

/// Walk backwards from the cursor to the open paren of the enclosing call.
/// Returns the callee, whether it's a method call, the argument index, and
/// the document position where the callee starts.
function findCallContext(doc, pos) {
    const startLine = Math.max(0, pos.line - 20);
    let text = '';
    const lineStarts = []; // index into text -> line number
    for (let l = startLine; l <= pos.line; l++) {
        lineStarts.push({ idx: text.length, line: l });
        const lineText = doc.lineAt(l).text;
        text += (l === pos.line ? lineText.slice(0, pos.character) : lineText) + '\n';
    }
    const toPos = (idx) => {
        let hit = lineStarts[0];
        for (const ls of lineStarts) {
            if (ls.idx <= idx) hit = ls;
            else break;
        }
        return new vscode.Position(hit.line, idx - hit.idx);
    };
    // neutralize comments and string/char contents so their parens don't count
    text = text.replace(/\/\/[^\n]*/g, (m) => ' '.repeat(m.length));
    text = text.replace(/"(\\.|[^"\\\n])*"?/g, (m) => '"' + '~'.repeat(Math.max(0, m.length - 2)) + (m.length > 1 ? '"' : ''));
    text = text.replace(/'(\\.|[^'\\\n])*'?/g, (m) => "'" + '~'.repeat(Math.max(0, m.length - 2)) + (m.length > 1 ? "'" : ''));

    let depth = 0;
    for (let i = text.length - 1; i >= 0; i--) {
        const ch = text[i];
        if (ch === ')' || ch === ']') {
            depth++;
        } else if (ch === '[') {
            if (depth > 0) depth--;
        } else if (ch === '(') {
            if (depth > 0) {
                depth--;
                continue;
            }
            const before = text.slice(0, i);
            const m = before.match(/(@?[A-Za-z_][A-Za-z0-9_]*)\s*(<[^>()]*>)?\s*$/);
            if (!m) return null;
            const calleeIdx = before.length - m[0].length;
            const isMethod = /\.\s*@?[A-Za-z_][A-Za-z0-9_]*\s*(<[^>()]*>)?\s*$/.test(before);
            // the '.' before the callee, for receiver typing
            let dotIdx = -1;
            for (let k = calleeIdx - 1; k >= 0; k--) {
                if (/\s/.test(text[k])) continue;
                if (text[k] === '.') dotIdx = k;
                break;
            }
            let commas = 0;
            let d = 0;
            for (let j = i + 1; j < text.length; j++) {
                const c = text[j];
                if (c === '(' || c === '[') d++;
                else if (c === ')' || c === ']') d--;
                else if (c === ',' && d === 0) commas++;
            }
            return {
                callee: m[1],
                isMethod,
                activeParameter: commas,
                dotPos: dotIdx >= 0 ? toPos(dotIdx) : null,
            };
        }
    }
    return null;
}

function sigInfoFromParams(label, paramsText, docstr) {
    const si = new vscode.SignatureInformation(
        label,
        docstr ? new vscode.MarkdownString(docstr) : undefined
    );
    const params = paramsText === '' ? [] : paramsText.split(',').map((s) => s.trim());
    si.parameters = params.map((p) => new vscode.ParameterInformation(p));
    return si;
}

function signatureHelp(doc, pos) {
    const ctx = findCallContext(doc, pos);
    if (!ctx) return null;
    const sigs = [];

    if (ctx.callee.startsWith('@')) {
        const name = ctx.callee.slice(1);
        const raw = INTRINSICS[name];
        if (raw) {
            const paramsText = (raw.match(/^\(([^)]*)\)/) || [, ''])[1].trim();
            sigs.push(sigInfoFromParams(
                `@${name}${raw}`,
                paramsText,
                'Intrinsic — only usable inside the standard library.'
            ));
        }
    } else {
        const { functions, classes } = allSymbols(doc.uri.fsPath);
        if (ctx.isMethod) {
            // type-aware: resolve the receiver, show only that class's method
            const t = ctx.dotPos && resolveReceiver(doc, ctx.dotPos.line, ctx.dotPos.character);
            const resolved = t && classFor(t, doc.uri.fsPath);
            if (resolved) {
                const { cls, sub, display } = resolved;
                const m = cls.methods.find((mm) => mm.name === ctx.callee);
                if (m) sigs.push(sigInfoFromParams(`${display}.${sub(m.sig)}`, sub(m.params), m.doc));
            }
            if (sigs.length === 0) {
                for (const c of classes) {
                    for (const m of c.methods) {
                        if (m.name === ctx.callee) {
                            sigs.push(sigInfoFromParams(`${c.name}${c.generic}.${m.sig}`, m.params, m.doc));
                        }
                    }
                }
            }
        } else {
            for (const fn of functions) {
                if (fn.name === ctx.callee) {
                    sigs.push(sigInfoFromParams(fn.sig, fn.params, fn.doc));
                }
            }
            for (const c of classes) {
                if (c.name === ctx.callee) {
                    // constructor call (auto default when none is declared)
                    const p = c.ctor ? c.ctor.params : '';
                    const docstr = (c.ctor && c.ctor.doc) || c.doc;
                    sigs.push(sigInfoFromParams(`${c.name}${c.generic}(${p})`, p, docstr));
                }
            }
        }
    }

    if (sigs.length === 0) return null;
    const help = new vscode.SignatureHelp();
    help.signatures = sigs;
    help.activeSignature = 0;
    help.activeParameter = ctx.activeParameter;
    return help;
}

// ---------- hover ----------

function importHover(doc, imp) {
    const md = new vscode.MarkdownString();
    const target = resolveImportTarget(doc.uri.fsPath, imp);
    if (!target) {
        md.appendCodeblock(`#import ${imp.kind === 'std' ? `<${imp.name}>` : `"${imp.name}"`}`, 'jim');
        md.appendMarkdown('*Cannot resolve this import (no std root found or file missing).*');
        return new vscode.Hover(md);
    }
    const syms = symbolsForFile(target);
    md.appendCodeblock(`module ${imp.kind === 'std' ? `<${imp.name}>` : imp.name}`, 'jim');
    md.appendMarkdown(`\`${target}\`\n\n`);
    if (syms && syms.moduleDoc) md.appendMarkdown(syms.moduleDoc + '\n\n');
    if (syms) {
        const decls = [
            ...syms.classes.map((c) => `- \`class ${c.name}${c.generic}\``),
            ...syms.functions.slice(0, 10).map((f) => `- \`${f.sig}\``),
        ];
        if (syms.functions.length > 10) decls.push(`- *…and ${syms.functions.length - 10} more*`);
        if (decls.length) md.appendMarkdown(`**declares**\n\n${decls.join('\n')}`);
    }
    return new vscode.Hover(md);
}

function hoverFor(doc, pos) {
    const lineText = doc.lineAt(pos.line).text;
    const imp = A.parseImportLine(lineText);
    if (imp) return importHover(doc, imp);

    const range = doc.getWordRangeAtPosition(pos, /[@A-Za-z_][A-Za-z0-9_]*/);
    if (!range) return null;
    const word = doc.getText(range);

    if (word.startsWith('@')) {
        const sig = INTRINSICS[word.slice(1)];
        if (!sig) return null;
        const md = new vscode.MarkdownString();
        md.appendCodeblock(`${word}${sig}`, 'jim');
        md.appendMarkdown('Intrinsic — only usable inside the standard library (docs/DESIGN.md §6).');
        return new vscode.Hover(md, range);
    }

    const { functions, classes } = allSymbols(doc.uri.fsPath);
    const show = (sig, docstr, extra) => {
        const md = new vscode.MarkdownString();
        md.appendCodeblock(sig, 'jim');
        if (docstr) md.appendMarkdown(docstr);
        if (extra) md.appendMarkdown(extra);
        return new vscode.Hover(md, range);
    };

    // member access with a receiver: `expr.word` — show ONLY that class's signature
    const before = lineText.slice(0, range.start.character);
    if (/\.\s*$/.test(before)) {
        const t = resolveReceiver(doc, pos.line, before.lastIndexOf('.'));
        const resolved = t && classFor(t, doc.uri.fsPath);
        if (resolved) {
            const { cls, sub, display } = resolved;
            const m = [...cls.methods, ...cls.fields].find((mm) => mm.name === word);
            if (m) return show(`${display}.${sub(m.sig)}`, m.doc);
        }
    }

    for (const fn of functions) {
        if (fn.name === word) return show(fn.sig, fn.doc);
    }
    for (const c of classes) {
        if (c.name === word) {
            const members = [...c.methods, ...c.fields]
                .filter((m) => m.vis === 'public')
                .map((m) => `- \`${m.sig}\``).join('\n');
            const extra = members ? `\n\n**public members**\n\n${members}` : '';
            return show(`class ${c.name}${c.generic}`, c.doc, extra);
        }
    }
    // a local variable: show its declared type
    {
        const local = A.localDeclAt(docLines(doc), pos.line, word);
        if (local) return show(`var ${word}: ${local.type}`, null);
    }
    // methods/fields with no resolvable receiver: every class that has one
    const hits = [];
    for (const c of classes) {
        for (const m of [...c.methods, ...c.fields]) {
            if (m.name === word) hits.push({ c, m });
        }
    }
    if (hits.length > 0) {
        const md = new vscode.MarkdownString();
        for (const { c, m } of hits.slice(0, 5)) {
            md.appendCodeblock(`${c.name}${c.generic}.${m.sig}`, 'jim');
            if (m.doc) md.appendMarkdown(m.doc + '\n\n');
        }
        return new vscode.Hover(md, range);
    }
    return null;
}

// ---------- go to definition ----------

function definitionFor(doc, pos) {
    const lineText = doc.lineAt(pos.line).text;
    const imp = A.parseImportLine(lineText);
    if (imp) {
        const target = resolveImportTarget(doc.uri.fsPath, imp);
        return target
            ? new vscode.Location(vscode.Uri.file(target), new vscode.Position(0, 0))
            : null;
    }

    const range = doc.getWordRangeAtPosition(pos, /[A-Za-z_][A-Za-z0-9_]*/);
    if (!range) return null;
    const word = doc.getText(range);
    const { functions, classes } = allSymbols(doc.uri.fsPath);
    const loc = (file, line) =>
        file ? new vscode.Location(vscode.Uri.file(file), new vscode.Position(line || 0, 0)) : null;

    // member access: resolve the receiver's class, jump to the member
    const before = lineText.slice(0, range.start.character);
    if (/\.\s*$/.test(before)) {
        const t = resolveReceiver(doc, pos.line, before.lastIndexOf('.'));
        const resolved = t && classFor(t, doc.uri.fsPath);
        if (resolved) {
            const m = [...resolved.cls.methods, ...resolved.cls.fields].find((mm) => mm.name === word);
            if (m) return loc(resolved.cls.file, m.line);
        }
        for (const c of classes) {
            const m = [...c.methods, ...c.fields].find((mm) => mm.name === word);
            if (m) return loc(c.file, m.line);
        }
        return null;
    }

    const fn = functions.find((f) => f.name === word);
    if (fn) return loc(fn.file, fn.line);
    const cls = classes.find((c) => c.name === word);
    if (cls) return loc(cls.file, cls.line);

    // a local variable: jump to its declaration in this file
    const local = A.localDeclAt(docLines(doc), pos.line, word);
    if (local) return new vscode.Location(doc.uri, new vscode.Position(local.line, 0));
    return null;
}

// ---------- activation ----------

function activate(context) {
    diagnostics = vscode.languages.createDiagnosticCollection('jim');
    context.subscriptions.push(diagnostics);

    scanWorkspace();

    let rescanTimer;
    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument((e) => {
            if (e.document.languageId !== 'jim') return;
            clearTimeout(rescanTimer);
            rescanTimer = setTimeout(() => {
                fileSymbols.set(e.document.uri.fsPath, A.scanText(e.document.getText(), e.document.uri.fsPath));
            }, 400);
        }),
        vscode.workspace.onDidSaveTextDocument((doc) => {
            if (doc.languageId !== 'jim') return;
            fileSymbols.set(doc.uri.fsPath, A.scanText(doc.getText(), doc.uri.fsPath));
            runCheck(doc);
        }),
        vscode.workspace.onDidOpenTextDocument((doc) => runCheck(doc)),
        vscode.languages.registerCompletionItemProvider(
            'jim',
            { provideCompletionItems: (doc, pos) => completionItems(doc, pos) },
            '.', '@'
        ),
        vscode.languages.registerHoverProvider('jim', {
            provideHover: (doc, pos) => hoverFor(doc, pos),
        }),
        vscode.languages.registerSignatureHelpProvider(
            'jim',
            { provideSignatureHelp: (doc, pos) => signatureHelp(doc, pos) },
            '(', ','
        ),
        vscode.languages.registerDefinitionProvider('jim', {
            provideDefinition: (doc, pos) => definitionFor(doc, pos),
        })
    );

    if (vscode.window.activeTextEditor) runCheck(vscode.window.activeTextEditor.document);
}

function deactivate() {}

module.exports = { activate, deactivate };
