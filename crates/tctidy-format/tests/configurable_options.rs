//! The non-destruction guarantees under every user-configurable value.
//!
//! [`style_options`] is what a `.tctidy.toml` can set, so every value it
//! accepts must keep formatting idempotent and meaning-preserving, alone and
//! combined. Each option is driven to its boundaries (both booleans, every
//! choice, the integer minimum and maximum) over a set of representative
//! declaration and implementation sources.

use proptest::prelude::*;

use tctidy_format::options::{style_options, OptionKind, OptionValue};
use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_declaration, parse_implementation, SyntaxKind, SyntaxNode};

const IMPL_SOURCES: &[&str] = &[
    include_str!("../../tctidy/benches/inputs/impl_body.st"),
    include_str!("../../tctidy/benches/inputs/impl_comments.st"),
    "IF a THEN\n\n\nb := 1; // c\n\n\nEND_IF;\n\
     result := compute_something(first_argument_name, second_argument_name, third_argument_name, fourth_argument_name);\n\
     result := first_operand_name + second_operand_name + third_operand_name + fourth_operand_name;\n\
     request := enabled AND ((first_condition AND second_condition) OR (third_condition AND fourth_condition) OR (fifth_condition AND sixth_condition));\n\
     sum := first_operand_name +\nsecond_operand_name;\nfb(x := 1, y => z);\n\
     IF NOT (first_long_condition OR second_long_condition OR third_long_condition OR fourth_long_condition) THEN result := TRUE; END_IF;\n\
     long_function_name(\nfirst_argument,\nsecond_argument\n);\n\
     CASE x OF\n\n1: y := 1;\n2, 3:\ny := 2;\n//comment\nz := 3;\n\nELSE\ny := 0;\nEND_CASE;\n\
     FOR i := 1 TO 10 BY 2 DO\nsum := sum+i*2; (* block *)\nEND_FOR;\n\
     WHILE run AND NOT done DO\nx:=x MOD 3;\nEND_WHILE;",
    "x := foo((* lead *) a, b);\ny := bar(a, (b + c));\nz := baz();\nw := (a);\n\
     v := qux(\n(* c *) a,\nb\n);\nu := ((a OR b) AND (c OR d)) XOR e;\n\
     IF ready THEN go := TRUE; END_IF\nIF reset THEN\ncount := 0;\nEND_IF; // done",
    "IF a (* c *) THEN\nb := 1;\nEND_IF;\nIF x\nTHEN\ny := 1;\nELSIF z // why\nTHEN\ny := 2;\nEND_IF;\n\
     CASE s // sel\nOF\n1: t := 1;\nEND_CASE;\nFOR i := 0 TO 3\nDO\n;\nEND_FOR;",
];

const DECL_SOURCES: &[&str] = &[
    include_str!("../../tctidy/benches/inputs/decl_block.st"),
    "VAR\nx : INT := 5; // c\ny AT %IX0.0 : BOOL;\nz : FB_Foo(\na := 1,\nb := 2\n);\nEND_VAR",
    "TYPE e : (red, green, blue); END_TYPE",
    "FUNCTION_BLOCK FB_A\nVAR_INPUT\na : INT;\nEND_VAR\n// state\nVAR\nb : INT;\nEND_VAR\n\n\n\nVAR_OUTPUT\nc : INT;\nEND_VAR",
    "TYPE ST_Test : STRUCT\nInlet : ST_Limits := (Low := 1.0, High := 2.0, Window := 3.0);\nEND_STRUCT\nEND_TYPE",
];

fn fmt_impl(src: &str, style: &StyleConfig) -> String {
    format_implementation(&parse_implementation(src), style, LineEnding::Lf)
        .unwrap_or_else(|e| panic!("formats: {e:?}\n{src}"))
}

fn fmt_decl(src: &str, style: &StyleConfig) -> String {
    format_declaration(&parse_declaration(src), style, LineEnding::Lf)
        .unwrap_or_else(|e| panic!("formats: {e:?}\n{src}"))
}

/// Non-trivia tokens with keyword case folded, the program's meaning. A
/// trailing separator (a `,` right before `)` or `]`) carries no meaning and is
/// what the trailing-comma options add or remove, so it is dropped.
fn semantic(node: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    let mut tokens = semantic_raw(node);
    let mut i = 0;
    while i + 1 < tokens.len() {
        if tokens[i].0 == SyntaxKind::COMMA
            && matches!(tokens[i + 1].0, SyntaxKind::R_PAREN | SyntaxKind::R_BRACK)
        {
            tokens.remove(i);
        } else {
            i += 1;
        }
    }
    tokens
}

fn semantic_raw(node: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .map(|t| {
            let text = if t.kind().is_keyword() {
                t.text().to_ascii_uppercase()
            } else {
                t.text().to_string()
            };
            (t.kind(), text)
        })
        .collect()
}

/// Comment and pragma tokens, whitespace-insensitive (the `//` spacing rule
/// rewrites only whitespace).
fn comments(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind().is_trivia() && !t.text().trim().is_empty())
        .map(|t| t.text().split_whitespace().collect::<Vec<_>>().join(""))
        .collect()
}

fn check(style: &StyleConfig, label: &str) {
    for src in IMPL_SOURCES {
        let once = fmt_impl(src, style);
        assert_eq!(once, fmt_impl(&once, style), "{label}: impl not idempotent");
        let (a, b) = (parse_implementation(src), parse_implementation(&once));
        assert!(
            b.errors().is_empty(),
            "{label}: output does not parse\n{once}"
        );
        assert_eq!(
            semantic(&a.syntax_node()),
            semantic(&b.syntax_node()),
            "{label}"
        );
        assert_eq!(
            comments(&a.syntax_node()),
            comments(&b.syntax_node()),
            "{label}"
        );
    }
    for src in DECL_SOURCES {
        let once = fmt_decl(src, style);
        assert_eq!(once, fmt_decl(&once, style), "{label}: decl not idempotent");
        let (a, b) = (parse_declaration(src), parse_declaration(&once));
        assert!(
            b.errors().is_empty(),
            "{label}: output does not parse\n{once}"
        );
        assert_eq!(
            semantic(&a.syntax_node()),
            semantic(&b.syntax_node()),
            "{label}"
        );
        assert_eq!(
            comments(&a.syntax_node()),
            comments(&b.syntax_node()),
            "{label}"
        );
    }
}

/// The boundary values of an option's domain.
fn boundary_values(kind: OptionKind) -> Vec<OptionValue> {
    match kind {
        OptionKind::Bool => vec![OptionValue::Bool(false), OptionValue::Bool(true)],
        OptionKind::Integer { min, max } => {
            vec![
                OptionValue::Integer(min.into()),
                OptionValue::Integer(max.into()),
            ]
        }
        OptionKind::Choice(names) => names
            .iter()
            .map(|n| OptionValue::Choice((*n).to_owned()))
            .collect(),
    }
}

#[test]
fn every_option_value_is_idempotent_and_meaning_preserving() {
    let mut failures = Vec::new();
    for option in style_options() {
        for value in boundary_values(option.kind) {
            for width in [None, Some(40)] {
                let mut style = StyleConfig::default();
                if let Some(w) = width {
                    style.line_width = w;
                }
                option.set(&mut style, &value).unwrap();
                let label = format!("{} = {value} (line_width {width:?})", option.key);
                if std::panic::catch_unwind(|| check(&style, &label)).is_err() {
                    failures.push(label);
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "failing values:\n{}",
        failures.join("\n")
    );
}

fn arb_style() -> impl Strategy<Value = StyleConfig> {
    let per_option: Vec<_> = style_options()
        .iter()
        .map(|o| proptest::sample::select(boundary_values(o.kind)))
        .collect();
    per_option.prop_map(|values| {
        let mut style = StyleConfig::default();
        for (option, value) in style_options().iter().zip(values) {
            option.set(&mut style, &value).unwrap();
        }
        style
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn combined_options_are_idempotent_and_meaning_preserving(style in arb_style()) {
        check(&style, &format!("{style:?}"));
    }
}
