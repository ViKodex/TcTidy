use tctidy::{format_st, FormatOptions, StSection};
use tctidy_syntax::{parse_implementation, SyntaxKind};

#[test]
fn time_and_ltime_clock_calls_format_without_changing_their_spelling() {
    for operator in ["TIME", "LTIME"] {
        let source = format!("tNow:={operator}();");
        let parsed = parse_implementation(&source);
        assert!(
            parsed.errors().is_empty(),
            "{operator}: {:?}",
            parsed.errors()
        );
        assert!(parsed.debug_text_matches(&source));
        assert_eq!(
            parsed
                .syntax_node()
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::CALL_EXPR)
                .count(),
            1,
            "{operator} should parse as a call"
        );

        let formatted = format_st(
            &source,
            StSection::Implementation,
            &FormatOptions::default(),
        )
        .expect("clock call formats");
        assert_eq!(formatted, format!("tNow := {operator}();"));
        assert_eq!(
            format_st(
                &formatted,
                StSection::Implementation,
                &FormatOptions::default()
            )
            .expect("clock call reformats"),
            formatted
        );
    }
}

#[test]
fn bare_time_type_is_not_an_expression() {
    for operator in ["TIME", "LTIME"] {
        let source = format!("tNow := {operator};");
        assert!(!parse_implementation(&source).errors().is_empty());
    }
}
