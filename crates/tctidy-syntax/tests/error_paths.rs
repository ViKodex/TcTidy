//! The other half of the parser contract: malformed input must report errors,
//! keep the tree lossless, and never panic. Every existing parser test asserts
//! `errors().is_empty()`; these assert the opposite on purpose.

use tctidy_syntax::{parse_declaration, parse_implementation};

fn assert_decl_errors(src: &str) {
    let p = parse_declaration(src);
    assert!(p.debug_text_matches(src), "decl round-trip failed:\n{src}");
    assert!(!p.errors().is_empty(), "expected decl errors in:\n{src}");
}

fn assert_impl_errors(src: &str) {
    let p = parse_implementation(src);
    assert!(p.debug_text_matches(src), "impl round-trip failed:\n{src}");
    assert!(!p.errors().is_empty(), "expected impl errors in:\n{src}");
}

#[test]
fn declaration_with_missing_type() {
    assert_decl_errors("VAR x : ; END_VAR");
}

#[test]
fn declaration_missing_end_var() {
    assert_decl_errors("VAR x : INT;");
}

#[test]
fn if_without_then() {
    assert_impl_errors("IF a\n    b := 1;\nEND_IF;");
}

#[test]
fn for_without_to() {
    assert_impl_errors("FOR i := 0 DO\n    x := 1;\nEND_FOR;");
}

#[test]
fn for_without_do() {
    assert_impl_errors("FOR i := 0 TO 10\n    x := 1;\nEND_FOR;");
}

#[test]
fn while_without_do() {
    assert_impl_errors("WHILE a\n    x := 1;\nEND_WHILE;");
}

#[test]
fn repeat_without_until() {
    assert_impl_errors("REPEAT\n    x := 1;\nEND_REPEAT;");
}

#[test]
fn dangling_dotted_name() {
    // `EXTENDS base.` with nothing after the dot exercises the dotted-name
    // recovery in grammar/mod.rs.
    assert_decl_errors("FUNCTION_BLOCK fb EXTENDS base.\n");
}

#[test]
fn type_with_missing_name() {
    assert_decl_errors("TYPE : STRUCT x : INT; END_STRUCT END_TYPE");
}

#[test]
fn type_missing_end_type() {
    assert_decl_errors("TYPE t : STRUCT x : INT; END_STRUCT");
}

#[test]
fn parse_ok_returns_the_tree_on_a_clean_parse() {
    let tree = parse_implementation("x := 1;").ok();
    assert!(tree.is_ok());
}

#[test]
fn parse_ok_returns_the_errors_on_a_broken_parse() {
    let errors = parse_implementation("IF a\n    b := 1;\nEND_IF;")
        .ok()
        .expect_err("broken input must not yield a tree");
    assert!(!errors.is_empty());
}

#[test]
fn incomplete_or_separated_typed_unicode_literals_are_errors() {
    for literal in [
        "UTF8#'unterminated",
        "UCHAR#'unterminated",
        "UTF8# 'text'",
        "UCHAR#\"text\"",
    ] {
        assert_impl_errors(&format!("x := {literal};"));
        assert_decl_errors(&format!("VAR\n    x : STRING := {literal};\nEND_VAR"));
    }
}

#[test]
fn malformed_generic_constant_declarations_are_errors() {
    for source in [
        "VAR_GENERIC\n    n : UDINT := 1;\nEND_VAR",
        "VAR\n    fb : FB_Sample<>;\nEND_VAR",
        "VAR\n    fb : FB_Sample<100;\nEND_VAR",
        "VAR\n    fb : FB_Sample<2 * cConst>;\nEND_VAR",
    ] {
        assert_decl_errors(source);
    }
}
