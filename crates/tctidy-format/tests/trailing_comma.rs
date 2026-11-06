//! Trailing-separator behavior for argument / initializer lists, driven by
//! `StyleConfig::trailing_comma_collection`.
//!
//! The snapshots (`snapshots.rs`) pin the *house style* output (the separator is
//! dropped); the "no style leaked" test (`style_leak.rs`) proves the field is
//! actually read. This file locks the two invariants those don't cover:
//! idempotence under *both* policies, and that a flat single-line list never
//! gains a separator regardless of policy ("when broken" really means broken).

use tctidy_format::style::TrailingSeparator;
use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_declaration, parse_implementation};

fn dec_with(src: &str, style: &StyleConfig) -> String {
    format_declaration(&parse_declaration(src), style, LineEnding::Lf).expect("declaration formats")
}

fn imp_with(src: &str, style: &StyleConfig) -> String {
    format_implementation(&parse_implementation(src), style, LineEnding::Lf)
        .expect("implementation formats")
}

fn when_broken() -> StyleConfig {
    StyleConfig {
        trailing_comma_collection: TrailingSeparator::WhenBroken,
        ..StyleConfig::default()
    }
}

#[test]
fn never_multiline_is_idempotent() {
    // The default (drop) path, re-run: `format(format(x)) == format(x)`.
    let style = StyleConfig::default();
    for src in [
        "VAR\nx : FB_Foo(\na := 1,\nb := 2\n);\nEND_VAR",
        "VAR\nx : ARRAY[0..2] OF INT := (\n1,\n2,\n3\n);\nEND_VAR",
    ] {
        let once = dec_with(src, &style);
        assert_eq!(once, dec_with(&once, &style), "not idempotent for: {src:?}");
    }
}

#[test]
fn when_broken_emits_and_is_idempotent() {
    // `WhenBroken` adds the trailing comma the default drops; the resulting
    // `,)` must re-format to itself (guards against a `paren_is_multiline`
    // re-break loop).
    let style = when_broken();
    let once = dec_with("VAR\nx : FB_Foo(\na := 1,\nb := 2\n);\nEND_VAR", &style);
    assert!(
        once.contains("b := 2,"),
        "WhenBroken should emit the trailing comma, got:\n{once}"
    );
    assert_eq!(
        once,
        dec_with(&once, &style),
        "WhenBroken must be idempotent"
    );
}

#[test]
fn single_line_list_never_emits_a_trailing_comma() {
    // A flat, single-line list is not "broken", so it never gains a separator,
    // even under `WhenBroken`. (And the redundant source `,)` is still dropped.)
    let style = when_broken();
    assert_eq!(imp_with("foo(a, b,);", &style), "foo(a, b);");
}
