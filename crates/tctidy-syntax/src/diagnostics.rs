//! Rendering parse errors as a compact list of the source lines they occur on.
//!
//! The parser records each [`SyntaxError`] at a single byte offset (its `range`
//! is zero-width). That offset is the position of the *next un-emitted token*,
//! which, because leading trivia is emitted before its token, lands on the
//! whitespace/newline **before** the offending token. So [`anchor`] moves forward to
//! the next significant character before taking its line, otherwise an error just
//! after a line break would be misreported on the previous line.
//!
//! The output is just the distinct, sorted line numbers (several errors on one
//! line collapse to one entry), capped. Callers add the section header.

use crate::SyntaxError;

/// At most this many lines are listed; the rest collapse into `(+K more)`.
const MAX_LINES: usize = 10;

/// Render the distinct source lines that `errors` occur on, sorted ascending:
/// `line 11` for one, `lines 11, 25, 38` for several, `… (+K more)` past the cap.
/// Returns an empty string when there are no errors.
pub fn render(src: &str, errors: &[SyntaxError]) -> String {
    let mut lines: Vec<usize> = errors
        .iter()
        .map(|e| line_of(src, e.range.start().into()))
        .collect();
    lines.sort_unstable();
    lines.dedup();
    if lines.is_empty() {
        return String::new();
    }

    let label = if lines.len() == 1 { "line" } else { "lines" };
    let shown = lines
        .iter()
        .take(MAX_LINES)
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if lines.len() > MAX_LINES {
        format!("{label} {shown} (+{} more)", lines.len() - MAX_LINES)
    } else {
        format!("{label} {shown}")
    }
}

/// The 1-based source line of the token at or after `offset` (skipping the
/// leading trivia the offset usually points at).
fn line_of(src: &str, offset: usize) -> usize {
    line_at(src, anchor(src, offset))
}

/// First non-whitespace byte at or after `from`, scanning across newlines. When
/// only whitespace remains (trailing whitespace / EOF), clamps back to the last
/// significant character so the line still points at real code.
fn anchor(src: &str, from: usize) -> usize {
    let from = from.min(src.len());
    if let Some((rel, _)) = src[from..].char_indices().find(|(_, c)| !c.is_whitespace()) {
        return from + rel;
    }
    src[..from]
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(0, |(i, _)| i)
}

/// 1-based line number of byte `offset`. `\r\n` counts as one break (only `\n` is
/// counted), so it is CRLF-safe.
fn line_at(src: &str, offset: usize) -> usize {
    let offset = offset.min(src.len());
    src[..offset].matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;
    use text_size::{TextRange, TextSize};

    fn err_at(offset: u32) -> SyntaxError {
        SyntaxError::new(
            ErrorKind::Unexpected,
            "boom",
            TextRange::empty(TextSize::from(offset)),
        )
    }

    #[test]
    fn line_at_counts_newlines_crlf_safe() {
        assert_eq!(line_at("a", 0), 1);
        assert_eq!(line_at("a\nb", 2), 2);
        assert_eq!(line_at("a\nb\nc", 4), 3);
        // CRLF is one line break, not two.
        assert_eq!(line_at("a\r\nb", 3), 2);
        // Clamped past the end.
        assert_eq!(line_at("ab", 99), 1);
    }

    #[test]
    fn anchor_skips_whitespace_to_the_offending_token() {
        // The motivating case: the offset sits on the newline before the token, so
        // the line must be taken from the token, not the blank gap before it.
        assert_eq!(anchor("x;\nEND_IF", 2), 3);
        assert_eq!(anchor("  \n  foo", 0), 5);
        assert_eq!(anchor("abc", 1), 1);
        // Only whitespace ahead → clamp back to the last significant char.
        assert_eq!(anchor("ab  \n  ", 4), 1);
        assert_eq!(anchor("", 0), 0);
    }

    #[test]
    fn line_of_uses_the_token_line_not_the_trivia() {
        // Offset 7 is the '\n' after `x := 1;`; the token END_IF is on line 2.
        assert_eq!(line_of("x := 1;\nEND_IF\n", 7), 2);
    }

    #[test]
    fn render_one_error_names_its_line() {
        assert_eq!(render("x := 1;\nEND_IF\n", &[err_at(7)]), "line 2");
    }

    #[test]
    fn render_dedupes_and_sorts_lines() {
        // Errors on lines 3, 1, 2, 2 → distinct, sorted.
        let src = "a\nbb\nccc";
        let errs = [err_at(5), err_at(0), err_at(3), err_at(2)];
        assert_eq!(render(src, &errs), "lines 1, 2, 3");
    }

    #[test]
    fn render_caps_at_ten_lines() {
        let src = "x\n".repeat(12);
        let errs: Vec<_> = (0..12).map(|i| err_at(i * 2)).collect();
        assert_eq!(
            render(&src, &errs),
            "lines 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 (+2 more)"
        );
    }

    #[test]
    fn render_empty_is_empty() {
        assert_eq!(render("x := 1;", &[]), "");
    }
}
