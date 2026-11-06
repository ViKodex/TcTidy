//! Non-fatal syntax errors collected during resilient parsing.
//!
//! An error never aborts parsing: the tree is always produced (with `ERROR`
//! nodes where recovery happened). Callers decide policy, the formatter, per
//! the non-destruction rule, refuses to write when any error is present.

use text_size::TextRange;

/// A single diagnostic with the source range it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub kind: ErrorKind,
    pub range: TextRange,
}

/// Coarse classification of a [`SyntaxError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// An expected token was missing.
    Expected,
    /// An unexpected token was wrapped in an `ERROR` node and skipped.
    Unexpected,
    /// A lexical problem (e.g. unterminated string/comment).
    Lex,
}

impl SyntaxError {
    pub fn new(kind: ErrorKind, message: impl Into<String>, range: TextRange) -> Self {
        SyntaxError {
            message: message.into(),
            kind,
            range,
        }
    }
}
