#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(String),
    Generic(String, Box<Type>),
    Pointer(Box<Type>),
    Optional(Box<Type>),
}

impl Type {
    pub fn named(n: &str) -> Type {
        Type::Named(n.to_string())
    }

    pub fn display(&self) -> String {
        match self {
            Type::Named(n) => n.clone(),
            Type::Generic(n, inner) => format!("{}<{}>", n, inner.display()),
            Type::Pointer(inner) => format!("*{}", inner.display()),
            Type::Optional(inner) => format!("{}?", inner.display()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImportKind {
    /// `#import <name>` - resolved against the std root
    Std(String),
    /// `#import "path.j"` - resolved relative to the importing file
    Local(String),
}

#[derive(Debug, Clone)]
pub struct Import {
    pub kind: ImportKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    /// `function max<C, T>(...)` - generic type parameters. Empty for normal
    /// functions. Generic functions are templates: monomorphized per call.
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Block,
    /// Index into the driver's source-file table (for error rendering).
    pub file_idx: usize,
    /// True when this function came from a file under the std root -
    /// the only place `@intrinsic` calls are allowed.
    pub from_std: bool,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub is_public: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Block,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub is_public: bool,
    pub name: String,
    pub ty: Type,
    /// Field defaults are mandatory - every instance starts fully initialized.
    pub default: Expr,
    pub line: u32,
    pub col: u32,
}

#[allow(dead_code)] // line/col kept for future diagnostics
#[derive(Debug, Clone)]
pub struct CtorDecl {
    pub params: Vec<Param>,
    pub body: Block,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    /// `class Vector<T>` - the single type parameter (std Array/Vector only).
    pub type_param: Option<String>,
    pub fields: Vec<FieldDecl>,
    /// At most one (no overloading); None means the auto-generated default.
    pub ctor: Option<CtorDecl>,
    pub methods: Vec<MethodDecl>,
    pub file_idx: usize,
    pub from_std: bool,
    pub line: u32,
    pub col: u32,
}

/// One parsed source file, before module merging.
#[derive(Debug, Clone)]
pub struct Module {
    pub imports: Vec<Import>,
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
}

/// The whole program after the driver merges all imported modules.
#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub enum AssignOp {
    Set, // =
    Add, // +=
    Sub, // -=
    Mul, // *=
    Div, // /=
}

// Some fields are parsed today but only consumed by later milestones.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum StmtKind {
    VarDecl { is_const: bool, name: String, ty: Type, init: Expr },
    Assign { target: Expr, op: AssignOp, value: Expr },
    /// `x++;` / `x--;` - statements only, by design.
    IncDec { target: Expr, inc: bool },
    ExprStmt(Expr),
    Return(Option<Expr>),
    /// `if` / `else if` chain: one (cond, block) per arm, plus optional final `else`.
    If { arms: Vec<(Expr, Block)>, else_block: Option<Block> },
    While { cond: Expr, body: Block },
    ForC { var_name: String, var_ty: Type, init: Expr, cond: Expr, step: Box<Stmt>, body: Block },
    ForIn { var_name: String, var_ty: Type, iterable: Expr, body: Block },
    Break,
    Continue,
    /// A bare `{ ... }` scope - produced by sema (for..in desugaring), never
    /// by the parser.
    Scope(Block),
    /// `try { ... } catch (e: Exception) { ... }`
    TryCatch { body: Block, var_name: String, var_ty: Type, catch_body: Block },
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    AddrOf,
    Deref,
}

// Some fields are parsed today but only consumed by later milestones.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    CharLit(u8), // a jim Char is one byte
    Bool(bool),
    NoneLit,
    Ident(String),
    This,
    ArrayLit(Vec<Expr>),
    Call { name: String, args: Vec<Expr> },
    /// `max<Vector<Float>, Float>(v)` - a call with explicit generic type
    /// arguments (the fallback when inference has no context).
    GenericCall { name: String, type_args: Vec<Type>, args: Vec<Expr> },
    MethodCall { recv: Box<Expr>, name: String, args: Vec<Expr> },
    /// A method call whose receiver class is resolved - produced by sema
    /// (never by the parser), consumed by codegen.
    CoreMethodCall { class: String, name: String, recv: Box<Expr>, args: Vec<Expr> },
    /// Optional machinery - all produced by sema only. `payload` is the T of
    /// the T? (core values use tagged structs; classes, containers, and
    /// pointers use nullable representations).
    OptWrap { payload: Type, expr: Box<Expr> },
    /// Unwrap-or-panic: using a T? where T is needed.
    OptUnwrap { payload: Type, expr: Box<Expr> },
    /// A typed None value.
    OptNone { payload: Type },
    /// Presence test: `x != None` lowers to this; `x == None` to `not` of it.
    OptHas { payload: Type, expr: Box<Expr> },
    /// Instantiation `Shape(1, 2)` - produced by sema from a Call whose name
    /// resolves to a class. `class` is a class key (may be "Vector<Integer>").
    New { class: String, args: Vec<Expr> },
    /// `[a, b, c]` after context-typing - builds a container through its
    /// constructor + set/push protocol (docs/DESIGN.md section 7a).
    ContainerLit { class: String, is_array: bool, elems: Vec<Expr> },
    /// `@buf_alloc(n)` with its element type resolved from context.
    BufAlloc { elem: Type, size: Box<Expr> },
    FieldAccess { recv: Box<Expr>, name: String },
    Index { recv: Box<Expr>, index: Box<Expr> },
    IntrinsicCall { name: String, args: Vec<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary { op: UnOp, operand: Box<Expr> },
}
