//! Unified-diff rendering for `--diff` (via the `similar` crate).

/// A unified diff between `old` and `new`, labelled with the file path (or
/// `<stdin>`). Empty when the two are identical.
pub fn unified(label: &str, old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .header(&format!("a/{label}"), &format!("b/{label}"))
        .to_string()
}
