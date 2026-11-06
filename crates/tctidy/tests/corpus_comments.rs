//! Corpus-wide comment-preservation invariant.
//!
//! The non-destruction promise extends to comments: formatting must never drop a
//! comment or pragma. This walks every corpus file, lexes the ST inside each
//! `<![CDATA[…]]>` section before and after formatting, and asserts no comment
//! token disappears. A file the formatter refuses fails too (unless it is a known
//! failure, see `support`): the output guard refuses rather than drop a comment.
//!
//! Trailing whitespace *inside* a multi-line block comment is normalized away by
//! the line-based trim (a blank comment line `"  "` becomes `""`), and so is the
//! one-space-after-`//` rule (`//x` and `//   x` -> `// x`): both are
//! content-preserving rewrites, not dropped comments. So comments are compared
//! *modulo* those two normalizations, applied to both sides, letting this run at
//! the real styles (the default and the non-default profiles of
//! `support::styles`) while still catching any genuine loss. Skips when there is
//! no corpus.

mod support;

use std::collections::BTreeMap;

use tctidy_syntax::tokens::lex;
use tctidy_syntax::SyntaxKind;

/// Comment/pragma token texts (per-line trailing whitespace stripped) found in
/// every CDATA section of `file`, with multiplicities.
fn cdata_comments(file: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let mut rest = file;
    while let Some(start) = rest.find("<![CDATA[") {
        let after = &rest[start + "<![CDATA[".len()..];
        let Some(end) = after.find("]]>") else { break };
        let st = &after[..end];
        let mut off = 0usize;
        for tok in lex(st) {
            let len: usize = tok.len.into();
            let text = &st[off..off + len];
            if matches!(
                tok.kind,
                SyntaxKind::LINE_COMMENT
                    | SyntaxKind::BLOCK_COMMENT
                    | SyntaxKind::C_BLOCK_COMMENT
                    | SyntaxKind::PRAGMA
            ) {
                *counts.entry(normalize(text)).or_insert(0) += 1;
            }
            off += len;
        }
        rest = &after[end + "]]>".len()..];
    }
    counts
}

/// Normalize a comment's text by the two intentional, content-preserving
/// rewrites the formatter applies, so the comparison ignores them: per-line
/// trailing whitespace, and exactly one space after a line comment's `//` (a
/// missing space is added and an excess run of spaces/tabs is collapsed).
fn normalize(s: &str) -> String {
    let trimmed = s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    match trimmed.strip_prefix("//") {
        Some(rest) => {
            let content = rest.trim_start_matches([' ', '\t']);
            if content.is_empty() {
                trimmed
            } else {
                format!("// {content}")
            }
        }
        None => trimmed,
    }
}

#[test]
fn no_comment_is_dropped_over_the_corpus() {
    let Some(root) = support::corpus_dir("comment preservation") else {
        return;
    };
    // Comment text is compared via `normalize` (above), which folds out the
    // intentional space-after-`//` and trailing-whitespace rewrites, so only a
    // genuinely *lost* comment can fail this.
    let files = support::twincat_files(&root);
    let mut report = support::Report::new("comment preservation");
    for style in support::styles(false) {
        for (path, key) in &files {
            let bytes = std::fs::read(path).expect("read a corpus file");
            let Some(out) = report.format(&style, key, &bytes) else {
                continue;
            };
            let before = cdata_comments(&String::from_utf8_lossy(&bytes));
            let after = cdata_comments(&String::from_utf8_lossy(&out.bytes));
            let dropped: usize = before
                .iter()
                .map(|(c, n)| n.saturating_sub(after.get(c).copied().unwrap_or(0)))
                .sum();
            if dropped == 0 {
                report.pass();
            } else {
                report.fail(&style, key, &format!("{dropped} comments dropped"));
            }
        }
    }
    report.finish();
}
