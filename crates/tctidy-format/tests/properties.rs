//! Property tests: the non-destruction invariants, checked against thousands of
//! randomly generated Structured Text programs.
//!
//! * **Totality**, parsing arbitrary input never panics; a clean parse always
//!   formats without panicking (on a parse error the caller keeps the original).
//! * **Semantic preservation**, `format` never adds, drops, or reorders a
//!   semantic token. Keyword case is normalized away (the formatter upper-cases
//!   keywords) so only meaning is compared.
//! * **Trivia stability**, every comment and pragma survives, in order, modulo
//!   the house-style line-comment spacing rewrite (`//  x` → `// x`).
//! * **Idempotence**, `format(format(x)) == format(x)`.

use proptest::prelude::*;

use tctidy_format::{format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_implementation, SyntaxKind, SyntaxNode};

/// The semantic token stream: every non-trivia token, with keyword case folded
/// (the formatter upper-cases keywords, which is meaning-preserving).
fn semantic_tokens(node: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
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

/// Re-derive the house-style line-comment rewrite the formatter applies (see
/// comment_style.rs): collapse the leading spaces/tabs after `//` to one space
/// and trim trailing whitespace; a content-less `//` stays bare `//`. Kept local
/// and independent so the property cross-checks the formatter rather than trusting
/// it. Block comments and pragmas are returned verbatim.
fn norm_comment(text: &str) -> String {
    let Some(rest) = text.strip_prefix("//") else {
        return text.to_string();
    };
    let content = rest.trim_matches([' ', '\t']);
    if content.is_empty() {
        "//".to_string()
    } else {
        format!("// {content}")
    }
}

/// Every comment / pragma token in source order, with line-comment spacing folded
/// to house style so the comparison ignores that intentional, content-preserving
/// rewrite, only a genuinely added, dropped, or reordered comment can fail.
fn comments(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| {
            matches!(
                t.kind(),
                SyntaxKind::LINE_COMMENT
                    | SyntaxKind::BLOCK_COMMENT
                    | SyntaxKind::C_BLOCK_COMMENT
                    | SyntaxKind::PRAGMA
            )
        })
        .map(|t| norm_comment(t.text()))
        .collect()
}

fn fmt(src: &str) -> Option<String> {
    let parse = parse_implementation(src);
    if !parse.errors().is_empty() {
        return None;
    }
    format_implementation(&parse, &StyleConfig::default(), LineEnding::Lf).ok()
}

// ---- generators: a small but real slice of ST ----------------------------

/// Identifiers from a fixed safe set (never collides with a keyword).
fn arb_ident() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("a"),
        Just("b"),
        Just("c"),
        Just("foo"),
        Just("bar"),
        Just("myVar"),
        Just("count"),
        Just("result"),
    ]
    .prop_map(String::from)
}

fn arb_bin_op() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("+"),
        Just("-"),
        Just("*"),
        Just("AND"),
        Just("OR"),
        Just("="),
        Just("<"),
        Just(">"),
        Just("MOD"),
    ]
}

fn arb_expr() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        arb_ident(),
        (0u32..1000).prop_map(|n| n.to_string()),
        Just("TRUE".to_string()),
        Just("FALSE".to_string()),
    ];
    leaf.prop_recursive(4, 24, 3, |inner| {
        prop_oneof![
            (inner.clone(), arb_bin_op(), inner.clone())
                .prop_map(|(l, op, r)| format!("{l} {op} {r}")),
            inner.clone().prop_map(|e| format!("({e})")),
            (arb_ident(), inner).prop_map(|(f, a)| format!("{f}({a})")),
        ]
    })
}

fn arb_stmt() -> impl Strategy<Value = String> {
    let assign = (arb_ident(), arb_expr()).prop_map(|(n, e)| format!("{n} := {e};"));
    assign.prop_recursive(3, 12, 2, |inner| {
        let block = prop::collection::vec(inner.clone(), 1..3).prop_map(|v| v.join("\n"));
        // 0 to 2 blank lines injected right after a block header (`g*`) and right
        // before its closer (`gt`), so both the block-start collapse
        // (`blank_lines_at_block_start`) and the block-end collapse
        // (`blank_lines_at_block_end`) are exercised by the idempotence and
        // preservation invariants, not just already-tight bodies.
        let gap = prop_oneof![Just(""), Just("\n"), Just("\n\n")];
        prop_oneof![
            (arb_expr(), gap.clone(), block.clone(), gap.clone())
                .prop_map(|(c, g, b, gt)| format!("IF {c} THEN\n{g}{b}\n{gt}END_IF;")),
            (
                arb_expr(),
                gap.clone(),
                block.clone(),
                gap.clone(),
                block.clone(),
                gap.clone(),
            )
                .prop_map(|(c, g1, t, g2, e, gt)| format!(
                    "IF {c} THEN\n{g1}{t}\nELSE\n{g2}{e}\n{gt}END_IF;"
                )),
            (arb_expr(), gap.clone(), block, gap)
                .prop_map(|(c, g, b, gt)| format!("WHILE {c} DO\n{g}{b}\n{gt}END_WHILE;")),
        ]
    })
}

/// A program: statements and standalone line comments, one per line. The comment
/// carries a varied run of spaces/tabs after `//` so the space-collapsing rewrite
/// is exercised, not just already-canonical comments.
fn arb_program() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        4 => arb_stmt(),
        1 => (r"//[ \t]{0,4}", "[a-z0-9 ]{0,8}").prop_map(|(p, s)| format!("{p}{s}")),
    ];
    prop::collection::vec(line, 1..6).prop_map(|v| v.join("\n"))
}

// ---- properties -----------------------------------------------------------

proptest! {
    /// Parsing is total and formatting a clean parse never panics, for fully
    /// arbitrary input (most of which is invalid ST, that is the point).
    #[test]
    fn parsing_and_formatting_are_total(s in any::<String>()) {
        let _ = fmt(&s);
    }

    /// For generated valid programs: meaning + comments preserved, idempotent.
    #[test]
    fn format_preserves_meaning_comments_and_is_idempotent(src in arb_program()) {
        let parse = parse_implementation(&src);
        prop_assume!(parse.errors().is_empty());

        let Some(out) = fmt(&src) else { return Ok(()) };

        let reparsed = parse_implementation(&out);
        prop_assert!(
            reparsed.errors().is_empty(),
            "formatted output no longer parses cleanly:\n{out}"
        );

        prop_assert_eq!(
            semantic_tokens(&parse.syntax_node()),
            semantic_tokens(&reparsed.syntax_node()),
            "semantic token stream changed"
        );
        prop_assert_eq!(
            comments(&parse.syntax_node()),
            comments(&reparsed.syntax_node()),
            "comments/pragmas changed"
        );

        let out2 = fmt(&out).expect("re-formatting clean output must succeed");
        prop_assert_eq!(&out, &out2, "formatting is not idempotent");
    }
}
