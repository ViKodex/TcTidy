//! Edge cases: the degenerate and awkward inputs a formatter must survive
//! without panicking or mangling. Each case pins the current contract so a
//! regression is visible.

use tctidy::{format_st, FormatOptions, StSection};

fn fmt(src: &str, section: StSection) -> Result<String, tctidy::Error> {
    format_st(src, section, &FormatOptions::default())
}

fn ok(src: &str, section: StSection) -> String {
    fmt(src, section).expect("formats cleanly")
}

#[test]
fn empty_input_is_empty_output() {
    assert_eq!(ok("", StSection::Implementation), "");
    assert_eq!(ok("", StSection::Declaration), "");
}

#[test]
fn whitespace_only_input() {
    // Nothing but blank lines collapses to nothing, no spurious content.
    assert_eq!(ok("\n\n   \n", StSection::Implementation), "");
}

// ---- final newline -----------------------------------------------------------

#[test]
fn final_newline_is_kept_in_the_input_eol() {
    assert_eq!(ok("x:=1;\n", StSection::Implementation), "x := 1;\n");
    assert_eq!(
        ok("x:=1;\r\ny:=2;\r\n", StSection::Implementation),
        "x := 1;\r\ny := 2;\r\n"
    );
    assert_eq!(
        ok("VAR\na:INT;\nEND_VAR\n", StSection::Declaration),
        "VAR\n    a : INT;\nEND_VAR\n"
    );
    // No final newline in, none out.
    assert_eq!(ok("x:=1;", StSection::Implementation), "x := 1;");
}

#[test]
fn trailing_blank_lines_collapse_to_one_final_newline() {
    // Blank lines and stray spaces after the last line break are not content:
    // the line break itself is kept, once.
    for src in ["x:=1;\n\n\n", "x:=1;\n \t", "x:=1;\n\n  \n"] {
        let out = ok(src, StSection::Implementation);
        assert_eq!(out, "x := 1;\n", "{src:?}");
        assert_eq!(ok(&out, StSection::Implementation), out, "not idempotent");
    }
}

#[test]
fn final_newline_after_a_trailing_comment() {
    assert_eq!(
        ok("x:=1; // c\r\n", StSection::Implementation),
        "x := 1; // c\r\n"
    );
    assert_eq!(
        ok("x:=1;\n// d   \n", StSection::Implementation),
        "x := 1;\n// d\n"
    );
}

#[test]
fn final_newline_follows_the_detected_eol() {
    // The first line break decides the EOL of the whole output, the final
    // line break included.
    assert_eq!(
        ok("x:=1;\ny:=2;\r\n", StSection::Implementation),
        "x := 1;\ny := 2;\n"
    );
}

#[test]
fn comment_only_body_keeps_no_leading_blank() {
    // A body that is *only* a comment must not gain a leading blank line.
    assert_eq!(
        ok("// just a note", StSection::Implementation),
        "// just a note"
    );
    assert_eq!(ok("(* block *)", StSection::Implementation), "(* block *)");
    assert_eq!(ok("// a\n// b", StSection::Implementation), "// a\n// b");
}

#[test]
fn empty_var_block() {
    assert_eq!(ok("VAR\nEND_VAR", StSection::Declaration), "VAR\nEND_VAR");
}

#[test]
fn optional_semicolon_after_end_var_is_preserved() {
    let source = "VAR_GLOBAL\nx:BOOL;\nEND_VAR;";
    let expected = "VAR_GLOBAL\n    x : BOOL;\nEND_VAR;";
    let output = ok(source, StSection::Declaration);

    assert_eq!(output, expected);
    assert_eq!(ok(&output, StSection::Declaration), output);
}

#[test]
fn multiline_pragma_is_preserved_verbatim() {
    // Pragmas are reproduced byte-for-byte (their interior is opaque), so a
    // pragma split across lines keeps its own line breaks.
    let src = "VAR\n{attribute 'foo' :=\n'bar'}\nx : INT;\nEND_VAR";
    let out = ok(src, StSection::Declaration);
    assert!(
        out.contains("{attribute 'foo' :=\n'bar'}"),
        "pragma altered: {out:?}"
    );
    // and it stays parse-clean + idempotent
    assert_eq!(ok(&out, StSection::Declaration), out);
}

#[test]
fn deep_nesting_does_not_overflow() {
    const DEPTH: usize = 80;
    let src = format!(
        "{}a := 1;{}",
        "IF a THEN\n".repeat(DEPTH),
        "\nEND_IF;".repeat(DEPTH)
    );
    let out = ok(&src, StSection::Implementation);
    // innermost statement indented DEPTH levels of 4 spaces
    assert!(out.contains(&format!("{}a := 1;", " ".repeat(DEPTH * 4))));
    assert_eq!(
        ok(&out, StSection::Implementation),
        out,
        "deep nesting not idempotent"
    );
}

#[test]
fn very_long_binary_line_wraps_and_is_idempotent() {
    // Binary chains are width-sensitive and wrap at the configured default.
    let chain = (0..40)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(" + ");
    let src = format!("x := {chain};");
    let out = ok(&src, StSection::Implementation);
    assert!(out.lines().count() > 1, "long line was not split: {out:?}");
    assert_eq!(ok(&out, StSection::Implementation), out);
}

#[test]
fn unicode_identifiers_are_refused_not_mangled() {
    // Non-ASCII identifiers aren't lexed yet. The contract is non-destruction:
    // the parse carries an error, so the formatter REFUSES (Err) and the caller
    // keeps the original byte-for-byte, it never emits a half-formatted result.
    // (Promote to a positive test if/when the lexer gains Unicode identifiers.)
    assert!(fmt("café := 1;", StSection::Implementation).is_err());
    assert!(fmt("Δx := 2;", StSection::Implementation).is_err());
}

// ---- blank lines at the start of a block -----------------------------------

#[test]
fn empty_block_body_has_no_spurious_blank() {
    // No statements at all: nothing to collapse, nothing added.
    assert_eq!(
        ok("IF c THEN\nEND_IF;", StSection::Implementation),
        "IF c THEN\nEND_IF;"
    );
}

#[test]
fn block_body_of_only_blanks_collapses() {
    // No statement: the blanks belong to the END_IF tail and now collapse to
    // none (blank_lines_at_block_end = 0). Idempotent.
    let out = ok("IF c THEN\n\n\nEND_IF;", StSection::Implementation);
    assert_eq!(out, "IF c THEN\nEND_IF;");
    assert_eq!(
        ok(&out, StSection::Implementation),
        out,
        "not idempotent: {out:?}"
    );
}

#[test]
fn comment_only_block_body_keeps_its_comment() {
    // A comment with no statement attaches to END_IF; it survives and gains no
    // leading blank line.
    assert_eq!(
        ok("IF c THEN\n// note\nEND_IF;", StSection::Implementation),
        "IF c THEN\n    // note\nEND_IF;"
    );
}

#[test]
fn block_start_collapse_respects_blank_lines_max_between_statements() {
    // The first statement's leading blanks collapse to none, while an ordinary
    // run between two statements stays capped at blank_lines_max (2).
    let out = ok(
        "IF c THEN\n\n\n\n\n\nx := 1;\n\n\n\n\n\ny := 2;\nEND_IF;",
        StSection::Implementation,
    );
    assert_eq!(out, "IF c THEN\n    x := 1;\n\n\n    y := 2;\nEND_IF;");
    assert_eq!(ok(&out, StSection::Implementation), out, "not idempotent");
}

// ---- blank lines at the end of a block -------------------------------------

#[test]
fn blanks_before_end_var_collapse_and_idempotent() {
    let out = ok("VAR\nx : INT;\n\n\nEND_VAR", StSection::Declaration);
    assert_eq!(out, "VAR\n    x : INT;\nEND_VAR");
    assert_eq!(ok(&out, StSection::Declaration), out, "not idempotent");
}

#[test]
fn blanks_before_end_struct_collapse_and_idempotent() {
    let out = ok(
        "TYPE t :\nSTRUCT\nx : INT;\n\n\nEND_STRUCT\nEND_TYPE",
        StSection::Declaration,
    );
    assert_eq!(out, "TYPE t :\nSTRUCT\n    x : INT;\nEND_STRUCT\nEND_TYPE");
    assert_eq!(ok(&out, StSection::Declaration), out, "not idempotent");
}

#[test]
fn comment_before_closer_survives_without_blank() {
    // Statement side: the blank above the trailing comment collapses; the
    // comment keeps its place at the body indent.
    assert_eq!(
        ok(
            "IF a THEN\nx := 1;\n\n// trailing\nEND_IF;",
            StSection::Implementation
        ),
        "IF a THEN\n    x := 1;\n    // trailing\nEND_IF;"
    );
    // Declaration side: same, at member indent.
    assert_eq!(
        ok(
            "VAR\nx : INT;\n\n// trailing\nEND_VAR",
            StSection::Declaration
        ),
        "VAR\n    x : INT;\n    // trailing\nEND_VAR"
    );
}

#[test]
fn block_end_collapse_respects_blank_lines_max_between_statements() {
    // Trailing blanks before END_IF collapse to none, while an ordinary run
    // between two statements stays capped at blank_lines_max (2).
    let out = ok(
        "IF c THEN\nx := 1;\n\n\n\n\n\ny := 2;\n\n\n\n\n\nEND_IF;",
        StSection::Implementation,
    );
    assert_eq!(out, "IF c THEN\n    x := 1;\n\n\n    y := 2;\nEND_IF;");
    assert_eq!(ok(&out, StSection::Implementation), out, "not idempotent");
}

#[test]
fn end_of_body_dangling_comment_keeps_its_blank() {
    // The top-level end-of-file dangling comment is NOT a block-end blank: the
    // blank line above it is intentional and preserved (guards the block-end
    // collapse from over-reaching).
    assert_eq!(
        ok(
            "IF a THEN\nx := 1;\nEND_IF;\n\n// note",
            StSection::Implementation
        ),
        "IF a THEN\n    x := 1;\nEND_IF;\n\n// note"
    );
}

// ---- trailing whitespace in line comments ----------------------------------

#[test]
fn line_comment_trailing_whitespace_is_trimmed() {
    assert_eq!(
        ok("// note   \nx := 1;", StSection::Implementation),
        "// note\nx := 1;"
    );
    assert_eq!(
        ok("x := 1; // c   ", StSection::Implementation),
        "x := 1; // c"
    );
    // An only-whitespace comment collapses to a bare `//`.
    assert_eq!(
        ok("//   \nx := 1;", StSection::Implementation),
        "//\nx := 1;"
    );
}

#[test]
fn dangling_comment_trailing_whitespace_is_trimmed() {
    // A trailing comment at the very end of the body (dangling) is trimmed too.
    assert_eq!(
        ok("x := 1;\n// d   ", StSection::Implementation),
        "x := 1;\n// d"
    );
}

#[test]
fn declaration_trailing_comment_whitespace_is_trimmed() {
    assert_eq!(
        ok("VAR\nx : INT; // c   \nEND_VAR", StSection::Declaration),
        "VAR\n    x : INT; // c\nEND_VAR"
    );
}

#[test]
fn block_comment_and_pragma_are_left_verbatim() {
    // Block comments and pragmas stay byte-for-byte; only line comments are
    // trimmed. A block comment's interior spacing is opaque content.
    assert_eq!(
        ok("(* a   *)\nx := 1;", StSection::Implementation),
        "(* a   *)\nx := 1;"
    );
    let p = ok(
        "VAR\n{attribute 'foo'}\nx : INT;\nEND_VAR",
        StSection::Declaration,
    );
    assert!(p.contains("{attribute 'foo'}"), "pragma altered: {p:?}");
}

#[test]
fn trailing_trim_mixed_is_idempotent() {
    let out = ok(
        "IF a THEN\nx := 1; // c   \n// note\t\nEND_IF;",
        StSection::Implementation,
    );
    assert_eq!(
        ok(&out, StSection::Implementation),
        out,
        "not idempotent: {out:?}"
    );
}
