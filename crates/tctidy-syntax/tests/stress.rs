//! Totality on pathological input: the two spots with no upper bound, the
//! `at_case_label` token-by-token lookahead and the `type_ref` recursion, must
//! not panic, must terminate quickly, and must stay lossless.

use tctidy_syntax::{parse_declaration, parse_implementation, SyntaxKind};

#[test]
fn case_arm_with_a_thousand_labels() {
    let labels = (0..1000)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!("CASE n OF\n    {labels}:\n        x := 1;\nEND_CASE;");
    let p = parse_implementation(&src);
    assert!(p.debug_text_matches(&src), "round-trip failed");
    assert!(p.errors().is_empty(), "errors: {:?}", p.errors());
}

/// A label-shaped statement inside an arm body must not be swallowed by the
/// label lookahead: `1 + 2;` starts like a label list but never reaches a `:`.
#[test]
fn non_label_statement_in_case_body_is_not_misread() {
    let src = "CASE n OF\n1:\n    x := 1;\n    1 + 2;\n2:\n    y := 2;\nEND_CASE;";
    let p = parse_implementation(src);
    assert!(p.debug_text_matches(src), "round-trip failed");
    let labels = p
        .syntax_node()
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::CASE_LABEL)
        .count();
    assert_eq!(labels, 2, "the lookahead misread the statement as a label");
}

#[test]
fn deeply_chained_pointer_type() {
    let ty = format!("{}INT", "POINTER TO ".repeat(100));
    let src = format!("VAR\n    p : {ty};\nEND_VAR");
    let p = parse_declaration(&src);
    assert!(p.debug_text_matches(&src), "round-trip failed");
    assert!(p.errors().is_empty(), "errors: {:?}", p.errors());
}

#[test]
fn deeply_nested_array_type() {
    let ty = format!("{}INT", "ARRAY[0..1] OF ".repeat(100));
    let src = format!("VAR\n    a : {ty};\nEND_VAR");
    let p = parse_declaration(&src);
    assert!(p.debug_text_matches(&src), "round-trip failed");
    assert!(p.errors().is_empty(), "errors: {:?}", p.errors());
}
