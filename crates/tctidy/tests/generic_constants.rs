use tctidy::{format_st, FormatOptions, StSection};
use tctidy_syntax::{parse_declaration, SyntaxKind, SyntaxNode};

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
    let parsed = parse_declaration(source);
    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    let expected_tokens = semantic_tokens(&parsed.syntax_node());
    let expected_comments = comments(&parsed.syntax_node());

    let output = format_st(source, StSection::Declaration, &FormatOptions::default())
        .expect("declaration formats");
    let reparsed = parse_declaration(&output);
    assert!(reparsed.errors().is_empty(), "{:?}", reparsed.errors());
    assert_eq!(semantic_tokens(&reparsed.syntax_node()), expected_tokens);
    assert_eq!(comments(&reparsed.syntax_node()), expected_comments);
    assert_eq!(
        format_st(&output, StSection::Declaration, &FormatOptions::default())
            .expect("declaration reformats"),
        output
    );
    output
}

#[test]
fn generic_section_formats_before_inheritance_clauses() {
    let source = "function_block FB_Sub\n\
                  // before generic section\n\
                  var_generic constant\n\
                  nMaxLen2:udint:=1;\n\
                  // before generic end\n\
                  end_var // after generic section\n\
                  // before extends\n\
                  extends FB_Sample<nMaxLen2>\n\
                  implements I_Sample\n\
                  var\n\
                  data:array[0..nMaxLen2-1] of byte;\n\
                  end_var";

    let output = assert_round_trip(source);
    assert!(
        output.find("VAR_GENERIC CONSTANT").unwrap()
            < output.find("EXTENDS FB_Sample<nMaxLen2>").unwrap()
    );
    assert!(
        output.find("EXTENDS FB_Sample<nMaxLen2>").unwrap()
            < output.find("IMPLEMENTS I_Sample").unwrap()
    );
}

#[test]
fn parameterized_types_preserve_angle_bracket_comments() {
    let source = "VAR\n\
                  literalArg : FB_Sample (* before open *) <(* before literal *) 100 (* after literal *)> (* after close *);\n\
                  expressionArg : FB_Sample<(* before expression *) (2*cConst) (* after expression *)>;\n\
                  values : ARRAY[0..5] OF FB_Sample<10>;\n\
                  END_VAR";

    let output = assert_round_trip(source);
    for marker in [
        "(* before open *)",
        "(* before literal *)",
        "(* after literal *)",
        "(* after close *)",
        "(* before expression *)",
        "(* after expression *)",
    ] {
        assert!(output.contains(marker), "missing {marker}:\n{output}");
    }
}

#[test]
fn trailing_generic_section_comment_is_emitted_once() {
    let source = "FUNCTION_BLOCK FB_Sample\n\
                  VAR_GENERIC CONSTANT\n\
                  nMaxLen : UDINT := 1;\n\
                  END_VAR // section marker";

    let output = assert_round_trip(source);
    assert_eq!(output.matches("// section marker").count(), 1, "{output}");
}
