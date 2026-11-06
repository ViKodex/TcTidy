use tctidy::{format_st, FormatOptions, StSection};
use tctidy_syntax::{parse_declaration, parse_implementation, SyntaxKind, SyntaxNode};

fn semantic_tokens(node: SyntaxNode) -> Vec<(SyntaxKind, String)> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| (token.kind(), token.text().to_string()))
        .collect()
}

fn parse_tokens(source: &str, section: StSection) -> Vec<(SyntaxKind, String)> {
    let node = match section {
        StSection::Declaration => {
            let parse = parse_declaration(source);
            assert!(parse.errors().is_empty(), "{:?}", parse.errors());
            parse.syntax_node()
        }
        StSection::Implementation => {
            let parse = parse_implementation(source);
            assert!(parse.errors().is_empty(), "{:?}", parse.errors());
            parse.syntax_node()
        }
        StSection::Auto => panic!("test cases must select an explicit grammar"),
    };
    semantic_tokens(node)
}

#[test]
fn typed_unicode_literals_preserve_semantics_and_are_idempotent() {
    let cases = [
        (
            StSection::Implementation,
            "s:=UTF8#'aä$'b$$';\nu:=UCHAR#'⳧';",
        ),
        (
            StSection::Declaration,
            "VAR\ns:STRING:=UTF8#'aä$'b$$';\nu:UINT:=UCHAR#'⳧';\nEND_VAR",
        ),
    ];

    for (section, source) in cases {
        let expected_tokens = parse_tokens(source, section);
        let output = format_st(source, section, &FormatOptions::default()).expect("formats");

        assert!(output.contains("UTF8#'aä$'b$$'"), "{output}");
        assert!(output.contains("UCHAR#'⳧'"), "{output}");
        assert!(!output.contains("UTF8#\n"), "{output}");
        assert!(!output.contains("UCHAR#\n"), "{output}");
        assert_eq!(parse_tokens(&output, section), expected_tokens);
        assert_eq!(
            format_st(&output, section, &FormatOptions::default()).expect("reformats"),
            output
        );
    }
}

#[test]
fn invalid_typed_unicode_literals_produce_no_formatted_output() {
    for literal in [
        "UTF8#'unterminated",
        "UCHAR#'unterminated",
        "UTF8# 'text'",
        "UCHAR#\"text\"",
    ] {
        let implementation = format!("x := {literal};");
        assert!(
            format_st(
                &implementation,
                StSection::Implementation,
                &FormatOptions::default()
            )
            .is_err(),
            "{implementation}"
        );

        let declaration = format!("VAR\n    x : STRING := {literal};\nEND_VAR");
        assert!(
            format_st(
                &declaration,
                StSection::Declaration,
                &FormatOptions::default()
            )
            .is_err(),
            "{declaration}"
        );
    }
}
