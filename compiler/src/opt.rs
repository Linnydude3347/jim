//! AST-level optimizations, run between sema and codegen.
//!
//! Two passes over the lowered program, applied in one traversal:
//!
//! 1. Forwarder inlining. A "trivial forwarder" is a function or method whose
//!    entire body hands its parameters, in declaration order, to a single
//!    @intrinsic - the standard library's dominant shape (`Integer.plus` is
//!    `return @i64_add(this, other);`). Every call site is rewritten to the
//!    intrinsic itself, so the one-line wrapper functions fall out of the
//!    generated C via dead-code elimination and literal arguments become
//!    visible to folding.
//!
//! 2. Intrinsic constant folding. Intrinsics are the compiler's own contract
//!    (docs/DESIGN.md section 6), so their semantics can be evaluated at
//!    compile time without assuming anything about std-library jim code.
//!    Folding is best-effort and only fires when it provably matches the
//!    runtime: an operation that could panic (overflow, division by zero,
//!    out-of-range conversion) is left in place so it still panics at
//!    runtime, and floats fold only to finite values (inf/nan have no C
//!    literal spelling). Transcendental float math never folds - libm
//!    results are not bit-exact across platforms.

use crate::ast::*;
use std::collections::HashMap;

struct Forwarders {
    /// function name -> intrinsic name
    fns: HashMap<String, String>,
    /// "Class#method" -> intrinsic name
    methods: HashMap<String, String>,
}

pub fn optimize(program: &mut Program) {
    let fw = Forwarders {
        fns: program
            .functions
            .iter()
            .filter(|f| f.name != "main")
            .filter_map(|f| {
                forwarder(&f.params, &f.ret, &f.body, false).map(|i| (f.name.clone(), i))
            })
            .collect(),
        methods: program
            .classes
            .iter()
            .filter(|c| !c.name.starts_with("RawBuffer")) // built-in, emitted specially
            .flat_map(|c| {
                c.methods.iter().filter_map(move |m| {
                    forwarder(&m.params, &m.ret, &m.body, true)
                        .map(|i| (format!("{}#{}", c.name, m.name), i))
                })
            })
            .collect(),
    };

    for f in &mut program.functions {
        rewrite_block(&mut f.body, &fw);
    }
    for c in &mut program.classes {
        for fld in &mut c.fields {
            rewrite_expr(&mut fld.default, &fw);
        }
        if let Some(ct) = &mut c.ctor {
            rewrite_block(&mut ct.body, &fw);
        }
        for m in &mut c.methods {
            rewrite_block(&mut m.body, &fw);
        }
    }
}

/// If this body is a pure forwarder to one intrinsic, return the intrinsic's
/// name. The arguments must be exactly `this` (for methods) followed by the
/// parameters in declaration order, so substituting the call site's actual
/// arguments preserves evaluation order and evaluates each exactly once.
fn forwarder(params: &[Param], ret: &Type, body: &Block, is_method: bool) -> Option<String> {
    if body.stmts.len() != 1 {
        return None;
    }
    let call = match &body.stmts[0].kind {
        StmtKind::Return(Some(e)) => e,
        // A `-> None` body may be a bare intrinsic statement. Other return
        // types fall through to an implicit return, which is not a forward.
        StmtKind::ExprStmt(e) if *ret == Type::named("None") => e,
        _ => return None,
    };
    let (name, args) = match &call.kind {
        ExprKind::IntrinsicCall { name, args } => (name, args),
        _ => return None,
    };
    // @panic bakes the emitting function's file/line into the call; inlining
    // would silently relocate panic reports, so panic wrappers stay wrappers.
    if name == "panic" {
        return None;
    }
    let shift = usize::from(is_method);
    if args.len() != params.len() + shift {
        return None;
    }
    if is_method && !matches!(args[0].kind, ExprKind::This) {
        return None;
    }
    for (a, p) in args[shift..].iter().zip(params) {
        match &a.kind {
            ExprKind::Ident(n) if *n == p.name => {}
            _ => return None,
        }
    }
    Some(name.clone())
}

fn rewrite_block(b: &mut Block, fw: &Forwarders) {
    for s in &mut b.stmts {
        rewrite_stmt(s, fw);
    }
}

fn rewrite_stmt(s: &mut Stmt, fw: &Forwarders) {
    match &mut s.kind {
        StmtKind::VarDecl { init, .. } => rewrite_expr(init, fw),
        StmtKind::Assign { target, value, .. } => {
            rewrite_expr(target, fw);
            rewrite_expr(value, fw);
        }
        StmtKind::IncDec { target, .. } => rewrite_expr(target, fw),
        StmtKind::ExprStmt(e) => rewrite_expr(e, fw),
        StmtKind::Return(Some(e)) => rewrite_expr(e, fw),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        StmtKind::If { arms, else_block } => {
            for (cond, body) in arms {
                rewrite_expr(cond, fw);
                rewrite_block(body, fw);
            }
            if let Some(b) = else_block {
                rewrite_block(b, fw);
            }
        }
        StmtKind::While { cond, body } => {
            rewrite_expr(cond, fw);
            rewrite_block(body, fw);
        }
        StmtKind::ForC { init, cond, step, body, .. } => {
            rewrite_expr(init, fw);
            rewrite_expr(cond, fw);
            rewrite_stmt(step, fw);
            rewrite_block(body, fw);
        }
        StmtKind::ForIn { iterable, body, .. } => {
            rewrite_expr(iterable, fw);
            rewrite_block(body, fw);
        }
        StmtKind::Scope(b) => rewrite_block(b, fw),
        StmtKind::TryCatch { body, catch_body, .. } => {
            rewrite_block(body, fw);
            rewrite_block(catch_body, fw);
        }
    }
}

/// Post-order: children are rewritten and folded first, so folded literals
/// flow upward and chains like `("n = " + 3.to_string())` collapse fully.
fn rewrite_expr(e: &mut Expr, fw: &Forwarders) {
    match &mut e.kind {
        ExprKind::Call { args, .. }
        | ExprKind::GenericCall { args, .. }
        | ExprKind::IntrinsicCall { args, .. }
        | ExprKind::New { args, .. }
        | ExprKind::ContainerLit { elems: args, .. }
        | ExprKind::ArrayLit(args) => {
            for a in args {
                rewrite_expr(a, fw);
            }
        }
        ExprKind::MethodCall { recv, args, .. }
        | ExprKind::CoreMethodCall { recv, args, .. } => {
            rewrite_expr(recv, fw);
            for a in args {
                rewrite_expr(a, fw);
            }
        }
        ExprKind::BufAlloc { size, .. } => rewrite_expr(size, fw),
        ExprKind::OptWrap { expr, .. }
        | ExprKind::OptUnwrap { expr, .. }
        | ExprKind::OptHas { expr, .. } => rewrite_expr(expr, fw),
        ExprKind::FieldAccess { recv, .. } => rewrite_expr(recv, fw),
        ExprKind::Index { recv, index } => {
            rewrite_expr(recv, fw);
            rewrite_expr(index, fw);
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            rewrite_expr(lhs, fw);
            rewrite_expr(rhs, fw);
        }
        ExprKind::Unary { operand, .. } => rewrite_expr(operand, fw),
        _ => {}
    }

    // Inline: a call to a forwarder becomes the intrinsic itself. The node
    // keeps the call site's line/col, so debug line marks stay accurate.
    let inlined = match &mut e.kind {
        ExprKind::Call { name, args } => fw.fns.get(name.as_str()).map(|intr| {
            ExprKind::IntrinsicCall { name: intr.clone(), args: std::mem::take(args) }
        }),
        ExprKind::CoreMethodCall { class, name, recv, args } => {
            fw.methods.get(&format!("{}#{}", class, name)).map(|intr| {
                let recv = std::mem::replace(
                    recv.as_mut(),
                    Expr { kind: ExprKind::NoneLit, line: 0, col: 0 },
                );
                let mut all = Vec::with_capacity(args.len() + 1);
                all.push(recv);
                all.append(args);
                ExprKind::IntrinsicCall { name: intr.clone(), args: all }
            })
        }
        _ => None,
    };
    if let Some(kind) = inlined {
        e.kind = kind;
    }

    // Fold: literal arguments to an intrinsic the compiler can evaluate.
    if let ExprKind::IntrinsicCall { name, args } = &e.kind {
        if let Some(kind) = try_fold(name, args) {
            e.kind = kind;
        }
    }
}

/// Evaluate one intrinsic over literal arguments, mirroring jim_runtime.c
/// exactly. `None` means "leave the call in place" - either an argument is
/// not a literal, or the runtime behavior (a panic, an inf/nan, an exotic
/// value) cannot be represented as a literal.
fn try_fold(name: &str, args: &[Expr]) -> Option<ExprKind> {
    use ExprKind as E;
    let int_at = |n: usize| match &args[n].kind {
        E::Int(v) => Some(*v),
        _ => None,
    };
    let flt_at = |n: usize| match &args[n].kind {
        E::Float(v) => Some(*v),
        _ => None,
    };
    let bool_at = |n: usize| match &args[n].kind {
        E::Bool(v) => Some(*v),
        _ => None,
    };
    let char_at = |n: usize| match &args[n].kind {
        E::CharLit(v) => Some(*v),
        _ => None,
    };
    let str_at = |n: usize| match &args[n].kind {
        E::Str(v) => Some(v.as_str()),
        _ => None,
    };
    // `INT64_C(-9223372036854775808)` overflows C's literal grammar (the
    // minus applies to an out-of-range positive constant); leave i64::MIN
    // results to the runtime.
    let int = |v: i64| if v == i64::MIN { None } else { Some(E::Int(v)) };
    // Only finite floats have a C literal spelling.
    let flt = |v: f64| if v.is_finite() { Some(E::Float(v)) } else { None };

    let kind = match name {
        // Checked Integer arithmetic folds only when it cannot panic, so a
        // literal overflow / division by zero still panics at runtime.
        "i64_add" => int(int_at(0)?.checked_add(int_at(1)?)?)?,
        "i64_sub" => int(int_at(0)?.checked_sub(int_at(1)?)?)?,
        "i64_mul" => int(int_at(0)?.checked_mul(int_at(1)?)?)?,
        "i64_divtrunc" => int(int_at(0)?.checked_div(int_at(1)?)?)?,
        "i64_mod" => {
            let (a, b) = (int_at(0)?, int_at(1)?);
            if b == 0 {
                return None; // runtime panics
            }
            if a == i64::MIN && b == -1 {
                E::Int(0) // the runtime defines this case as 0
            } else {
                int(a % b)?
            }
        }
        "i64_neg" => int(int_at(0)?.checked_neg()?)?,
        "i64_eq" => E::Bool(int_at(0)? == int_at(1)?),
        "i64_lt" => E::Bool(int_at(0)? < int_at(1)?),
        "i64_to_f64" => E::Float(int_at(0)? as f64),
        "i64_to_string" => E::Str(int_at(0)?.to_string()), // matches %lld

        // Float arithmetic is exact IEEE, deterministic at compile time.
        "f64_add" => flt(flt_at(0)? + flt_at(1)?)?,
        "f64_sub" => flt(flt_at(0)? - flt_at(1)?)?,
        "f64_mul" => flt(flt_at(0)? * flt_at(1)?)?,
        "f64_div" => flt(flt_at(0)? / flt_at(1)?)?,
        "f64_neg" => flt(-flt_at(0)?)?,
        "f64_eq" => E::Bool(flt_at(0)? == flt_at(1)?),
        "f64_lt" => E::Bool(flt_at(0)? < flt_at(1)?),
        "f64_to_i64" => {
            let v = flt_at(0)?;
            // same bounds as the runtime; out of range panics there
            if !(v >= -9223372036854775808.0 && v < 9223372036854775808.0) {
                return None;
            }
            int(v as i64)?
        }

        "bool_eq" => E::Bool(bool_at(0)? == bool_at(1)?),
        "char_eq" => E::Bool(char_at(0)? == char_at(1)?),
        "char_lt" => E::Bool(char_at(0)? < char_at(1)?),
        "char_to_i64" => E::Int(char_at(0)? as i64),
        "i64_to_char" => {
            let v = int_at(0)?;
            if !(0..=255).contains(&v) {
                return None; // runtime panics
            }
            E::CharLit(v as u8)
        }
        // Str holds UTF-8; a Char above 0x7f is a lone byte with no literal.
        "char_to_string" => {
            let c = char_at(0)?;
            if c > 0x7f {
                return None;
            }
            E::Str((c as char).to_string())
        }

        "str_len" => E::Int(str_at(0)?.len() as i64),
        "str_concat" => E::Str(format!("{}{}", str_at(0)?, str_at(1)?)),
        "str_eq" => E::Bool(str_at(0)? == str_at(1)?),
        "str_lt" => E::Bool(str_at(0)? < str_at(1)?), // both byte-lexicographic

        "i64_and" => int(int_at(0)? & int_at(1)?)?,
        "i64_or" => int(int_at(0)? | int_at(1)?)?,
        "i64_xor" => int(int_at(0)? ^ int_at(1)?)?,
        "i64_not" => int(!int_at(0)?)?,
        "i64_shl" => {
            let (a, b) = (int_at(0)?, int_at(1)?);
            if !(0..=63).contains(&b) {
                return None; // runtime panics
            }
            int(((a as u64) << b as u64) as i64)?
        }
        "i64_shr" => {
            let (a, b) = (int_at(0)?, int_at(1)?);
            if !(0..=63).contains(&b) {
                return None; // runtime panics
            }
            int(a >> b)? // Rust >> on i64 is arithmetic, like the runtime
        }

        _ => return None,
    };
    Some(kind)
}
