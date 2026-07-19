// jim-analysis.js — pure language-analysis helpers for the jim extension.
// No vscode dependency, so everything here is unit-testable with plain node.
//
// The centerpiece is the receiver-type resolver: given a cursor position
// right after a '.', it walks the postfix chain backwards ("parts[1].upper"
// → base `parts`, index step, method step), types the base from local
// declarations / literals / `this`, then walks the chain forward through the
// scanned class signatures — substituting container payloads (Vector<Integer>
// methods report Integer, not T).

// ---------- scanning ----------

function docAbove(lines, i) {
    const out = [];
    for (let j = i - 1; j >= 0; j--) {
        const t = lines[j].trim();
        if (t.startsWith('//')) out.unshift(t.replace(/^\/\/\s?/, ''));
        else break;
    }
    return out.join('\n');
}

/// The module docstring: consecutive `//` lines at the very top of the file.
function moduleDocOf(lines) {
    const out = [];
    for (const raw of lines) {
        const t = raw.trim();
        if (t.startsWith('//')) out.push(t.replace(/^\/\/\s?/, ''));
        else if (t === '') { if (out.length) break; }
        else break;
    }
    return out.join('\n');
}

function scanText(text, filePath) {
    const lines = text.split(/\r?\n/);
    const syms = { functions: [], classes: [], moduleDoc: moduleDocOf(lines), file: filePath || null };
    let currentClass = null;
    let depth = 0;
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i].replace(/\/\/.*$/, '');
        let m;
        if ((m = line.match(/^\s*class\s+([A-Za-z_]\w*)\s*(<\s*\w+\s*>)?/))) {
            const generic = (m[2] || '').replace(/\s/g, '');
            currentClass = {
                name: m[1],
                generic,
                typeParam: generic ? generic.slice(1, -1) : null,
                methods: [], fields: [],
                doc: docAbove(lines, i), line: i, file: filePath || null,
            };
            syms.classes.push(currentClass);
        } else if ((m = line.match(/^\s*function\s+([A-Za-z_]\w*)\s*(<[^>()]*>)?\s*\(([^)]*)\)\s*->\s*([^\s{][^{]*)/))) {
            const generics = (m[2] || '').replace(/\s+/g, ' ');
            syms.functions.push({
                name: m[1],
                generics,
                typeParams: generics ? generics.slice(1, -1).split(',').map((s) => s.trim()) : [],
                params: m[3].trim(),
                ret: m[4].trim(),
                sig: `function ${m[1]}${generics}(${m[3].trim()}) -> ${m[4].trim()}`,
                doc: docAbove(lines, i), line: i, file: filePath || null,
            });
        } else if (currentClass && depth >= 1 &&
            (m = line.match(/^\s*(public|private)\s+([A-Za-z_]\w*)\s*\(([^)]*)\)\s*->\s*([^\s{][^{]*)/))) {
            currentClass.methods.push({
                name: m[2],
                params: m[3].trim(),
                ret: m[4].trim(),
                sig: `${m[1]} ${m[2]}(${m[3].trim()}) -> ${m[4].trim()}`,
                vis: m[1], className: currentClass.name,
                doc: docAbove(lines, i), line: i,
            });
        } else if (currentClass && depth >= 1 &&
            (m = line.match(/^\s*([A-Za-z_]\w*)\s*\(([^)]*)\)\s*\{/)) &&
            m[1] === currentClass.name) {
            currentClass.ctor = { params: m[2].trim(), doc: docAbove(lines, i), line: i };
        } else if (currentClass && depth >= 1 &&
            (m = line.match(/^\s*(public|private)\s+([A-Za-z_]\w*)\s*:\s*([^=]+)=/))) {
            currentClass.fields.push({
                name: m[2],
                type: m[3].trim(),
                sig: `${m[1]} ${m[2]}: ${m[3].trim()}`,
                vis: m[1], className: currentClass.name,
                doc: docAbove(lines, i), line: i,
            });
        }
        for (const ch of line) {
            if (ch === '{') depth++;
            else if (ch === '}') {
                depth--;
                if (depth <= 0) currentClass = null;
            }
        }
    }
    return syms;
}

// ---------- text preparation ----------

/// Blank out comments and string/char contents, preserving length (so
/// offsets survive) and keeping the quotes (so literals stay typeable).
function neutralize(line) {
    let s = line.replace(/"(\\.|[^"\\])*"/g, (m) => '"' + '~'.repeat(m.length - 2) + '"');
    s = s.replace(/'(\\.|[^'\\])*'/g, (m) => "'" + '~'.repeat(m.length - 2) + "'");
    const c = s.indexOf('//');
    if (c >= 0) s = s.slice(0, c) + ' '.repeat(s.length - c);
    return s;
}

/// Neutralized text of everything up to (line, col) — col exclusive —
/// looking back at most `back` lines.
function buildPrefix(lines, line, col, back = 40) {
    const start = Math.max(0, line - back);
    let s = '';
    for (let l = start; l < line; l++) s += neutralize(lines[l]) + '\n';
    s += neutralize(lines[line]).slice(0, col);
    return s;
}

// ---------- type strings ----------

function stripOpt(t) {
    t = (t || '').trim();
    return t.endsWith('?') ? t.slice(0, -1).trim() : t;
}

/// "Vector<Integer>" -> { base: "Vector", payload: "Integer" }. Pointers and
/// anything unparseable return null (no methods to offer anyway).
function parseTypeStr(t) {
    t = stripOpt(t);
    if (!t || t.startsWith('*')) return null;
    const m = t.match(/^([A-Za-z_]\w*)\s*(?:<\s*(.+?)\s*>)?$/);
    if (!m) return null;
    return { base: m[1], payload: m[2] || null };
}

/// Replace a type parameter (whole word) with its concrete binding.
function substWord(text, param, arg) {
    if (!param) return text;
    return text.replace(new RegExp(`\\b${param}\\b`, 'g'), arg);
}

/// Split "Array<Integer>, Integer" on top-level commas only.
function splitTop(s) {
    const out = [];
    let depth = 0, cur = '';
    for (const ch of s) {
        if (ch === '<') depth++;
        else if (ch === '>') depth--;
        if (ch === ',' && depth === 0) { out.push(cur); cur = ''; }
        else cur += ch;
    }
    if (cur.trim()) out.push(cur);
    return out.map((x) => x.trim());
}

// ---------- the backward chain parser ----------

/// Parse the postfix chain that ends at the end of `s` (which itself ends
/// just before a '.'). Returns [base, step, step, ...] or null.
/// Steps: {kind:'base', base|call+name+typeArgs} | {kind:'baseName', name}
///        | {kind:'method', name, typeArgs} | {kind:'field', name}
///        | {kind:'index'}
function backwardChain(s) {
    let i = s.length - 1;
    const steps = [];
    const isIdent = (c) => /[A-Za-z0-9_]/.test(c);
    const skipWs = () => { while (i >= 0 && /\s/.test(s[i])) i--; };
    for (;;) {
        skipWs();
        if (i < 0) return null;
        const c = s[i];
        if (c === '"' || c === "'") {
            let j = i - 1;
            while (j >= 0 && s[j] !== c) j--;
            if (j < 0) return null;
            steps.unshift({ kind: 'base', base: c === '"' ? 'String' : 'Char' });
            return steps;
        }
        if (c === ']') {
            let d = 0, j = i;
            for (; j >= 0; j--) {
                if (s[j] === ']') d++;
                else if (s[j] === '[') { d--; if (d === 0) break; }
            }
            if (j < 0) return null;
            steps.unshift({ kind: 'index' });
            i = j - 1;
            continue;
        }
        if (c === ')') {
            let d = 0, j = i;
            for (; j >= 0; j--) {
                if (s[j] === ')') d++;
                else if (s[j] === '(') { d--; if (d === 0) break; }
            }
            if (j < 0) return null;
            i = j - 1;
            skipWs();
            // optional explicit type arguments: name<...>(...)
            let typeArgs = null;
            if (i >= 0 && s[i] === '>') {
                let d2 = 0, k = i;
                for (; k >= 0; k--) {
                    if (s[k] === '>') d2++;
                    else if (s[k] === '<') { d2--; if (d2 === 0) break; }
                }
                if (k >= 0) {
                    typeArgs = s.slice(k + 1, i);
                    i = k - 1;
                    skipWs();
                }
            }
            let e = i;
            while (i >= 0 && isIdent(s[i])) i--;
            const name = s.slice(i + 1, e + 1);
            if (!name || /^\d/.test(name)) return null; // `(expr)` — give up
            const save = i;
            skipWs();
            if (i >= 0 && s[i] === '.') {
                steps.unshift({ kind: 'method', name, typeArgs });
                i--;
                continue;
            }
            i = save;
            steps.unshift({ kind: 'base', call: true, name, typeArgs });
            return steps;
        }
        if (isIdent(c)) {
            let e = i;
            while (i >= 0 && isIdent(s[i])) i--;
            const name = s.slice(i + 1, e + 1);
            const save = i;
            skipWs();
            if (i >= 0 && s[i] === '.') {
                if (/^\d+$/.test(name)) {
                    // probably the fraction of a Float literal: `1.5`
                    i--;
                    skipWs();
                    let e2 = i;
                    while (i >= 0 && isIdent(s[i])) i--;
                    if (/^\d+$/.test(s.slice(i + 1, e2 + 1))) {
                        steps.unshift({ kind: 'base', base: 'Float' });
                        return steps;
                    }
                    return null;
                }
                steps.unshift({ kind: 'field', name });
                i--;
                continue;
            }
            i = save;
            if (/^\d+$/.test(name)) {
                steps.unshift({ kind: 'base', base: 'Integer' });
                return steps;
            }
            steps.unshift({ kind: 'baseName', name });
            return steps;
        }
        return null;
    }
}

// ---------- local declarations & enclosing scopes ----------

/// Find the declaration that binds `name` at `useLine`: nearest preceding
/// var/const/for/catch, else a parameter of the enclosing function/method.
/// Returns { type, line } or null.
function localDeclAt(lines, useLine, name) {
    const varRe = new RegExp(`\\b(?:var|const)\\s+${name}\\s*:\\s*([^=;]+)=`);
    const forRe = new RegExp(`\\bfor\\s*\\(\\s*${name}\\s*:\\s*(.+?)\\s*(?:=|\\bin\\b)`);
    const catchRe = new RegExp(`\\bcatch\\s*\\(\\s*${name}\\s*:\\s*([A-Za-z_]\\w*)`);
    const headerRe = /^\s*(?:function\s+[A-Za-z_]\w*\s*(?:<[^>()]*>)?|(?:public|private)\s+[A-Za-z_]\w*|[A-Za-z_]\w*)\s*\(([^)]*)\)/;
    const isHeader = (line) =>
        /^\s*(function|public|private)\b/.test(line) ||
        /^\s*[A-Za-z_]\w*\s*\([^)]*\)\s*\{\s*$/.test(line); // constructor
    for (let l = useLine; l >= 0; l--) {
        const line = neutralize(lines[l]);
        let m;
        if ((m = line.match(varRe))) return { type: m[1].trim(), line: l };
        if ((m = line.match(forRe))) return { type: m[1].trim(), line: l };
        if ((m = line.match(catchRe))) return { type: m[1].trim(), line: l };
        if (isHeader(line)) {
            const h = line.match(headerRe);
            if (h) {
                const pm = h[1].match(new RegExp(`(?:^|,)\\s*${name}\\s*:\\s*([^,]+)`));
                if (pm) return { type: pm[1].trim(), line: l };
            }
            return null; // function boundary — stop
        }
    }
    return null;
}

/// Which class (from this file's scan) encloses `targetLine`, if any.
function enclosingClassAt(lines, targetLine, classes) {
    let current = null;
    let depth = 0;
    for (let i = 0; i <= targetLine && i < lines.length; i++) {
        const line = lines[i].replace(/\/\/.*$/, '');
        const m = line.match(/^\s*class\s+([A-Za-z_]\w*)/);
        if (m) current = (classes || []).find((c) => c.name === m[1]) || { name: m[1] };
        for (const ch of line) {
            if (ch === '{') depth++;
            else if (ch === '}') {
                depth--;
                if (depth <= 0) current = null;
            }
        }
    }
    return current;
}

// ---------- the resolver ----------

/// Type of the expression ending just before the '.' at (line, col).
/// ctx: { classes, functions, enclosingClass, localType(name) -> type|null }
function resolveTypeAt(lines, line, col, ctx) {
    const chain = backwardChain(buildPrefix(lines, line, col));
    if (!chain) return null;
    return resolveChainType(chain, ctx);
}

function resolveChainType(chain, ctx) {
    const first = chain[0];
    let t = null;
    if (first.kind === 'base' && first.base) {
        t = first.base;
    } else if (first.kind === 'base' && first.call) {
        const cls = ctx.classes.find((c) => c.name === first.name);
        if (cls) {
            t = cls.name; // constructor (context-typed containers stay untyped)
        } else {
            const fn = ctx.functions.find((f) => f.name === first.name);
            if (!fn) return null;
            t = fn.ret;
            if (fn.typeParams && fn.typeParams.length) {
                if (first.typeArgs) {
                    const args = splitTop(first.typeArgs);
                    fn.typeParams.forEach((p, idx) => {
                        if (args[idx]) t = substWord(t, p, args[idx]);
                    });
                }
                if (fn.typeParams.some((p) => new RegExp(`\\b${p}\\b`).test(t))) {
                    return null; // type parameters still unresolved
                }
            }
        }
    } else if (first.kind === 'baseName') {
        const n = first.name;
        if (n === 'this') {
            t = ctx.enclosingClass
                ? ctx.enclosingClass.name + (ctx.enclosingClass.generic || '')
                : null;
        } else if (n === 'true' || n === 'false') {
            t = 'Bool';
        } else {
            t = ctx.localType(n);
        }
    } else {
        return null;
    }
    if (!t) return null;

    for (let i = 1; i < chain.length; i++) {
        const pt = parseTypeStr(t);
        if (!pt) return null;
        const cls = ctx.classes.find((c) => c.name === pt.base);
        if (!cls) return null;
        const sub = (x) => (cls.typeParam && pt.payload ? substWord(x, cls.typeParam, pt.payload) : x);
        const step = chain[i];
        if (step.kind === 'index') {
            const g = cls.methods.find((m) => m.name === 'get');
            if (!g) return null;
            t = sub(g.ret);
        } else if (step.kind === 'method') {
            const m = cls.methods.find((mm) => mm.name === step.name);
            if (!m) return null;
            t = sub(m.ret);
        } else if (step.kind === 'field') {
            const f = cls.fields.find((ff) => ff.name === step.name);
            if (!f) return null;
            t = sub(f.type);
        } else {
            return null;
        }
    }
    return stripOpt(t);
}

// ---------- imports ----------

/// Parse an `#import` line. Returns {kind:'std'|'local', name} or null.
function parseImportLine(lineText) {
    let m = lineText.match(/^\s*#import\s*<\s*([A-Za-z_]\w*)\s*>/);
    if (m) return { kind: 'std', name: m[1] };
    m = lineText.match(/^\s*#import\s*"([^"]+)"/);
    if (m) return { kind: 'local', name: m[1] };
    return null;
}

module.exports = {
    scanText,
    moduleDocOf,
    neutralize,
    buildPrefix,
    backwardChain,
    resolveChainType,
    resolveTypeAt,
    localDeclAt,
    enclosingClassAt,
    parseTypeStr,
    stripOpt,
    substWord,
    splitTop,
    parseImportLine,
};
