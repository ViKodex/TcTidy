//! House-style normalization of comment *spacing* (content is otherwise verbatim).
//!
//! A line comment gets exactly one space after the `//`: a missing space is added
//! (`//x` → `// x`) and an excess run of spaces/tabs is collapsed (`//   x` → `// x`,
//! `//\tx` → `// x`). Internal spacing, empty `//`, and block comments are left
//! untouched.

use tctidy_format::{format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::parse_implementation;

fn imp(src: &str) -> String {
    format_implementation(
        &parse_implementation(src),
        &StyleConfig::default(),
        LineEnding::Lf,
    )
    .expect("formats")
}

#[test]
fn adds_one_space_after_double_slash() {
    assert_eq!(
        imp("x := 1; //Low tank level alarm"),
        "x := 1; // Low tank level alarm"
    );
    assert_eq!(imp("//header\nx := 1;"), "// header\nx := 1;");
}

#[test]
fn collapses_excess_leading_whitespace() {
    // A long run of spaces after `//` collapses to exactly one.
    assert_eq!(
        imp("//         This is a comment\nx := 1;"),
        "// This is a comment\nx := 1;"
    );
    // Trailing comment, two spaces → one.
    assert_eq!(imp("x := 1; //  two"), "x := 1; // two");
    // A leading tab is collapsed to a single space too.
    assert_eq!(imp("x := 1; //\ttab"), "x := 1; // tab");
    // Only the *leading* run is collapsed; internal spacing is preserved.
    assert_eq!(imp("x := 1; //  a  b"), "x := 1; // a  b");
}

#[test]
fn leaves_canonical_and_block_comments_untouched() {
    // Already exactly one space after `//`: nothing to do.
    assert_eq!(
        imp("x := 1; // already spaced"),
        "x := 1; // already spaced"
    );
    // The rule applies only to `//` line comments, never to empty comments or
    // block comments.
    assert_eq!(imp("x := 1; //"), "x := 1; //");
    assert_eq!(imp("x := a (*inline*) AND b;"), "x := a (*inline*) AND b;");
}

#[test]
fn idempotent() {
    let once = imp("a := 1; //no space");
    assert_eq!(imp(&once), once);
    let collapsed = imp("a := 1; //    spaced");
    assert_eq!(imp(&collapsed), collapsed);
}

#[test]
fn trims_trailing_whitespace() {
    // Trailing spaces/tabs in line comments are stripped, own-line and trailing
    // alike.
    assert_eq!(imp("// note   \nx := 1;"), "// note\nx := 1;");
    assert_eq!(imp("x := 1; // c   "), "x := 1; // c");
    assert_eq!(imp("// note\t\t\nx := 1;"), "// note\nx := 1;");
    // A comment that is only `//` plus whitespace collapses to a bare `//`.
    assert_eq!(imp("//   \nx := 1;"), "//\nx := 1;");
    // Internal spacing is preserved; only the trailing run is trimmed.
    assert_eq!(imp("x := 1; // a  b   "), "x := 1; // a  b");
}

#[test]
fn trailing_trim_is_idempotent() {
    let once = imp("// note   \nx := 1; // c\t");
    assert_eq!(imp(&once), once);
}

#[test]
fn no_line_comment_leaves_trailing_whitespace() {
    for src in [
        "// a   \nx := 1;",
        "//   \nx := 1;",
        "// a\t\t\nx := 1;",
        "x := 1; // c   ",
        "//b  \n//c  \nx := 1;",
    ] {
        let out = imp(src);
        for line in out.lines() {
            assert!(
                !line.ends_with(' ') && !line.ends_with('\t'),
                "trailing whitespace in output of {src:?}:\n{out:?}"
            );
        }
    }
}

#[test]
fn trailing_trim_applies_even_without_space_after() {
    // Trailing-whitespace trim is unconditional: it runs even when the
    // space-after-`//` rule is disabled (leading spacing is then left as-is).
    let style = StyleConfig {
        space_after_line_comment: false,
        ..StyleConfig::default()
    };
    let out = format_implementation(
        &parse_implementation("//note   \nx := 1;"),
        &style,
        LineEnding::Lf,
    )
    .expect("formats");
    assert_eq!(out, "//note\nx := 1;");
}
