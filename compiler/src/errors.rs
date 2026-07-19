use std::path::Path;

/// A compile-time diagnostic anchored to a line/column in some source file.
/// Which file is known by whoever holds the error (the driver tracks sources).
#[derive(Debug, Clone)]
pub struct JimError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

impl JimError {
    pub fn new(msg: impl Into<String>, line: u32, col: u32) -> Self {
        JimError { msg: msg.into(), line, col }
    }
}

pub type JResult<T> = Result<T, JimError>;

/// Render `path:line:col: error: msg` followed by the offending source line
/// and a caret pointing at the column.
pub fn render(path: &Path, src: &str, err: &JimError) -> String {
    let mut out = format!(
        "{}:{}:{}: error: {}\n",
        path.display(),
        err.line,
        err.col,
        err.msg
    );
    if err.line >= 1 {
        if let Some(line_text) = src.lines().nth(err.line as usize - 1) {
            out.push_str(&format!("    {}\n", line_text));
            let pad = " ".repeat(err.col.saturating_sub(1) as usize);
            out.push_str(&format!("    {}^\n", pad));
        }
    }
    out
}
