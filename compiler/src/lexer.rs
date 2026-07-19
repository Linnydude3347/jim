use crate::errors::{JResult, JimError};
use crate::token::{Token, TokenKind};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        // Windows editors love UTF-8 BOMs; treat one as whitespace.
        let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
        Lexer { chars: src.chars().collect(), pos: 0, line: 1, col: 1 }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn err(&self, msg: impl Into<String>) -> JimError {
        JimError::new(msg, self.line, self.col)
    }

    pub fn tokenize(mut self) -> JResult<Vec<Token>> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            let (line, col) = (self.line, self.col);
            let c = match self.peek() {
                None => {
                    out.push(Token { kind: TokenKind::Eof, line, col });
                    return Ok(out);
                }
                Some(c) => c,
            };
            let kind = self.next_kind(c)?;
            out.push(Token { kind, line, col });
        }
    }

    /// Skip whitespace and `//` line comments.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek2() == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }

    fn next_kind(&mut self, c: char) -> JResult<TokenKind> {
        if c.is_ascii_digit() {
            return self.lex_number();
        }
        if c.is_alphabetic() || c == '_' {
            return Ok(self.lex_ident_or_keyword());
        }
        match c {
            '"' => self.lex_string(),
            '\'' => self.lex_char(),
            '#' => self.lex_hash_directive(),
            '@' => self.lex_intrinsic(),
            _ => self.lex_symbol(c),
        }
    }

    fn lex_number(&mut self) -> JResult<TokenKind> {
        let (line, col) = (self.line, self.col);
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        // A '.' followed by a digit continues a float; `1.to_string()` stays an Integer.
        if self.peek() == Some('.') && self.peek2().map_or(false, |c| c.is_ascii_digit()) {
            text.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            let v: f64 = text
                .parse()
                .map_err(|_| JimError::new(format!("invalid float literal '{}'", text), line, col))?;
            return Ok(TokenKind::Float(v));
        }
        match text.parse::<i64>() {
            Ok(v) => Ok(TokenKind::Int(v)),
            Err(_) => Err(JimError::new(
                format!("integer literal '{}' does not fit in 64 bits", text),
                line,
                col,
            )),
        }
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                self.bump();
            } else {
                break;
            }
        }
        match name.as_str() {
            "function" => TokenKind::KwFunction,
            "var" => TokenKind::KwVar,
            "const" => TokenKind::KwConst,
            "if" => TokenKind::KwIf,
            "else" => TokenKind::KwElse,
            "for" => TokenKind::KwFor,
            "while" => TokenKind::KwWhile,
            "in" => TokenKind::KwIn,
            "break" => TokenKind::KwBreak,
            "continue" => TokenKind::KwContinue,
            "return" => TokenKind::KwReturn,
            "class" => TokenKind::KwClass,
            "public" => TokenKind::KwPublic,
            "private" => TokenKind::KwPrivate,
            "this" => TokenKind::KwThis,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "None" => TokenKind::KwNone,
            "and" => TokenKind::KwAnd,
            "or" => TokenKind::KwOr,
            "not" => TokenKind::KwNot,
            "div" => TokenKind::KwDiv,
            "try" => TokenKind::KwTry,
            "catch" => TokenKind::KwCatch,
            _ => TokenKind::Ident(name),
        }
    }

    fn lex_escape(&mut self) -> JResult<char> {
        // caller consumed the backslash
        match self.bump() {
            Some('n') => Ok('\n'),
            Some('t') => Ok('\t'),
            Some('r') => Ok('\r'),
            Some('0') => Ok('\0'),
            Some('\\') => Ok('\\'),
            Some('"') => Ok('"'),
            Some('\'') => Ok('\''),
            Some(c) => Err(self.err(format!("unknown escape sequence '\\{}'", c))),
            None => Err(self.err("unterminated escape sequence")),
        }
    }

    fn lex_string(&mut self) -> JResult<TokenKind> {
        let (line, col) = (self.line, self.col);
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(JimError::new("unterminated string literal", line, col)),
                Some('\n') => {
                    return Err(JimError::new(
                        "unterminated string literal (strings cannot span lines)",
                        line,
                        col,
                    ))
                }
                Some('"') => {
                    self.bump();
                    return Ok(TokenKind::Str(s));
                }
                Some('\\') => {
                    self.bump();
                    s.push(self.lex_escape()?);
                }
                Some(c) => {
                    s.push(c);
                    self.bump();
                }
            }
        }
    }

    fn lex_char(&mut self) -> JResult<TokenKind> {
        let (line, col) = (self.line, self.col);
        self.bump(); // opening quote
        let c = match self.peek() {
            None => return Err(JimError::new("unterminated char literal", line, col)),
            Some('\\') => {
                self.bump();
                self.lex_escape()?
            }
            Some('\'') => return Err(JimError::new("empty char literal", line, col)),
            Some(c) => {
                self.bump();
                c
            }
        };
        match self.peek() {
            Some('\'') => {
                self.bump();
                if !c.is_ascii() {
                    return Err(JimError::new(
                        format!(
                            "'{}' does not fit in a Char: Char is a single byte, so literals must be ASCII (use String for Unicode text)",
                            c
                        ),
                        line,
                        col,
                    ));
                }
                Ok(TokenKind::CharLit(c as u8))
            }
            _ => Err(JimError::new(
                "char literal must contain exactly one character",
                line,
                col,
            )),
        }
    }

    fn lex_hash_directive(&mut self) -> JResult<TokenKind> {
        let (line, col) = (self.line, self.col);
        self.bump(); // '#'
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if word == "import" {
            Ok(TokenKind::KwImport)
        } else {
            Err(JimError::new(
                format!("unknown directive '#{}' (did you mean '#import'?)", word),
                line,
                col,
            ))
        }
    }

    fn lex_intrinsic(&mut self) -> JResult<TokenKind> {
        let (line, col) = (self.line, self.col);
        self.bump(); // '@'
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if name.is_empty() {
            return Err(JimError::new("expected intrinsic name after '@'", line, col));
        }
        Ok(TokenKind::Intrinsic(name))
    }

    fn lex_symbol(&mut self, c: char) -> JResult<TokenKind> {
        self.bump();
        let next = self.peek();
        let two = |lx: &mut Lexer, k: TokenKind| {
            lx.bump();
            Ok(k)
        };
        match c {
            '(' => Ok(TokenKind::LParen),
            ')' => Ok(TokenKind::RParen),
            '{' => Ok(TokenKind::LBrace),
            '}' => Ok(TokenKind::RBrace),
            '[' => Ok(TokenKind::LBracket),
            ']' => Ok(TokenKind::RBracket),
            ',' => Ok(TokenKind::Comma),
            ';' => Ok(TokenKind::Semicolon),
            ':' => Ok(TokenKind::Colon),
            '.' => Ok(TokenKind::Dot),
            '?' => Ok(TokenKind::Question),
            '&' => Ok(TokenKind::Ampersand),
            '+' => match next {
                Some('+') => two(self, TokenKind::PlusPlus),
                Some('=') => two(self, TokenKind::PlusEq),
                _ => Ok(TokenKind::Plus),
            },
            '-' => match next {
                Some('-') => two(self, TokenKind::MinusMinus),
                Some('=') => two(self, TokenKind::MinusEq),
                Some('>') => two(self, TokenKind::Arrow),
                _ => Ok(TokenKind::Minus),
            },
            '*' => match next {
                Some('=') => two(self, TokenKind::StarEq),
                _ => Ok(TokenKind::Star),
            },
            '/' => match next {
                Some('=') => two(self, TokenKind::SlashEq),
                _ => Ok(TokenKind::Slash),
            },
            '%' => Ok(TokenKind::Percent),
            '=' => match next {
                Some('=') => two(self, TokenKind::EqEq),
                _ => Ok(TokenKind::Assign),
            },
            '!' => match next {
                Some('=') => two(self, TokenKind::NotEq),
                _ => Err(self.err("unexpected '!' (jim uses the 'not' keyword)")),
            },
            '<' => match next {
                Some('=') => two(self, TokenKind::LtEq),
                _ => Ok(TokenKind::Lt),
            },
            '>' => match next {
                Some('=') => two(self, TokenKind::GtEq),
                _ => Ok(TokenKind::Gt),
            },
            _ => Err(self.err(format!("unexpected character '{}'", c))),
        }
    }
}
