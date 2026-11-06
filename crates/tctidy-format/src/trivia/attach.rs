//! The attachment pass.
//!
//! Walk the CST token stream left-to-right. Significant tokens advance a cursor;
//! each maximal run of trivia between two significant tokens is a "gap". Within a
//! gap, a comment on the same line as the preceding significant token is
//! **trailing** of it; any comment after a newline is **leading** of the next
//! significant token; comments with no following significant token are
//! **dangling**. Newline counts in the gap yield blank-line metadata.
//!
//! Every comment/pragma is recorded exactly once, so nothing is dropped.

use tctidy_syntax::{SyntaxNode, SyntaxToken};

use super::classify::Piece;
use super::{Comments, LeadingPiece};

pub(crate) fn attach_comments(root: &SyntaxNode) -> Comments {
    let tokens: Vec<SyntaxToken> = root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .collect();

    let mut comments = Comments::default();
    let mut prev_sig: Option<SyntaxToken> = None;
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].kind().is_trivia() {
            prev_sig = Some(tokens[i].clone());
            i += 1;
            continue;
        }
        let start = i;
        while i < tokens.len() && tokens[i].kind().is_trivia() {
            i += 1;
        }
        let next_sig = tokens.get(i).cloned();
        process_gap(
            &tokens[start..i],
            prev_sig.as_ref(),
            next_sig.as_ref(),
            &mut comments,
        );
    }

    comments
}

/// Classify one trivia run between `prev_sig` and `next_sig`.
fn process_gap(
    run: &[SyntaxToken],
    prev_sig: Option<&SyntaxToken>,
    next_sig: Option<&SyntaxToken>,
    c: &mut Comments,
) {
    // Newlines seen so far in the gap (total), and since the last recorded piece
    // / the preceding significant token (for blank-line metadata).
    let mut newlines_total = 0u16;
    let mut newlines_since_last = 0u16;

    for tok in run {
        if tok.kind() == tctidy_syntax::SyntaxKind::NEWLINE {
            newlines_total = newlines_total.saturating_add(1);
            newlines_since_last = newlines_since_last.saturating_add(1);
            continue;
        }
        let Some(piece) = Piece::from_token(tok) else {
            continue; // whitespace
        };

        if let (0, Some(ps)) = (newlines_total, prev_sig) {
            // Same line as the preceding code → trailing of it.
            c.trailing.entry(ps.clone()).or_default().push(piece);
            c.count += 1;
        } else if let Some(ns) = next_sig {
            let blank_before = newlines_since_last.saturating_sub(1);
            c.leading.entry(ns.clone()).or_default().push(LeadingPiece {
                piece,
                blank_before,
            });
            c.count += 1;
            newlines_since_last = 0;
        } else {
            let blank_before = newlines_since_last.saturating_sub(1);
            c.dangling.push(LeadingPiece {
                piece,
                blank_before,
            });
            c.count += 1;
            newlines_since_last = 0;
        }
    }

    // Blank lines between the last piece (or prev_sig) and next_sig.
    if let Some(ns) = next_sig {
        let blanks = newlines_since_last.saturating_sub(1);
        if blanks > 0 {
            c.blanks_before.insert(ns.clone(), blanks);
        }
    }
}
