//! Type checking + lowering.
//!
//! This pass consumes the parsed AST and returns a *lowered* AST in which
//! every operator has been desugared into a resolved method call
//! (`ExprKind::CoreMethodCall`) per the contract in docs/DESIGN.md Â§3.
//! `and`/`or`/`not` stay native (short-circuit). Codegen only ever sees
//! lowered trees.

use crate::ast::*;
use crate::errors::JimError;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// A semantic error plus which source file it belongs to (for rendering).
pub struct SemaError {
    pub file_idx: usize,
    pub err: JimError,
}

type SResult<T> = Result<T, JimError>;

struct FuncSig {
    params: Vec<Type>,
    ret: Type,
}

struct MethodSig {
    params: Vec<Type>,
    ret: Type,
    is_public: bool,
}

struct FieldInfo {
    ty: Type,
    is_public: bool,
}

#[derive(Clone, PartialEq)]
enum ClassKind {
    /// Integer/Float/Bool/Char/String â€” value classes, methods only.
    CoreValue,
    /// A user reference class (arena-allocated).
    UserRef,
    /// A monomorphized Array<T>/Vector<T> instantiation.
    Container,
    /// A RawBuffer<T> instantiation (compiler-provided, std-only creation).
    RawBuf,
}

struct ClassInfo {
    methods: HashMap<String, MethodSig>,
    fields: HashMap<String, FieldInfo>,
    ctor_params: Vec<Type>,
    kind: ClassKind,
}

/// A queued monomorphization request for a generic function.
struct FnInst {
    template: String,
    /// (type parameter, concrete argument) in declared order.
    bindings: Vec<(String, Type)>,
    /// Instance key, e.g. "max<Vector<Float>,Float>" — becomes the emitted name.
    key: String,
}

struct Tables {
    funcs: HashMap<String, FuncSig>,
    classes: HashMap<String, ClassInfo>,
    /// Class templates (the std Array<T>/Vector<T>).
    templates: HashMap<String, ClassDecl>,
    /// Generic function templates, monomorphized per call.
    fn_templates: HashMap<String, FunctionDecl>,
    /// Instantiation requests found while lowering; drained by check()'s
    /// fixpoint loop (lowering itself never mutates the tables).
    pending_fns: RefCell<Vec<FnInst>>,
    /// Instance keys already queued or lowered.
    known_fn_insts: RefCell<HashSet<String>>,
    allow_intrinsics: bool,
}

/// Why a core call is being made â€” shapes the error messages.
enum Origin {
    /// e.g. "operator '+'", "operator '++'", "unary '-'",
    /// "mixed Integer/Float arithmetic"
    Operator(String),
    Plain,
}

/// Lexical scopes: name -> (type, is_const)
struct Env {
    scopes: Vec<HashMap<String, (Type, bool)>>,
}

impl Env {
    fn new() -> Self {
        Env { scopes: vec![HashMap::new()] }
    }
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn pop(&mut self) {
        self.scopes.pop();
    }
    fn declare(&mut self, name: &str, ty: Type, is_const: bool) -> bool {
        let top = self.scopes.last_mut().unwrap();
        if top.contains_key(name) {
            return false;
        }
        top.insert(name.to_string(), (ty, is_const));
        true
    }
    fn lookup(&self, name: &str) -> Option<&(Type, bool)> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// How deep the scope holding `name` is (0 = outermost).
    fn lookup_depth(&self, name: &str) -> Option<usize> {
        self.scopes.iter().enumerate().rev().find_map(|(i, s)| {
            if s.contains_key(name) {
                Some(i)
            } else {
                None
            }
        })
    }
}

struct FnCtx<'t> {
    tables: &'t Tables,
    fn_name: String,
    ret: Type,
    from_std: bool,
    /// The class-table key of the enclosing class ("Vector<Integer>", "Shape").
    current_class: Option<String>,
    /// The type of `this` inside the enclosing class.
    this_ty: Option<Type>,
    env: Env,
    loop_depth: u32,
    /// loop_depth captured at each enclosing `try` â€” control flow may not
    /// leave a try block (the handler stack must unwind cleanly).
    try_marks: Vec<u32>,
}

pub fn check(
    program: Program,
    allow_intrinsics: bool,
    require_main: bool,
) -> Result<Program, SemaError> {
    // ---- collect function signatures (no overloading) ----
    // Generic functions (type_params non-empty) are templates: they get no
    // FuncSig — each call monomorphizes an instance instead.
    let mut funcs: HashMap<String, FuncSig> = HashMap::new();
    let mut fn_templates: HashMap<String, FunctionDecl> = HashMap::new();
    for f in &program.functions {
        if funcs.contains_key(&f.name) || fn_templates.contains_key(&f.name) {
            return Err(SemaError {
                file_idx: f.file_idx,
                err: JimError::new(
                    format!("duplicate function '{}' (jim has no function overloading)", f.name),
                    f.line,
                    f.col,
                ),
            });
        }
        if !f.type_params.is_empty() {
            if f.name == "main" {
                return Err(SemaError {
                    file_idx: f.file_idx,
                    err: JimError::new("'main' cannot be generic", f.line, f.col),
                });
            }
            fn_templates.insert(f.name.clone(), f.clone());
            continue;
        }
        funcs.insert(
            f.name.clone(),
            FuncSig { params: f.params.iter().map(|p| p.ty.clone()).collect(), ret: f.ret.clone() },
        );
    }

    // ---- collect + validate class declarations ----
    let mut classes: HashMap<String, ClassInfo> = HashMap::new();
    let mut templates: HashMap<String, ClassDecl> = HashMap::new();
    for c in &program.classes {
        let fail = |msg: String| -> Result<Program, SemaError> {
            Err(SemaError { file_idx: c.file_idx, err: JimError::new(msg, c.line, c.col) })
        };
        if c.type_param.is_some() {
            // a generic template â€” expanded per instantiation, never emitted itself
            if !matches!(c.name.as_str(), "Array" | "Vector") {
                return fail(
                    "user generic classes are not supported (only the std Array<T>/Vector<T>)"
                        .to_string(),
                );
            }
            if !c.from_std && !allow_intrinsics {
                return fail(format!(
                    "class '{}' can only be defined in the standard library",
                    c.name
                ));
            }
            if templates.contains_key(&c.name) {
                return fail(format!("duplicate class '{}'", c.name));
            }
            // catch duplicate members once, on the template itself
            if let Err(msg) = class_info_of(c, ClassKind::Container) {
                return fail(msg);
            }
            templates.insert(c.name.clone(), c.clone());
            continue;
        }
        let kind = if matches!(
            c.name.as_str(),
            "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception"
        ) {
            ClassKind::CoreValue
        } else {
            ClassKind::UserRef
        };
        match c.name.as_str() {
            "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception" => {
                if !c.from_std && !allow_intrinsics {
                    return fail(format!(
                        "class '{}' is a core class and can only be defined in the standard library",
                        c.name
                    ));
                }
                if !c.fields.is_empty() {
                    return fail(format!(
                        "core class '{}' is a value class â€” methods only, no fields",
                        c.name
                    ));
                }
                if c.ctor.is_some() {
                    return fail(format!("core class '{}' cannot have a constructor", c.name));
                }
            }
            "None" => return fail("'None' has no methods to implement".to_string()),
            "Array" | "Vector" => {
                return fail(format!(
                    "class {} must take a type parameter: 'class {}<T>'",
                    c.name, c.name
                ))
            }
            "RawBuffer" => return fail("'RawBuffer' is provided by the compiler".to_string()),
            _ => {} // user-defined class â€” allowed anywhere
        }
        if classes.contains_key(&c.name) {
            return fail(format!("duplicate class '{}'", c.name));
        }
        let mut methods = HashMap::new();
        for m in &c.methods {
            if methods.contains_key(&m.name) {
                return Err(SemaError {
                    file_idx: c.file_idx,
                    err: JimError::new(
                        format!("duplicate method '{}' in class '{}' (jim has no overloading)", m.name, c.name),
                        m.line,
                        m.col,
                    ),
                });
            }
            methods.insert(
                m.name.clone(),
                MethodSig {
                    params: m.params.iter().map(|p| p.ty.clone()).collect(),
                    ret: m.ret.clone(),
                    is_public: m.is_public,
                },
            );
        }
        let mut fields = HashMap::new();
        for fld in &c.fields {
            let dup = fields.contains_key(&fld.name);
            let clash = methods.contains_key(&fld.name);
            if dup || clash {
                return Err(SemaError {
                    file_idx: c.file_idx,
                    err: JimError::new(
                        if dup {
                            format!("duplicate field '{}' in class '{}'", fld.name, c.name)
                        } else {
                            format!("field '{}' collides with a method of the same name in class '{}'", fld.name, c.name)
                        },
                        fld.line,
                        fld.col,
                    ),
                });
            }
            fields.insert(fld.name.clone(), FieldInfo { ty: fld.ty.clone(), is_public: fld.is_public });
        }
        let ctor_params: Vec<Type> = c
            .ctor
            .as_ref()
            .map(|ct| ct.params.iter().map(|p| p.ty.clone()).collect())
            .unwrap_or_default();
        classes.insert(c.name.clone(), ClassInfo { methods, fields, ctor_params, kind });
    }

    // function names and class names share the call syntax â€” no collisions
    for f in &program.functions {
        if classes.contains_key(&f.name) || templates.contains_key(&f.name) {
            return Err(SemaError {
                file_idx: f.file_idx,
                err: JimError::new(
                    format!(
                        "function '{}' collides with class '{}' (constructor calls use the class name)",
                        f.name, f.name
                    ),
                    f.line,
                    f.col,
                ),
            });
        }
    }

    // ---- validate generic function templates ----
    // Bodies are duck-typed per instantiation (like the container templates);
    // here we catch what is knowable up front: shadowed parameter names and
    // signature typos.
    for t in fn_templates.values() {
        let fail = |msg: String, line: u32, col: u32| SemaError {
            file_idx: t.file_idx,
            err: JimError::new(msg, line, col),
        };
        for p in &t.type_params {
            let shadows = matches!(
                p.as_str(),
                "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception" | "None"
                    | "Array" | "Vector" | "RawBuffer"
            ) || classes.contains_key(p)
                || templates.contains_key(p);
            if shadows {
                return Err(fail(
                    format!("type parameter '{}' of '{}' shadows the type '{}'", p, t.name, p),
                    t.line,
                    t.col,
                ));
            }
        }
        for param in &t.params {
            check_template_type(&param.ty, &t.type_params, &classes).map_err(|msg| {
                fail(format!("in parameter '{}': {}", param.name, msg), param.line, param.col)
            })?;
        }
        check_template_type(&t.ret, &t.type_params, &classes).map_err(|msg| {
            fail(format!("in return type of '{}': {}", t.name, msg), t.line, t.col)
        })?;
        if contains_pointer(&t.ret) {
            return Err(fail(
                format!(
                    "function '{}' cannot return a pointer (a pointer to a local would dangle after the call)",
                    t.name
                ),
                t.line,
                t.col,
            ));
        }
    }

    // ---- discover container instantiations & monomorphize ----
    // Generic function templates are skipped: their types mention type
    // parameters and only make sense after substitution.
    let mut mentioned: Vec<Type> = Vec::new();
    for f in &program.functions {
        if !f.type_params.is_empty() {
            continue;
        }
        for p in &f.params {
            mentioned.push(p.ty.clone());
        }
        mentioned.push(f.ret.clone());
        scan_block(&f.body, &mut mentioned);
    }
    for c in &program.classes {
        if c.type_param.is_none() {
            scan_class(c, &mut mentioned);
        }
    }
    // Explicit generic-call type arguments (`max<Vector<Float>, Float>(v)`)
    // can mention container types written nowhere else — keep only the ones
    // that are fully concrete (sema reports unknown names at the call site).
    mentioned.retain(|ty| type_is_concrete(ty, &classes, &templates));
    let mut mono: Vec<(ClassDecl, Type)> = Vec::new();
    for ty in &mentioned {
        ensure_instantiated(ty, &mut classes, &mut mono, &templates, 0)
            .map_err(|msg| SemaError { file_idx: 0, err: JimError::new(msg, 1, 1) })?;
    }
    // Templates are only checked when expanded — probe-instantiate any that
    // nothing mentions yet, so `jimc check core.j` validates their bodies.
    let template_names: Vec<String> = templates.keys().cloned().collect();
    for name in template_names {
        let probe = Type::Generic(name, Box::new(Type::named("Integer")));
        ensure_instantiated(&probe, &mut classes, &mut mono, &templates, 0)
            .map_err(|msg| SemaError { file_idx: 0, err: JimError::new(msg, 1, 1) })?;
    }

    // ---- entry point ----
    match program.functions.iter().find(|f| f.name == "main") {
        None if !require_main => {} // `jimc check` accepts library files
        None => {
            return Err(SemaError {
                file_idx: 0,
                err: JimError::new("program has no entry point: 'function main() -> Integer'", 1, 1),
            })
        }
        Some(m) => {
            if m.ret != Type::named("Integer") {
                return Err(SemaError {
                    file_idx: m.file_idx,
                    err: JimError::new("main must return Integer (the process exit code)", m.line, m.col),
                });
            }
            if !m.params.is_empty() {
                let argv_ty = Type::Generic("Array".to_string(), Box::new(Type::named("String")));
                if m.params.len() != 1 || m.params[0].ty != argv_ty {
                    return Err(SemaError {
                        file_idx: m.file_idx,
                        err: JimError::new(
                            "main must be 'function main() -> Integer' or 'function main(argv: Array<String>) -> Integer'",
                            m.line,
                            m.col,
                        ),
                    });
                }
                // argv is built through the Array construction protocol
                let ok = classes.get("Array<String>").map_or(false, |info| {
                    info.ctor_params == vec![Type::named("Integer")]
                        && info.methods.get("set").map_or(false, |s| {
                            s.params == vec![Type::named("Integer"), Type::named("String")]
                        })
                });
                if !ok {
                    return Err(SemaError {
                        file_idx: m.file_idx,
                        err: JimError::new(
                            "'main(argv)' needs class Array<T> in core.j with 'Array(len: Integer)' and 'set(i: Integer, value: T)'",
                            m.line,
                            m.col,
                        ),
                    });
                }
            }
        }
    }

    let mut tables = Tables {
        funcs,
        classes,
        templates,
        fn_templates,
        pending_fns: RefCell::new(Vec::new()),
        known_fn_insts: RefCell::new(HashSet::new()),
        allow_intrinsics,
    };

    // ---- lower everything ----
    let mut lowered_functions = Vec::with_capacity(program.functions.len());
    for f in program.functions {
        if !f.type_params.is_empty() {
            continue; // generic templates are lowered per instance below
        }
        let file_idx = f.file_idx;
        lowered_functions
            .push(lower_function(f, &tables).map_err(|err| SemaError { file_idx, err })?);
    }
    let mut lowered_classes = Vec::with_capacity(program.classes.len() + mono.len());
    for c in program.classes {
        if c.type_param.is_some() {
            continue; // templates were expanded into `mono`
        }
        let file_idx = c.file_idx;
        let self_ty = Type::named(&c.name);
        lowered_classes
            .push(lower_class(c, self_ty, &tables).map_err(|err| SemaError { file_idx, err })?);
    }
    for (inst, self_ty) in mono {
        let file_idx = inst.file_idx;
        lowered_classes
            .push(lower_class(inst, self_ty, &tables).map_err(|err| SemaError { file_idx, err })?);
    }

    // ---- monomorphize generic functions (fixpoint) ----
    // Lowering queues instantiation requests; lowering an instance can queue
    // more (nested generic calls), so drain until quiet. The counter is a
    // backstop against runaway chains like f<T> calling f<Array<T>>.
    let mut instantiated = 0usize;
    loop {
        let batch: Vec<FnInst> = tables.pending_fns.borrow_mut().drain(..).collect();
        if batch.is_empty() {
            break;
        }
        for inst in batch {
            instantiated += 1;
            if instantiated > 1000 {
                let t = &tables.fn_templates[&inst.template];
                return Err(SemaError {
                    file_idx: t.file_idx,
                    err: JimError::new(
                        "too many generic instantiations (is a generic function calling itself with ever-growing type arguments?)",
                        t.line,
                        t.col,
                    ),
                });
            }
            let tmpl = tables.fn_templates[&inst.template].clone();
            let file_idx = tmpl.file_idx;
            let decl = subst_function(tmpl, &inst.bindings, &inst.key);
            // the instance may mention container types nothing else wrote out
            let mut inst_mentioned: Vec<Type> = Vec::new();
            for p in &decl.params {
                inst_mentioned.push(p.ty.clone());
            }
            inst_mentioned.push(decl.ret.clone());
            scan_block(&decl.body, &mut inst_mentioned);
            inst_mentioned.retain(|ty| type_is_concrete(ty, &tables.classes, &tables.templates));
            let mut new_mono: Vec<(ClassDecl, Type)> = Vec::new();
            for ty in &inst_mentioned {
                ensure_instantiated(ty, &mut tables.classes, &mut new_mono, &tables.templates, 0)
                    .map_err(|msg| SemaError {
                        file_idx,
                        err: JimError::new(msg, decl.line, decl.col),
                    })?;
            }
            for (c, self_ty) in new_mono {
                let fi = c.file_idx;
                lowered_classes.push(
                    lower_class(c, self_ty, &tables).map_err(|err| SemaError { file_idx: fi, err })?,
                );
            }
            let key = inst.key.clone();
            let lowered = lower_function(decl, &tables).map_err(|mut err| {
                err.msg = format!("{} (in the instantiation '{}')", err.msg, key);
                SemaError { file_idx, err }
            })?;
            lowered_functions.push(lowered);
        }
    }

    Ok(Program { functions: lowered_functions, classes: lowered_classes })
}

/// Lenient signature check for generic function templates: type parameters
/// stand in for any type, so only obvious typos are reportable here. Full
/// validation happens per instantiation.
fn check_template_type(
    ty: &Type,
    tparams: &[String],
    classes: &HashMap<String, ClassInfo>,
) -> Result<(), String> {
    match ty {
        Type::Named(n) => {
            if tparams.iter().any(|p| p == n) {
                return Ok(());
            }
            match n.as_str() {
                "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception" | "None" => Ok(()),
                "Array" | "Vector" => Err(format!("{}<T> requires a type argument", n)),
                "RawBuffer" => Err("RawBuffer is reserved for the standard library".to_string()),
                other => {
                    if classes.contains_key(other) {
                        Ok(())
                    } else {
                        Err(format!("unknown type '{}'", other))
                    }
                }
            }
        }
        Type::Generic(n, p) => match n.as_str() {
            "Array" | "Vector" | "RawBuffer" => check_template_type(p, tparams, classes),
            other => Err(format!(
                "'{}' is not a generic type — only Array<T> and Vector<T> exist",
                other
            )),
        },
        Type::Optional(p) | Type::Pointer(p) => check_template_type(p, tparams, classes),
    }
}

/// Does this type name only things that exist? (Filters type-parameter
/// mentions and typos out of the container-instantiation scan.)
fn type_is_concrete(
    ty: &Type,
    classes: &HashMap<String, ClassInfo>,
    templates: &HashMap<String, ClassDecl>,
) -> bool {
    match ty {
        Type::Named(n) => {
            matches!(
                n.as_str(),
                "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception" | "None"
            ) || classes.contains_key(n)
        }
        Type::Generic(n, p) => {
            (templates.contains_key(n) || n == "RawBuffer") && type_is_concrete(p, classes, templates)
        }
        Type::Optional(p) | Type::Pointer(p) => type_is_concrete(p, classes, templates),
    }
}

/// Register the class-table entry for a generic instantiation, expanding the
/// template (and anything the expansion mentions) as needed.
fn ensure_instantiated(
    ty: &Type,
    classes: &mut HashMap<String, ClassInfo>,
    mono: &mut Vec<(ClassDecl, Type)>,
    templates: &HashMap<String, ClassDecl>,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err("generic instantiation is too deep (recursive container types?)".to_string());
    }
    match ty {
        Type::Named(_) => Ok(()),
        Type::Optional(inner) | Type::Pointer(inner) => {
            ensure_instantiated(inner, classes, mono, templates, depth + 1)
        }
        Type::Generic(n, p) => {
            ensure_instantiated(p, classes, mono, templates, depth + 1)?;
            let key = class_key(ty).expect("generic types always have a key");
            if classes.contains_key(&key) {
                return Ok(());
            }
            match n.as_str() {
                "RawBuffer" => {
                    let elem = (**p).clone();
                    let mut methods = HashMap::new();
                    methods.insert(
                        "get".to_string(),
                        MethodSig { params: vec![Type::named("Integer")], ret: elem.clone(), is_public: true },
                    );
                    methods.insert(
                        "set".to_string(),
                        MethodSig {
                            params: vec![Type::named("Integer"), elem],
                            ret: Type::named("None"),
                            is_public: true,
                        },
                    );
                    methods.insert(
                        "capacity".to_string(),
                        MethodSig { params: vec![], ret: Type::named("Integer"), is_public: true },
                    );
                    classes.insert(
                        key,
                        ClassInfo {
                            methods,
                            fields: HashMap::new(),
                            ctor_params: vec![],
                            kind: ClassKind::RawBuf,
                        },
                    );
                    Ok(())
                }
                "Array" | "Vector" => {
                    let tmpl = templates
                        .get(n.as_str())
                        .ok_or_else(|| format!("{}<T> is not defined (is core.j loaded?)", n))?;
                    let param = tmpl.type_param.clone().expect("templates have a type param");
                    let inst = subst_class(tmpl, &param, p, key.clone());
                    // register the signature first so self-referential mentions resolve
                    let info = class_info_of(&inst, ClassKind::Container)?;
                    classes.insert(key, info);
                    let mut more = Vec::new();
                    scan_class(&inst, &mut more);
                    for t in more {
                        ensure_instantiated(&t, classes, mono, templates, depth + 1)?;
                    }
                    mono.push((inst, ty.clone()));
                    Ok(())
                }
                other => Err(format!(
                    "'{}' is not a generic type â€” only Array<T> and Vector<T> exist",
                    other
                )),
            }
        }
    }
}

/// Build a ClassInfo from a declaration (used for templates/instantiations).
fn class_info_of(c: &ClassDecl, kind: ClassKind) -> Result<ClassInfo, String> {
    let mut methods = HashMap::new();
    for m in &c.methods {
        let sig = MethodSig {
            params: m.params.iter().map(|p| p.ty.clone()).collect(),
            ret: m.ret.clone(),
            is_public: m.is_public,
        };
        if methods.insert(m.name.clone(), sig).is_some() {
            return Err(format!("duplicate method '{}' in class '{}'", m.name, c.name));
        }
    }
    let mut fields = HashMap::new();
    for f in &c.fields {
        if methods.contains_key(&f.name) {
            return Err(format!(
                "field '{}' collides with a method of the same name in class '{}'",
                f.name, c.name
            ));
        }
        let info = FieldInfo { ty: f.ty.clone(), is_public: f.is_public };
        if fields.insert(f.name.clone(), info).is_some() {
            return Err(format!("duplicate field '{}' in class '{}'", f.name, c.name));
        }
    }
    let ctor_params = c
        .ctor
        .as_ref()
        .map(|ct| ct.params.iter().map(|p| p.ty.clone()).collect())
        .unwrap_or_default();
    Ok(ClassInfo { methods, fields, ctor_params, kind })
}

fn lower_function(f: FunctionDecl, tables: &Tables) -> SResult<FunctionDecl> {
    let mut ctx = FnCtx {
        tables,
        fn_name: f.name.clone(),
        ret: f.ret.clone(),
        from_std: f.from_std,
        current_class: None,
        this_ty: None,
        env: Env::new(),
        loop_depth: 0,
        try_marks: Vec::new(),
    };
    type_available(tables, &f.ret, true)
        .map_err(|msg| JimError::new(format!("in return type of '{}': {}", f.name, msg), f.line, f.col))?;
    if contains_pointer(&f.ret) {
        return Err(JimError::new(
            format!(
                "function '{}' cannot return a pointer (a pointer to a local would dangle after the call)",
                f.name
            ),
            f.line,
            f.col,
        ));
    }
    for p in &f.params {
        type_available(tables, &p.ty, false)
            .map_err(|msg| JimError::new(format!("in parameter '{}': {}", p.name, msg), p.line, p.col))?;
        if !ctx.env.declare(&p.name, p.ty.clone(), false) {
            return Err(JimError::new(format!("duplicate parameter '{}'", p.name), p.line, p.col));
        }
    }
    let body = ctx.lower_block(&f.body)?;
    // Optional-returning functions may fall off the end (implicit `return None`).
    if f.ret != Type::named("None")
        && !matches!(f.ret, Type::Optional(_))
        && f.name != "main"
        && !block_returns(&body)
    {
        return Err(JimError::new(
            format!(
                "function '{}' may reach the end without returning {} (every path must return)",
                f.name,
                f.ret.display()
            ),
            f.line,
            f.col,
        ));
    }
    Ok(FunctionDecl { body, ..f })
}

fn lower_class(c: ClassDecl, self_ty: Type, tables: &Tables) -> SResult<ClassDecl> {
    let ClassDecl { name: class_name, type_param, fields, ctor, methods, file_idx, from_std, line, col } = c;
    debug_assert!(type_param.is_none(), "templates are expanded before lowering");

    // Field defaults are lowered in a plain context: `this` is not available
    // there (an instance does not exist yet while its defaults run).
    let mut lowered_fields = Vec::with_capacity(fields.len());
    for fld in fields {
        type_available(tables, &fld.ty, false).map_err(|msg| {
            JimError::new(format!("in field '{}': {}", fld.name, msg), fld.line, fld.col)
        })?;
        if contains_pointer(&fld.ty) {
            return Err(JimError::new(
                format!(
                    "field '{}' cannot hold a pointer (the pointed-to variable may die before the object)",
                    fld.name
                ),
                fld.line,
                fld.col,
            ));
        }
        let mut ctx = FnCtx {
            tables,
            fn_name: format!("{}.{}", class_name, fld.name),
            ret: Type::named("None"),
            from_std,
            current_class: None,
            this_ty: None,
            env: Env::new(),
            loop_depth: 0,
            try_marks: Vec::new(),
        };
        let fname = fld.name.clone();
        let default = ctx.lower_expecting(&fld.default, &fld.ty, |want, got| {
            format!("field '{}' is declared {} but its default value is {}", fname, want, got)
        })?;
        lowered_fields.push(FieldDecl { default, ..fld });
    }

    // Constructor: acts like a None-returning method (implicitly returns the instance).
    let lowered_ctor = match ctor {
        Some(ct) => {
            let mut ctx = FnCtx {
                tables,
                fn_name: format!("{} constructor", class_name),
                ret: Type::named("None"),
                from_std,
                current_class: Some(class_name.clone()),
                this_ty: Some(self_ty.clone()),
                env: Env::new(),
                loop_depth: 0,
                try_marks: Vec::new(),
            };
            for p in &ct.params {
                type_available(tables, &p.ty, false).map_err(|msg| {
                    JimError::new(format!("in parameter '{}': {}", p.name, msg), p.line, p.col)
                })?;
                if !ctx.env.declare(&p.name, p.ty.clone(), false) {
                    return Err(JimError::new(format!("duplicate parameter '{}'", p.name), p.line, p.col));
                }
            }
            let body = ctx.lower_block(&ct.body)?;
            Some(CtorDecl { body, ..ct })
        }
        None => None,
    };

    let mut lowered_methods = Vec::with_capacity(methods.len());
    for m in methods {
        let mut ctx = FnCtx {
            tables,
            fn_name: format!("{}.{}", class_name, m.name),
            ret: m.ret.clone(),
            from_std,
            current_class: Some(class_name.clone()),
            this_ty: Some(self_ty.clone()),
            env: Env::new(),
            loop_depth: 0,
            try_marks: Vec::new(),
        };
        type_available(tables, &m.ret, true).map_err(|msg| {
            JimError::new(format!("in return type of '{}.{}': {}", class_name, m.name, msg), m.line, m.col)
        })?;
        if contains_pointer(&m.ret) {
            return Err(JimError::new(
                format!(
                    "method '{}.{}' cannot return a pointer (a pointer to a local would dangle after the call)",
                    class_name, m.name
                ),
                m.line,
                m.col,
            ));
        }
        for p in &m.params {
            type_available(tables, &p.ty, false).map_err(|msg| {
                JimError::new(format!("in parameter '{}': {}", p.name, msg), p.line, p.col)
            })?;
            if !ctx.env.declare(&p.name, p.ty.clone(), false) {
                return Err(JimError::new(format!("duplicate parameter '{}'", p.name), p.line, p.col));
            }
        }
        let body = ctx.lower_block(&m.body)?;
        if m.ret != Type::named("None")
            && !matches!(m.ret, Type::Optional(_))
            && !block_returns(&body)
        {
            return Err(JimError::new(
                format!(
                    "method '{}.{}' may reach the end without returning {} (every path must return)",
                    class_name,
                    m.name,
                    m.ret.display()
                ),
                m.line,
                m.col,
            ));
        }
        lowered_methods.push(MethodDecl { body, ..m });
    }
    Ok(ClassDecl {
        name: class_name,
        type_param: None,
        fields: lowered_fields,
        ctor: lowered_ctor,
        methods: lowered_methods,
        file_idx,
        from_std,
        line,
        col,
    })
}

/// Which types exist in the current milestone.
fn type_available(tables: &Tables, ty: &Type, is_return: bool) -> Result<(), String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "Integer" | "Float" | "Bool" | "Char" | "String" | "Exception" => Ok(()),
            "None" => {
                if is_return {
                    Ok(())
                } else {
                    Err("None is only usable as a return type".to_string())
                }
            }
            "Array" | "Vector" => Err(format!("{}<T> requires a type argument", n)),
            "RawBuffer" => Err("RawBuffer is reserved for the standard library".to_string()),
            other => {
                if tables.classes.contains_key(other) {
                    Ok(())
                } else {
                    Err(format!("unknown type '{}'", other))
                }
            }
        },
        Type::Generic(n, p) => match n.as_str() {
            "Array" | "Vector" => {
                if contains_pointer(p) {
                    return Err("containers of pointers are not allowed (the pointed-to variable may die before the container)".to_string());
                }
                let key = class_key(ty).expect("generic types always have a key");
                if tables.classes.contains_key(&key) {
                    type_available(tables, p, false)
                } else {
                    Err(format!("{}<T> is not defined (is core.j loaded?)", n))
                }
            }
            "RawBuffer" => {
                if contains_pointer(p) {
                    Err("buffers of pointers are not allowed".to_string())
                } else {
                    Ok(()) // creation is std-gated via @buf_alloc
                }
            }
            other => Err(format!(
                "'{}' is not a generic type â€” only Array<T> and Vector<T> exist",
                other
            )),
        },
        Type::Pointer(inner) => match inner.as_ref() {
            Type::Pointer(_) => Err("**T (pointer to pointer) is not supported".to_string()),
            Type::Optional(_) => Err("pointers to optionals are not supported (did you mean '*T?', an optional pointer?)".to_string()),
            _ => type_available(tables, inner, false),
        },
        Type::Optional(inner) => {
            if matches!(inner.as_ref(), Type::Optional(_)) {
                return Err("nested optionals (T??) are not allowed".to_string());
            }
            if let Type::Generic(n, _) = inner.as_ref() {
                if n == "RawBuffer" {
                    return Err("RawBuffer? is not allowed".to_string());
                }
            }
            if matches!(inner.as_ref(), Type::Named(n) if n == "Exception") {
                return Err("Exception? is not supported".to_string());
            }
            type_available(tables, inner, false)
        }
    }
}

/// The class-table key for a type that can have methods/fields.
/// Named("Shape") -> "Shape"; Generic(Vector, Integer) -> "Vector<Integer>".
fn class_key(ty: &Type) -> Option<String> {
    match ty {
        Type::Named(n) => Some(n.clone()),
        Type::Generic(n, p) => Some(format!("{}<{}>", n, p.display())),
        _ => None,
    }
}

/// For `T?`, the payload T â€” when T has an optional representation
/// (core values, classes, containers, pointers).
fn opt_payload(ty: &Type) -> Option<Type> {
    match ty {
        Type::Optional(inner) => {
            if class_key(inner).is_some() || matches!(inner.as_ref(), Type::Pointer(_)) {
                Some((**inner).clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// If the value is a `T?`, insert the unwrap-or-panic so it becomes a `T`.
fn deopt(pair: (Expr, Type)) -> (Expr, Type) {
    match opt_payload(&pair.1) {
        Some(inner) => {
            let (e, _) = pair;
            let (line, col) = (e.line, e.col);
            (
                Expr {
                    kind: ExprKind::OptUnwrap { payload: inner.clone(), expr: Box::new(e) },
                    line,
                    col,
                },
                inner,
            )
        }
        None => pair,
    }
}

/// jim's optional coercions: exact match, `T -> T?` (wrap, always safe),
/// or `T? -> T` (unwrap, runtime-checked). Anything else is a type error â€”
/// the pair comes back so the caller can build its own message.
fn try_coerce(pair: (Expr, Type), want: &Type) -> Result<Expr, (Expr, Type)> {
    if &pair.1 == want {
        return Ok(pair.0);
    }
    if let Some(payload) = opt_payload(want) {
        if pair.1 == payload {
            let (e, _) = pair;
            let (line, col) = (e.line, e.col);
            return Ok(Expr { kind: ExprKind::OptWrap { payload, expr: Box::new(e) }, line, col });
        }
    }
    if let Some(payload) = opt_payload(&pair.1) {
        if want == &payload {
            let (e, _) = pair;
            let (line, col) = (e.line, e.col);
            return Ok(Expr { kind: ExprKind::OptUnwrap { payload, expr: Box::new(e) }, line, col });
        }
    }
    Err(pair)
}

/// Does this type contain a pointer anywhere? (Pointers must not outlive the
/// variable they point at â€” so they cannot be returned, stored in fields, or
/// put in containers.)
fn contains_pointer(ty: &Type) -> bool {
    match ty {
        Type::Pointer(_) => true,
        Type::Optional(i) => contains_pointer(i),
        Type::Generic(_, i) => contains_pointer(i),
        Type::Named(_) => false,
    }
}

/// Substitute type parameters with concrete types.
fn subst_type(ty: &Type, bindings: &[(String, Type)]) -> Type {
    match ty {
        Type::Named(n) => bindings
            .iter()
            .find(|(p, _)| p == n)
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| ty.clone()),
        Type::Generic(n, p) => Type::Generic(n.clone(), Box::new(subst_type(p, bindings))),
        Type::Pointer(p) => Type::Pointer(Box::new(subst_type(p, bindings))),
        Type::Optional(p) => Type::Optional(Box::new(subst_type(p, bindings))),
    }
}

/// Types hide inside expressions in exactly one place: the explicit type
/// arguments of a generic call (`g<T>(x)` inside another template).
fn subst_expr(e: &Expr, bindings: &[(String, Type)]) -> Expr {
    let sub = |e: &Expr| subst_expr(e, bindings);
    let subv = |es: &[Expr]| es.iter().map(sub).collect::<Vec<_>>();
    let kind = match &e.kind {
        ExprKind::GenericCall { name, type_args, args } => ExprKind::GenericCall {
            name: name.clone(),
            type_args: type_args.iter().map(|t| subst_type(t, bindings)).collect(),
            args: subv(args),
        },
        ExprKind::Call { name, args } => {
            ExprKind::Call { name: name.clone(), args: subv(args) }
        }
        ExprKind::MethodCall { recv, name, args } => ExprKind::MethodCall {
            recv: Box::new(sub(recv)),
            name: name.clone(),
            args: subv(args),
        },
        ExprKind::IntrinsicCall { name, args } => {
            ExprKind::IntrinsicCall { name: name.clone(), args: subv(args) }
        }
        ExprKind::ArrayLit(items) => ExprKind::ArrayLit(subv(items)),
        ExprKind::FieldAccess { recv, name } => {
            ExprKind::FieldAccess { recv: Box::new(sub(recv)), name: name.clone() }
        }
        ExprKind::Index { recv, index } => {
            ExprKind::Index { recv: Box::new(sub(recv)), index: Box::new(sub(index)) }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            ExprKind::Binary { op: *op, lhs: Box::new(sub(lhs)), rhs: Box::new(sub(rhs)) }
        }
        ExprKind::Unary { op, operand } => {
            ExprKind::Unary { op: *op, operand: Box::new(sub(operand)) }
        }
        other => other.clone(),
    };
    Expr { kind, line: e.line, col: e.col }
}

fn subst_block(b: &Block, bindings: &[(String, Type)]) -> Block {
    Block { stmts: b.stmts.iter().map(|s| subst_stmt(s, bindings)).collect() }
}

fn subst_stmt(s: &Stmt, bindings: &[(String, Type)]) -> Stmt {
    let kind = match &s.kind {
        StmtKind::VarDecl { is_const, name, ty, init } => StmtKind::VarDecl {
            is_const: *is_const,
            name: name.clone(),
            ty: subst_type(ty, bindings),
            init: subst_expr(init, bindings),
        },
        StmtKind::Assign { target, op, value } => StmtKind::Assign {
            target: subst_expr(target, bindings),
            op: op.clone(),
            value: subst_expr(value, bindings),
        },
        StmtKind::IncDec { target, inc } => {
            StmtKind::IncDec { target: subst_expr(target, bindings), inc: *inc }
        }
        StmtKind::ExprStmt(e) => StmtKind::ExprStmt(subst_expr(e, bindings)),
        StmtKind::Return(v) => StmtKind::Return(v.as_ref().map(|e| subst_expr(e, bindings))),
        StmtKind::If { arms, else_block } => StmtKind::If {
            arms: arms
                .iter()
                .map(|(c, b)| (subst_expr(c, bindings), subst_block(b, bindings)))
                .collect(),
            else_block: else_block.as_ref().map(|b| subst_block(b, bindings)),
        },
        StmtKind::While { cond, body } => StmtKind::While {
            cond: subst_expr(cond, bindings),
            body: subst_block(body, bindings),
        },
        StmtKind::ForC { var_name, var_ty, init, cond, step, body } => StmtKind::ForC {
            var_name: var_name.clone(),
            var_ty: subst_type(var_ty, bindings),
            init: subst_expr(init, bindings),
            cond: subst_expr(cond, bindings),
            step: Box::new(subst_stmt(step, bindings)),
            body: subst_block(body, bindings),
        },
        StmtKind::ForIn { var_name, var_ty, iterable, body } => StmtKind::ForIn {
            var_name: var_name.clone(),
            var_ty: subst_type(var_ty, bindings),
            iterable: subst_expr(iterable, bindings),
            body: subst_block(body, bindings),
        },
        StmtKind::TryCatch { body, var_name, var_ty, catch_body } => StmtKind::TryCatch {
            body: subst_block(body, bindings),
            var_name: var_name.clone(),
            var_ty: subst_type(var_ty, bindings),
            catch_body: subst_block(catch_body, bindings),
        },
        other => other.clone(),
    };
    Stmt { kind, line: s.line, col: s.col }
}

fn subst_class(tmpl: &ClassDecl, param: &str, arg: &Type, key: String) -> ClassDecl {
    let bindings = [(param.to_string(), arg.clone())];
    ClassDecl {
        name: key,
        type_param: None,
        fields: tmpl
            .fields
            .iter()
            .map(|f| FieldDecl {
                ty: subst_type(&f.ty, &bindings),
                default: subst_expr(&f.default, &bindings),
                ..f.clone()
            })
            .collect(),
        ctor: tmpl.ctor.as_ref().map(|ct| CtorDecl {
            params: ct
                .params
                .iter()
                .map(|p| Param { ty: subst_type(&p.ty, &bindings), ..p.clone() })
                .collect(),
            body: subst_block(&ct.body, &bindings),
            line: ct.line,
            col: ct.col,
        }),
        methods: tmpl
            .methods
            .iter()
            .map(|m| MethodDecl {
                params: m
                    .params
                    .iter()
                    .map(|p| Param { ty: subst_type(&p.ty, &bindings), ..p.clone() })
                    .collect(),
                ret: subst_type(&m.ret, &bindings),
                body: subst_block(&m.body, &bindings),
                ..m.clone()
            })
            .collect(),
        file_idx: tmpl.file_idx,
        from_std: tmpl.from_std,
        line: tmpl.line,
        col: tmpl.col,
    }
}

/// Stamp a concrete instance of a generic function template.
fn subst_function(tmpl: FunctionDecl, bindings: &[(String, Type)], key: &str) -> FunctionDecl {
    FunctionDecl {
        name: key.to_string(),
        type_params: Vec::new(),
        params: tmpl
            .params
            .iter()
            .map(|p| Param { ty: subst_type(&p.ty, bindings), ..p.clone() })
            .collect(),
        ret: subst_type(&tmpl.ret, bindings),
        body: subst_block(&tmpl.body, bindings),
        ..tmpl
    }
}

/// Structural unification: type-parameter names in `pat` bind to the
/// corresponding part of `got`; everything else must match exactly.
fn unify(
    pat: &Type,
    got: &Type,
    tparams: &[String],
    bindings: &mut HashMap<String, Type>,
) -> bool {
    if let Type::Named(n) = pat {
        if tparams.iter().any(|p| p == n) {
            return match bindings.get(n) {
                Some(bound) => bound == got,
                None => {
                    bindings.insert(n.clone(), got.clone());
                    true
                }
            };
        }
    }
    match (pat, got) {
        (Type::Named(a), Type::Named(b)) => a == b,
        (Type::Generic(a, p), Type::Generic(b, q)) if a == b => unify(p, q, tparams, bindings),
        (Type::Optional(p), Type::Optional(q)) => unify(p, q, tparams, bindings),
        (Type::Pointer(p), Type::Pointer(q)) => unify(p, q, tparams, bindings),
        _ => false,
    }
}

/// Best-effort unification against the *expected* type at the call site —
/// fills unbound parameters, never overwrites, never fails (the final
/// coercion check reports any real mismatch). `T` unifying with a `T?`
/// context looks through the optional (T -> T? wrapping is a coercion).
fn unify_soft(pat: &Type, got: &Type, tparams: &[String], bindings: &mut HashMap<String, Type>) {
    if let Type::Named(n) = pat {
        if tparams.iter().any(|p| p == n) {
            if !bindings.contains_key(n) {
                bindings.insert(n.clone(), got.clone());
            }
            return;
        }
    }
    match (pat, got) {
        (Type::Generic(a, p), Type::Generic(b, q)) if a == b => unify_soft(p, q, tparams, bindings),
        (Type::Optional(p), Type::Optional(q)) => unify_soft(p, q, tparams, bindings),
        (Type::Pointer(p), Type::Pointer(q)) => unify_soft(p, q, tparams, bindings),
        (pat, Type::Optional(q)) if !matches!(pat, Type::Optional(_)) => {
            unify_soft(pat, q, tparams, bindings)
        }
        _ => {}
    }
}

/// Does this type mention a type parameter that has no binding yet?
fn mentions_unbound(ty: &Type, tparams: &[String], bindings: &HashMap<String, Type>) -> bool {
    match ty {
        Type::Named(n) => tparams.iter().any(|p| p == n) && !bindings.contains_key(n),
        Type::Generic(_, p) | Type::Optional(p) | Type::Pointer(p) => {
            mentions_unbound(p, tparams, bindings)
        }
    }
}

/// Collect every type written in a block: declarations, plus the explicit
/// type arguments of generic calls (the one place expressions carry types).
fn scan_block(b: &Block, out: &mut Vec<Type>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::VarDecl { ty, init, .. } => {
                out.push(ty.clone());
                scan_expr(init, out);
            }
            StmtKind::Assign { target, value, .. } => {
                scan_expr(target, out);
                scan_expr(value, out);
            }
            StmtKind::IncDec { target, .. } => scan_expr(target, out),
            StmtKind::ExprStmt(e) => scan_expr(e, out),
            StmtKind::Return(v) => {
                if let Some(e) = v {
                    scan_expr(e, out);
                }
            }
            StmtKind::If { arms, else_block } => {
                for (c, b) in arms {
                    scan_expr(c, out);
                    scan_block(b, out);
                }
                if let Some(b) = else_block {
                    scan_block(b, out);
                }
            }
            StmtKind::While { cond, body } => {
                scan_expr(cond, out);
                scan_block(body, out);
            }
            StmtKind::ForC { var_ty, init, cond, step, body, .. } => {
                out.push(var_ty.clone());
                scan_expr(init, out);
                scan_expr(cond, out);
                scan_block(&Block { stmts: vec![(**step).clone()] }, out);
                scan_block(body, out);
            }
            StmtKind::ForIn { var_ty, iterable, body, .. } => {
                out.push(var_ty.clone());
                scan_expr(iterable, out);
                scan_block(body, out);
            }
            StmtKind::TryCatch { body, catch_body, .. } => {
                scan_block(body, out);
                scan_block(catch_body, out);
            }
            _ => {}
        }
    }
}

fn scan_expr(e: &Expr, out: &mut Vec<Type>) {
    match &e.kind {
        ExprKind::GenericCall { type_args, args, .. } => {
            out.extend(type_args.iter().cloned());
            for a in args {
                scan_expr(a, out);
            }
        }
        ExprKind::Call { args, .. } | ExprKind::IntrinsicCall { args, .. } => {
            for a in args {
                scan_expr(a, out);
            }
        }
        ExprKind::MethodCall { recv, args, .. } => {
            scan_expr(recv, out);
            for a in args {
                scan_expr(a, out);
            }
        }
        ExprKind::ArrayLit(items) => {
            for i in items {
                scan_expr(i, out);
            }
        }
        ExprKind::FieldAccess { recv, .. } => scan_expr(recv, out),
        ExprKind::Index { recv, index } => {
            scan_expr(recv, out);
            scan_expr(index, out);
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            scan_expr(lhs, out);
            scan_expr(rhs, out);
        }
        ExprKind::Unary { operand, .. } => scan_expr(operand, out),
        _ => {}
    }
}

fn scan_class(c: &ClassDecl, out: &mut Vec<Type>) {
    for f in &c.fields {
        out.push(f.ty.clone());
        scan_expr(&f.default, out);
    }
    if let Some(ct) = &c.ctor {
        for p in &ct.params {
            out.push(p.ty.clone());
        }
        scan_block(&ct.body, out);
    }
    for m in &c.methods {
        for p in &m.params {
            out.push(p.ty.clone());
        }
        out.push(m.ret.clone());
        scan_block(&m.body, out);
    }
}

fn block_returns(block: &Block) -> bool {
    block.stmts.iter().any(stmt_returns)
}

fn stmt_returns(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::If { arms, else_block } => {
            else_block.as_ref().map_or(false, block_returns)
                && arms.iter().all(|(_, b)| block_returns(b))
        }
        StmtKind::Scope(b) => block_returns(b),
        _ => false,
    }
}

impl<'t> FnCtx<'t> {
    fn lower_block(&mut self, block: &Block) -> SResult<Block> {
        self.env.push();
        let mut stmts = Vec::with_capacity(block.stmts.len());
        for stmt in &block.stmts {
            stmts.push(self.lower_stmt(stmt)?);
        }
        self.env.pop();
        Ok(Block { stmts })
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> SResult<Stmt> {
        let (line, col) = (stmt.line, stmt.col);
        let kind = match &stmt.kind {
            StmtKind::VarDecl { is_const, name, ty, init } => {
                type_available(self.tables, ty, false).map_err(|msg| {
                    JimError::new(format!("in declaration of '{}': {}", name, msg), line, col)
                })?;
                let init = self.lower_expecting(init, ty, |want, got| {
                    format!(
                        "type mismatch: '{}' is declared {} but initialized with {}",
                        name, want, got
                    )
                })?;
                if !self.env.declare(name, ty.clone(), *is_const) {
                    return Err(JimError::new(
                        format!("'{}' is already declared in this scope", name),
                        line,
                        col,
                    ));
                }
                StmtKind::VarDecl { is_const: *is_const, name: name.clone(), ty: ty.clone(), init }
            }
            StmtKind::Assign { target, op, value } => {
                match &target.kind {
                    ExprKind::Ident(name) => {
                        let (var_ty, is_const) = self
                            .env
                            .lookup(name)
                            .ok_or_else(|| {
                                JimError::new(format!("unknown variable '{}'", name), target.line, target.col)
                            })?
                            .clone();
                        if is_const {
                            return Err(JimError::new(
                                format!("cannot assign to constant '{}'", name),
                                target.line,
                                target.col,
                            ));
                        }
                        // scope-escape check: `p = &y` must not let a pointer
                        // outlive its target
                        if let ExprKind::Unary { op: UnOp::AddrOf, operand } = &value.kind {
                            if let ExprKind::Ident(src) = &operand.kind {
                                if let (Some(dst_d), Some(src_d)) =
                                    (self.env.lookup_depth(name), self.env.lookup_depth(src))
                                {
                                    if dst_d < src_d {
                                        return Err(JimError::new(
                                            format!(
                                                "cannot store a pointer to '{}' in '{}': '{}' lives in an inner scope and dies first",
                                                src, name, src
                                            ),
                                            line,
                                            col,
                                        ));
                                    }
                                }
                            }
                        }
                        match op {
                            AssignOp::Set => {
                                let value = self.lower_expecting(value, &var_ty, |want, got| {
                                    format!(
                                        "type mismatch: '{}' is {} but the assigned value is {}",
                                        name, want, got
                                    )
                                })?;
                                StmtKind::Assign { target: target.clone(), op: AssignOp::Set, value }
                            }
                            AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div => {
                                let value_pair = self.lower_expr(value)?;
                                let (sym, method) = match op {
                                    AssignOp::Add => ("+=", "plus"),
                                    AssignOp::Sub => ("-=", "minus"),
                                    AssignOp::Mul => ("*=", "times"),
                                    AssignOp::Div => ("/=", "divide"),
                                    AssignOp::Set => unreachable!(),
                                };
                                self.desugar_augmented(
                                    format!("'{}'", name),
                                    target.clone(),
                                    &var_ty,
                                    method,
                                    sym,
                                    value_pair,
                                    line,
                                    col,
                                )?
                            }
                        }
                    }
                    ExprKind::Index { recv, index } => {
                        // compound assignment evaluates receiver and index twice
                        if !matches!(op, AssignOp::Set) {
                            let simple_recv =
                                matches!(recv.kind, ExprKind::Ident(_) | ExprKind::This);
                            let simple_idx =
                                matches!(index.kind, ExprKind::Ident(_) | ExprKind::Int(_));
                            if !simple_recv || !simple_idx {
                                return Err(JimError::new(
                                    "compound assignment to an index needs a simple receiver and index (e.g. 'v[i] += x')",
                                    line,
                                    col,
                                ));
                            }
                        }
                        let recv_l = deopt(self.lower_expr(recv)?);
                        let idx = self.lower_expecting(index, &Type::named("Integer"), |w, g| {
                            format!("index must be {}, found {}", w, g)
                        })?;
                        let int = Type::named("Integer");
                        let set_params: Option<Vec<Type>> = class_key(&recv_l.1)
                            .and_then(|k| self.tables.classes.get(&k))
                            .and_then(|i| i.methods.get("set"))
                            .map(|s| s.params.clone());
                        match (op, set_params) {
                            (AssignOp::Set, Some(ps)) if ps.len() == 2 => {
                                let elem_ty = ps[1].clone();
                                let value = self.lower_expecting(value, &elem_ty, |w, g| {
                                    format!(
                                        "type mismatch: the element type is {} but the assigned value is {}",
                                        w, g
                                    )
                                })?;
                                let (call, _) = self.core_call(
                                    &recv_l.1.clone(),
                                    "set",
                                    recv_l.0,
                                    vec![(idx, int), (value, elem_ty)],
                                    line,
                                    col,
                                    Origin::Operator("index assignment '[...] ='".to_string()),
                                )?;
                                StmtKind::ExprStmt(call)
                            }
                            (AssignOp::Set, _) => {
                                // no usable `set` â€” let core_call report it nicely
                                let value_pair = self.lower_expr(value)?;
                                let (call, _) = self.core_call(
                                    &recv_l.1.clone(),
                                    "set",
                                    recv_l.0,
                                    vec![(idx, int), value_pair],
                                    line,
                                    col,
                                    Origin::Operator("index assignment '[...] ='".to_string()),
                                )?;
                                StmtKind::ExprStmt(call)
                            }
                            (compound_op, set_params) => {
                                let (sym, method) = match compound_op {
                                    AssignOp::Add => ("+=", "plus"),
                                    AssignOp::Sub => ("-=", "minus"),
                                    AssignOp::Mul => ("*=", "times"),
                                    AssignOp::Div => ("/=", "divide"),
                                    AssignOp::Set => unreachable!(),
                                };
                                let origin = || Origin::Operator(format!("operator '{}'", sym));
                                let value_pair = self.lower_expr(value)?;
                                let (get_call, elem_got) = self.core_call(
                                    &recv_l.1.clone(),
                                    "get",
                                    recv_l.0.clone(),
                                    vec![(idx.clone(), int.clone())],
                                    line,
                                    col,
                                    origin(),
                                )?;
                                let recv2 = deopt((get_call, elem_got));
                                let value_pair = deopt(value_pair);
                                let (recv2, value_pair) = self.widen(recv2, value_pair)?;
                                let (combined, result_ty) = self.core_call(
                                    &recv2.1.clone(),
                                    method,
                                    recv2.0,
                                    vec![value_pair],
                                    line,
                                    col,
                                    origin(),
                                )?;
                                let elem_want = match set_params {
                                    Some(ps) if ps.len() == 2 => ps[1].clone(),
                                    _ => result_ty.clone(),
                                };
                                let combined =
                                    try_coerce((combined, result_ty.clone()), &elem_want).map_err(
                                        |_| {
                                            JimError::new(
                                                format!(
                                                    "operator '{}': result is {} but the element type is {}",
                                                    sym,
                                                    result_ty.display(),
                                                    elem_want.display()
                                                ),
                                                line,
                                                col,
                                            )
                                        },
                                    )?;
                                let (set_call, _) = self.core_call(
                                    &recv_l.1.clone(),
                                    "set",
                                    recv_l.0,
                                    vec![(idx, int), (combined, elem_want)],
                                    line,
                                    col,
                                    origin(),
                                )?;
                                StmtKind::ExprStmt(set_call)
                            }
                        }
                    }
                    ExprKind::FieldAccess { recv, name: fname } => {
                        // compound assignment evaluates the receiver twice (get + set):
                        // only allow receivers without side effects
                        if !matches!(op, AssignOp::Set)
                            && !matches!(recv.kind, ExprKind::This | ExprKind::Ident(_))
                        {
                            return Err(JimError::new(
                                "compound assignment to a field needs a simple receiver ('this.x += v' or 'obj.x += v')",
                                line,
                                col,
                            ));
                        }
                        let (recv_l, rty) = deopt(self.lower_expr(recv)?);
                        let (_, fty) = self.resolve_field(&rty, fname, target.line, target.col)?;
                        let lowered_target = Expr {
                            kind: ExprKind::FieldAccess { recv: Box::new(recv_l), name: fname.clone() },
                            line: target.line,
                            col: target.col,
                        };
                        match op {
                            AssignOp::Set => {
                                let value = self.lower_expecting(value, &fty, |want, got| {
                                    format!(
                                        "type mismatch: field '{}' is {} but the assigned value is {}",
                                        fname, want, got
                                    )
                                })?;
                                StmtKind::Assign { target: lowered_target, op: AssignOp::Set, value }
                            }
                            AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div => {
                                let (sym, method) = match op {
                                    AssignOp::Add => ("+=", "plus"),
                                    AssignOp::Sub => ("-=", "minus"),
                                    AssignOp::Mul => ("*=", "times"),
                                    AssignOp::Div => ("/=", "divide"),
                                    AssignOp::Set => unreachable!(),
                                };
                                let value_pair = self.lower_expr(value)?;
                                self.desugar_augmented(
                                    format!("field '{}'", fname),
                                    lowered_target,
                                    &fty,
                                    method,
                                    sym,
                                    value_pair,
                                    line,
                                    col,
                                )?
                            }
                        }
                    }
                    ExprKind::Unary { op: UnOp::Deref, operand } => {
                        // compound assignment evaluates the pointer twice
                        if !matches!(op, AssignOp::Set) && !matches!(operand.kind, ExprKind::Ident(_)) {
                            return Err(JimError::new(
                                "compound assignment through a pointer needs a simple pointer variable ('*p += v')",
                                line,
                                col,
                            ));
                        }
                        let (ptr, pty) = deopt(self.lower_expr(operand)?);
                        let inner = match pty {
                            Type::Pointer(i) => *i,
                            other => {
                                return Err(JimError::new(
                                    format!("'*' needs a pointer, found {}", other.display()),
                                    ptr.line,
                                    ptr.col,
                                ))
                            }
                        };
                        let lowered_target = Expr {
                            kind: ExprKind::Unary { op: UnOp::Deref, operand: Box::new(ptr) },
                            line: target.line,
                            col: target.col,
                        };
                        match op {
                            AssignOp::Set => {
                                let value = self.lower_expecting(value, &inner, |w, g| {
                                    format!(
                                        "type mismatch: the pointer targets {} but the assigned value is {}",
                                        w, g
                                    )
                                })?;
                                StmtKind::Assign { target: lowered_target, op: AssignOp::Set, value }
                            }
                            AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div => {
                                let (sym, method) = match op {
                                    AssignOp::Add => ("+=", "plus"),
                                    AssignOp::Sub => ("-=", "minus"),
                                    AssignOp::Mul => ("*=", "times"),
                                    AssignOp::Div => ("/=", "divide"),
                                    AssignOp::Set => unreachable!(),
                                };
                                let value_pair = self.lower_expr(value)?;
                                self.desugar_augmented(
                                    "the pointed-to value".to_string(),
                                    lowered_target,
                                    &inner,
                                    method,
                                    sym,
                                    value_pair,
                                    line,
                                    col,
                                )?
                            }
                        }
                    }
                    _ => unreachable!("parser validated assignment targets"),
                }
            }
            StmtKind::IncDec { target, inc } => {
                // `x++` is exactly `x += 1` â€” reuse that path (variables,
                // fields, and indexes all work uniformly).
                let one = Expr { kind: ExprKind::Int(1), line, col };
                let op = if *inc { AssignOp::Add } else { AssignOp::Sub };
                let synthetic = Stmt {
                    kind: StmtKind::Assign { target: target.clone(), op, value: one },
                    line,
                    col,
                };
                return self.lower_stmt(&synthetic);
            }
            StmtKind::ExprStmt(e) => {
                let (e, _) = self.lower_expr(e)?;
                StmtKind::ExprStmt(e)
            }
            StmtKind::Return(value) => match value {
                _ if !self.try_marks.is_empty() => {
                    return Err(JimError::new(
                        "'return' inside a try block is not supported yet (the handler must unwind first — milestone 9)",
                        line,
                        col,
                    ))
                }
                Some(e) => {
                    if self.ret == Type::named("None") {
                        return Err(JimError::new(
                            format!("'{}' returns None and cannot return a value", self.fn_name),
                            line,
                            col,
                        ));
                    }
                    let ret = self.ret.clone();
                    let fn_name = self.fn_name.clone();
                    let e = self.lower_expecting(e, &ret, |want, got| {
                        format!(
                            "return type mismatch: '{}' returns {} but this returns {}",
                            fn_name, want, got
                        )
                    })?;
                    StmtKind::Return(Some(e))
                }
                None => {
                    if self.ret != Type::named("None") {
                        let hint = if matches!(self.ret, Type::Optional(_)) {
                            " (use 'return None;' to return no value)"
                        } else {
                            ""
                        };
                        return Err(JimError::new(
                            format!(
                                "'return;' without a value in '{}' which returns {}{}",
                                self.fn_name,
                                self.ret.display(),
                                hint
                            ),
                            line,
                            col,
                        ));
                    }
                    StmtKind::Return(None)
                }
            },
            StmtKind::If { arms, else_block } => {
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for (cond, body) in arms {
                    let cond = self.lower_condition(cond, "if")?;
                    let body = self.lower_block(body)?;
                    lowered_arms.push((cond, body));
                }
                let else_block = match else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                StmtKind::If { arms: lowered_arms, else_block }
            }
            StmtKind::While { cond, body } => {
                let cond = self.lower_condition(cond, "while")?;
                self.loop_depth += 1;
                let body = self.lower_block(body)?;
                self.loop_depth -= 1;
                StmtKind::While { cond, body }
            }
            StmtKind::ForC { var_name, var_ty, init, cond, step, body } => {
                type_available(self.tables, var_ty, false).map_err(|msg| {
                    JimError::new(format!("in declaration of '{}': {}", var_name, msg), line, col)
                })?;
                let init = self.lower_expecting(init, var_ty, |want, got| {
                    format!(
                        "type mismatch: '{}' is declared {} but initialized with {}",
                        var_name, want, got
                    )
                })?;
                // loop variable lives in its own scope around cond/step/body
                self.env.push();
                self.env.declare(var_name, var_ty.clone(), false);
                let cond = self.lower_condition(cond, "for")?;
                self.loop_depth += 1;
                let body = self.lower_block(body)?;
                let step = Box::new(self.lower_stmt(step)?);
                self.loop_depth -= 1;
                self.env.pop();
                StmtKind::ForC {
                    var_name: var_name.clone(),
                    var_ty: var_ty.clone(),
                    init,
                    cond,
                    step,
                    body,
                }
            }
            StmtKind::ForIn { var_name, var_ty, iterable, body } => {
                // desugar: { var $it = iterable;
                //           for ($i: Integer = 0; $i < $it.length(); $i++) {
                //               var <x>: T = $it.get($i); ...body } }
                // Works for anything exposing length() and get(Integer).
                let (it_e, it_ty) = deopt(self.lower_expr(iterable)?);
                let it_name = format!("__jim_it_{}_{}", line, col);
                let idx_name = format!("__jim_i_{}_{}", line, col);
                self.env.push();
                self.env.declare(&it_name, it_ty.clone(), false);
                let it_decl = Stmt {
                    kind: StmtKind::VarDecl {
                        is_const: false,
                        name: it_name.clone(),
                        ty: it_ty,
                        init: it_e,
                    },
                    line,
                    col,
                };
                let ident = |n: &str| Expr { kind: ExprKind::Ident(n.to_string()), line, col };
                let raw_cond = Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Lt,
                        lhs: Box::new(ident(&idx_name)),
                        rhs: Box::new(Expr {
                            kind: ExprKind::MethodCall {
                                recv: Box::new(ident(&it_name)),
                                name: "length".to_string(),
                                args: vec![],
                            },
                            line,
                            col,
                        }),
                    },
                    line,
                    col,
                };
                let raw_step = Stmt {
                    kind: StmtKind::IncDec { target: ident(&idx_name), inc: true },
                    line,
                    col,
                };
                let mut inner = vec![Stmt {
                    kind: StmtKind::VarDecl {
                        is_const: false,
                        name: var_name.clone(),
                        ty: var_ty.clone(),
                        init: Expr {
                            kind: ExprKind::MethodCall {
                                recv: Box::new(ident(&it_name)),
                                name: "get".to_string(),
                                args: vec![ident(&idx_name)],
                            },
                            line,
                            col,
                        },
                    },
                    line,
                    col,
                }];
                inner.extend(body.stmts.iter().cloned());
                let raw_for = Stmt {
                    kind: StmtKind::ForC {
                        var_name: idx_name,
                        var_ty: Type::named("Integer"),
                        init: Expr { kind: ExprKind::Int(0), line, col },
                        cond: raw_cond,
                        step: Box::new(raw_step),
                        body: Block { stmts: inner },
                    },
                    line,
                    col,
                };
                let lowered_for = self.lower_stmt(&raw_for)?;
                self.env.pop();
                StmtKind::Scope(Block { stmts: vec![it_decl, lowered_for] })
            }
            StmtKind::Scope(_) => unreachable!("parser never produces Scope statements"),
            StmtKind::TryCatch { body, var_name, var_ty, catch_body } => {
                if var_ty != &Type::named("Exception") {
                    return Err(JimError::new(
                        format!(
                            "catch variables must be typed Exception, found {}",
                            var_ty.display()
                        ),
                        line,
                        col,
                    ));
                }
                self.try_marks.push(self.loop_depth);
                let body = self.lower_block(body)?;
                self.try_marks.pop();
                self.env.push();
                self.env.declare(var_name, Type::named("Exception"), false);
                let catch_body = self.lower_block(catch_body)?;
                self.env.pop();
                StmtKind::TryCatch {
                    body,
                    var_name: var_name.clone(),
                    var_ty: var_ty.clone(),
                    catch_body,
                }
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    return Err(JimError::new("'break' outside a loop", line, col));
                }
                if self.try_marks.last().map_or(false, |&mark| self.loop_depth <= mark) {
                    return Err(JimError::new(
                        "'break' cannot leave a try block (milestone 9)",
                        line,
                        col,
                    ));
                }
                StmtKind::Break
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    return Err(JimError::new("'continue' outside a loop", line, col));
                }
                if self.try_marks.last().map_or(false, |&mark| self.loop_depth <= mark) {
                    return Err(JimError::new(
                        "'continue' cannot leave a try block (milestone 9)",
                        line,
                        col,
                    ));
                }
                StmtKind::Continue
            }
        };
        Ok(Stmt { kind, line, col })
    }

    /// `x += v`, `x++`, `this.w -= v`, ... -> `<target> = <target>.method(v)`
    /// The lowered target doubles as the receiver expression (safe: callers
    /// only pass side-effect-free targets).
    #[allow(clippy::too_many_arguments)]
    fn desugar_augmented(
        &mut self,
        target_desc: String,
        lowered_target: Expr,
        store_ty: &Type,
        method: &str,
        sym: &str,
        value: (Expr, Type),
        line: u32,
        col: u32,
    ) -> SResult<StmtKind> {
        let recv = deopt((lowered_target.clone(), store_ty.clone()));
        let value = deopt(value);
        let (recv, value) = self.widen(recv, value)?;
        let origin = Origin::Operator(format!("operator '{}'", sym));
        let (call, result_ty) = self.core_call(&recv.1, method, recv.0, vec![value], line, col, origin)?;
        // the result stores back into the target; wrapping into T? is allowed
        let call = match try_coerce((call, result_ty.clone()), store_ty) {
            Ok(c) => c,
            Err(_) => {
                return Err(JimError::new(
                    format!(
                        "operator '{}': result is {} but {} is {} (the target's type cannot change)",
                        sym,
                        result_ty.display(),
                        target_desc,
                        store_ty.display()
                    ),
                    line,
                    col,
                ))
            }
        };
        Ok(StmtKind::Assign { target: lowered_target, op: AssignOp::Set, value: call })
    }

    /// `[a, b, c]` against an expected `Array<T>`/`Vector<T>`.
    fn lower_container_lit(
        &mut self,
        base: &str,
        payload: &Type,
        elems: &[Expr],
        line: u32,
        col: u32,
    ) -> SResult<Expr> {
        let is_array = base == "Array";
        let key = format!("{}<{}>", base, payload.display());
        {
            let info = self.tables.classes.get(&key).ok_or_else(|| {
                JimError::new(format!("{}<T> is not defined (is core.j loaded?)", base), line, col)
            })?;
            if is_array {
                if info.ctor_params != vec![Type::named("Integer")] {
                    return Err(JimError::new(
                        "array literals need class Array<T> to declare a constructor 'Array(len: Integer)' in core.j",
                        line,
                        col,
                    ));
                }
                let set_ok = info.methods.get("set").map_or(false, |s| {
                    s.params.len() == 2
                        && s.params[0] == Type::named("Integer")
                        && &s.params[1] == payload
                });
                if !set_ok {
                    return Err(JimError::new(
                        "array literals need Array<T> to declare 'set(i: Integer, value: T)' in core.j",
                        line,
                        col,
                    ));
                }
            } else {
                if !info.ctor_params.is_empty() {
                    return Err(JimError::new(
                        "vector literals need class Vector<T> to declare a no-argument constructor 'Vector()' in core.j",
                        line,
                        col,
                    ));
                }
                let push_ok = info
                    .methods
                    .get("push")
                    .map_or(false, |s| s.params.len() == 1 && &s.params[0] == payload);
                if !push_ok {
                    return Err(JimError::new(
                        "vector literals need Vector<T> to declare 'push(value: T)' in core.j",
                        line,
                        col,
                    ));
                }
            }
        }
        let mut lowered = Vec::with_capacity(elems.len());
        for el in elems {
            let el = self.lower_expecting(el, payload, |w, g| {
                format!("literal element: expected {}, found {}", w, g)
            })?;
            lowered.push(el);
        }
        Ok(Expr { kind: ExprKind::ContainerLit { class: key, is_array, elems: lowered }, line, col })
    }

    /// Resolve `recv.field` on a class type: existence + visibility.
    /// Returns (class name, field type).
    fn resolve_field(&self, rty: &Type, fname: &str, line: u32, col: u32) -> SResult<(String, Type)> {
        let class = match class_key(rty) {
            Some(k) => k,
            None => {
                return Err(JimError::new(
                    format!("type {} has no fields", rty.display()),
                    line,
                    col,
                ))
            }
        };
        let info = self.tables.classes.get(&class).ok_or_else(|| {
            JimError::new(format!("type {} has no fields", class), line, col)
        })?;
        let f = info.fields.get(fname).ok_or_else(|| {
            JimError::new(format!("class '{}' has no field '{}'", class, fname), line, col)
        })?;
        if !f.is_public && self.current_class.as_deref() != Some(class.as_str()) {
            return Err(JimError::new(
                format!("field '{}' of class '{}' is private", fname, class),
                line,
                col,
            ));
        }
        Ok((class, f.ty.clone()))
    }

    fn lower_condition(&mut self, cond: &Expr, what: &str) -> SResult<Expr> {
        let (cond, ty) = deopt(self.lower_expr(cond)?);
        if ty != Type::named("Bool") {
            return Err(JimError::new(
                format!("{} condition must be Bool, found {}", what, ty.display()),
                cond.line,
                cond.col,
            ));
        }
        Ok(cond)
    }

    /// Lower a value that must fit `want`: gives `None` its type from context
    /// and applies the optional coercions. `msg(want, got)` builds the error.
    fn lower_expecting<F>(&mut self, e: &Expr, want: &Type, msg: F) -> SResult<Expr>
    where
        F: FnOnce(&str, &str) -> String,
    {
        // The generic wrapper delegates to a non-generic core: the core calls
        // itself recursively with new closures, which would otherwise make
        // rustc instantiate this function infinitely.
        let mut msg_once = Some(msg);
        self.lower_expecting_dyn(e, want, &mut |w, g| {
            msg_once.take().map(|f| f(w, g)).unwrap_or_default()
        })
    }

    fn lower_expecting_dyn(
        &mut self,
        e: &Expr,
        want: &Type,
        msg: &mut dyn FnMut(&str, &str) -> String,
    ) -> SResult<Expr> {
        if matches!(e.kind, ExprKind::NoneLit) {
            return match opt_payload(want) {
                Some(payload) => Ok(Expr {
                    kind: ExprKind::OptNone { payload },
                    line: e.line,
                    col: e.col,
                }),
                None => Err(JimError::new(
                    format!(
                        "'None' only fits optional types (T?), but {} is expected here",
                        want.display()
                    ),
                    e.line,
                    e.col,
                )),
            };
        }
        // `[a, b, c]` takes its container type from context and builds through
        // the constructor + set/push protocol.
        if let ExprKind::ArrayLit(elems) = &e.kind {
            if let Type::Generic(base, payload) = want {
                if base == "Array" || base == "Vector" {
                    return self.lower_container_lit(base, payload, elems, e.line, e.col);
                }
            }
        }
        // `Array(10)` / `Vector()` — generic construction takes its type
        // argument from context, exactly like literals do.
        if let ExprKind::Call { name, args } = &e.kind {
            if let Type::Generic(base, _) = want {
                if name == base && matches!(base.as_str(), "Array" | "Vector") {
                    let key = class_key(want).expect("generic types always have a key");
                    let params = match self.tables.classes.get(&key) {
                        Some(info) => info.ctor_params.clone(),
                        None => {
                            return Err(JimError::new(
                                format!("{}<T> is not defined (is core.j loaded?)", base),
                                e.line,
                                e.col,
                            ))
                        }
                    };
                    if args.len() != params.len() {
                        return Err(JimError::new(
                            format!(
                                "constructor '{}' expects {} argument(s), got {}",
                                key,
                                params.len(),
                                args.len()
                            ),
                            e.line,
                            e.col,
                        ));
                    }
                    let mut lowered = Vec::with_capacity(args.len());
                    for (arg, want_ty) in args.iter().zip(&params) {
                        let key_c = key.clone();
                        let a = self.lower_expecting(arg, want_ty, |w, g| {
                            format!(
                                "argument type mismatch in constructor '{}': expected {}, found {}",
                                key_c, w, g
                            )
                        })?;
                        lowered.push(a);
                    }
                    return Ok(Expr {
                        kind: ExprKind::New { class: key, args: lowered },
                        line: e.line,
                        col: e.col,
                    });
                }
            }
        }
        // `@buf_alloc(n)` takes its element type from context (std-only).
        if let ExprKind::IntrinsicCall { name, args } = &e.kind {
            if name == "buf_alloc" {
                if !self.from_std && !self.tables.allow_intrinsics {
                    return Err(JimError::new(
                        "'@buf_alloc' â€” intrinsics are only allowed in the standard library (or with --allow-intrinsics)",
                        e.line,
                        e.col,
                    ));
                }
                let elem = match want {
                    Type::Generic(n, p) if n == "RawBuffer" => (**p).clone(),
                    other => {
                        return Err(JimError::new(
                            format!(
                                "'@buf_alloc' creates a RawBuffer<T>, but {} is expected here",
                                other.display()
                            ),
                            e.line,
                            e.col,
                        ))
                    }
                };
                if args.len() != 1 {
                    return Err(JimError::new(
                        "'@buf_alloc' expects exactly 1 argument (the capacity)",
                        e.line,
                        e.col,
                    ));
                }
                let size = self.lower_expecting(&args[0], &Type::named("Integer"), |w, g| {
                    format!("'@buf_alloc' capacity: expected {}, found {}", w, g)
                })?;
                return Ok(Expr {
                    kind: ExprKind::BufAlloc { elem, size: Box::new(size) },
                    line: e.line,
                    col: e.col,
                });
            }
        }
        // A generic call sees the expected type: still-unbound type parameters
        // infer from it (`var m: Float = max(v);` binds T = Float).
        if let ExprKind::Call { name, args } = &e.kind {
            if self.tables.fn_templates.contains_key(name) {
                let pair =
                    self.lower_generic_call(name, None, args, Some(want), e.line, e.col)?;
                let (line, col) = (pair.0.line, pair.0.col);
                return try_coerce(pair, want).map_err(|(_, got)| {
                    JimError::new(msg(&want.display(), &got.display()), line, col)
                });
            }
        }
        let pair = self.lower_expr(e)?;
        let (line, col) = (pair.0.line, pair.0.col);
        try_coerce(pair, want)
            .map_err(|(_, got)| JimError::new(msg(&want.display(), &got.display()), line, col))
    }

    fn lower_expr(&mut self, e: &Expr) -> SResult<(Expr, Type)> {
        let (line, col) = (e.line, e.col);
        match &e.kind {
            ExprKind::Int(_) => Ok((e.clone(), Type::named("Integer"))),
            ExprKind::Float(_) => Ok((e.clone(), Type::named("Float"))),
            ExprKind::Str(_) => Ok((e.clone(), Type::named("String"))),
            ExprKind::CharLit(_) => Ok((e.clone(), Type::named("Char"))),
            ExprKind::Bool(_) => Ok((e.clone(), Type::named("Bool"))),
            ExprKind::NoneLit => Err(JimError::new(
                "'None' can only be used where an optional type (T?) is expected, or compared with == / !=",
                line,
                col,
            )),
            ExprKind::Ident(name) => match self.env.lookup(name) {
                Some((ty, _)) => Ok((e.clone(), ty.clone())),
                None => {
                    // helpful hint: bare field names are not in scope (spec rule)
                    let is_field = self
                        .current_class
                        .as_ref()
                        .and_then(|c| self.tables.classes.get(c))
                        .map_or(false, |info| info.fields.contains_key(name));
                    let msg = if is_field {
                        format!(
                            "unknown variable '{}' â€” member access must be written 'this.{}'",
                            name, name
                        )
                    } else {
                        format!("unknown variable '{}'", name)
                    };
                    Err(JimError::new(msg, line, col))
                }
            },
            ExprKind::This => match &self.this_ty {
                Some(t) => Ok((e.clone(), t.clone())),
                None => Err(JimError::new(
                    "'this' is only valid inside a class method",
                    line,
                    col,
                )),
            },
            ExprKind::ArrayLit(_) => Err(JimError::new(
                "a container literal needs a declared type for context, e.g. 'var v: Vector<Integer> = [1, 2, 3];'",
                line,
                col,
            )),
            ExprKind::Call { name, args } => {
                // `Shape(1, 2)` â€” a call whose name is a class is instantiation
                if let Some(info) = self.tables.classes.get(name) {
                    if info.kind == ClassKind::CoreValue {
                        return Err(JimError::new(
                            format!("class '{}' has no constructor (it is a value class)", name),
                            line,
                            col,
                        ));
                    }
                    let params = info.ctor_params.clone();
                    if args.len() != params.len() {
                        return Err(JimError::new(
                            format!(
                                "constructor '{}' expects {} argument(s), got {}",
                                name,
                                params.len(),
                                args.len()
                            ),
                            line,
                            col,
                        ));
                    }
                    let mut lowered = Vec::with_capacity(args.len());
                    for (arg, want_ty) in args.iter().zip(&params) {
                        let arg = self.lower_expecting(arg, want_ty, |want, got| {
                            format!(
                                "argument type mismatch in constructor '{}': expected {}, found {}",
                                name, want, got
                            )
                        })?;
                        lowered.push(arg);
                    }
                    return Ok((
                        Expr { kind: ExprKind::New { class: name.clone(), args: lowered }, line, col },
                        Type::named(name),
                    ));
                }
                if matches!(name.as_str(), "Array" | "Vector") && !self.tables.funcs.contains_key(name) {
                    return Err(JimError::new(
                        format!(
                            "{}(...) is generic — construction takes its type from context, e.g. 'var a: {}<Integer> = {}(...);'",
                            name, name, name
                        ),
                        line,
                        col,
                    ));
                }
                // a call to a generic function with no expected type — infer
                // the type parameters from the arguments alone
                if self.tables.fn_templates.contains_key(name) {
                    return self.lower_generic_call(name, None, args, None, line, col);
                }
                let sig = self.tables.funcs.get(name).ok_or_else(|| {
                    JimError::new(format!("unknown function '{}'", name), line, col)
                })?;
                if args.len() != sig.params.len() {
                    return Err(JimError::new(
                        format!(
                            "function '{}' expects {} argument(s), got {}",
                            name,
                            sig.params.len(),
                            args.len()
                        ),
                        line,
                        col,
                    ));
                }
                let want: Vec<Type> = sig.params.clone();
                let ret = sig.ret.clone();
                let mut lowered = Vec::with_capacity(args.len());
                for (arg, want_ty) in args.iter().zip(&want) {
                    let arg = self.lower_expecting(arg, want_ty, |want, got| {
                        format!(
                            "argument type mismatch in call to '{}': expected {}, found {}",
                            name, want, got
                        )
                    })?;
                    lowered.push(arg);
                }
                Ok((
                    Expr { kind: ExprKind::Call { name: name.clone(), args: lowered }, line, col },
                    ret,
                ))
            }
            ExprKind::IntrinsicCall { name, args } => {
                if name == "buf_alloc" {
                    return Err(JimError::new(
                        "'@buf_alloc' needs a RawBuffer<T> context, e.g. 'var b: RawBuffer<Integer> = @buf_alloc(8);'",
                        line,
                        col,
                    ));
                }
                if !self.from_std && !self.tables.allow_intrinsics {
                    return Err(JimError::new(
                        format!(
                            "'@{}' â€” intrinsics are only allowed in the standard library (or with --allow-intrinsics)",
                            name
                        ),
                        line,
                        col,
                    ));
                }
                let (params, ret) = intrinsic_sig(name).ok_or_else(|| {
                    JimError::new(format!("unknown intrinsic '@{}'", name), line, col)
                })?;
                if args.len() != params.len() {
                    return Err(JimError::new(
                        format!("'@{}' expects {} argument(s), got {}", name, params.len(), args.len()),
                        line,
                        col,
                    ));
                }
                let mut lowered = Vec::with_capacity(args.len());
                for (arg, want) in args.iter().zip(&params) {
                    let (arg, got) = self.lower_expr(arg)?;
                    if &got != want {
                        return Err(JimError::new(
                            format!(
                                "argument type mismatch in '@{}': expected {}, found {}",
                                name,
                                want.display(),
                                got.display()
                            ),
                            arg.line,
                            arg.col,
                        ));
                    }
                    lowered.push(arg);
                }
                Ok((
                    Expr { kind: ExprKind::IntrinsicCall { name: name.clone(), args: lowered }, line, col },
                    ret,
                ))
            }
            ExprKind::MethodCall { recv, name, args } => {
                // calling a method on a T? unwraps it (runtime-checked)
                let recv = deopt(self.lower_expr(recv)?);
                // when the signature is known, thread parameter expectations
                // through (so `None` and literals type themselves correctly)
                let expected: Option<Vec<Type>> = class_key(&recv.1)
                    .and_then(|k| self.tables.classes.get(&k))
                    .and_then(|i| i.methods.get(name))
                    .map(|s| s.params.clone());
                let mut lowered_args = Vec::with_capacity(args.len());
                match expected {
                    Some(params) if params.len() == args.len() => {
                        for (arg, want) in args.iter().zip(&params) {
                            let a = self.lower_expecting(arg, want, |w, g| {
                                format!(
                                    "argument type mismatch in '{}': expected {}, found {}",
                                    name, w, g
                                )
                            })?;
                            lowered_args.push((a, want.clone()));
                        }
                    }
                    _ => {
                        for arg in args {
                            lowered_args.push(self.lower_expr(arg)?);
                        }
                    }
                }
                self.core_call(&recv.1.clone(), name, recv.0, lowered_args, line, col, Origin::Plain)
            }
            ExprKind::GenericCall { name, type_args, args } => {
                if self.tables.fn_templates.contains_key(name) {
                    return self.lower_generic_call(name, Some(type_args), args, None, line, col);
                }
                let msg = if self.tables.funcs.contains_key(name) {
                    format!(
                        "function '{}' is not generic — remove the explicit type arguments",
                        name
                    )
                } else if self.tables.classes.contains_key(name)
                    || self.tables.templates.contains_key(name)
                {
                    format!(
                        "constructors take their type from context ('var v: {0}<Integer> = {0}(...);') — explicit type arguments are for generic functions",
                        name
                    )
                } else if self.env.lookup(name).is_some() {
                    format!(
                        "'{}' is a variable, not a generic function — if you meant comparisons, parenthesize them: '(a < b), (c > d)'",
                        name
                    )
                } else {
                    format!("unknown function '{}'", name)
                };
                Err(JimError::new(msg, line, col))
            }
            ExprKind::CoreMethodCall { .. }
            | ExprKind::OptWrap { .. }
            | ExprKind::OptUnwrap { .. }
            | ExprKind::OptNone { .. }
            | ExprKind::OptHas { .. }
            | ExprKind::New { .. }
            | ExprKind::ContainerLit { .. }
            | ExprKind::BufAlloc { .. } => {
                unreachable!("parser never produces lowered nodes")
            }
            ExprKind::FieldAccess { recv, name } => {
                let (recv, rty) = deopt(self.lower_expr(recv)?);
                let (_, fty) = self.resolve_field(&rty, name, line, col)?;
                Ok((
                    Expr {
                        kind: ExprKind::FieldAccess { recv: Box::new(recv), name: name.clone() },
                        line,
                        col,
                    },
                    fty,
                ))
            }
            ExprKind::Index { recv, index } => {
                let recv = deopt(self.lower_expr(recv)?);
                let idx = self.lower_expecting(index, &Type::named("Integer"), |w, g| {
                    format!("index must be {}, found {}", w, g)
                })?;
                self.core_call(
                    &recv.1.clone(),
                    "get",
                    recv.0,
                    vec![(idx, Type::named("Integer"))],
                    line,
                    col,
                    Origin::Operator("indexing '[...]'".to_string()),
                )
            }
            ExprKind::Binary { op, lhs, rhs } => self.lower_binary(*op, lhs, rhs, line, col),
            ExprKind::Unary { op, operand } => match op {
                UnOp::Not => {
                    let (operand, ty) = deopt(self.lower_expr(operand)?);
                    if ty != Type::named("Bool") {
                        return Err(JimError::new(
                            format!("'not' needs a Bool operand, found {}", ty.display()),
                            operand.line,
                            operand.col,
                        ));
                    }
                    Ok((
                        Expr {
                            kind: ExprKind::Unary { op: UnOp::Not, operand: Box::new(operand) },
                            line,
                            col,
                        },
                        Type::named("Bool"),
                    ))
                }
                UnOp::Neg => {
                    // fold literals so `-5` works (and stays a compile-time constant)
                    if let ExprKind::Int(v) = &operand.kind {
                        return Ok((Expr { kind: ExprKind::Int(-v), line, col }, Type::named("Integer")));
                    }
                    if let ExprKind::Float(v) = &operand.kind {
                        return Ok((Expr { kind: ExprKind::Float(-v), line, col }, Type::named("Float")));
                    }
                    let (operand, ty) = deopt(self.lower_expr(operand)?);
                    self.core_call(
                        &ty,
                        "negate",
                        operand,
                        vec![],
                        line,
                        col,
                        Origin::Operator("unary '-'".to_string()),
                    )
                }
                UnOp::AddrOf => {
                    let name = match &operand.kind {
                        ExprKind::Ident(n) => n.clone(),
                        _ => {
                            return Err(JimError::new(
                                "'&' needs a variable (only locals and parameters have addresses)",
                                line,
                                col,
                            ))
                        }
                    };
                    let (ty, is_const) = self
                        .env
                        .lookup(&name)
                        .ok_or_else(|| {
                            JimError::new(format!("unknown variable '{}'", name), operand.line, operand.col)
                        })?
                        .clone();
                    if is_const {
                        return Err(JimError::new(
                            format!(
                                "cannot take the address of constant '{}' (writing through the pointer would mutate it)",
                                name
                            ),
                            line,
                            col,
                        ));
                    }
                    Ok((
                        Expr {
                            kind: ExprKind::Unary {
                                op: UnOp::AddrOf,
                                operand: Box::new((**operand).clone()),
                            },
                            line,
                            col,
                        },
                        Type::Pointer(Box::new(ty)),
                    ))
                }
                UnOp::Deref => {
                    let (operand, ty) = deopt(self.lower_expr(operand)?);
                    match ty {
                        Type::Pointer(inner) => Ok((
                            Expr {
                                kind: ExprKind::Unary { op: UnOp::Deref, operand: Box::new(operand) },
                                line,
                                col,
                            },
                            *inner,
                        )),
                        other => Err(JimError::new(
                            format!("'*' needs a pointer, found {}", other.display()),
                            operand.line,
                            operand.col,
                        )),
                    }
                }
            },
        }
    }

    fn lower_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, line: u32, col: u32) -> SResult<(Expr, Type)> {
        // and/or stay native for short-circuit evaluation
        if matches!(op, BinOp::And | BinOp::Or) {
            let word = if matches!(op, BinOp::And) { "and" } else { "or" };
            let (l, lt) = deopt(self.lower_expr(lhs)?);
            if lt != Type::named("Bool") {
                return Err(JimError::new(
                    format!("'{}' needs Bool operands, found {}", word, lt.display()),
                    l.line,
                    l.col,
                ));
            }
            let (r, rt) = deopt(self.lower_expr(rhs)?);
            if rt != Type::named("Bool") {
                return Err(JimError::new(
                    format!("'{}' needs Bool operands, found {}", word, rt.display()),
                    r.line,
                    r.col,
                ));
            }
            return Ok((
                Expr { kind: ExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, line, col },
                Type::named("Bool"),
            ));
        }

        // `x == None` / `x != None` are native presence tests, not method calls.
        if matches!(op, BinOp::Eq | BinOp::NotEq) {
            let l_none = matches!(lhs.kind, ExprKind::NoneLit);
            let r_none = matches!(rhs.kind, ExprKind::NoneLit);
            if l_none && r_none {
                return Err(JimError::new("cannot compare None with None", line, col));
            }
            if l_none || r_none {
                let value = if l_none { rhs } else { lhs };
                let (v, vt) = self.lower_expr(value)?;
                let payload = match opt_payload(&vt) {
                    Some(p) => p,
                    None => {
                        return Err(JimError::new(
                            format!(
                                "only optional values (T?) can be compared with None â€” this is {}",
                                vt.display()
                            ),
                            v.line,
                            v.col,
                        ))
                    }
                };
                let has = Expr { kind: ExprKind::OptHas { payload, expr: Box::new(v) }, line, col };
                return Ok(match op {
                    BinOp::NotEq => (has, Type::named("Bool")),
                    _ => (
                        Expr {
                            kind: ExprKind::Unary { op: UnOp::Not, operand: Box::new(has) },
                            line,
                            col,
                        },
                        Type::named("Bool"),
                    ),
                });
            }
        }

        // operators act on values: optionals unwrap (runtime-checked) first
        let l = deopt(self.lower_expr(lhs)?);
        let r = deopt(self.lower_expr(rhs)?);
        let (l, r) = self.widen(l, r)?;

        let arith = |sym: &str, method: &'static str| (sym.to_string(), method);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::IntDiv | BinOp::Mod => {
                let (sym, method) = match op {
                    BinOp::Add => arith("+", "plus"),
                    BinOp::Sub => arith("-", "minus"),
                    BinOp::Mul => arith("*", "times"),
                    BinOp::Div => arith("/", "divide"),
                    BinOp::IntDiv => arith("div", "int_divide"),
                    BinOp::Mod => arith("%", "mod"),
                    _ => unreachable!(),
                };
                let origin = Origin::Operator(format!("operator '{}'", sym));
                self.core_call(&l.1.clone(), method, l.0, vec![r], line, col, origin)
            }
            BinOp::Eq => self.comparison_call("==", "equals", l, r, false, line, col),
            BinOp::NotEq => self.comparison_call("!=", "equals", l, r, true, line, col),
            BinOp::Lt => self.comparison_call("<", "less_than", l, r, false, line, col),
            BinOp::Gt => self.comparison_call(">", "less_than", r, l, false, line, col),
            BinOp::LtEq => self.comparison_call("<=", "less_than", r, l, true, line, col),
            BinOp::GtEq => self.comparison_call(">=", "less_than", l, r, true, line, col),
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    /// `recv.method(arg)`, optionally wrapped in a native `not`
    /// (that's how !=, <=, >= derive from equals/less_than).
    #[allow(clippy::too_many_arguments)]
    fn comparison_call(
        &mut self,
        sym: &str,
        method: &str,
        recv: (Expr, Type),
        arg: (Expr, Type),
        negate: bool,
        line: u32,
        col: u32,
    ) -> SResult<(Expr, Type)> {
        let origin = Origin::Operator(format!("operator '{}'", sym));
        let class = match &recv.1 {
            Type::Named(n) => n.clone(),
            other => {
                return Err(JimError::new(
                    format!("operator '{}' cannot be applied to {}", sym, other.display()),
                    line,
                    col,
                ))
            }
        };
        let (call, ret) = self.core_call(&recv.1.clone(), method, recv.0, vec![arg], line, col, origin)?;
        if ret != Type::named("Bool") {
            return Err(JimError::new(
                format!(
                    "'{}.{}' must return Bool to back operator '{}' (it returns {})",
                    class,
                    method,
                    sym,
                    ret.display()
                ),
                line,
                col,
            ));
        }
        if negate {
            Ok((
                Expr { kind: ExprKind::Unary { op: UnOp::Not, operand: Box::new(call) }, line, col },
                Type::named("Bool"),
            ))
        } else {
            Ok((call, Type::named("Bool")))
        }
    }

    /// The single implicit coercion in jim: Integer widens to Float when mixed.
    fn widen(&mut self, l: (Expr, Type), r: (Expr, Type)) -> SResult<((Expr, Type), (Expr, Type))> {
        let int = Type::named("Integer");
        let float = Type::named("Float");
        if l.1 == int && r.1 == float {
            let l = self.to_float(l)?;
            Ok((l, r))
        } else if l.1 == float && r.1 == int {
            let r = self.to_float(r)?;
            Ok((l, r))
        } else {
            Ok((l, r))
        }
    }

    fn to_float(&mut self, e: (Expr, Type)) -> SResult<(Expr, Type)> {
        let (line, col) = (e.0.line, e.0.col);
        self.core_call(
            &e.1.clone(),
            "to_float",
            e.0,
            vec![],
            line,
            col,
            Origin::Operator("mixed Integer/Float arithmetic".to_string()),
        )
    }

    /// Resolve and type-check a call to `Class.method`, producing a
    /// CoreMethodCall. All operator desugaring funnels through here.
    #[allow(clippy::too_many_arguments)]
    fn core_call(
        &mut self,
        recv_ty: &Type,
        method: &str,
        recv: Expr,
        args: Vec<(Expr, Type)>,
        line: u32,
        col: u32,
        origin: Origin,
    ) -> SResult<(Expr, Type)> {
        let class = match class_key(recv_ty) {
            Some(k) => k,
            None => {
                let msg = match &origin {
                    Origin::Operator(label) => {
                        format!("{} cannot be applied to {}", label, recv_ty.display())
                    }
                    Origin::Plain => format!("type {} has no methods", recv_ty.display()),
                };
                return Err(JimError::new(msg, line, col));
            }
        };
        let info = match self.tables.classes.get(&class) {
            Some(i) => i,
            None => {
                let msg = match &origin {
                    Origin::Operator(label) => format!(
                        "{} needs {}.{}(), but class '{}' is not defined (is core.j loaded?)",
                        label, class, method, class
                    ),
                    Origin::Plain => format!(
                        "type {} has no method '{}' (class '{}' is not defined â€” is core.j loaded?)",
                        class, method, class
                    ),
                };
                return Err(JimError::new(msg, line, col));
            }
        };
        let sig = match info.methods.get(method) {
            Some(s) => s,
            None => {
                let msg = match &origin {
                    Origin::Operator(label) => format!(
                        "{} needs {}.{}(), but class '{}' has no method '{}'",
                        label, class, method, class, method
                    ),
                    Origin::Plain => format!("class '{}' has no method '{}'", class, method),
                };
                return Err(JimError::new(msg, line, col));
            }
        };
        if !sig.is_public && self.current_class.as_deref() != Some(class.as_str()) {
            return Err(JimError::new(
                format!("method '{}' of class '{}' is private", method, class),
                line,
                col,
            ));
        }
        if args.len() != sig.params.len() {
            let msg = match &origin {
                Origin::Operator(label) => format!(
                    "{} calls {}.{} with {} argument(s), but it is declared with {} parameter(s)",
                    label,
                    class,
                    method,
                    args.len(),
                    sig.params.len()
                ),
                Origin::Plain => format!(
                    "method '{}.{}' expects {} argument(s), got {}",
                    class,
                    method,
                    sig.params.len(),
                    args.len()
                ),
            };
            return Err(JimError::new(msg, line, col));
        }
        let mut lowered_args = Vec::with_capacity(args.len());
        for ((arg, got), want) in args.into_iter().zip(&sig.params) {
            let (aline, acol) = (arg.line, arg.col);
            match try_coerce((arg, got), want) {
                Ok(e) => lowered_args.push(e),
                Err((_, got)) => {
                    let msg = match &origin {
                        Origin::Operator(label) => format!(
                            "{}: {}.{} expects {}, found {}",
                            label,
                            class,
                            method,
                            want.display(),
                            got.display()
                        ),
                        Origin::Plain => format!(
                            "argument type mismatch in '{}.{}': expected {}, found {}",
                            class,
                            method,
                            want.display(),
                            got.display()
                        ),
                    };
                    return Err(JimError::new(msg, aline, acol));
                }
            }
        }
        let ret = sig.ret.clone();
        Ok((
            Expr {
                kind: ExprKind::CoreMethodCall {
                    class,
                    name: method.to_string(),
                    recv: Box::new(recv),
                    args: lowered_args,
                },
                line,
                col,
            },
            ret,
        ))
    }

    /// A call to a generic function: bind the type parameters (explicit
    /// arguments > argument types > expected type, in that order), queue the
    /// instantiation, and emit a plain Call to the instance key.
    fn lower_generic_call(
        &mut self,
        name: &str,
        explicit: Option<&[Type]>,
        args: &[Expr],
        want: Option<&Type>,
        line: u32,
        col: u32,
    ) -> SResult<(Expr, Type)> {
        let tmpl = &self.tables.fn_templates[name];
        let tparams = tmpl.type_params.clone();
        let param_tys: Vec<Type> = tmpl.params.iter().map(|p| p.ty.clone()).collect();
        let ret_ty = tmpl.ret.clone();

        if args.len() != param_tys.len() {
            return Err(JimError::new(
                format!(
                    "function '{}' expects {} argument(s), got {}",
                    name,
                    param_tys.len(),
                    args.len()
                ),
                line,
                col,
            ));
        }

        let mut bindings: HashMap<String, Type> = HashMap::new();

        // 1. explicit type arguments bind everything up front
        if let Some(type_args) = explicit {
            if type_args.len() != tparams.len() {
                return Err(JimError::new(
                    format!(
                        "function '{}' takes {} type argument(s) (<{}>), got {}",
                        name,
                        tparams.len(),
                        tparams.join(", "),
                        type_args.len()
                    ),
                    line,
                    col,
                ));
            }
            for (p, a) in tparams.iter().zip(type_args) {
                type_available(self.tables, a, false).map_err(|m| {
                    JimError::new(format!("in type argument for '{}': {}", p, m), line, col)
                })?;
                bindings.insert(p.clone(), a.clone());
            }
        }

        // 2. arguments whose declared type still mentions an unbound parameter
        //    are lowered plainly and drive unification
        let mut pre: Vec<Option<(Expr, Type)>> = Vec::with_capacity(args.len());
        for (arg, pty) in args.iter().zip(&param_tys) {
            if mentions_unbound(pty, &tparams, &bindings) {
                let pair = self.lower_expr(arg)?;
                if !unify(pty, &pair.1, &tparams, &mut bindings) {
                    return Err(JimError::new(
                        format!(
                            "cannot infer the type parameters of '{}': the parameter is declared {} but the argument is {}",
                            name,
                            pty.display(),
                            pair.1.display()
                        ),
                        pair.0.line,
                        pair.0.col,
                    ));
                }
                pre.push(Some(pair));
            } else {
                pre.push(None);
            }
        }

        // 3. anything still unbound infers from the expected type
        if let Some(w) = want {
            unify_soft(&ret_ty, w, &tparams, &mut bindings);
        }
        let missing: Vec<String> =
            tparams.iter().filter(|p| !bindings.contains_key(*p)).cloned().collect();
        if !missing.is_empty() {
            return Err(JimError::new(
                format!(
                    "cannot infer type parameter{} {} of '{}' — annotate the target ('var x: Type = {}(...);') or pass explicit type arguments ('{}<...>(...)') ",
                    if missing.len() == 1 { "" } else { "s" },
                    missing.join(", "),
                    name,
                    name,
                    name
                )
                .trim_end()
                .to_string(),
                line,
                col,
            ));
        }

        let ordered: Vec<(String, Type)> =
            tparams.iter().map(|p| (p.clone(), bindings[p].clone())).collect();
        let key = format!(
            "{}<{}>",
            name,
            ordered.iter().map(|(_, t)| t.display()).collect::<Vec<_>>().join(",")
        );

        // queue the instantiation (check()'s fixpoint loop drains this)
        {
            let mut known = self.tables.known_fn_insts.borrow_mut();
            if known.insert(key.clone()) {
                self.tables.pending_fns.borrow_mut().push(FnInst {
                    template: name.to_string(),
                    bindings: ordered.clone(),
                    key: key.clone(),
                });
            }
        }

        // 4. final argument lowering against the substituted parameter types
        let mut final_args = Vec::with_capacity(args.len());
        for ((arg, pty), pre_pair) in args.iter().zip(&param_tys).zip(pre) {
            let want_ty = subst_type(pty, &ordered);
            match pre_pair {
                Some(pair) => {
                    let (aline, acol) = (pair.0.line, pair.0.col);
                    let e = try_coerce(pair, &want_ty).map_err(|(_, got)| {
                        JimError::new(
                            format!(
                                "argument type mismatch in call to '{}': expected {}, found {}",
                                name,
                                want_ty.display(),
                                got.display()
                            ),
                            aline,
                            acol,
                        )
                    })?;
                    final_args.push(e);
                }
                None => {
                    let e = self.lower_expecting(arg, &want_ty, |w, g| {
                        format!(
                            "argument type mismatch in call to '{}': expected {}, found {}",
                            name, w, g
                        )
                    })?;
                    final_args.push(e);
                }
            }
        }

        let result_ty = subst_type(&ret_ty, &ordered);
        Ok((
            Expr { kind: ExprKind::Call { name: key, args: final_args }, line, col },
            result_ty,
        ))
    }
}

/// The v0 intrinsic table â€” see docs/DESIGN.md Â§6. Grows on demand.
fn intrinsic_sig(name: &str) -> Option<(Vec<Type>, Type)> {
    let int = || Type::named("Integer");
    let float = || Type::named("Float");
    let boolean = || Type::named("Bool");
    let ch = || Type::named("Char");
    let string = || Type::named("String");
    let none = || Type::named("None");
    let opt = |t: Type| Type::Optional(Box::new(t));
    Some(match name {
        "i64_add" | "i64_sub" | "i64_mul" | "i64_divtrunc" | "i64_mod" => {
            (vec![int(), int()], int())
        }
        "i64_neg" => (vec![int()], int()),
        "i64_eq" | "i64_lt" => (vec![int(), int()], boolean()),
        "i64_to_f64" => (vec![int()], float()),
        "i64_to_string" => (vec![int()], string()),
        "i64_to_char" => (vec![int()], ch()),
        "f64_add" | "f64_sub" | "f64_mul" | "f64_div" => (vec![float(), float()], float()),
        "f64_neg" => (vec![float()], float()),
        "f64_eq" | "f64_lt" => (vec![float(), float()], boolean()),
        "f64_to_i64" => (vec![float()], int()),
        "f64_to_string" => (vec![float()], string()),
        "bool_eq" => (vec![boolean(), boolean()], boolean()),
        "char_eq" | "char_lt" => (vec![ch(), ch()], boolean()),
        "char_to_i64" => (vec![ch()], int()),
        "char_to_string" => (vec![ch()], string()),
        "exc_msg" => (vec![Type::named("Exception")], string()),
        "str_len" => (vec![string()], int()),
        "str_byte" => (vec![string(), int()], ch()),
        "str_concat" => (vec![string(), string()], string()),
        "str_eq" | "str_lt" => (vec![string(), string()], boolean()),
        // zero-copy view (unchecked) / one-copy builder finish
        "str_slice" => (vec![string(), int(), int()], string()),
        "str_from_buf" => {
            (vec![Type::Generic("RawBuffer".to_string(), Box::new(ch())), int()], string())
        }
        "str_to_i64" => (vec![string()], opt(int())),
        "str_to_f64" => (vec![string()], opt(float())),
        // Float math (libm; IEEE-permissive — domain errors yield nan/inf)
        "f64_sqrt" | "f64_cbrt" | "f64_exp" | "f64_log" | "f64_log2" | "f64_log10"
        | "f64_sin" | "f64_cos" | "f64_tan" | "f64_asin" | "f64_acos" | "f64_atan" => {
            (vec![float()], float())
        }
        "f64_hypot" | "f64_atan2" | "f64_fmod" | "f64_pow" => (vec![float(), float()], float()),
        "f64_is_nan" | "f64_is_inf" | "f64_is_finite" => (vec![float()], boolean()),
        // Integer bit operations (shifts panic outside 0-63)
        "i64_and" | "i64_or" | "i64_xor" | "i64_shl" | "i64_shr" => (vec![int(), int()], int()),
        "i64_not" => (vec![int()], int()),
        "print_string" => (vec![string()], none()),
        "print_err" => (vec![string()], none()),
        "read_line" => (vec![], opt(string())),
        "read_file" => (vec![string()], opt(string())),
        "write_file" | "append_file" => (vec![string(), string()], opt(int())),
        "file_exists" => (vec![string()], boolean()),
        "panic" => (vec![string()], none()),
        _ => return None,
    })
}
