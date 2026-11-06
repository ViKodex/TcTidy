//! Comment & pragma attachment, the algorithm that makes "no comment ever lost
//! or reordered" true.
//!
//! A single up-front pass over the CST token stream (source order) classifies
//! each comment/pragma as **leading** or **trailing** of a *significant* token,
//! or **dangling** at end of tree, building a [`Comments`] map the lowering rules
//! consult. Because the pass walks the tree in source order and every piece is
//! recorded exactly once, the concatenation order of comments equals source
//! order by construction.
//!
//! The map is keyed by the significant [`SyntaxToken`] a comment binds to (rowan
//! tokens are cheap to clone and hash by identity). A rule fetches a node's
//! comments through the first/last significant token of that node, see
//! [`Comments::leading_of`] / [`Comments::trailing_of`].

mod attach;
mod classify;
mod print;

use std::collections::HashMap;

use tctidy_syntax::{SyntaxNode, SyntaxToken};

pub(crate) use classify::Piece;
pub(crate) use print::{dangling_doc, gap_doc, leading_doc, trailing_doc};

/// A leading piece together with the number of blank lines that preceded it.
#[derive(Debug, Clone)]
pub(crate) struct LeadingPiece {
    pub piece: Piece,
    /// Blank lines immediately above this piece (already in source, uncapped).
    pub blank_before: u16,
}

/// The attachment map consulted during lowering.
#[derive(Debug, Default)]
pub struct Comments {
    leading: HashMap<SyntaxToken, Vec<LeadingPiece>>,
    trailing: HashMap<SyntaxToken, Vec<Piece>>,
    /// Blank lines immediately before a significant token (after its leading
    /// pieces, if any), uncapped, the lowering caps per [`StyleConfig`].
    blanks_before: HashMap<SyntaxToken, u16>,
    /// Comments after the last significant token of the tree, each with the blank
    /// lines that preceded it (so an intentional blank above an end-of-body
    /// comment is preserved).
    dangling: Vec<LeadingPiece>,
    count: usize,
}

/// Normalize a line comment's surrounding whitespace. Trailing spaces/tabs are
/// **always** trimmed (they are never meaningful). When `space_after` is set, the
/// run after `//` is collapsed to exactly one space (`//x` → `// x`,
/// `//   x` → `// x`, `//\tx` → `// x`) and a content-less comment becomes a bare
/// `//`. Returns `None` when the comment is already canonical or is not a line
/// comment (block comments and pragmas are never touched); the content's internal
/// spacing is preserved.
fn normalize_line_comment(text: &str, space_after: bool) -> Option<String> {
    let rest = text.strip_prefix("//")?;
    let rest = rest.trim_end_matches([' ', '\t']);
    let normalized = if space_after {
        let content = rest.trim_start_matches([' ', '\t']);
        if content.is_empty() {
            "//".to_string()
        } else {
            format!("// {content}")
        }
    } else {
        format!("//{rest}")
    };
    (normalized != text).then_some(normalized)
}

impl Comments {
    /// Build the attachment map for a tree.
    pub fn attach(root: &SyntaxNode) -> Comments {
        attach::attach_comments(root)
    }

    /// Normalize every line comment (leading, trailing, dangling): always trim
    /// trailing whitespace, and when `space_after` enforce exactly one space after
    /// the `//`. Block comments and pragmas are never touched.
    pub(crate) fn normalize_line_comments(&mut self, space_after: bool) {
        fn fix(piece: &mut Piece, space_after: bool) {
            if let Some(text) = normalize_line_comment(&piece.text, space_after) {
                piece.text = text;
            }
        }
        for pieces in self.leading.values_mut() {
            for lp in pieces {
                fix(&mut lp.piece, space_after);
            }
        }
        for pieces in self.trailing.values_mut() {
            for piece in pieces {
                fix(piece, space_after);
            }
        }
        for lp in &mut self.dangling {
            fix(&mut lp.piece, space_after);
        }
    }

    /// Leading pieces bound to `tok` (own-line comments/pragmas above it).
    pub(crate) fn leading(&self, tok: &SyntaxToken) -> &[LeadingPiece] {
        self.leading.get(tok).map_or(&[], Vec::as_slice)
    }

    /// Trailing pieces bound to `tok` (same-line comments after it).
    pub(crate) fn trailing(&self, tok: &SyntaxToken) -> &[Piece] {
        self.trailing.get(tok).map_or(&[], Vec::as_slice)
    }

    /// The trailing pieces of the *last* item of a separated list: those after
    /// its last token, then those after a trailing comma the source wrote after
    /// it. The comma itself may be dropped or kept by the trailing-separator
    /// policy, but a comment bound to it must survive either way.
    pub(crate) fn trailing_of_last_item(&self, item: &SyntaxNode) -> Vec<Piece> {
        let mut pieces: Vec<Piece> = last_sig_token(item)
            .map(|t| self.trailing(&t).to_vec())
            .unwrap_or_default();
        let mut cur = item.next_sibling_or_token();
        while let Some(el) = cur {
            match el.as_token() {
                Some(t) if t.kind() == tctidy_syntax::SyntaxKind::COMMA => {
                    pieces.extend_from_slice(self.trailing(t));
                    break;
                }
                Some(t) if t.kind().is_trivia() => {}
                _ => break,
            }
            cur = el.next_sibling_or_token();
        }
        pieces
    }

    /// Blank lines recorded immediately before `tok`.
    pub(crate) fn blanks_before(&self, tok: &SyntaxToken) -> u16 {
        self.blanks_before.get(tok).copied().unwrap_or(0)
    }

    /// Leading pieces bound to the first significant token of `node`.
    pub(crate) fn leading_of(&self, node: &SyntaxNode) -> &[LeadingPiece] {
        first_sig_token(node).map_or(&[], |t| self.leading(&t))
    }

    /// Trailing pieces bound to the last significant token of `node`.
    pub(crate) fn trailing_of(&self, node: &SyntaxNode) -> &[Piece] {
        last_sig_token(node).map_or(&[], |t| self.trailing(&t))
    }

    /// Blank lines before the first significant token of `node`.
    pub(crate) fn blanks_before_of(&self, node: &SyntaxNode) -> u16 {
        first_sig_token(node).map_or(0, |t| self.blanks_before(&t))
    }

    /// End-of-tree comments not bound to any following construct (each with the
    /// blank lines that preceded it).
    pub(crate) fn dangling(&self) -> &[LeadingPiece] {
        &self.dangling
    }

    /// Total number of comment/pragma pieces recorded.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether any piece was recorded.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Whether a construct's leading comments start with a block comment spanning
/// several lines (`blank_lines_before_multiline_comment` applies above it).
pub(crate) fn starts_with_multiline_comment(lead: &[LeadingPiece]) -> bool {
    lead.first().is_some_and(|lp| {
        let text = &lp.piece.text;
        (text.starts_with("(*") || text.starts_with("/*")) && text.contains('\n')
    })
}

/// The first non-trivia token in source order under `node`.
pub(crate) fn first_sig_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
}

/// The last non-trivia token in source order under `node`.
pub(crate) fn last_sig_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .last()
}

/// Whether `tok` is preceded by a newline in its trivia gap.
pub(crate) fn token_after_newline(tok: &SyntaxToken) -> bool {
    let mut cur = tok.prev_token();
    while let Some(token) = cur {
        match token.kind() {
            tctidy_syntax::SyntaxKind::NEWLINE => return true,
            kind if kind.is_trivia() => cur = token.prev_token(),
            _ => return false,
        }
    }
    false
}
