//! Emission of attached comments into the `Doc`.
//!
//! Leading pieces render before the construct, each on its own line (with capped
//! blank lines above); trailing pieces render via `LineSuffix` so they stay at
//! end-of-line; dangling pieces render on their own lines at the end of a
//! container. All comment/pragma text is emitted **verbatim** (never reflowed).

use tctidy_pretty::prelude::*;
use tctidy_syntax::SyntaxToken;

use super::classify::Piece;
use super::{token_after_newline, Comments, LeadingPiece};
use crate::verbatim::verbatim;

/// Leading comments/pragmas, each followed by a hard break so the construct that
/// follows starts on its own line. `blank_cap` caps preserved blank lines
/// *between* stacked pieces; the blank line above the whole block is the caller's
/// responsibility (it belongs to the construct's separation, not the comment).
pub(crate) fn leading_doc(pieces: &[LeadingPiece], blank_cap: u16) -> Doc {
    let mut parts = Vec::new();
    for (i, lp) in pieces.iter().enumerate() {
        let blanks = if i == 0 {
            0
        } else {
            lp.blank_before.min(blank_cap)
        };
        for _ in 0..blanks {
            parts.push(hard_line());
        }
        parts.push(verbatim(&lp.piece.text));
        parts.push(hard_line());
    }
    concat(parts)
}

/// Trailing comments deferred to end of line via `LineSuffix`. `min_spaces` is
/// the gap before the first piece (`StyleConfig::trailing_comment_min_spaces`).
pub(crate) fn trailing_doc(pieces: &[Piece], min_spaces: usize) -> Doc {
    if pieces.is_empty() {
        return nil();
    }
    let mut inner = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let spaces = if i == 0 { min_spaces.max(1) } else { 1 };
        inner.push(text(" ".repeat(spaces)));
        inner.push(verbatim(&p.text));
    }
    line_suffix(concat(inner))
}

/// Render the complete trivia gap between two significant tokens.
///
/// Inline comments stay between the tokens with `inline_prefix` before the
/// first comment and one space after each comment. A broken gap flushes the
/// previous token's trailing comments, emits the source line break, then emits
/// comments attached to the next token. The boolean reports that broken form so
/// callers can apply their construct-specific continuation indentation.
pub(crate) fn gap_doc(
    previous: &SyntaxToken,
    next: &SyntaxToken,
    comments: &Comments,
    min_spaces: usize,
    blank_cap: u16,
    inline_prefix: &str,
) -> (Doc, bool) {
    if token_after_newline(next) {
        return (
            concat([
                trailing_doc(comments.trailing(previous), min_spaces),
                hard_line(),
                leading_doc(comments.leading(next), blank_cap),
            ]),
            true,
        );
    }

    let pieces = comments.trailing(previous);
    if pieces.is_empty() {
        return (text(inline_prefix.to_string()), false);
    }

    let mut parts = vec![text(inline_prefix.to_string())];
    for piece in pieces {
        parts.push(verbatim(&piece.text));
        parts.push(text(" "));
    }
    (concat(parts), false)
}

/// Dangling comments (end of container / tree), each on its own line. The blank
/// lines that preceded each comment are preserved (capped at `blank_cap`), so an
/// intentional blank above an end-of-body comment survives. The separator from
/// the preceding content is the caller's responsibility.
pub(crate) fn dangling_doc(pieces: &[LeadingPiece], blank_cap: u16) -> Doc {
    let mut parts = Vec::new();
    for (i, lp) in pieces.iter().enumerate() {
        if i > 0 {
            parts.push(hard_line());
        }
        for _ in 0..lp.blank_before.min(blank_cap) {
            parts.push(hard_line());
        }
        parts.push(verbatim(&lp.piece.text));
    }
    concat(parts)
}
