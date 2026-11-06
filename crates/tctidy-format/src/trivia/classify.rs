//! The recorded comment/pragma [`Piece`].
//!
//! The leading / trailing / dangling *decision* lives in [`super::attach`]; this
//! module owns the small piece data type. A piece is emitted verbatim, so it only
//! needs to carry its exact source text.
//!
//! * **Trailing**, same line as the preceding code (`x := 1; // c`) → emitted
//!   via `LineSuffix`.
//! * **Leading**, on its own line above the next construct → emitted before it.
//! * **Dangling**, after the last significant token of the tree → emitted at the
//!   end.

use tctidy_syntax::{SyntaxKind, SyntaxToken};

/// A comment or pragma recovered from the source, with the exact text needed to
/// re-emit it byte-for-byte.
#[derive(Debug, Clone)]
pub(crate) struct Piece {
    pub text: String,
}

impl Piece {
    /// Build a piece from a trivia token, or `None` if the token is plain
    /// whitespace/newline (carries no content to preserve).
    pub(crate) fn from_token(tok: &SyntaxToken) -> Option<Piece> {
        match tok.kind() {
            SyntaxKind::LINE_COMMENT
            | SyntaxKind::BLOCK_COMMENT
            | SyntaxKind::C_BLOCK_COMMENT
            | SyntaxKind::PRAGMA => Some(Piece {
                text: tok.text().to_string(),
            }),
            _ => None,
        }
    }
}
