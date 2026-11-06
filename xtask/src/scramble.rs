//! Semantics-preserving "scramble" of Structured Text.
//!
//! Given clean ST, produce a messy variant that parses to the *same* tree modulo
//! trivia, so a correct formatter must recover the clean original. The scramble:
//!
//! * rewrites horizontal whitespace (indentation, inter-token spacing, column
//!   alignment) with random amounts of spaces/tabs;
//! * lower/mixed-cases keyword tokens (the formatter normalizes their case);
//! * preserves the number of newlines in every gap (so comment attachment and
//!   blank-line structure are unchanged), only inflating runs that already
//!   contain a blank line (which the formatter caps anyway);
//! * never touches the interior of comments, pragmas, or string literals.
//!
//! It is deterministic given a seed, so the same file always scrambles the same way.
//!
//! The corpus tests include this file too (`crates/tctidy/tests/support`), so the
//! canonicalization invariant and `xtask scramble` produce the same messy files.

use tctidy_syntax::kind::keywords::keyword_kind;
use tctidy_syntax::SyntaxKind;

/// Keywords the parser reads as names (`ADR(x)`, `SIZEOF(x)`, `THIS^`): the
/// formatter keeps their case as written, so the scramble must not change it.
const CASE_PRESERVED: [SyntaxKind; 6] = [
    SyntaxKind::ADR_KW,
    SyntaxKind::SIZEOF_KW,
    SyntaxKind::XSIZEOF_KW,
    SyntaxKind::INDEXOF_KW,
    SyntaxKind::THIS_KW,
    SyntaxKind::SUPER_KW,
];

/// Scramble every ST section of a TwinCAT document. The seed comes from `key` (the
/// file's path relative to the corpus root), so a file always scrambles the same
/// way. A section whose scramble would change more than whitespace and keyword case
/// is kept as is; the second value counts those sections.
pub fn scramble_document(
    bytes: &[u8],
    key: &str,
) -> Result<(Vec<u8>, u32), tctidy_twincat::TwincatError> {
    let seed = fnv1a(key);
    let mut counter: u64 = 0;
    let mut kept = 0u32;
    let out = tctidy_twincat::reformat_document(bytes, |st, _cx| {
        let s = scramble(st, seed.wrapping_add(counter.wrapping_mul(0x9E3779B1)));
        counter += 1;
        // Safety net: scrambling must only change whitespace and keyword case.
        if normalized(&s) != normalized(st) {
            kept += 1;
            return Ok(st.to_string());
        }
        Ok(s)
    })?;
    Ok((out, kept))
}

fn fnv1a(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Small deterministic PRNG (LCG). Reproducible across runs.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    /// A value in `0..n` (n > 0).
    fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

enum Item<'a> {
    /// Comment / pragma / string, emitted verbatim, never altered.
    Opaque(&'a str),
    /// A whitespace run, replaced by a scrambled run preserving newline count.
    Ws(&'a str),
    /// A run of code (identifiers, operators, literals), emitted verbatim,
    /// except a chunk that is exactly one keyword, whose case is scrambled.
    Chunk(&'a str),
}

/// Scramble ST `src` deterministically from `seed`.
pub fn scramble(src: &str, seed: u64) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let items = tokenize(src);
    let mut rng = Lcg::new(seed);
    let mut out = String::with_capacity(src.len() + src.len() / 4);
    // A `//` line comment runs to the newline, so any trailing whitespace injected
    // before that newline would be lexed *into* the comment token,
    // corrupting it (the clean file keeps the comment byte-exact). Track it.
    let mut after_line_comment = false;
    for item in &items {
        match item {
            Item::Opaque(t) => {
                out.push_str(t);
                after_line_comment = t.starts_with("//");
            }
            Item::Chunk(t) => {
                if keyword_kind(t).is_some_and(|kind| !CASE_PRESERVED.contains(&kind)) {
                    out.push_str(&recase(t, &mut rng));
                } else {
                    out.push_str(t);
                }
                after_line_comment = false;
            }
            Item::Ws(w) => {
                out.push_str(&scramble_ws(w, eol, &mut rng, after_line_comment));
                after_line_comment = false;
            }
        }
    }
    out
}

/// Content-integrity invariant: scrambling changes only whitespace and the case
/// of keyword tokens, so the non-whitespace text, lowercased, is unchanged.
pub fn normalized(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn tokenize(s: &str) -> Vec<Item<'_>> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let mut items = Vec::new();
    while i < b.len() {
        let c = b[i];
        if is_ws(c) {
            let start = i;
            while i < b.len() && is_ws(b[i]) {
                i += 1;
            }
            items.push(Item::Ws(&s[start..i]));
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            let start = i;
            i += 2;
            while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
                i += 1;
            }
            items.push(Item::Opaque(&s[start..i]));
        } else if c == b'(' && i + 1 < b.len() && b[i + 1] == b'*' {
            let start = i;
            i += 2;
            i += scan_nested(&s[i..], b'(', b'*', b'*', b')');
            items.push(Item::Opaque(&s[start..i]));
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let start = i;
            i += 2;
            i += scan_nested(&s[i..], b'/', b'*', b'*', b'/');
            items.push(Item::Opaque(&s[start..i]));
        } else if c == b'{' {
            let start = i;
            i += 1;
            i += scan_pragma(&s[i..]);
            items.push(Item::Opaque(&s[start..i]));
        } else if c == b'\'' || c == b'"' {
            let start = i;
            i += 1;
            i += scan_string(&s[i..], c);
            items.push(Item::Opaque(&s[start..i]));
        } else {
            let start = i;
            while i < b.len() {
                let c = b[i];
                if is_ws(c) {
                    break;
                }
                if c == b'/' && i + 1 < b.len() && (b[i + 1] == b'/' || b[i + 1] == b'*') {
                    break;
                }
                if c == b'(' && i + 1 < b.len() && b[i + 1] == b'*' {
                    break;
                }
                if c == b'{' || c == b'\'' || c == b'"' {
                    break;
                }
                i += 1;
            }
            items.push(Item::Chunk(&s[start..i]));
        }
    }
    items
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

/// Replace a whitespace run, preserving the exact newline count (so blank-line
/// structure is byte-recoverable, the formatter normalizes blanks itself).
/// Horizontal whitespace is randomized.
fn scramble_ws(w: &str, eol: &str, rng: &mut Lcg, after_line_comment: bool) -> String {
    let newlines = w.bytes().filter(|&b| b == b'\n').count();
    if newlines == 0 {
        // Inline gap between two tokens on the same line, keep >= 1 space so
        // tokens never merge. Wide range: the formatter collapses to one space.
        return horizontal(rng, 1, 9);
    }
    // Trailing whitespace of the previous line, but never after a `//` comment,
    // where it would be absorbed into the comment token.
    let mut s = if after_line_comment {
        String::new()
    } else {
        horizontal(rng, 0, 8)
    };
    // Preserve the exact number of line breaks (do not inflate blank runs): the
    // formatter is responsible for any blank-line capping, and the golden must be
    // byte-recoverable.
    let emit = newlines;
    for _ in 0..emit {
        s.push_str(eol);
        s.push_str(&horizontal(rng, 0, 28)); // indentation of the next line (heavy)
    }
    s
}

/// A scrambled run of horizontal whitespace, at least `min` columns wide.
fn horizontal(rng: &mut Lcg, min: u32, max_extra: u32) -> String {
    let n = min + rng.below(max_extra + 1);
    if n > 0 && rng.below(3) == 0 {
        // occasionally use tabs to exercise tab→space conversion
        "\t".repeat(((n / 4) + 1) as usize)
    } else {
        " ".repeat(n as usize)
    }
}

/// Lower- or mixed-case a keyword token (the formatter restores canonical case).
fn recase(t: &str, rng: &mut Lcg) -> String {
    if rng.below(2) == 0 {
        t.to_ascii_lowercase()
    } else {
        t.chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect()
    }
}

// --- opaque-run scanners (lengths measured from just after the opener) ---

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

fn scan_pragma(rest: &str) -> usize {
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

fn scan_string(rest: &str, quote: u8) -> usize {
    let b = rest.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'$' if i + 1 < b.len() => i += 2,
            c if c == quote => return i + 1,
            b'\n' | b'\r' => return i,
            _ => i += 1,
        }
    }
    rest.len()
}
