//! Formatting errors.

use std::fmt;

use tctidy_syntax::SyntaxError;

/// Why a format attempt failed. On any error the caller keeps the original input
/// (non-destruction).
#[derive(Debug)]
pub enum FormatError {
    /// The input did not parse cleanly; refusing to format. Carries the parser's
    /// diagnostics so a caller holding the source can render them with location
    /// and context (see [`tctidy_syntax::diagnostics`]).
    Parse(Vec<SyntaxError>),
    /// An internal invariant was violated while lowering.
    Internal(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Terse, source-free summary; callers with the source render the
            // located diagnostics instead.
            FormatError::Parse(errs) => write!(f, "parse error: {} parse error(s)", errs.len()),
            FormatError::Internal(m) => write!(f, "internal formatting error: {m}"),
        }
    }
}

impl std::error::Error for FormatError {}
