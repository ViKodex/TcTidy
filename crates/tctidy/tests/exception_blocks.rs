use tctidy::{format_st, FormatOptions, StSection};
use tctidy_syntax::{parse_implementation, SyntaxKind, SyntaxNode};

fn semantic_tokens(node: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| {
            let text = if token.kind().is_keyword() {
                token.text().to_ascii_uppercase()
            } else {
                token.text().to_string()
            };
            (token.kind(), text)
        })
        .collect()
}

fn comments(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| {
            matches!(
                token.kind(),
                SyntaxKind::LINE_COMMENT
                    | SyntaxKind::BLOCK_COMMENT
                    | SyntaxKind::C_BLOCK_COMMENT
                    | SyntaxKind::PRAGMA
            )
        })
        .map(|token| token.text().to_string())
        .collect()
}

fn assert_round_trip(source: &str) -> String {
    let parsed = parse_implementation(source);
    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    let expected_tokens = semantic_tokens(&parsed.syntax_node());
    let expected_comments = comments(&parsed.syntax_node());

    let output = format_st(source, StSection::Implementation, &FormatOptions::default())
        .expect("implementation formats");
    let reparsed = parse_implementation(&output);
    assert!(reparsed.errors().is_empty(), "{:?}", reparsed.errors());
    assert_eq!(semantic_tokens(&reparsed.syntax_node()), expected_tokens);
    assert_eq!(comments(&reparsed.syntax_node()), expected_comments);
    assert_eq!(
        format_st(
            &output,
            StSection::Implementation,
            &FormatOptions::default(),
        )
        .expect("implementation reformats"),
        output
    );
    output
}

#[test]
fn formats_exception_markers_without_semicolons() {
    let output = assert_round_trip("__try\nx:=1;\n__catch(exc)\ny:=2;\n__finally\nz:=3;\n__endtry");
    assert_eq!(
        output,
        "__TRY\n    x := 1;\n__CATCH(exc)\n    y := 2;\n__FINALLY\n    z := 3;\n__ENDTRY"
    );
    for marker in ["__TRY;", "__CATCH(exc);", "__FINALLY;", "__ENDTRY;"] {
        assert!(
            !output.contains(marker),
            "unexpected marker terminator: {output}"
        );
    }
}

#[test]
fn optional_exception_sections_format_independently() {
    for (source, expected) in [
        (
            "__TRY\nx:=1;\n__CATCH(exc)\ny:=2;\n__ENDTRY",
            "__TRY\n    x := 1;\n__CATCH(exc)\n    y := 2;\n__ENDTRY",
        ),
        (
            "__TRY\nx:=1;\n__FINALLY\ny:=2;\n__ENDTRY",
            "__TRY\n    x := 1;\n__FINALLY\n    y := 2;\n__ENDTRY",
        ),
    ] {
        assert_eq!(assert_round_trip(source), expected);
    }
}

#[test]
fn preserves_comments_at_every_exception_boundary() {
    let source = "__TRY // try header\n\
                  // try body\n\
                  x := 1;\n\
                  // before catch\n\
                  __CATCH(exc) // catch header\n\
                  // catch body\n\
                  y := 2;\n\
                  // before finally\n\
                  __FINALLY // finally header\n\
                  // finally body\n\
                  z := 3;\n\
                  // before end\n\
                  __ENDTRY // after end";
    let output = assert_round_trip(source);
    for marker in [
        "// try header",
        "// try body",
        "// before catch",
        "// catch header",
        "// catch body",
        "// before finally",
        "// finally header",
        "// finally body",
        "// before end",
        "// after end",
    ] {
        assert_eq!(
            output.matches(marker).count(),
            1,
            "missing/duplicate {marker}"
        );
    }
}

#[test]
fn nested_exception_blocks_keep_indentation() {
    let output = assert_round_trip(
        "__TRY\n__TRY\nx:=1;\n__CATCH(inner)\ny:=2;\n__ENDTRY\n__CATCH(outer)\nz:=3;\n__ENDTRY",
    );
    assert!(output.contains("    __TRY\n        x := 1;"), "{output}");
    assert!(
        output.contains("    __CATCH(inner)\n        y := 2;"),
        "{output}"
    );
}

#[test]
fn malformed_exception_blocks_are_refused() {
    for source in [
        "__TRY\n    x := 1;\n__CATCH(exc)",
        "__TRY\n    x := 1;\n__CATCH exc\n__ENDTRY",
        "__TRY\n    x := 1;\n__ENDTRY",
    ] {
        assert!(
            format_st(source, StSection::Implementation, &FormatOptions::default()).is_err(),
            "malformed block formatted:\n{source}"
        );
    }
}
