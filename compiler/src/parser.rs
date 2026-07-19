use crate::ast::*;
use crate::errors::{JResult, JimError};
use crate::token::{Token, TokenKind};

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

enum Member {
    Field(FieldDecl),
    Method(MethodDecl),
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, what: &str) -> JResult<Token> {
        if self.check(kind) {
            Ok(self.bump())
        } else {
            let t = self.peek();
            Err(JimError::new(
                format!("expected {} but found {}", what, t.kind.describe()),
                t.line,
                t.col,
            ))
        }
    }

    fn expect_ident(&mut self, what: &str) -> JResult<(String, u32, u32)> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Ident(name) => {
                self.bump();
                Ok((name, t.line, t.col))
            }
            _ => Err(JimError::new(
                format!("expected {} but found {}", what, t.kind.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn err_here(&self, msg: impl Into<String>) -> JimError {
        let t = self.peek();
        JimError::new(msg, t.line, t.col)
    }

    // ---------------- top level ----------------

    pub fn parse_module(&mut self, file_idx: usize, from_std: bool) -> JResult<Module> {
        let mut imports = Vec::new();
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::Eof => break,
                TokenKind::KwImport => imports.push(self.parse_import()?),
                TokenKind::KwFunction => functions.push(self.parse_function(file_idx, from_std)?),
                TokenKind::KwClass => classes.push(self.parse_class(file_idx, from_std)?),
                TokenKind::KwTry => {
                    return Err(self.err_here(
                        "try/catch is a statement — put it inside a function",
                    ))
                }
                _ => {
                    return Err(self.err_here(format!(
                        "expected 'function', 'class' or '#import' at top level, found {}",
                        self.peek_kind().describe()
                    )))
                }
            }
        }
        Ok(Module { imports, functions, classes })
    }

    fn parse_class(&mut self, file_idx: usize, from_std: bool) -> JResult<ClassDecl> {
        let kw = self.expect(&TokenKind::KwClass, "'class'")?;
        let (name, _, _) = self.expect_ident("a class name")?;
        let type_param = if self.eat(&TokenKind::Lt) {
            let (p, _, _) = self.expect_ident("a type parameter name")?;
            self.expect(&TokenKind::Gt, "'>' to close the type parameter")?;
            Some(p)
        } else {
            None
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields = Vec::new();
        let mut ctor: Option<CtorDecl> = None;
        let mut methods = Vec::new();
        loop {
            match self.peek_kind().clone() {
                TokenKind::RBrace => {
                    self.bump();
                    break;
                }
                TokenKind::Eof => {
                    return Err(self.err_here("unexpected end of file inside a class (missing '}')"))
                }
                TokenKind::KwPublic | TokenKind::KwPrivate => match self.parse_member()? {
                    Member::Field(f) => fields.push(f),
                    Member::Method(m) => methods.push(m),
                },
                TokenKind::Ident(n) => {
                    if n == name {
                        if ctor.is_some() {
                            return Err(self.err_here(format!(
                                "class '{}' already has a constructor (jim has no overloading)",
                                name
                            )));
                        }
                        ctor = Some(self.parse_ctor()?);
                    } else {
                        return Err(self.err_here(format!(
                            "class members start with 'public' or 'private' (or a '{}(...)' constructor), found '{}'",
                            name, n
                        )));
                    }
                }
                other => {
                    return Err(self.err_here(format!(
                        "expected a class member or '}}', found {}",
                        other.describe()
                    )))
                }
            }
        }
        Ok(ClassDecl {
            name,
            type_param,
            fields,
            ctor,
            methods,
            file_idx,
            from_std,
            line: kw.line,
            col: kw.col,
        })
    }

    fn parse_ctor(&mut self) -> JResult<CtorDecl> {
        let t = self.bump(); // the class-name ident (caller checked)
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let (pname, pline, pcol) = self.expect_ident("a parameter name")?;
                self.expect(&TokenKind::Colon, "':' (jim parameters are 'name: Type')")?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, line: pline, col: pcol });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        if self.check(&TokenKind::Arrow) {
            return Err(self.err_here("constructors have no return type"));
        }
        let body = self.parse_block()?;
        Ok(CtorDecl { params, body, line: t.line, col: t.col })
    }

    fn parse_member(&mut self) -> JResult<Member> {
        let vis = self.bump(); // public | private (caller checked)
        let is_public = matches!(vis.kind, TokenKind::KwPublic);
        let (name, _, _) = self.expect_ident("a member name")?;
        if self.eat(&TokenKind::Colon) {
            // field: `private width: Integer = 1;`
            let ty = self.parse_type()?;
            self.expect(
                &TokenKind::Assign,
                "'=' (fields need a default value: 'name: Type = value;')",
            )?;
            let default = self.parse_expr()?;
            self.expect(&TokenKind::Semicolon, "';'")?;
            return Ok(Member::Field(FieldDecl {
                is_public,
                name,
                ty,
                default,
                line: vis.line,
                col: vis.col,
            }));
        }
        self.parse_method_rest(is_public, name, vis.line, vis.col).map(Member::Method)
    }

    fn parse_method_rest(
        &mut self,
        is_public: bool,
        name: String,
        line: u32,
        col: u32,
    ) -> JResult<MethodDecl> {
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let (pname, pline, pcol) = self.expect_ident("a parameter name")?;
                self.expect(&TokenKind::Colon, "':' (jim parameters are 'name: Type')")?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, line: pline, col: pcol });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        if !self.check(&TokenKind::Arrow) {
            return Err(self.err_here(format!(
                "method '{}' is missing its return type: jim requires '-> Type' (use '-> None' for no value)",
                name
            )));
        }
        self.bump(); // ->
        let ret = self.parse_type()?;
        let body = self.parse_block()?;
        Ok(MethodDecl { is_public, name, params, ret, body, line, col })
    }

    fn parse_import(&mut self) -> JResult<Import> {
        let t = self.expect(&TokenKind::KwImport, "'#import'")?;
        match self.peek_kind().clone() {
            TokenKind::Lt => {
                self.bump();
                let (name, _, _) = self.expect_ident("a library name")?;
                self.expect(&TokenKind::Gt, "'>'")?;
                Ok(Import { kind: ImportKind::Std(name), line: t.line, col: t.col })
            }
            TokenKind::Str(s) => {
                self.bump();
                Ok(Import { kind: ImportKind::Local(s), line: t.line, col: t.col })
            }
            other => Err(JimError::new(
                format!(
                    "expected '<library>' or \"file.j\" after '#import', found {}",
                    other.describe()
                ),
                t.line,
                t.col,
            )),
        }
    }

    fn parse_function(&mut self, file_idx: usize, from_std: bool) -> JResult<FunctionDecl> {
        let kw = self.expect(&TokenKind::KwFunction, "'function'")?;
        let (name, _, _) = self.expect_ident("a function name")?;
        // `function max<C, T>(...)` — generic type parameters
        let mut type_params = Vec::new();
        if self.eat(&TokenKind::Lt) {
            loop {
                let (p, pline, pcol) = self.expect_ident("a type parameter name")?;
                if type_params.contains(&p) {
                    return Err(JimError::new(
                        format!("duplicate type parameter '{}'", p),
                        pline,
                        pcol,
                    ));
                }
                type_params.push(p);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::Gt, "'>' to close the type parameter list")?;
        }
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let (pname, pline, pcol) = self.expect_ident("a parameter name")?;
                self.expect(&TokenKind::Colon, "':' (jim parameters are 'name: Type')")?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, line: pline, col: pcol });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        if !self.check(&TokenKind::Arrow) {
            return Err(self.err_here(format!(
                "function '{}' is missing its return type: jim requires '-> Type' (use '-> None' for no value)",
                name
            )));
        }
        self.bump(); // ->
        let ret = self.parse_type()?;
        let body = self.parse_block()?;
        Ok(FunctionDecl {
            name,
            type_params,
            params,
            ret,
            body,
            file_idx,
            from_std,
            line: kw.line,
            col: kw.col,
        })
    }

    // ---------------- types ----------------

    fn parse_type(&mut self) -> JResult<Type> {
        if self.eat(&TokenKind::Star) {
            let inner = self.parse_type()?;
            return Ok(match inner {
                // `*T?` reads as "maybe-pointer": Optional(Pointer(T))
                Type::Optional(t) => Type::Optional(Box::new(Type::Pointer(t))),
                t => Type::Pointer(Box::new(t)),
            });
        }
        // `None` is a keyword but also the name of the unit type.
        let (name, _, _) = if self.check(&TokenKind::KwNone) {
            let t = self.bump();
            ("None".to_string(), t.line, t.col)
        } else {
            self.expect_ident("a type name")?
        };
        let mut ty = if self.eat(&TokenKind::Lt) {
            let inner = self.parse_type()?;
            self.expect(&TokenKind::Gt, "'>' to close the generic type")?;
            Type::Generic(name, Box::new(inner))
        } else {
            Type::Named(name)
        };
        if self.eat(&TokenKind::Question) {
            ty = Type::Optional(Box::new(ty));
        }
        Ok(ty)
    }

    // ---------------- statements ----------------

    fn parse_block(&mut self) -> JResult<Block> {
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(self.err_here("unexpected end of file inside a block (missing '}')"));
            }
            stmts.push(self.parse_stmt()?);
        }
        self.bump(); // }
        Ok(Block { stmts })
    }

    /// A `{ ... }` block, or a single statement (braceless `if (x) continue;`).
    fn parse_block_or_single(&mut self) -> JResult<Block> {
        if self.check(&TokenKind::LBrace) {
            self.parse_block()
        } else {
            let stmt = self.parse_stmt()?;
            Ok(Block { stmts: vec![stmt] })
        }
    }

    fn parse_stmt(&mut self) -> JResult<Stmt> {
        let t = self.peek().clone();
        let (line, col) = (t.line, t.col);
        match t.kind {
            TokenKind::KwVar | TokenKind::KwConst => {
                let is_const = matches!(t.kind, TokenKind::KwConst);
                self.bump();
                let (name, _, _) = self.expect_ident("a variable name")?;
                self.expect(&TokenKind::Colon, "':' (jim declarations are 'var name: Type = ...')")?;
                let ty = self.parse_type()?;
                self.expect(&TokenKind::Assign, "'=' (jim variables must be initialized)")?;
                let init = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon, "';'")?;
                Ok(Stmt { kind: StmtKind::VarDecl { is_const, name, ty, init }, line, col })
            }
            TokenKind::KwReturn => {
                self.bump();
                let value = if self.check(&TokenKind::Semicolon) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(&TokenKind::Semicolon, "';'")?;
                Ok(Stmt { kind: StmtKind::Return(value), line, col })
            }
            TokenKind::KwBreak => {
                self.bump();
                self.expect(&TokenKind::Semicolon, "';'")?;
                Ok(Stmt { kind: StmtKind::Break, line, col })
            }
            TokenKind::KwContinue => {
                self.bump();
                self.expect(&TokenKind::Semicolon, "';'")?;
                Ok(Stmt { kind: StmtKind::Continue, line, col })
            }
            TokenKind::KwIf => self.parse_if(line, col),
            TokenKind::KwWhile => {
                self.bump();
                self.expect(&TokenKind::LParen, "'('")?;
                let cond = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "')'")?;
                let body = self.parse_block_or_single()?;
                Ok(Stmt { kind: StmtKind::While { cond, body }, line, col })
            }
            TokenKind::KwFor => self.parse_for(line, col),
            TokenKind::KwTry => {
                self.bump();
                let body = self.parse_block()?;
                self.expect(&TokenKind::KwCatch, "'catch' after the try block")?;
                self.expect(&TokenKind::LParen, "'('")?;
                let (var_name, _, _) = self.expect_ident("the exception variable name")?;
                self.expect(&TokenKind::Colon, "':' (catch clauses are 'catch (e: Exception)')")?;
                let var_ty = self.parse_type()?;
                self.expect(&TokenKind::RParen, "')'")?;
                let catch_body = self.parse_block()?;
                Ok(Stmt { kind: StmtKind::TryCatch { body, var_name, var_ty, catch_body }, line, col })
            }
            _ => {
                let stmt = self.parse_simple_stmt()?;
                self.expect(&TokenKind::Semicolon, "';'")?;
                Ok(stmt)
            }
        }
    }

    /// Assignment, ++/--, or a bare expression — without the trailing ';'
    /// (shared between normal statements and the step slot of a C-style for).
    fn parse_simple_stmt(&mut self) -> JResult<Stmt> {
        let t = self.peek().clone();
        let (line, col) = (t.line, t.col);
        let expr = self.parse_expr()?;
        let op = match self.peek_kind() {
            TokenKind::Assign => Some(AssignOp::Set),
            TokenKind::PlusEq => Some(AssignOp::Add),
            TokenKind::MinusEq => Some(AssignOp::Sub),
            TokenKind::StarEq => Some(AssignOp::Mul),
            TokenKind::SlashEq => Some(AssignOp::Div),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            Self::check_assign_target(&expr)?;
            let value = self.parse_expr()?;
            return Ok(Stmt { kind: StmtKind::Assign { target: expr, op, value }, line, col });
        }
        if self.check(&TokenKind::PlusPlus) || self.check(&TokenKind::MinusMinus) {
            let inc = matches!(self.peek_kind(), TokenKind::PlusPlus);
            self.bump();
            Self::check_assign_target(&expr)?;
            return Ok(Stmt { kind: StmtKind::IncDec { target: expr, inc }, line, col });
        }
        Ok(Stmt { kind: StmtKind::ExprStmt(expr), line, col })
    }

    fn check_assign_target(e: &Expr) -> JResult<()> {
        match &e.kind {
            ExprKind::Ident(_) | ExprKind::Index { .. } | ExprKind::FieldAccess { .. } => Ok(()),
            ExprKind::Unary { op: UnOp::Deref, .. } => Ok(()),
            _ => Err(JimError::new(
                "invalid assignment target (expected a variable, index, field, or '*pointer')",
                e.line,
                e.col,
            )),
        }
    }

    fn parse_if(&mut self, line: u32, col: u32) -> JResult<Stmt> {
        let mut arms = Vec::new();
        let mut else_block = None;
        loop {
            self.expect(&TokenKind::KwIf, "'if'")?;
            self.expect(&TokenKind::LParen, "'('")?;
            let cond = self.parse_expr()?;
            self.expect(&TokenKind::RParen, "')'")?;
            let body = self.parse_block_or_single()?;
            arms.push((cond, body));
            if !self.check(&TokenKind::KwElse) {
                break;
            }
            self.bump(); // else
            if self.check(&TokenKind::KwIf) {
                continue; // else if -> next arm
            }
            else_block = Some(self.parse_block_or_single()?);
            break;
        }
        Ok(Stmt { kind: StmtKind::If { arms, else_block }, line, col })
    }

    fn parse_for(&mut self, line: u32, col: u32) -> JResult<Stmt> {
        self.expect(&TokenKind::KwFor, "'for'")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let (var_name, _, _) = self.expect_ident("a loop variable name")?;
        self.expect(&TokenKind::Colon, "':' (loop variables are 'name: Type')")?;
        let var_ty = self.parse_type()?;
        match self.peek_kind().clone() {
            TokenKind::Assign => {
                self.bump();
                let init = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon, "';'")?;
                let cond = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon, "';'")?;
                let step = Box::new(self.parse_simple_stmt()?);
                self.expect(&TokenKind::RParen, "')'")?;
                let body = self.parse_block_or_single()?;
                Ok(Stmt {
                    kind: StmtKind::ForC { var_name, var_ty, init, cond, step, body },
                    line,
                    col,
                })
            }
            TokenKind::KwIn => {
                self.bump();
                let iterable = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "')'")?;
                let body = self.parse_block_or_single()?;
                Ok(Stmt {
                    kind: StmtKind::ForIn { var_name, var_ty, iterable, body },
                    line,
                    col,
                })
            }
            other => Err(self.err_here(format!(
                "expected '=' (C-style for) or 'in' (for-in) after the loop variable, found {}",
                other.describe()
            ))),
        }
    }

    // ---------------- expressions ----------------
    // precedence (loosest to tightest): or, and, not, comparisons, + -, * / % div, unary, postfix

    pub fn parse_expr(&mut self) -> JResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> JResult<Expr> {
        let mut lhs = self.parse_and()?;
        while self.check(&TokenKind::KwOr) {
            let t = self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr {
                kind: ExprKind::Binary { op: BinOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: t.line,
                col: t.col,
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> JResult<Expr> {
        let mut lhs = self.parse_not()?;
        while self.check(&TokenKind::KwAnd) {
            let t = self.bump();
            let rhs = self.parse_not()?;
            lhs = Expr {
                kind: ExprKind::Binary { op: BinOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: t.line,
                col: t.col,
            };
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> JResult<Expr> {
        if self.check(&TokenKind::KwNot) {
            let t = self.bump();
            let operand = self.parse_not()?;
            return Ok(Expr {
                kind: ExprKind::Unary { op: UnOp::Not, operand: Box::new(operand) },
                line: t.line,
                col: t.col,
            });
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> JResult<Expr> {
        let mut lhs = self.parse_additive()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::NotEq => BinOp::NotEq,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::LtEq => BinOp::LtEq,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::GtEq => BinOp::GtEq,
                _ => break,
            };
            let t = self.bump();
            let rhs = self.parse_additive()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: t.line,
                col: t.col,
            };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self) -> JResult<Expr> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let t = self.bump();
            let rhs = self.parse_multiplicative()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: t.line,
                col: t.col,
            };
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> JResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                TokenKind::KwDiv => BinOp::IntDiv,
                _ => break,
            };
            let t = self.bump();
            let rhs = self.parse_unary()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: t.line,
                col: t.col,
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> JResult<Expr> {
        let op = match self.peek_kind() {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Ampersand => Some(UnOp::AddrOf),
            TokenKind::Star => Some(UnOp::Deref),
            _ => None,
        };
        if let Some(op) = op {
            let t = self.bump();
            let operand = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary { op, operand: Box::new(operand) },
                line: t.line,
                col: t.col,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> JResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    let t = self.bump();
                    let (name, _, _) = self.expect_ident("a member name after '.'")?;
                    if self.check(&TokenKind::LParen) {
                        let args = self.parse_call_args()?;
                        expr = Expr {
                            kind: ExprKind::MethodCall { recv: Box::new(expr), name, args },
                            line: t.line,
                            col: t.col,
                        };
                    } else {
                        expr = Expr {
                            kind: ExprKind::FieldAccess { recv: Box::new(expr), name },
                            line: t.line,
                            col: t.col,
                        };
                    }
                }
                TokenKind::LBracket => {
                    let t = self.bump();
                    let index = self.parse_expr()?;
                    self.expect(&TokenKind::RBracket, "']'")?;
                    expr = Expr {
                        kind: ExprKind::Index { recv: Box::new(expr), index: Box::new(index) },
                        line: t.line,
                        col: t.col,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    /// Try `<Type, Type, ...>` followed by '('. On any mismatch the position
    /// is restored and None returned (the '<' was a comparison after all).
    fn try_parse_type_args(&mut self) -> Option<Vec<Type>> {
        let saved = self.pos;
        if !self.eat(&TokenKind::Lt) {
            return None;
        }
        let mut type_args = Vec::new();
        loop {
            match self.parse_type() {
                Ok(t) => type_args.push(t),
                Err(_) => {
                    self.pos = saved;
                    return None;
                }
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        if self.eat(&TokenKind::Gt) && self.check(&TokenKind::LParen) {
            Some(type_args)
        } else {
            self.pos = saved;
            None
        }
    }

    fn parse_call_args(&mut self) -> JResult<Vec<Expr>> {
        self.expect(&TokenKind::LParen, "'('")?;
        let mut args = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        Ok(args)
    }

    fn parse_primary(&mut self) -> JResult<Expr> {
        let t = self.peek().clone();
        let (line, col) = (t.line, t.col);
        match t.kind {
            TokenKind::Int(v) => {
                self.bump();
                Ok(Expr { kind: ExprKind::Int(v), line, col })
            }
            TokenKind::Float(v) => {
                self.bump();
                Ok(Expr { kind: ExprKind::Float(v), line, col })
            }
            TokenKind::Str(s) => {
                self.bump();
                Ok(Expr { kind: ExprKind::Str(s), line, col })
            }
            TokenKind::CharLit(c) => {
                self.bump();
                Ok(Expr { kind: ExprKind::CharLit(c), line, col })
            }
            TokenKind::KwTrue => {
                self.bump();
                Ok(Expr { kind: ExprKind::Bool(true), line, col })
            }
            TokenKind::KwFalse => {
                self.bump();
                Ok(Expr { kind: ExprKind::Bool(false), line, col })
            }
            TokenKind::KwNone => {
                self.bump();
                Ok(Expr { kind: ExprKind::NoneLit, line, col })
            }
            TokenKind::KwThis => {
                self.bump();
                Ok(Expr { kind: ExprKind::This, line, col })
            }
            TokenKind::Ident(name) => {
                self.bump();
                // `max<Vector<Float>, Float>(v)` — explicit generic arguments.
                // Tentative: commits only when `<types>` parses cleanly AND is
                // followed by '(' — otherwise '<' stays a comparison.
                if self.check(&TokenKind::Lt) {
                    if let Some(type_args) = self.try_parse_type_args() {
                        let args = self.parse_call_args()?;
                        return Ok(Expr {
                            kind: ExprKind::GenericCall { name, type_args, args },
                            line,
                            col,
                        });
                    }
                }
                if self.check(&TokenKind::LParen) {
                    let args = self.parse_call_args()?;
                    Ok(Expr { kind: ExprKind::Call { name, args }, line, col })
                } else {
                    Ok(Expr { kind: ExprKind::Ident(name), line, col })
                }
            }
            TokenKind::Intrinsic(name) => {
                self.bump();
                if !self.check(&TokenKind::LParen) {
                    return Err(self.err_here(format!(
                        "intrinsic '@{}' must be called: @{}(...)",
                        name, name
                    )));
                }
                let args = self.parse_call_args()?;
                Ok(Expr { kind: ExprKind::IntrinsicCall { name, args }, line, col })
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            TokenKind::LBracket => {
                self.bump();
                let mut items = Vec::new();
                if !self.check(&TokenKind::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RBracket, "']'")?;
                Ok(Expr { kind: ExprKind::ArrayLit(items), line, col })
            }
            other => Err(JimError::new(
                format!("expected an expression, found {}", other.describe()),
                line,
                col,
            )),
        }
    }
}
