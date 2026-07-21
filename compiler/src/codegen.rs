use crate::ast::*;
use std::collections::{BTreeMap, HashSet};

const RUNTIME: &str = include_str!("../runtime/jim_runtime.c");

fn is_core_class(name: &str) -> bool {
    matches!(name, "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception")
}

/// Make a class key ("Vector<Integer>") a valid C identifier fragment.
fn c_name(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// Emit the whole program as a single C11 translation unit. `files` maps a
/// declaration's file_idx to a display path (for @panic locations). `debug`
/// adds shadow-stack maintenance so every panic prints a full jim stack
/// trace; release builds emit none of it (zero cost).
pub fn generate(program: &Program, files: &[String], debug: bool, panic_abort: bool) -> String {
    let cg = Cg {
        user_classes: program
            .classes
            .iter()
            .filter(|c| !is_core_class(&c.name))
            .map(|c| c.name.clone())
            .collect(),
        files: files.iter().map(|f| f.replace('\\', "/")).collect(),
        panic_ctx: std::cell::RefCell::new((String::from("?"), String::from("?"))),
        debug,
        panic_abort,
        ret_ctype: std::cell::RefCell::new(String::from("void")),
    };
    cg.generate(program)
}

/// Reachability from `main`: which functions, methods, and class constructors
/// the program can actually use. jim is statically dispatched (no vtables, no
/// reflection), so a mark-and-sweep over the lowered AST is exact, and codegen
/// can emit only reachable bodies. That keeps the generated C to the user's
/// program plus what it touches, instead of the whole standard library.
struct Reach {
    fns: HashSet<String>,
    methods: HashSet<String>, // "Class#method"
    instantiated: HashSet<String>, // classes whose constructor can run
    intrinsics: HashSet<String>, // @intrinsic names reached (drives runtime blocks)
    uses_opt: bool,             // any optional (T?) machinery reached
    uses_trycatch: bool,        // any try/catch reached (needs the handler stack)
}

/// Worklists (drained) plus the flags/intrinsics accumulated during the walk.
#[derive(Default)]
struct Work {
    fns: Vec<String>,
    methods: Vec<(String, String)>,
    classes: Vec<String>,
    intrinsics: HashSet<String>,
    uses_opt: bool,
    uses_trycatch: bool,
}

fn method_key(class: &str, method: &str) -> String {
    format!("{}#{}", class, method)
}

fn compute_reachable(program: &Program, main_takes_argv: bool) -> Reach {
    use std::collections::HashMap;
    let fn_by: HashMap<&str, &FunctionDecl> =
        program.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let class_by: HashMap<&str, &ClassDecl> =
        program.classes.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut method_by: HashMap<(&str, &str), &MethodDecl> = HashMap::new();
    for c in &program.classes {
        for m in &c.methods {
            method_by.insert((c.name.as_str(), m.name.as_str()), m);
        }
    }

    let mut reach = Reach {
        fns: HashSet::new(),
        methods: HashSet::new(),
        instantiated: HashSet::new(),
        intrinsics: HashSet::new(),
        uses_opt: false,
        uses_trycatch: false,
    };
    let mut w = Work::default();
    w.fns.push("main".to_string()); // the sole root
    // The argv entry point builds an Array<String> directly in the C wrapper.
    if main_takes_argv {
        w.classes.push("Array<String>".to_string());
        w.methods.push(("Array<String>".to_string(), "set".to_string()));
    }

    loop {
        if let Some(name) = w.fns.pop() {
            if !reach.fns.insert(name.clone()) {
                continue;
            }
            if let Some(f) = fn_by.get(name.as_str()) {
                walk_block(&f.body, &mut w);
            }
        } else if let Some((cls, m)) = w.methods.pop() {
            if !reach.methods.insert(method_key(&cls, &m)) {
                continue;
            }
            if let Some(md) = method_by.get(&(cls.as_str(), m.as_str())) {
                walk_block(&md.body, &mut w);
            }
        } else if let Some(cls) = w.classes.pop() {
            if !reach.instantiated.insert(cls.clone()) {
                continue;
            }
            if let Some(c) = class_by.get(cls.as_str()) {
                // Field defaults run in the constructor, so their calls count.
                for fld in &c.fields {
                    walk_expr(&fld.default, &mut w);
                }
                if let Some(ct) = &c.ctor {
                    walk_block(&ct.body, &mut w);
                }
            }
        } else {
            break;
        }
    }
    reach.intrinsics = w.intrinsics;
    reach.uses_opt = w.uses_opt;
    reach.uses_trycatch = w.uses_trycatch;
    reach
}

fn walk_block(b: &Block, w: &mut Work) {
    for s in &b.stmts {
        walk_stmt(s, w);
    }
}

fn walk_stmt(s: &Stmt, w: &mut Work) {
    match &s.kind {
        StmtKind::VarDecl { init, .. } => walk_expr(init, w),
        StmtKind::Assign { target, value, .. } => {
            walk_expr(target, w);
            walk_expr(value, w);
        }
        StmtKind::IncDec { target, .. } => walk_expr(target, w),
        StmtKind::ExprStmt(e) => walk_expr(e, w),
        StmtKind::Return(Some(e)) => walk_expr(e, w),
        StmtKind::Return(None) => {}
        StmtKind::If { arms, else_block } => {
            for (cond, body) in arms {
                walk_expr(cond, w);
                walk_block(body, w);
            }
            if let Some(b) = else_block {
                walk_block(b, w);
            }
        }
        StmtKind::While { cond, body } => {
            walk_expr(cond, w);
            walk_block(body, w);
        }
        StmtKind::ForC { init, cond, step, body, .. } => {
            walk_expr(init, w);
            walk_expr(cond, w);
            walk_stmt(step, w);
            walk_block(body, w);
        }
        StmtKind::ForIn { iterable, body, .. } => {
            walk_expr(iterable, w);
            walk_block(body, w);
        }
        StmtKind::Break | StmtKind::Continue => {}
        StmtKind::Scope(b) => walk_block(b, w),
        StmtKind::TryCatch { body, catch_body, .. } => {
            w.uses_trycatch = true;
            walk_block(body, w);
            walk_block(catch_body, w);
        }
    }
}

fn walk_expr(e: &Expr, w: &mut Work) {
    match &e.kind {
        ExprKind::Call { name, args } => {
            w.fns.push(name.clone());
            for a in args {
                walk_expr(a, w);
            }
        }
        ExprKind::GenericCall { name, args, .. } => {
            w.fns.push(name.clone());
            for a in args {
                walk_expr(a, w);
            }
        }
        ExprKind::MethodCall { recv, args, .. } => {
            walk_expr(recv, w);
            for a in args {
                walk_expr(a, w);
            }
        }
        ExprKind::CoreMethodCall { class, name, recv, args } => {
            w.methods.push((class.clone(), name.clone()));
            walk_expr(recv, w);
            for a in args {
                walk_expr(a, w);
            }
        }
        ExprKind::New { class, args } => {
            w.classes.push(class.clone());
            for a in args {
                walk_expr(a, w);
            }
        }
        ExprKind::ContainerLit { class, is_array, elems } => {
            w.classes.push(class.clone());
            w.methods
                .push((class.clone(), if *is_array { "set" } else { "push" }.to_string()));
            for el in elems {
                walk_expr(el, w);
            }
        }
        ExprKind::BufAlloc { size, .. } => walk_expr(size, w),
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                walk_expr(el, w);
            }
        }
        ExprKind::OptWrap { expr, .. }
        | ExprKind::OptUnwrap { expr, .. }
        | ExprKind::OptHas { expr, .. } => {
            w.uses_opt = true;
            walk_expr(expr, w);
        }
        ExprKind::FieldAccess { recv, .. } => walk_expr(recv, w),
        ExprKind::Index { recv, index } => {
            walk_expr(recv, w);
            walk_expr(index, w);
        }
        ExprKind::IntrinsicCall { name, args } => {
            w.intrinsics.insert(name.clone());
            for a in args {
                walk_expr(a, w);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, w);
            walk_expr(rhs, w);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, w),
        ExprKind::OptNone { .. } => w.uses_opt = true,
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::CharLit(_)
        | ExprKind::Bool(_)
        | ExprKind::NoneLit
        | ExprKind::Ident(_)
        | ExprKind::This => {}
    }
}

/// Evaluate the runtime's own conditionals so the emitted source only contains
/// the kept blocks. The runtime uses exactly two conditional forms:
/// `#ifdef JIM_RT_<BLOCK>` (a feature block) and `#ifndef JIM_PANIC_ABORT` (the
/// setjmp machinery); every `#endif` closes one of those. Nesting is handled by
/// requiring all enclosing conditions to hold.
fn filter_runtime(src: &str, panic_abort: bool, blocks: &HashSet<&str>) -> String {
    let mut out = String::with_capacity(src.len());
    let mut stack: Vec<bool> = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("#ifdef ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            // Unknown (non JIM_RT_) conditionals keep their content, so an
            // unrelated future guard is never silently dropped.
            let active = match name.strip_prefix("JIM_RT_") {
                Some(b) => blocks.contains(b),
                None => true,
            };
            stack.push(active);
        } else if let Some(rest) = t.strip_prefix("#ifndef ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            let active = if name == "JIM_PANIC_ABORT" { !panic_abort } else { true };
            stack.push(active);
        } else if t.starts_with("#endif") {
            stack.pop();
        } else if stack.iter().all(|&b| b) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Does this block definitely return on every path? Conservative: only says
/// yes when it is certain, so codegen can safely omit a trailing implicit
/// return (which otherwise produces a dead second `return`). Loops say no.
fn block_always_returns(b: &Block) -> bool {
    b.stmts.last().map_or(false, stmt_always_returns)
}

fn stmt_always_returns(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::Scope(b) => block_always_returns(b),
        StmtKind::If { arms, else_block } => {
            else_block.as_ref().map_or(false, block_always_returns)
                && arms.iter().all(|(_, body)| block_always_returns(body))
        }
        StmtKind::TryCatch { body, catch_body, .. } => {
            block_always_returns(body) && block_always_returns(catch_body)
        }
        _ => false,
    }
}

/// Map an intrinsic to the runtime feature block that defines its `rt_` helper.
/// `None` means the helper is part of the always-emitted core runtime.
fn intrinsic_block(name: &str) -> Option<&'static str> {
    let block = match name {
        "i64_add" | "i64_sub" | "i64_mul" | "i64_divtrunc" | "i64_mod" | "i64_neg" | "i64_eq"
        | "i64_lt" | "i64_to_f64" | "i64_to_string" => "INT",
        "f64_add" | "f64_sub" | "f64_mul" | "f64_div" | "f64_neg" | "f64_eq" | "f64_lt"
        | "f64_to_i64" | "f64_to_string" => "FLOAT",
        "f64_sqrt" | "f64_cbrt" | "f64_hypot" | "f64_exp" | "f64_log" | "f64_log2" | "f64_log10"
        | "f64_sin" | "f64_cos" | "f64_tan" | "f64_asin" | "f64_acos" | "f64_atan" | "f64_atan2"
        | "f64_fmod" | "f64_pow" | "f64_is_nan" | "f64_is_inf" | "f64_is_finite" => "FLOATMATH",
        "bool_eq" | "char_eq" | "char_lt" | "char_to_i64" | "i64_to_char" | "char_to_string" => {
            "BOOLCHAR"
        }
        "str_len" | "str_byte" | "str_concat" | "str_eq" | "str_lt" | "str_slice"
        | "str_from_buf" => "STRING",
        "str_to_i64" | "str_to_f64" => "STRPARSE",
        "i64_and" | "i64_or" | "i64_xor" | "i64_not" | "i64_shl" | "i64_shr" => "BITOPS",
        "print_string" | "print_err" => "IOPRINT",
        "read_line" | "read_file" | "write_file" | "append_file" | "file_exists" => "IOFILE",
        _ => return None,
    };
    Some(block)
}

struct Cg {
    /// Reference classes (arena-allocated, represented as pointers).
    user_classes: HashSet<String>,
    /// file_idx -> display path, for @panic locations.
    files: Vec<String>,
    /// (file, function) of the body currently being emitted - @panic call
    /// sites bake these in as C string literals (zero runtime cost).
    panic_ctx: std::cell::RefCell<(String, String)>,
    /// Emit shadow-stack maintenance (rt_push_frame / rt_frame_line / pops).
    debug: bool,
    /// Panics print + exit instead of setjmp/longjmp unwinding; try/catch emits
    /// its body without a handler (the browser playground's run path - wasm
    /// setjmp isn't portably runnable). Catch blocks become unreachable.
    panic_abort: bool,
    /// C return type of the body being emitted (debug return-wrapping).
    ret_ctype: std::cell::RefCell<String>,
}

impl Cg {
    fn generate(&self, program: &Program) -> String {
        let main_takes_argv = program
            .functions
            .iter()
            .find(|f| f.name == "main")
            .map_or(false, |m| !m.params.is_empty());
        // Dead-code elimination: only emit what `main` can reach.
        let reach = compute_reachable(program, main_takes_argv);
        let bufs = self.collect_buffers(program);

        let mut out = String::with_capacity(RUNTIME.len() + 8192);
        out.push_str("/* generated by jimc - do not edit */\n");
        // Keep only the runtime blocks a reachable intrinsic needs, and drop the
        // setjmp machinery in panic=abort. jimc evaluates the runtime's
        // `#ifdef JIM_RT_*` / `#ifndef JIM_PANIC_ABORT` itself, so the emitted C
        // actually shrinks (a bare #ifdef would only hide the text from the C
        // compiler, not from the source we show).
        let mut blocks: HashSet<&str> =
            reach.intrinsics.iter().filter_map(|i| intrinsic_block(i)).collect();
        // Optionals: any T? machinery, or the intrinsics that return optionals.
        let needs_opt =
            reach.uses_opt || blocks.contains("STRPARSE") || blocks.contains("IOFILE");
        // Panics: anything that can raise (checked arithmetic, None/bounds via
        // optionals, @panic) or catch (try/catch needs the handler stack).
        let needs_panic = needs_opt
            || reach.uses_trycatch
            || reach.intrinsics.contains("panic")
            || ["INT", "FLOAT", "BOOLCHAR", "BITOPS"].iter().any(|b| blocks.contains(b));
        if needs_opt {
            blocks.insert("OPT");
        }
        if needs_panic {
            blocks.insert("PANIC");
        }
        if !bufs.is_empty() {
            blocks.insert("BUF");
        }
        if self.debug {
            blocks.insert("DEBUG");
        }
        out.push_str(&filter_runtime(RUNTIME, self.panic_abort, &blocks));

        // ---- user class layouts ----
        out.push_str("\n/* ==== jim classes ==== */\n\n");
        for c in &program.classes {
            if self.user_classes.contains(&c.name) {
                out.push_str(&format!("typedef struct jim_c_{0} jim_c_{0};\n", c_name(&c.name)));
            }
        }
        out.push('\n');

        // ---- RawBuffer instantiations (structs of T*, so class typedefs suffice) ----
        for (sfx, elem_c) in &bufs {
            out.push_str(&format!("JIM_DEFINE_BUF({}, {})\n", sfx, elem_c));
        }
        out.push('\n');

        for c in &program.classes {
            if !self.user_classes.contains(&c.name) {
                continue;
            }
            out.push_str(&format!("struct jim_c_{} {{\n", c_name(&c.name)));
            if c.fields.is_empty() {
                out.push_str("    char jim_empty_; /* C requires at least one member */\n");
            }
            for f in &c.fields {
                out.push_str(&format!("    {} f_{};\n", self.ctype(&f.ty), f.name));
            }
            out.push_str("};\n\n");
        }

        // ---- prototypes ----
        out.push_str("/* ==== jim program ==== */\n\n");
        for c in &program.classes {
            if self.user_classes.contains(&c.name) && reach.instantiated.contains(&c.name) {
                out.push_str(&self.ctor_signature(c));
                out.push_str(";\n");
            }
            for m in &c.methods {
                if reach.methods.contains(&method_key(&c.name, &m.name)) {
                    out.push_str(&self.method_signature(c, m));
                    out.push_str(";\n");
                }
            }
        }
        for f in &program.functions {
            if reach.fns.contains(&f.name) {
                out.push_str(&self.signature(f));
                out.push_str(";\n");
            }
        }
        out.push('\n');

        // ---- definitions ----
        for c in &program.classes {
            if self.user_classes.contains(&c.name) && reach.instantiated.contains(&c.name) {
                self.set_panic_ctx(c.file_idx, format!("{} constructor", c.name));
                *self.ret_ctype.borrow_mut() = "void".to_string();
                out.push_str(&self.ctor_signature(c));
                out.push_str(" {\n");
                out.push_str(&self.frame_push());
                out.push_str(&format!(
                    "    jim_c_{0}* jl_this = (jim_c_{0}*)rt_arena_alloc(sizeof(jim_c_{0}));\n",
                    c_name(&c.name)
                ));
                for f in &c.fields {
                    out.push_str(&format!(
                        "    jl_this->f_{} = {};\n",
                        f.name,
                        self.cg_expr(&f.default)
                    ));
                }
                if let Some(ct) = &c.ctor {
                    self.emit_block(&mut out, &ct.body, 1);
                }
                out.push_str(&self.frame_pop());
                out.push_str("    return jl_this;\n}\n\n");
            }
            for m in &c.methods {
                if !reach.methods.contains(&method_key(&c.name, &m.name)) {
                    continue;
                }
                self.set_panic_ctx(c.file_idx, format!("{}.{}", c.name, m.name));
                *self.ret_ctype.borrow_mut() = self.ctype(&m.ret);
                out.push_str(&self.method_signature(c, m));
                out.push_str(" {\n");
                out.push_str(&self.frame_push());
                self.emit_block(&mut out, &m.body, 1);
                if !block_always_returns(&m.body) {
                    if let Some(ret) = self.implicit_none_return(&m.ret) {
                        out.push_str(&self.frame_pop());
                        out.push_str(&ret);
                    } else if m.ret == Type::named("None") {
                        out.push_str(&self.frame_pop());
                    }
                }
                out.push_str("}\n\n");
            }
        }
        for f in &program.functions {
            if !reach.fns.contains(&f.name) {
                continue;
            }
            self.set_panic_ctx(f.file_idx, f.name.clone());
            *self.ret_ctype.borrow_mut() = self.ctype(&f.ret);
            out.push_str(&self.signature(f));
            out.push_str(" {\n");
            out.push_str(&self.frame_push());
            self.emit_block(&mut out, &f.body, 1);
            if !block_always_returns(&f.body) {
                if f.name == "main" {
                    // main may omit `return` - the process exits 0.
                    out.push_str(&self.frame_pop());
                    out.push_str("    return INT64_C(0);\n");
                } else if let Some(ret) = self.implicit_none_return(&f.ret) {
                    out.push_str(&self.frame_pop());
                    out.push_str(&ret);
                } else if f.ret == Type::named("None") {
                    out.push_str(&self.frame_pop());
                }
            }
            out.push_str("}\n\n");
        }

        // ---- the C entry point (argv form builds Array<String> first) ----
        if main_takes_argv {
            let k = c_name("Array<String>");
            out.push_str(&format!(
                "int main(int argc, char** argv) {{\n    rt_init();\n    jim_c_{k}* jim_args = jim_new_{k}((int64_t)argc);\n    for (int jim_i = 0; jim_i < argc; jim_i++) {{\n        jim_m_{k}_set(jim_args, (int64_t)jim_i, rt_str_lit(argv[jim_i], (int64_t)strlen(argv[jim_i])));\n    }}\n    int64_t jim_exit_code = jim_user_main(jim_args);\n    rt_shutdown();\n    return (int)jim_exit_code;\n}}\n",
                k = k
            ));
        } else {
            out.push_str(
                "int main(void) {\n    rt_init();\n    int64_t jim_exit_code = jim_user_main();\n    rt_shutdown();\n    return (int)jim_exit_code;\n}\n",
            );
        }
        out
    }

    fn set_panic_ctx(&self, file_idx: usize, func: String) {
        let file = self.files.get(file_idx).cloned().unwrap_or_else(|| "?".to_string());
        *self.panic_ctx.borrow_mut() = (file, func);
    }

    /// Debug builds: enter the shadow-stack frame for the current body.
    fn frame_push(&self) -> String {
        if !self.debug {
            return String::new();
        }
        let (file, func) = self.panic_ctx.borrow().clone();
        format!(
            "    rt_push_frame(\"{}\", \"{}\");\n",
            escape_c(file.as_bytes()),
            escape_c(func.as_bytes())
        )
    }

    fn frame_pop(&self) -> String {
        if self.debug { "    rt_pop_frame();\n".to_string() } else { String::new() }
    }

    /// Debug builds: record the current line in the active frame before a
    /// call, so panics deeper down can point back at this call site.
    fn line_mark(&self, line: u32, call: String) -> String {
        if self.debug {
            format!("(rt_frame_line({}), {})", line, call)
        } else {
            call
        }
    }

    /// Every RawBuffer<T> element type used anywhere - (sanitized sfx, C type).
    /// BTreeMap for deterministic output order.
    fn collect_buffers(&self, program: &Program) -> BTreeMap<String, String> {
        let mut tys: Vec<Type> = Vec::new();
        for f in &program.functions {
            for p in &f.params {
                tys.push(p.ty.clone());
            }
            tys.push(f.ret.clone());
            collect_types_in_block(&f.body, &mut tys);
        }
        for c in &program.classes {
            for fld in &c.fields {
                tys.push(fld.ty.clone());
            }
            if let Some(ct) = &c.ctor {
                for p in &ct.params {
                    tys.push(p.ty.clone());
                }
                collect_types_in_block(&ct.body, &mut tys);
            }
            for m in &c.methods {
                for p in &m.params {
                    tys.push(p.ty.clone());
                }
                tys.push(m.ret.clone());
                collect_types_in_block(&m.body, &mut tys);
            }
        }
        let mut out = BTreeMap::new();
        for ty in &tys {
            self.collect_buf_elems(ty, &mut out);
        }
        out
    }

    fn collect_buf_elems(&self, ty: &Type, out: &mut BTreeMap<String, String>) {
        match ty {
            Type::Generic(n, p) => {
                if n == "RawBuffer" {
                    out.insert(c_name(&p.display()), self.ctype(p));
                }
                self.collect_buf_elems(p, out);
            }
            Type::Optional(p) | Type::Pointer(p) => self.collect_buf_elems(p, out),
            Type::Named(_) => {}
        }
    }

    fn signature(&self, f: &FunctionDecl) -> String {
        let params = if f.params.is_empty() {
            "void".to_string()
        } else {
            f.params
                .iter()
                .map(|p| format!("{} {}", self.ctype(&p.ty), local(&p.name)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!("static {} {}({})", self.ctype(&f.ret), mangle(&f.name), params)
    }

    /// Methods take the receiver as an explicit first parameter (`this`).
    fn method_signature(&self, c: &ClassDecl, m: &MethodDecl) -> String {
        let mut params = vec![format!("{} jl_this", self.ctype(&Type::named(&c.name)))];
        params.extend(
            m.params
                .iter()
                .map(|p| format!("{} {}", self.ctype(&p.ty), local(&p.name))),
        );
        format!(
            "static {} {}({})",
            self.ctype(&m.ret),
            mangle_method(&c.name, &m.name),
            params.join(", ")
        )
    }

    fn ctor_signature(&self, c: &ClassDecl) -> String {
        let params: Vec<String> = match &c.ctor {
            Some(ct) => ct
                .params
                .iter()
                .map(|p| format!("{} {}", self.ctype(&p.ty), local(&p.name)))
                .collect(),
            None => Vec::new(),
        };
        let ps = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        format!("static jim_c_{0}* jim_new_{0}({1})", c_name(&c.name), ps)
    }

    fn ctype(&self, ty: &Type) -> String {
        match ty {
            Type::Named(n) => match n.as_str() {
                "Integer" => "int64_t".to_string(),
                "Float" => "double".to_string(),
                "Bool" => "bool".to_string(),
                "Char" => "uint8_t".to_string(),
                "String" => "jim_str".to_string(),
                "Exception" => "jim_str".to_string(), // an Exception is its message
                "None" => "void".to_string(),
                other => {
                    debug_assert!(self.user_classes.contains(other));
                    format!("jim_c_{}*", c_name(other))
                }
            },
            Type::Generic(n, p) => {
                if n == "RawBuffer" {
                    format!("jim_buf_{}", c_name(&p.display()))
                } else {
                    // monomorphized containers are reference classes
                    format!("jim_c_{}*", c_name(&class_key_of(ty)))
                }
            }
            Type::Optional(inner) => {
                if opt_is_struct(inner) {
                    format!("jim_opt_{}", core_opt_sfx(inner))
                } else {
                    // classes, containers, and pointers: nullable representation
                    self.ctype(inner)
                }
            }
            Type::Pointer(inner) => format!("{}*", self.ctype(inner)),
        }
    }

    /// For functions/methods returning `T?`: the implicit `return None;`.
    fn implicit_none_return(&self, ret: &Type) -> Option<String> {
        if let Type::Optional(inner) = ret {
            return Some(if opt_is_struct(inner) {
                format!("    return rt_opt_{}_none();\n", core_opt_sfx(inner))
            } else {
                format!("    return ({})NULL;\n", self.ctype(inner))
            });
        }
        None
    }

    fn emit_block(&self, out: &mut String, block: &Block, depth: usize) {
        for stmt in &block.stmts {
            self.emit_stmt(out, stmt, depth);
        }
    }

    fn emit_stmt(&self, out: &mut String, stmt: &Stmt, depth: usize) {
        match &stmt.kind {
            StmtKind::VarDecl { name, ty, init, .. } => {
                indent(out, depth);
                out.push_str(&format!(
                    "{} {} = {};\n",
                    self.ctype(ty),
                    local(name),
                    self.cg_expr(init)
                ));
            }
            StmtKind::Assign { target, value, .. } => {
                indent(out, depth);
                out.push_str(&format!("{} = {};\n", self.cg_assign_target(target), self.cg_expr(value)));
            }
            StmtKind::ExprStmt(e) => {
                indent(out, depth);
                out.push_str(&format!("{};\n", self.cg_expr(e)));
            }
            StmtKind::Return(value) => {
                indent(out, depth);
                match value {
                    Some(e) => {
                        if self.debug {
                            // evaluate first (the expression may push/pop
                            // frames of its own), then leave our frame
                            let rc = self.ret_ctype.borrow().clone();
                            out.push_str(&format!(
                                "{{ {} jim_rv = {}; rt_pop_frame(); return jim_rv; }}\n",
                                rc,
                                self.cg_expr(e)
                            ));
                        } else {
                            out.push_str(&format!("return {};\n", self.cg_expr(e)));
                        }
                    }
                    None => {
                        if self.debug {
                            out.push_str("{ rt_pop_frame(); return; }\n");
                        } else {
                            out.push_str("return;\n");
                        }
                    }
                }
            }
            StmtKind::If { arms, else_block } => {
                indent(out, depth);
                for (i, (cond, body)) in arms.iter().enumerate() {
                    if i > 0 {
                        out.push_str(" else ");
                    }
                    out.push_str(&format!("if ({}) {{\n", self.cg_expr(cond)));
                    self.emit_block(out, body, depth + 1);
                    indent(out, depth);
                    out.push('}');
                }
                if let Some(b) = else_block {
                    out.push_str(" else {\n");
                    self.emit_block(out, b, depth + 1);
                    indent(out, depth);
                    out.push('}');
                }
                out.push('\n');
            }
            StmtKind::While { cond, body } => {
                indent(out, depth);
                out.push_str(&format!("while ({}) {{\n", self.cg_expr(cond)));
                self.emit_block(out, body, depth + 1);
                indent(out, depth);
                out.push_str("}\n");
            }
            StmtKind::ForC { var_name, var_ty, init, cond, step, body } => {
                indent(out, depth);
                out.push_str(&format!(
                    "for ({} {} = {}; {}; {}) {{\n",
                    self.ctype(var_ty),
                    local(var_name),
                    self.cg_expr(init),
                    self.cg_expr(cond),
                    self.cg_step_expr(step)
                ));
                self.emit_block(out, body, depth + 1);
                indent(out, depth);
                out.push_str("}\n");
            }
            StmtKind::Break => {
                indent(out, depth);
                out.push_str("break;\n");
            }
            StmtKind::Continue => {
                indent(out, depth);
                out.push_str("continue;\n");
            }
            StmtKind::Scope(block) => {
                indent(out, depth);
                out.push_str("{\n");
                self.emit_block(out, block, depth + 1);
                indent(out, depth);
                out.push_str("}\n");
            }
            StmtKind::TryCatch { body, var_name, catch_body, .. } if self.panic_abort => {
                // No unwinding in this mode: a panic prints + exits, so the
                // catch is unreachable. Emit the try body as a plain scope.
                let _ = (var_name, catch_body);
                indent(out, depth);
                out.push_str("{ /* try (panic=abort: catch omitted) */\n");
                self.emit_block(out, body, depth + 1);
                indent(out, depth);
                out.push_str("}\n");
            }
            StmtKind::TryCatch { body, var_name, catch_body, .. } => {
                indent(out, depth);
                out.push_str("{\n");
                indent(out, depth + 1);
                out.push_str("rt_handler jim_h;\n");
                indent(out, depth + 1);
                out.push_str("jim_h.prev = rt_handlers;\n");
                indent(out, depth + 1);
                out.push_str("jim_h.frame_top = rt_frame_top;\n");
                indent(out, depth + 1);
                out.push_str("rt_handlers = &jim_h;\n");
                indent(out, depth + 1);
                out.push_str("if (setjmp(jim_h.buf) == 0) {\n");
                self.emit_block(out, body, depth + 2);
                indent(out, depth + 2);
                out.push_str("rt_handlers = jim_h.prev; /* normal exit */\n");
                indent(out, depth + 1);
                out.push_str("} else {\n");
                indent(out, depth + 2);
                out.push_str(&format!("jim_str {} = rt_current_exc;\n", local(var_name)));
                self.emit_block(out, catch_body, depth + 2);
                indent(out, depth + 1);
                out.push_str("}\n");
                indent(out, depth);
                out.push_str("}\n");
            }
            other => unreachable!("sema admitted unsupported statement {:?}", other),
        }
    }

    fn cg_assign_target(&self, target: &Expr) -> String {
        match &target.kind {
            ExprKind::Ident(name) => local(name),
            ExprKind::FieldAccess { recv, name } => {
                format!("({})->f_{}", self.cg_expr(recv), name)
            }
            ExprKind::Unary { op: UnOp::Deref, operand } => {
                format!("(*{})", self.cg_expr(operand))
            }
            other => unreachable!("sema admitted unsupported assignment target {:?}", other),
        }
    }

    /// The step slot of a C `for` is an expression; after lowering the step
    /// statement can only be an assignment or a bare expression.
    fn cg_step_expr(&self, step: &Stmt) -> String {
        match &step.kind {
            StmtKind::Assign { target, value, .. } => {
                format!("{} = {}", self.cg_assign_target(target), self.cg_expr(value))
            }
            StmtKind::ExprStmt(e) => self.cg_expr(e),
            other => unreachable!("sema admitted unsupported for-step {:?}", other),
        }
    }

    fn cg_expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v) => format!("INT64_C({})", v),
            ExprKind::Float(v) => {
                let mut s = format!("{}", v);
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    s.push_str(".0");
                }
                s
            }
            ExprKind::Str(s) => {
                let bytes = s.as_bytes();
                format!("rt_str_lit(\"{}\", {})", escape_c(bytes), bytes.len())
            }
            ExprKind::CharLit(c) => format!("{}u", c),
            ExprKind::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            ExprKind::Ident(name) => local(name),
            ExprKind::This => "jl_this".to_string(),
            ExprKind::Call { name, args } => {
                let args: Vec<String> = args.iter().map(|a| self.cg_expr(a)).collect();
                self.line_mark(e.line, format!("{}({})", mangle(name), args.join(", ")))
            }
            ExprKind::New { class, args } => {
                let args: Vec<String> = args.iter().map(|a| self.cg_expr(a)).collect();
                self.line_mark(e.line, format!("jim_new_{}({})", c_name(class), args.join(", ")))
            }
            ExprKind::CoreMethodCall { class, name, recv, args } => {
                let mut all = vec![self.cg_expr(recv)];
                all.extend(args.iter().map(|a| self.cg_expr(a)));
                // RawBuffer<T> methods are runtime helpers, not jim methods -
                // unchecked and hot, so no line marks either
                if let Some(payload) = class.strip_prefix("RawBuffer<").and_then(|s| s.strip_suffix('>')) {
                    return format!("jim_buf_{}_{}({})", c_name(payload), name, all.join(", "));
                }
                self.line_mark(e.line, format!("{}({})", mangle_method(class, name), all.join(", ")))
            }
            ExprKind::ContainerLit { class, is_array, elems } => {
                let k = c_name(class);
                let mut s = format!("({{ jim_c_{k}* jim_lit = jim_new_{k}(", k = k);
                if *is_array {
                    s.push_str(&format!("INT64_C({})", elems.len()));
                }
                s.push_str("); ");
                for (i, el) in elems.iter().enumerate() {
                    if *is_array {
                        s.push_str(&format!(
                            "jim_m_{}_set(jim_lit, INT64_C({}), {}); ",
                            k,
                            i,
                            self.cg_expr(el)
                        ));
                    } else {
                        s.push_str(&format!("jim_m_{}_push(jim_lit, {}); ", k, self.cg_expr(el)));
                    }
                }
                s.push_str("jim_lit; })");
                s
            }
            ExprKind::BufAlloc { elem, size } => {
                format!("jim_buf_{}_alloc({})", c_name(&elem.display()), self.cg_expr(size))
            }
            ExprKind::FieldAccess { recv, name } => {
                format!("({})->f_{}", self.cg_expr(recv), name)
            }
            ExprKind::IntrinsicCall { name, args } => {
                let args: Vec<String> = args.iter().map(|a| self.cg_expr(a)).collect();
                if name == "panic" {
                    // bake the compile-time location into the call site
                    let (file, func) = self.panic_ctx.borrow().clone();
                    return self.line_mark(
                        e.line,
                        format!(
                            "rt_panic_at({}, \"{}\", {}, \"{}\")",
                            args[0],
                            escape_c(file.as_bytes()),
                            e.line,
                            escape_c(func.as_bytes())
                        ),
                    );
                }
                if name == "str_from_buf" {
                    // the runtime takes raw bytes; unwrap the buffer struct
                    return format!("rt_str_from_bytes(({}).data, {})", args[0], args[1]);
                }
                self.line_mark(e.line, format!("rt_{}({})", name, args.join(", ")))
            }
            ExprKind::OptWrap { payload, expr } => {
                if opt_is_struct(payload) {
                    format!("rt_opt_{}_some({})", core_opt_sfx(payload), self.cg_expr(expr))
                } else {
                    format!("({})", self.cg_expr(expr)) // a non-null pointer is already "some"
                }
            }
            ExprKind::OptUnwrap { payload, expr } => {
                if opt_is_struct(payload) {
                    format!("rt_opt_{}_get({})", core_opt_sfx(payload), self.cg_expr(expr))
                } else {
                    format!(
                        "(({})rt_nonnull({}, \"{}\"))",
                        self.ctype(payload),
                        self.cg_expr(expr),
                        payload.display()
                    )
                }
            }
            ExprKind::OptNone { payload } => {
                if opt_is_struct(payload) {
                    format!("rt_opt_{}_none()", core_opt_sfx(payload))
                } else {
                    format!("(({})NULL)", self.ctype(payload))
                }
            }
            ExprKind::OptHas { payload, expr } => {
                if opt_is_struct(payload) {
                    format!("rt_opt_{}_has({})", core_opt_sfx(payload), self.cg_expr(expr))
                } else {
                    format!("(({}) != NULL)", self.cg_expr(expr))
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let c_op = match op {
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                    other => unreachable!("sema left operator {:?} unlowered", other),
                };
                format!("({} {} {})", self.cg_expr(lhs), c_op, self.cg_expr(rhs))
            }
            ExprKind::Unary { op: UnOp::Not, operand } => format!("!({})", self.cg_expr(operand)),
            ExprKind::Unary { op: UnOp::AddrOf, operand } => {
                format!("(&{})", self.cg_expr(operand))
            }
            ExprKind::Unary { op: UnOp::Deref, operand } => {
                format!("(*{})", self.cg_expr(operand))
            }
            other => unreachable!("sema admitted unsupported expression {:?}", other),
        }
    }
}

fn mangle(name: &str) -> String {
    // generic instances are named "max<Vector<Float>,Float>" - sanitize
    format!("jim_user_{}", c_name(name))
}

fn mangle_method(class: &str, name: &str) -> String {
    format!("jim_m_{}_{}", c_name(class), name)
}

/// Mirror of sema's class_key for the codegen side.
fn class_key_of(ty: &Type) -> String {
    match ty {
        Type::Named(n) => n.clone(),
        Type::Generic(n, p) => format!("{}<{}>", n, p.display()),
        other => unreachable!("no class key for {}", other.display()),
    }
}

/// Types written in a lowered block (declarations carry them).
fn collect_types_in_block(b: &Block, out: &mut Vec<Type>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::VarDecl { ty, .. } => out.push(ty.clone()),
            StmtKind::If { arms, else_block } => {
                for (_, blk) in arms {
                    collect_types_in_block(blk, out);
                }
                if let Some(blk) = else_block {
                    collect_types_in_block(blk, out);
                }
            }
            StmtKind::While { body, .. } => collect_types_in_block(body, out),
            StmtKind::ForC { var_ty, body, .. } => {
                out.push(var_ty.clone());
                collect_types_in_block(body, out);
            }
            StmtKind::Scope(blk) => collect_types_in_block(blk, out),
            _ => {}
        }
    }
}

fn local(name: &str) -> String {
    format!("jl_{}", name)
}

/// Does this optional payload use the tagged-struct representation?
/// (Everything else - classes, containers, pointers - is a nullable pointer.)
fn opt_is_struct(payload: &Type) -> bool {
    matches!(payload, Type::Named(n) if is_core_class(n))
}

/// The runtime-helper suffix for a core optional's payload.
fn core_opt_sfx(payload: &Type) -> &'static str {
    match payload {
        Type::Named(n) => match n.as_str() {
            "Integer" => "i64",
            "Float" => "f64",
            "Bool" => "bool",
            "Char" => "char",
            "String" => "str",
            other => unreachable!("not a core optional payload: {}", other),
        },
        other => unreachable!("not a core optional payload: {}", other.display()),
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

/// Escape bytes for a C string literal. Non-printable bytes use 3-digit octal
/// escapes (never hex: C hex escapes greedily consume following hex digits).
fn escape_c(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\{:03o}", b)),
        }
    }
    out
}
