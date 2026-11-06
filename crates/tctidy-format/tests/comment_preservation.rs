//! Comment preservation, the non-destruction promise for trivia.
//!
//! Trailing comments on the operands of a *multi-line* binary expression (e.g.
//! `IF (a) // why\n OR (b) // why2\nTHEN`) must be kept. Comments are data,
//! losing them silently is the worst class of bug a formatter can have.

use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::tokens::lex;
use tctidy_syntax::{parse_declaration, parse_implementation, SyntaxKind, SyntaxNode};

fn imp(src: &str) -> String {
    imp_with_eol(src, LineEnding::Lf)
}

fn imp_with_eol(src: &str, eol: LineEnding) -> String {
    format_implementation(&parse_implementation(src), &StyleConfig::default(), eol)
        .expect("implementation formats cleanly")
}

fn dec(src: &str) -> String {
    format_declaration(
        &parse_declaration(src),
        &StyleConfig::default(),
        LineEnding::Lf,
    )
    .expect("declaration formats cleanly")
}

/// Apply the formatter's content-preserving line-comment rewrite (`//x` and
/// `//   x` -> `// x`, plus trailing-whitespace trim) so preservation checks run
/// at the default style yet ignore that intentional spacing; only a genuinely
/// missing comment fails. The rule itself is covered by comment_style.rs.
fn norm(comment: &str) -> String {
    match comment.strip_prefix("//") {
        Some(rest) => {
            let content = rest.trim_matches([' ', '\t']);
            if content.is_empty() {
                "//".to_string()
            } else {
                format!("// {content}")
            }
        }
        None => comment.to_string(),
    }
}

/// Every `//` and `(* *)` comment in `src` must still appear in the output.
fn assert_comments_kept(src: &str) {
    assert_kept(src, imp, "implementation");
}

/// Declaration variant.
fn assert_decl_comments_kept(src: &str) {
    assert_kept(src, dec, "declaration");
}

fn assert_kept(src: &str, fmt: impl Fn(&str) -> String, what: &str) {
    let out = fmt(src);
    for comment in extract_comments(src) {
        assert!(
            out.contains(&norm(&comment)),
            "[{what}] comment dropped: {comment:?}\n--- input ---\n{src}\n--- output ---\n{out}"
        );
    }
    assert_eq!(fmt(&out), out, "[{what}] not idempotent:\n{out}");
}

/// Crude comment scanner good enough for these fixtures (no strings with `//`).
fn extract_comments(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        if let Some(i) = line.find("//") {
            out.push(line[i..].trim_end().to_string());
        }
        let mut rest = line;
        while let Some(i) = rest.find("(*") {
            if let Some(j) = rest[i..].find("*)") {
                out.push(rest[i..i + j + 2].to_string());
                rest = &rest[i + j + 2..];
            } else {
                break;
            }
        }
    }
    out
}

#[test]
fn blank_line_before_end_of_body_comment_is_kept() {
    // An intentional blank line above an end-of-body (dangling) comment survives.
    let out = imp("IF a THEN\n    x := 1;\nEND_IF\n\n//Request.Reset := TRUE;");
    // (default style adds the space after `//`)
    assert!(
        out.contains("END_IF\n\n// Request.Reset := TRUE;"),
        "blank line before the end-of-body comment was dropped:\n{out}"
    );
}

#[test]
fn trailing_comment_on_wrapped_if_condition() {
    assert_comments_kept(
        "IF NOT bInitDone // only on the first PLC cycle\nTHEN\n    x := 1;\nEND_IF;",
    );
}

#[test]
fn trailing_comments_on_multiline_or_condition() {
    assert_comments_kept(
        "IF (a = 1) // first index\n    OR (b = 2) // second index\nTHEN\n    x := 1;\nEND_IF;",
    );
}

#[test]
fn trailing_comment_on_multiline_assignment_rhs() {
    assert_comments_kept("x := a // term one\n    + b;");
}

#[test]
fn trailing_comments_on_multiline_while_condition() {
    assert_comments_kept("WHILE (a) // cond a\n    AND (b) // cond b\nDO\n    x := 1;\nEND_WHILE;");
}

#[test]
fn block_comment_operand_in_multiline_condition() {
    // Commented-out operands are real, meaningful code (e.g. `(*OR bReset[1]*)`).
    assert_comments_kept("IF a\n    OR b (*OR bReset[1]*)\n    OR c\nTHEN\n    x := 1;\nEND_IF;");
}

#[test]
fn inline_block_comment_between_operands() {
    // Single-line expression, block comment mid-chain (`a (*OR z*) AND b`).
    assert_comments_kept("x := a (*OR bReset[1]*) AND b;");
}

#[test]
fn leading_comment_before_operator_in_multiline_expr() {
    assert_comments_kept("x := a\n    // why this term\n    AND b;");
}

#[test]
fn leading_comment_before_operand_in_multiline_expr() {
    // Comment on its own line between the operator and the right operand.
    assert_comments_kept("x := a\n    OR\n    // why this operand\n    b;");
}

#[test]
fn leading_comment_on_assignment_rhs() {
    // `lhs :=` then the value on the next line, with a comment above the value.
    assert_comments_kept("x :=\n    // why this value\n    a OR b;");
    assert_comments_kept("x R=\n    // reset reason\n    a AND b;");
}

#[test]
fn leading_comment_on_multiline_call_argument() {
    assert_comments_kept("fb(\n    //sServer:='',\n    sPath := a,\n    sName := b);");
}

#[test]
fn inline_block_comment_in_single_line_call() {
    assert_comments_kept("x := SEL( (* TODO check limits*) FALSE, p, q);");
}

#[test]
fn trailing_comment_on_initializer_element() {
    assert_decl_comments_kept(
        "VAR\narr : ARRAY[1..2] OF ST_x := [\n    (a := 1), // first device\n    (a := 2)\n];\nEND_VAR",
    );
}

#[test]
fn named_initializer_with_comment_over_inline_limit_keeps_comment() {
    let source =
        "VAR\nlimits : ST_Limits := (Low := 1.0, Mid := (* keep *) 2.0, High := 3.0);\nEND_VAR";
    assert_decl_comments_kept(source);
    let formatted = format_declaration(
        &parse_declaration(source),
        &StyleConfig::default(),
        LineEnding::Lf,
    )
    .expect("declaration formats");
    assert!(
        formatted.contains("ST_Limits := (Low  := 1.0,"),
        "{formatted}"
    );
}

#[test]
fn supply_comments_keep_the_same_hanging_layout_as_drain() {
    let source = "TYPE ST_TankConfig : STRUCT\n\
        Supply : ST_Limits := (\n\
            Enabled := FALSE, // fixed by the wiring\n\
            MinLevel := 100.0,\n\
            MaxLevel := 2500.0,\n\
            // one sensor step\n\
            MinFlowLitresPerMinute := 5.0,\n\
            MaxFlow := 100.0 // placeholder\n\
        );\n\
        Drain : ST_Limits := (MinLevel := 500.0, MaxLevel := 2500.0, Hysteresis := 1.0);\n\
        END_STRUCT\nEND_TYPE";
    assert_decl_comments_kept(source);
    let formatted = dec(source);
    assert!(
        formatted.contains("Supply : ST_Limits := (Enabled"),
        "{formatted}"
    );
    assert!(
        formatted.contains("Drain  : ST_Limits := (MinLevel"),
        "{formatted}"
    );
}

#[test]
fn following_comment_moves_after_previous_named_field_comma() {
    let comment = "// Below one sensor step (0.1 mm) per 100 ms sample window, a level change counts as noise, not as a fill.";
    let source = format!(
        "TYPE ST_Test : STRUCT\nSupply : ST_Limits := (Low := 1.0, LevelHysteresisPercent := 3.0,\n{comment}\nMinFlowLitresPerMinute := 5.0);\nEND_STRUCT\nEND_TYPE"
    );
    assert_decl_comments_kept(&source);
    let formatted = dec(&source);
    let line = formatted
        .lines()
        .find(|line| line.contains("LevelHysteresisPercent"))
        .expect("hysteresis field");
    assert!(line.ends_with(&format!(", {comment}")), "{formatted}");
}

#[test]
fn blank_line_keeps_comment_above_next_named_field() {
    let source = "VAR\nlimits : ST_Limits := (Low := 1.0,\n\n// next field\nMid := 2.0, High := 3.0);\nEND_VAR";
    assert_decl_comments_kept(source);
    let formatted = dec(source);
    let low_line = formatted
        .lines()
        .find(|line| line.contains("Low") && line.contains("1.0"))
        .expect("first field");
    assert!(!low_line.contains("// next field"), "{formatted}");
    assert!(
        formatted.lines().any(|line| line.trim() == "// next field"),
        "{formatted}"
    );
}

// Comments inside a declaration's name list, between two names, or between the
// last name and the `:`, must survive. The attachment pass records them
// (as trailing of the comma/name, or leading of the next name); the declaration
// rule must emit them instead of only the `;`'s trailing.

#[test]
fn line_comment_between_declaration_names() {
    assert_decl_comments_kept("VAR\na, // first\nb, c : INT;\nEND_VAR");
}

#[test]
fn block_comment_between_declaration_names() {
    assert_decl_comments_kept("VAR\na, (* mid *) b, c : INT;\nEND_VAR");
}

#[test]
fn per_name_line_comments_in_multi_member_decl() {
    assert_decl_comments_kept("VAR\nnX, // X axis\nnY, // Y axis\nnZ : INT;\nEND_VAR");
}

#[test]
fn trailing_comment_after_last_name_before_colon() {
    assert_decl_comments_kept("VAR\na, b, c // tail\n: INT;\nEND_VAR");
}

#[test]
fn trailing_comment_on_single_member_name() {
    assert_decl_comments_kept("VAR\na // note\n: INT;\nEND_VAR");
}

#[test]
fn leading_comment_above_a_middle_name() {
    assert_decl_comments_kept("VAR\na,\n// separator\nb, c : INT;\nEND_VAR");
}

#[test]
fn multiline_trailing_comments_keep_internal_newlines_at_eof() {
    for (open, close) in [("(*", "*)"), ("/*", "*/")] {
        for (eol, ending) in [(LineEnding::Lf, "\n"), (LineEnding::Crlf, "\r\n")] {
            for final_newline in [false, true] {
                let suffix = if final_newline { ending } else { "" };
                let source = format!("x := 1; {open}alpha{ending}beta{close}{suffix}");
                let output = imp_with_eol(&source, eol);

                assert!(
                    output.contains(&format!("{open}alpha{ending}beta{close}")),
                    "multiline suffix collapsed:\n{output:?}"
                );
                assert_eq!(imp_with_eol(&output, eol), output);
            }
        }
    }
}

#[test]
fn multiline_trailing_comments_render_the_same_before_following_content() {
    for (open, close) in [("(*", "*)"), ("/*", "*/")] {
        let source = format!("x := 1; {open}alpha\nbeta{close}\ny := 2;");
        let output = imp(&source);

        assert!(output.contains(&format!("{open}alpha\nbeta{close}\ny := 2;")));
        assert_eq!(imp(&output), output);
    }
}

fn lexed_comments(source: &str) -> Vec<String> {
    let mut offset = 0usize;
    let mut comments = Vec::new();
    for token in lex(source) {
        let len = u32::from(token.len) as usize;
        let text = &source[offset..offset + len];
        if matches!(
            token.kind,
            SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT | SyntaxKind::C_BLOCK_COMMENT
        ) {
            comments.push(norm(text));
        }
        offset += len;
    }
    comments
}

fn semantic_tokens(node: SyntaxNode) -> Vec<(SyntaxKind, String)> {
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

fn implementation_semantic_tokens(source: &str) -> Vec<(SyntaxKind, String)> {
    semantic_tokens(parse_implementation(source).syntax_node())
}

fn declaration_semantic_tokens(source: &str) -> Vec<(SyntaxKind, String)> {
    semantic_tokens(parse_declaration(source).syntax_node())
}

#[test]
fn implementation_token_gaps_preserve_comments_in_source_order() {
    let cases = [
        ("assign", "y := (*assign*) 1;"),
        ("ref-assign", "y REF= (*ref*) x;"),
        ("set-assign", "y S= (*set*) x;"),
        ("reset-assign", "y R= (*reset*) x;"),
        ("binary", "y := a + (*binary*) b;"),
        ("unary", "y := NOT (*unary*) b;"),
        ("unary-symbol", "y := -(*unary-symbol*) b;"),
        ("comma", "F(a, (*comma*) b);"),
        ("initializer-comma", "y := [1, (*initializer-comma*) 2];"),
        ("index-comma", "y := data[i, (*index-comma*) j];"),
        ("named-input", "F(a := (*input*) 1);"),
        ("named-output", "F(a => (*output*) b);"),
        ("call-open-paren", "F((*call-paren*) a);"),
        ("expr-open-paren", "y := ((*expr-paren*) a);"),
        (
            "initializer-open-bracket",
            "y := [(*initializer-bracket*) 1, 2];",
        ),
        ("index-open-bracket", "y := data[(*index-bracket*) i];"),
        ("before-then", "IF a (*then*) THEN\n    y := 1;\nEND_IF;"),
        (
            "before-elsif-then",
            "IF a THEN\n    y := 1;\nELSIF b (*elsif-then*) THEN\n    y := 2;\nEND_IF;",
        ),
        ("before-do", "WHILE a (*do*) DO\n    y := 1;\nEND_WHILE;"),
        (
            "before-for-do",
            "FOR i := 1 TO 2 (*for-do*) DO\n    y := 1;\nEND_FOR;",
        ),
        ("before-of", "CASE a (*of*) OF\n    1: y := 1;\nEND_CASE;"),
        ("assign-line", "y := //assign-line\n1;"),
        ("ref-line", "y REF= //ref-line\nx;"),
        ("set-line", "y S= //set-line\nx;"),
        ("reset-line", "y R= //reset-line\nx;"),
        ("binary-line", "y := a + //binary-line\nb;"),
        ("unary-line", "y := NOT //unary-line\nb;"),
        ("unary-symbol-line", "y := -//unary-symbol-line\nb;"),
        ("comma-line", "F(a, //comma-line\nb);"),
        (
            "initializer-comma-line",
            "y := [1, //initializer-comma-line\n2];",
        ),
        ("index-comma-line", "y := data[i, //index-comma-line\nj];"),
        ("named-input-line", "F(a := //input-line\n1);"),
        ("named-output-line", "F(a => //output-line\nb);"),
        ("call-paren-line", "F(//call-paren-line\na);"),
        ("expr-paren-line", "y := (//expr-paren-line\na);"),
        (
            "initializer-bracket-line",
            "y := [//initializer-bracket-line\n1];",
        ),
        ("index-bracket-line", "y := data[//index-bracket-line\ni];"),
        ("then-line", "IF a //then-line\nTHEN\n    y := 1;\nEND_IF;"),
        (
            "elsif-then-line",
            "IF a THEN\n    y := 1;\nELSIF b //elsif-then-line\nTHEN\n    y := 2;\nEND_IF;",
        ),
        ("do-line", "WHILE a //do-line\nDO\n    y := 1;\nEND_WHILE;"),
        (
            "for-do-line",
            "FOR i := 1 TO 2 //for-do-line\nDO\n    y := 1;\nEND_FOR;",
        ),
        ("of-line", "CASE a //of-line\nOF\n    1: y := 1;\nEND_CASE;"),
    ];

    let mut failures = Vec::new();
    for (name, source) in cases {
        let expected_tokens = implementation_semantic_tokens(source);
        let expected = lexed_comments(source);
        let output = imp(source);
        let actual = lexed_comments(&output);
        if actual != expected {
            failures.push(format!(
                "{name}: comments {actual:?}, expected {expected:?}\n{output}"
            ));
            continue;
        }
        let parsed = parse_implementation(&output);
        if !parsed.errors().is_empty() {
            failures.push(format!("{name}: output did not parse\n{output}"));
        } else if implementation_semantic_tokens(&output) != expected_tokens {
            failures.push(format!("{name}: semantic token stream changed\n{output}"));
        } else if imp(&output) != output {
            failures.push(format!("{name}: output was not idempotent\n{output}"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[test]
fn declaration_token_gaps_preserve_comments_in_source_order() {
    let cases = [
        ("var-colon", "VAR\nx : (*var-colon*) INT;\nEND_VAR"),
        (
            "struct-colon",
            "TYPE T : STRUCT\nx : (*struct-colon*) INT;\nEND_STRUCT\nEND_TYPE",
        ),
        ("type-colon", "TYPE T : (*type-colon*) INT;\nEND_TYPE"),
        (
            "return-type-colon",
            "FUNCTION F : (*return-type-colon*) INT",
        ),
        (
            "var-initializer",
            "VAR\nx : INT := (*var-initializer*) 1;\nEND_VAR",
        ),
        (
            "struct-initializer",
            "TYPE T : STRUCT\nx : INT := (*struct-initializer*) 1;\nEND_STRUCT\nEND_TYPE",
        ),
        (
            "type-initializer",
            "TYPE T : INT := (*type-initializer*) 1;\nEND_TYPE",
        ),
        (
            "enum-initializer",
            "TYPE T : (A := (*enum-initializer*) 1, B);\nEND_TYPE",
        ),
        (
            "combined-block-comments",
            "VAR\na, (*name*) b : (*colon*) INT := (*initializer*) 1;\nEND_VAR",
        ),
        ("var-colon-line", "VAR\nx : //var-colon-line\nINT;\nEND_VAR"),
        (
            "struct-colon-line",
            "TYPE T : STRUCT\nx : //struct-colon-line\nINT;\nEND_STRUCT\nEND_TYPE",
        ),
        (
            "type-colon-line",
            "TYPE T : //type-colon-line\nINT;\nEND_TYPE",
        ),
        (
            "return-type-colon-line",
            "FUNCTION F : //return-type-colon-line\nINT",
        ),
        (
            "var-initializer-line",
            "VAR\nx : INT := //var-initializer-line\n1;\nEND_VAR",
        ),
        (
            "struct-initializer-line",
            "TYPE T : STRUCT\nx : INT := //struct-initializer-line\n1;\nEND_STRUCT\nEND_TYPE",
        ),
        (
            "type-initializer-line",
            "TYPE T : INT := //type-initializer-line\n1;\nEND_TYPE",
        ),
        (
            "enum-initializer-line",
            "TYPE T : (A := //enum-initializer-line\n1, B);\nEND_TYPE",
        ),
        (
            "combined-line-comments",
            "VAR\na, //name-line\nb : //colon-line\nINT := //initializer-line\n1;\nEND_VAR",
        ),
    ];

    let mut failures = Vec::new();
    for (name, source) in cases {
        let parsed = parse_declaration(source);
        assert!(
            parsed.errors().is_empty(),
            "{name}: invalid test input: {:?}",
            parsed.errors()
        );
        let expected_tokens = declaration_semantic_tokens(source);
        let expected_comments = lexed_comments(source);
        let output = dec(source);
        let actual_comments = lexed_comments(&output);
        if actual_comments != expected_comments {
            failures.push(format!(
                "{name}: comments {actual_comments:?}, expected {expected_comments:?}\n{output}"
            ));
            continue;
        }
        let reparsed = parse_declaration(&output);
        if !reparsed.errors().is_empty() {
            failures.push(format!("{name}: output did not parse\n{output}"));
        } else if declaration_semantic_tokens(&output) != expected_tokens {
            failures.push(format!("{name}: semantic token stream changed\n{output}"));
        } else if dec(&output) != output {
            failures.push(format!("{name}: output was not idempotent\n{output}"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

/// A comment or pragma between a statement's last token and its `;` trails
/// that token, and used to be printed by no rule at all: `x := a (* c *);`
/// lost its comment.
#[test]
fn comment_before_statement_semicolon_is_kept() {
    let cases = [
        ("z := a (* c *);", "z := a (* c *);"),
        ("z := a /* c */ /* d */;", "z := a /* c */ /* d */;"),
        ("z := a {attribute 'x'} ;", "z := a {attribute 'x'};"),
        ("f(1) (* c *);", "f(1) (* c *);"),
        ("RETURN (* c *);", "RETURN (* c *);"),
        ("EXIT (* c *);", "EXIT (* c *);"),
        ("JMP lbl (* c *);", "JMP lbl (* c *);"),
        ("x := 1 // c\n;", "x := 1 // c\n;"),
        ("x := 1\n(* c *)\n;", "x := 1\n(* c *)\n;"),
        (
            "IF a THEN b := 1; END_IF (* c *);",
            "IF a THEN\n    b := 1;\nEND_IF (* c *);",
        ),
        (
            "IF a THEN\nx := 1 (* c *); // d\nEND_IF",
            "IF a THEN\n    x := 1 (* c *); // d\nEND_IF",
        ),
    ];
    for (src, expected) in cases {
        assert_eq!(imp(src), expected, "{src:?}");
        assert_comments_kept(src);
    }
}

/// The output guard: a comment that some rule fails to print makes formatting
/// fail (the caller keeps the input) rather than disappear. `JMP (* c *) lbl;`
/// is such a position; once a rule keeps that comment, pick another unprinted
/// position for this test.
#[test]
fn a_comment_no_rule_prints_is_refused_not_dropped() {
    let src = "JMP (* c *) lbl;";
    let err = format_implementation(
        &parse_implementation(src),
        &StyleConfig::default(),
        LineEnding::Lf,
    )
    .expect_err("an unprinted comment must refuse formatting");
    let message = err.to_string();
    assert!(
        message.contains("`(* c *)` would be lost"),
        "unexpected message: {message}"
    );
}
