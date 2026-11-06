//! Verbatim emission helper.
//!
//! Reproduces a source span byte-for-byte as a `Doc`: multi-line content (pragma
//! interiors, `pytmc` is whitespace-sensitive, and block comments) is split on
//! existing newlines and rejoined with `HardLiteral` lines so the interior is
//! never re-indented. Used by the comment rules and the verbatim fallback for
//! unhandled / `ERROR` nodes.

use tctidy_pretty::prelude::*;

/// Emit `src` verbatim. Newlines become `HardLiteral` breaks (no re-indent);
/// the carriage return of a CRLF pair is dropped from the text fragment because
/// the printer re-emits the document's configured EOL.
pub(crate) fn verbatim(src: &str) -> Doc {
    let mut parts = Vec::new();
    for (i, line_text) in src.split('\n').enumerate() {
        if i > 0 {
            parts.push(hard_literal());
        }
        let trimmed = line_text.strip_suffix('\r').unwrap_or(line_text);
        if !trimmed.is_empty() {
            parts.push(text(trimmed));
        }
    }
    concat(parts)
}
