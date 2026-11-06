//! Splice formatted ST back into the original byte buffer.
//!
//! Byte-identity of everything outside the edited ranges is a *theorem* here, not
//! an aspiration: the gaps between edits are copied verbatim from the original,
//! so if every replacement equals the bytes it replaces, the output equals the
//! input exactly.

use std::ops::Range;

/// Apply non-overlapping, source-ordered `edits` to `original`.
pub(crate) fn splice(original: &[u8], edits: &[(Range<usize>, String)]) -> Vec<u8> {
    debug_assert!(
        edits.windows(2).all(|w| w[0].0.end <= w[1].0.start),
        "edits must be ordered and non-overlapping"
    );
    let mut out = Vec::with_capacity(original.len());
    let mut cursor = 0usize;
    for (range, replacement) in edits {
        out.extend_from_slice(&original[cursor..range.start]);
        out.extend_from_slice(replacement.as_bytes());
        cursor = range.end;
    }
    out.extend_from_slice(&original[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_replacement_is_byte_identical() {
        let original = b"abcDEFghi";
        let edits = vec![(3..6, "DEF".to_string())];
        assert_eq!(splice(original, &edits), original);
    }

    #[test]
    fn replacement_changes_only_the_range() {
        let original = b"abcDEFghi";
        let edits = vec![(3..6, "XY".to_string())];
        assert_eq!(splice(original, &edits), b"abcXYghi");
    }

    #[test]
    fn two_edits_preserve_the_gap_between_them() {
        let original = b"aaBBccDDee";
        let edits = vec![(2..4, "XX".to_string()), (6..8, "YY".to_string())];
        assert_eq!(splice(original, &edits), b"aaXXccYYee");
    }

    #[test]
    fn three_edits_of_different_lengths() {
        let original = b"0A1BB2CCC3";
        let edits = vec![
            (1..2, "xx".to_string()),
            (3..5, "y".to_string()),
            (6..9, String::new()),
        ];
        assert_eq!(splice(original, &edits), b"0xx1y23");
    }

    #[test]
    fn adjacent_edits_do_not_duplicate_or_drop_bytes() {
        let original = b"aaBBCCdd";
        let edits = vec![(2..4, "x".to_string()), (4..6, "y".to_string())];
        assert_eq!(splice(original, &edits), b"aaxydd");
    }

    #[test]
    fn empty_replacement_deletes_the_range() {
        let original = b"keepDELETEkeep";
        let edits = vec![(4..10, String::new())];
        assert_eq!(splice(original, &edits), b"keepkeep");
    }
}
