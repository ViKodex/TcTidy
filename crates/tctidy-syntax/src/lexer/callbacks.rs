//! Hand-written scanners for the token shapes `logos`' regex engine cannot
//! express: depth-counting nested comments, verbatim brace-matched pragmas, and
//! `$`-escaped strings.
//!
//! These standalone helpers return the byte length consumed and are wired into
//! [`super::logos_token`] as `logos` callbacks. Keeping the scanning logic in
//! plain functions also makes the depth and escape behavior independently
//! testable.

/// Length (in bytes, from just after the opening `(*`) of a possibly-nested
/// `(* ... *)` comment, including the closing `*)`. On an unterminated comment,
/// returns the remaining length (lossless: the bytes are still consumed).
pub(crate) fn scan_paren_block_comment(rest: &str) -> usize {
    scan_nested(rest, b'(', b'*', b'*', b')')
}

/// Same as [`scan_paren_block_comment`] for `/* ... */`, which nests like `(* *)`.
pub(crate) fn scan_c_block_comment(rest: &str) -> usize {
    scan_nested(rest, b'/', b'*', b'*', b'/')
}

fn scan_nested(rest: &str, o0: u8, o1: u8, c0: u8, c1: u8) -> usize {
    let b = rest.as_bytes();
    let mut depth = 1usize;
    let mut i = 0usize;
    while i + 1 < b.len() {
        if b[i] == o0 && b[i + 1] == o1 {
            depth += 1;
            i += 2;
        } else if b[i] == c0 && b[i + 1] == c1 {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    rest.len()
}

/// Length of a `{ ... }` pragma from just after the opening `{`, brace-matched
/// and quote-aware (so a `}` inside a string does not close it early). Multi-line
/// and copied verbatim by the formatter.
pub(crate) fn scan_pragma(rest: &str) -> usize {
    let b = rest.as_bytes();
    let mut depth = 1usize;
    let mut i = 0usize;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == b'$' && i + 1 < b.len() {
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'\'' | b'"' => quote = Some(c),
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    rest.len()
}

/// Length of a `$`-escaped string body from just after the opening `quote`,
/// including the closing quote. Stops at a bare newline (ST strings are
/// single-line), still lossless.
pub(crate) fn scan_string(rest: &str, quote: u8) -> usize {
    scan_string_with_status(rest, quote).0
}

/// Whether a scanned string body contains an unescaped closing quote.
pub(crate) fn is_terminated_string(rest: &str, quote: u8) -> bool {
    scan_string_with_status(rest, quote).1
}

fn scan_string_with_status(rest: &str, quote: u8) -> (usize, bool) {
    let b = rest.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'$' if i + 1 < b.len() => i += 2,
            c if c == quote => return (i + 1, true),
            b'\n' | b'\r' => return (i, false),
            _ => i += 1,
        }
    }
    (rest.len(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_paren_comment() {
        // "inner (* x *) done *)" after the outer "(*"
        let rest = "inner (* x *) done *) after";
        let n = scan_paren_block_comment(rest);
        assert_eq!(&rest[..n], "inner (* x *) done *)");
    }

    #[test]
    fn pragma_with_brace_in_string() {
        let rest = "attribute 'a}b'} rest";
        let n = scan_pragma(rest);
        assert_eq!(&rest[..n], "attribute 'a}b'}");
    }

    #[test]
    fn string_with_escaped_quote() {
        let rest = "a$'b' rest";
        let n = scan_string(rest, b'\'');
        assert_eq!(&rest[..n], "a$'b'");
    }

    // Unterminated input: every scanner must consume to end of input (lossless),
    // never past it, never panic.

    #[test]
    fn unterminated_paren_comment_consumes_to_eof() {
        let rest = "never closed";
        assert_eq!(scan_paren_block_comment(rest), rest.len());
    }

    #[test]
    fn unterminated_nested_paren_comment_consumes_to_eof() {
        // After the outer "(*": the inner pair closes, the outer never does.
        let rest = "outer (* inner *)";
        assert_eq!(scan_paren_block_comment(rest), rest.len());
    }

    #[test]
    fn unterminated_c_comment_consumes_to_eof() {
        let rest = "never closed";
        assert_eq!(scan_c_block_comment(rest), rest.len());
    }

    #[test]
    fn unterminated_pragma_consumes_to_eof() {
        let rest = "unclosed pragma";
        assert_eq!(scan_pragma(rest), rest.len());
    }

    #[test]
    fn unterminated_pragma_with_open_quote_consumes_to_eof() {
        // The quote never closes, so the final `}` (if any) would not count;
        // here there is none and the scan must stop at end of input.
        let rest = "msg 'unclosed";
        assert_eq!(scan_pragma(rest), rest.len());
    }

    #[test]
    fn unterminated_string_consumes_to_eof() {
        let rest = "unterminated string";
        assert_eq!(scan_string(rest, b'\''), rest.len());
    }

    #[test]
    fn string_stops_before_a_bare_newline() {
        // ST strings are single-line: the newline is not part of the token.
        let rest = "no close\nx := 1;";
        assert_eq!(&rest[..scan_string(rest, b'\'')], "no close");
    }
}
