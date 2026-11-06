//! "No style leaked" test: a non-default [`StyleConfig`] must change the
//! output. This is the proof that the rules read their decisions from the
//! config rather than hard-coding them.
//!
//! Every `StyleConfig` field has a `*_leaks` test here, except `comment_style`:
//! its only variant is `Preserve`, so there is no other value to try. A new
//! field gets its leak test with the rule that reads it.

use tctidy_format::style::{
    BlockTokenPlacement, CallArgumentLayout, CollectionBreak, ColonAlignment, KeywordCase,
    LogicalExpressionLayout, ParenthesizedExpressionLayout, TrailingSeparator,
};
use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::{IndentStyle, LineEnding};
use tctidy_syntax::{parse_declaration, parse_implementation};

fn impl_fmt(src: &str, style: &StyleConfig) -> String {
    format_implementation(&parse_implementation(src), style, LineEnding::Lf).expect("formats")
}

fn decl_fmt(src: &str, style: &StyleConfig) -> String {
    format_declaration(&parse_declaration(src), style, LineEnding::Lf).expect("formats")
}

/// Assert that `mutate` actually changes the formatted output of `src`.
fn assert_impl_leaks(src: &str, mutate: impl FnOnce(&mut StyleConfig)) {
    let base = StyleConfig::default();
    let mut tweaked = StyleConfig::default();
    mutate(&mut tweaked);
    assert_ne!(
        impl_fmt(src, &base),
        impl_fmt(src, &tweaked),
        "style change did not affect the output for: {src:?}"
    );
}

fn assert_decl_leaks(src: &str, mutate: impl FnOnce(&mut StyleConfig)) {
    let base = StyleConfig::default();
    let mut tweaked = StyleConfig::default();
    mutate(&mut tweaked);
    assert_ne!(
        decl_fmt(src, &base),
        decl_fmt(src, &tweaked),
        "style change did not affect the output for: {src:?}"
    );
}

/// Assert that `mutate` does not change the output of `src`.
fn assert_impl_no_leak(src: &str, msg: &str, mutate: impl FnOnce(&mut StyleConfig)) {
    let base = StyleConfig::default();
    let mut tweaked = StyleConfig::default();
    mutate(&mut tweaked);
    assert_eq!(impl_fmt(src, &base), impl_fmt(src, &tweaked), "{msg}");
}

#[test]
fn keyword_case_leaks() {
    assert_impl_leaks("IF a THEN\nb := 1;\nEND_IF;", |s| {
        s.keyword_case = KeywordCase::Lower
    });
}

#[test]
fn type_keyword_case_leaks() {
    assert_decl_leaks("VAR\nx : BOOL;\nEND_VAR", |s| {
        s.type_keyword_case = KeywordCase::Lower
    });
}

#[test]
fn build_4026_long_date_time_types_obey_type_keyword_case() {
    let source = "VAR\ntimestamp : ldate_and_time;\ndaytime : ltime_of_day;\nEND_VAR";
    let upper = decl_fmt(source, &StyleConfig::default());
    assert!(upper.contains("LDATE_AND_TIME"), "{upper}");
    assert!(upper.contains("LTIME_OF_DAY"), "{upper}");

    let lower_style = StyleConfig {
        type_keyword_case: KeywordCase::Lower,
        ..StyleConfig::default()
    };
    let lower = decl_fmt(source, &lower_style);
    assert!(lower.contains("ldate_and_time"), "{lower}");
    assert!(lower.contains("ltime_of_day"), "{lower}");
}

#[test]
fn indent_width_leaks() {
    assert_impl_leaks("IF a THEN\nb := 1;\nEND_IF;", |s| s.indent_width = 2);
}

#[test]
fn space_before_colon_leaks() {
    assert_decl_leaks("VAR\nx : BOOL;\nEND_VAR", |s| s.space_before_colon = false);
}

#[test]
fn space_after_colon_leaks() {
    assert_decl_leaks("VAR\nx : BOOL;\nEND_VAR", |s| s.space_after_colon = false);
}

#[test]
fn space_after_line_comment_leaks() {
    assert_impl_leaks("x := 1; //no space", |s| s.space_after_line_comment = false);
}

#[test]
fn space_around_assign_leaks() {
    // The declaration-initializer `:=` reads `assign_op()`.
    assert_decl_leaks("VAR\nx : INT := 5;\nEND_VAR", |s| {
        s.space_around_assign = false
    });
}

#[test]
fn trailing_comment_min_spaces_leaks() {
    assert_impl_leaks("a := 1; // c", |s| s.trailing_comment_min_spaces = 4);
}

#[test]
fn blank_lines_max_leaks() {
    assert_impl_leaks("a := 1;\n\n\n\n\nb := 2;", |s| s.blank_lines_max = 0);
}

#[test]
fn blank_lines_at_block_start_leaks() {
    // Blank lines before the first statement of a block body (the `stmt_items`
    // path: IF/ELSIF/ELSE, FOR/WHILE/REPEAT, CASE arm bodies).
    assert_impl_leaks("IF a THEN\n\n\nb := 1;\nEND_IF;", |s| {
        s.blank_lines_at_block_start = 2
    });
}

#[test]
fn blank_lines_at_block_start_leaks_case() {
    // Blank lines before the first arm after `CASE … OF` (the `case_stmt` path).
    assert_impl_leaks("CASE x OF\n\n\n1: y := 1;\nEND_CASE;", |s| {
        s.blank_lines_at_block_start = 2
    });
}

#[test]
fn blank_lines_at_block_end_leaks() {
    // Blank lines before a statement block's closer (the `block_tail` path).
    assert_impl_leaks("IF a THEN\nb := 1;\n\n\nEND_IF;", |s| {
        s.blank_lines_at_block_end = 2
    });
}

#[test]
fn blank_lines_at_block_end_leaks_decl() {
    // Blank lines before a declaration container's closer (the `dangling_inner`
    // path: END_VAR/END_STRUCT/END_UNION/enum `)`).
    assert_decl_leaks("VAR\nx : INT;\n\n\nEND_VAR", |s| {
        s.blank_lines_at_block_end = 2
    });
}

#[test]
fn trailing_comma_collection_leaks() {
    // A broken (multi-line) argument list, here an FB instance's default args:
    // the `WhenBroken` policy must emit the trailing comma the house style drops.
    assert_decl_leaks("VAR\nx : FB_Foo(\na := 1,\nb := 2\n);\nEND_VAR", |s| {
        s.trailing_comma_collection = TrailingSeparator::WhenBroken
    });
}

#[test]
fn trailing_comma_enum_leaks() {
    // An enum is always exploded one variant per line; `WhenBroken` adds the
    // trailing comma after the last variant that the house style drops.
    assert_decl_leaks("TYPE e : (red, green, blue); END_TYPE", |s| {
        s.trailing_comma_enum = TrailingSeparator::WhenBroken
    });
}

#[test]
fn trailing_comma_keeps_a_comment_after_the_last_item() {
    // `WhenBroken` adds a comma after the last item; on the next pass the
    // comment that followed the item now follows that comma and must still be
    // printed (it used to be dropped, which the output guard then refused).
    let style = StyleConfig {
        trailing_comma_collection: TrailingSeparator::WhenBroken,
        trailing_comma_enum: TrailingSeparator::WhenBroken,
        ..StyleConfig::default()
    };
    let impls = [
        "x := f(\na,\nb // last\n);",
        "x := f(\na,\nb, // last\n);",
        "x := f(\np := 1,\nq := 2 (* last *)\n);",
    ];
    for src in impls {
        let once = impl_fmt(src, &style);
        assert!(once.contains("last"), "{src:?} lost its comment:\n{once}");
        assert_eq!(impl_fmt(&once, &style), once, "not idempotent for {src:?}");
    }
    let decls = [
        "TYPE e : (\nred,\nblue // last\n); END_TYPE",
        "VAR\nx : FB_Foo(\na := 1,\nb := 2 // last\n);\nEND_VAR",
        "VAR\nx : ARRAY[0..1] OF INT := [\n1,\n2 // last\n];\nEND_VAR",
    ];
    for src in decls {
        let once = decl_fmt(src, &style);
        assert!(once.contains("last"), "{src:?} lost its comment:\n{once}");
        assert_eq!(decl_fmt(&once, &style), once, "not idempotent for {src:?}");
    }
}

#[test]
fn a_removed_blank_line_does_not_split_alignment() {
    // `blank_lines_max = 0` drops the blank line between the two runs, so they
    // must align as one group already on the first pass (idempotence).
    let style = StyleConfig {
        blank_lines_max: 0,
        ..StyleConfig::default()
    };
    let src = "VAR\na : INT;\n\nlonger_name : BOOL;\nEND_VAR\nTYPE e : (\nx := 1,\n\nlonger := 2\n); END_TYPE";
    let once = decl_fmt(src, &style);
    assert!(once.contains("a           : INT;"), "{once}");
    assert!(once.contains("x      := 1,"), "{once}");
    assert_eq!(decl_fmt(&once, &style), once);
}

// A call comfortably wider than 80 columns.
const LONG_CALL: &str = "result := compute_something(first_argument_name, second_argument_name, third_argument_name, fourth_argument_name);";
const LONG_BINARY: &str =
    "result := first_operand_name + second_operand_name + third_operand_name + fourth_operand_name;";
const LONG_LOGICAL: &str = "request := enabled AND ((first_condition AND second_condition) OR (third_condition AND fourth_condition) OR (fifth_condition AND sixth_condition));";

#[test]
fn indent_style_tabs_leaks() {
    assert_impl_leaks("IF a THEN\nb := 1;\nEND_IF;", |s| {
        s.indent_style = IndentStyle::Tabs
    });
    let style = StyleConfig {
        indent_style: IndentStyle::Tabs,
        ..StyleConfig::default()
    };
    assert!(impl_fmt("IF a THEN\nb := 1;\nEND_IF;", &style).contains('\t'));
}

#[test]
fn space_around_binary_op_leaks() {
    assert_impl_leaks("a := b + c;", |s| s.space_around_binary_op = false);
}

#[test]
fn space_after_comma_leaks() {
    assert_impl_leaks("x := foo(a, b);", |s| s.space_after_comma = false);
}

#[test]
fn call_argument_layout_leaks() {
    assert_impl_leaks(
        "long_function_name(\nfirst_argument,\nsecond_argument\n);",
        |s| s.call_argument_layout = CallArgumentLayout::Vertical,
    );
}

#[test]
fn line_width_leaks() {
    assert_impl_leaks(LONG_BINARY, |s| s.line_width = 40);
}

#[test]
fn preserve_line_breaks_leaks() {
    assert_impl_leaks("x := a +\nb;", |s| s.preserve_line_breaks = true);
}

#[test]
fn wrap_before_operator_leaks() {
    let before = StyleConfig {
        line_width: 40,
        ..StyleConfig::default()
    };
    let after = StyleConfig {
        wrap_before_operator: false,
        ..before.clone()
    };
    assert_ne!(
        impl_fmt(LONG_BINARY, &before),
        impl_fmt(LONG_BINARY, &after)
    );
}

#[test]
fn logical_expression_layout_leaks() {
    let expanded = StyleConfig {
        line_width: 80,
        ..StyleConfig::default()
    };
    let compact = StyleConfig {
        logical_expression_layout: LogicalExpressionLayout::Compact,
        ..expanded.clone()
    };
    assert_ne!(
        impl_fmt(LONG_LOGICAL, &expanded),
        impl_fmt(LONG_LOGICAL, &compact)
    );
}

#[test]
fn parenthesized_expression_layout_leaks() {
    assert_impl_leaks(
        "IF NOT (first_long_condition OR second_long_condition OR third_long_condition OR fourth_long_condition) THEN result := TRUE; END_IF;",
        |s| {
            s.line_width = 80;
            s.parenthesized_expression_layout = ParenthesizedExpressionLayout::Vertical;
        },
    );
}

#[test]
fn then_placement_leaks() {
    assert_impl_leaks("IF a THEN\nb := 1;\nEND_IF;", |s| {
        s.then_placement = BlockTokenPlacement::OwnLine
    });
}

#[test]
fn do_placement_leaks() {
    assert_impl_leaks("WHILE a DO\nb := 1;\nEND_WHILE;", |s| {
        s.do_placement = BlockTokenPlacement::OwnLine
    });
}

#[test]
fn of_placement_leaks() {
    assert_impl_leaks("CASE x OF\n1: y := 1;\nEND_CASE;", |s| {
        s.of_placement = BlockTokenPlacement::OwnLine
    });
}

#[test]
fn indent_block_token_leaks() {
    // Only meaningful once a placement is OwnLine.
    let own_line = StyleConfig {
        then_placement: BlockTokenPlacement::OwnLine,
        ..StyleConfig::default()
    };
    let indented = StyleConfig {
        indent_block_token: true,
        ..own_line.clone()
    };
    let src = "IF a THEN\nb := 1;\nEND_IF;";
    assert_eq!(impl_fmt(src, &own_line), "IF a\nTHEN\n    b := 1;\nEND_IF;");
    assert_eq!(
        impl_fmt(src, &indented),
        "IF a\n    THEN\n    b := 1;\nEND_IF;"
    );
    // Alone (same-line placement) it changes nothing.
    assert_impl_no_leak(
        src,
        "indent_block_token applies to own-line tokens only",
        |s| s.indent_block_token = true,
    );
}

#[test]
fn place_simple_if_on_single_line_leaks() {
    assert_impl_leaks("IF a THEN\nb := 1;\nEND_IF;", |s| {
        s.place_simple_if_on_single_line = true
    });
}

#[test]
fn block_token_placement_keeps_a_commented_gap() {
    let style = StyleConfig {
        then_placement: BlockTokenPlacement::OwnLine,
        ..StyleConfig::default()
    };
    assert_eq!(
        impl_fmt("IF a (* why *) THEN\nb := 1;\nEND_IF;", &style),
        "IF a (* why *) THEN\n    b := 1;\nEND_IF;"
    );
}

#[test]
fn space_inside_parens_leaks() {
    let style = StyleConfig {
        space_inside_parens: true,
        ..StyleConfig::default()
    };
    assert_eq!(
        impl_fmt("x := foo(a + (b * c), bar());", &style),
        "x := foo( a + ( b * c ), bar() );"
    );
    // Array brackets are not parentheses.
    assert_eq!(
        decl_fmt("VAR\nx : ARRAY[0..2] OF INT := [1, 2, 3];\nEND_VAR", &style),
        "VAR\n    x : ARRAY[0..2] OF INT := [1, 2, 3];\nEND_VAR"
    );
}

#[test]
fn align_decl_colon_leaks() {
    let style = StyleConfig {
        align_decl_colon: ColonAlignment::Off,
        ..StyleConfig::default()
    };
    assert_eq!(
        decl_fmt(
            "VAR\nshort : INT := 1;\naMuchLongerName : BOOL := TRUE;\nEND_VAR",
            &style
        ),
        "VAR\n    short : INT := 1;\n    aMuchLongerName : BOOL := TRUE;\nEND_VAR"
    );
}

#[test]
fn align_at_addresses_leaks() {
    let style = StyleConfig {
        align_at_addresses: false,
        ..StyleConfig::default()
    };
    // `:` still aligns; only the `AT` column is no longer padded.
    assert_eq!(
        decl_fmt(
            "VAR\nx AT %IX0.0 : BOOL;\nyy AT %IW10 : UINT;\nEND_VAR",
            &style
        ),
        "VAR\n    x AT %IX0.0 : BOOL;\n    yy AT %IW10 : UINT;\nEND_VAR"
    );
}

// Every wired option, toggled to a non-default value, must keep formatting
// idempotent (format twice = format once).

type Mutation = fn(&mut StyleConfig);

#[test]
fn wired_options_stay_idempotent() {
    let mutations: &[(&str, Mutation)] = &[
        ("keyword_case", |s| s.keyword_case = KeywordCase::Lower),
        ("type_keyword_case", |s| {
            s.type_keyword_case = KeywordCase::Lower
        }),
        ("indent_width", |s| s.indent_width = 2),
        ("indent_style", |s| s.indent_style = IndentStyle::Tabs),
        ("space_around_binary_op", |s| {
            s.space_around_binary_op = false
        }),
        ("space_after_comma", |s| s.space_after_comma = false),
        ("call_argument_layout", |s| {
            s.call_argument_layout = CallArgumentLayout::Vertical
        }),
        ("line_width", |s| s.line_width = 40),
        ("preserve_line_breaks", |s| s.preserve_line_breaks = true),
        ("wrap_before_operator", |s| {
            s.line_width = 40;
            s.wrap_before_operator = false;
        }),
        ("logical_expression_layout", |s| {
            s.line_width = 80;
            s.logical_expression_layout = LogicalExpressionLayout::Compact
        }),
        ("parenthesized_expression_layout", |s| {
            s.line_width = 80;
            s.parenthesized_expression_layout = ParenthesizedExpressionLayout::Vertical
        }),
        ("space_before_colon", |s| s.space_before_colon = false),
        ("space_after_colon", |s| s.space_after_colon = false),
        ("space_around_assign", |s| s.space_around_assign = false),
        ("space_after_line_comment", |s| {
            s.space_after_line_comment = false
        }),
        ("trailing_comment_min_spaces", |s| {
            s.trailing_comment_min_spaces = 4
        }),
        ("blank_lines_max", |s| s.blank_lines_max = 0),
        ("blank_lines_at_block_start", |s| {
            s.blank_lines_at_block_start = 2
        }),
        ("blank_lines_at_block_end", |s| {
            s.blank_lines_at_block_end = 2
        }),
        ("trailing_comma_collection", |s| {
            s.trailing_comma_collection = TrailingSeparator::WhenBroken
        }),
        ("trailing_comma_enum", |s| {
            s.trailing_comma_enum = TrailingSeparator::WhenBroken
        }),
        ("block_token_placement", |s| {
            s.then_placement = BlockTokenPlacement::OwnLine;
            s.do_placement = BlockTokenPlacement::OwnLine;
            s.of_placement = BlockTokenPlacement::OwnLine;
            s.indent_block_token = true;
        }),
        ("place_simple_if_on_single_line", |s| {
            s.place_simple_if_on_single_line = true
        }),
        ("collection_max_inline", |s| s.collection_max_inline = 1),
        ("arg_break", |s| s.arg_break = CollectionBreak::Fit),
        ("align_impl_assignments", |s| {
            s.align_impl_assignments = true
        }),
        ("align_trailing_comments", |s| {
            s.align_trailing_comments = true
        }),
        ("blank_lines_after_decl", |s| s.blank_lines_after_decl = 1),
        ("blank_lines_around_block_statement", |s| {
            s.blank_lines_around_block_statement = 1
        }),
        ("blank_lines_after_case", |s| {
            s.blank_lines_after_case = 1;
            s.blank_lines_after_multiline_case = 2;
        }),
        ("blank_lines_before_multiline_comment", |s| {
            s.blank_lines_before_multiline_comment = 1
        }),
        ("space_inside_parens", |s| s.space_inside_parens = true),
        ("align_decl_colon", |s| {
            s.align_decl_colon = ColonAlignment::Off
        }),
        ("align_at_addresses", |s| s.align_at_addresses = false),
    ];
    let impl_src = format!(
        "IF a THEN\n\n\nb := 1; // c\n\n\nEND_IF;\n{LONG_CALL}\n{LONG_BINARY}\n{LONG_LOGICAL}\nsum := first_operand_name +\nsecond_operand_name;\nfb(x := 1, y => z);\nIF ready THEN\ngo := TRUE;\nEND_IF;\nCASE s OF\n1: t := 1;\n2:\nt := 2;\nu := 3;\nELSE\nt := 0;\nEND_CASE;\n(* a\nmultiline note *)\nv := 4;"
    );
    // The blank line separates two alignment groups: blank_lines_max = 0 removes
    // it, and the second pass must still align the members the same way.
    let var_src = "VAR\nx : INT := 5; // c\n\nlonger_name : DINT;\ny AT %IX0.0 : BOOL;\nz : FB_Foo(\na := 1,\nb := 2\n);\nEND_VAR";
    let enum_src = "TYPE e : (red, green, blue); END_TYPE";

    for (name, mutate) in mutations {
        let mut style = StyleConfig::default();
        mutate(&mut style);
        let once = impl_fmt(&impl_src, &style);
        assert_eq!(
            once,
            impl_fmt(&once, &style),
            "impl output not idempotent under non-default {name}"
        );
        for decl_src in [var_src, enum_src] {
            let once = decl_fmt(decl_src, &style);
            assert_eq!(
                once,
                decl_fmt(&once, &style),
                "decl output not idempotent under non-default {name}"
            );
        }
    }
}

#[test]
fn collection_max_inline_leaks() {
    assert_impl_leaks("x := foo(a, b);", |s| s.collection_max_inline = 1);
}

#[test]
fn struct_initializer_max_inline_leaks() {
    assert_decl_leaks(
        "TYPE ST_Test : STRUCT\nInlet : ST_Limits := (Low := 1.0, High := 2.0, Window := 3.0);\nEND_STRUCT\nEND_TYPE",
        |s| s.struct_initializer_max_inline = 3,
    );
}

#[test]
fn array_initializer_max_inline_leaks() {
    assert_decl_leaks(
        "VAR\naSteps : ARRAY[1..5] OF INT := [10, 20, 30, 40, 50];\nEND_VAR",
        |s| s.array_initializer_max_inline = 0,
    );
}

#[test]
fn logical_chain_max_inline_leaks() {
    assert_impl_leaks("bReady := bA AND bB AND bC AND bD;", |s| {
        s.logical_chain_max_inline = 0
    });
}

#[test]
fn arg_break_leaks() {
    // Source-broken on purpose: a flat call renders the same under both.
    assert_impl_leaks("x := foo(\na,\nb\n);", |s| {
        s.arg_break = CollectionBreak::Fit
    });
}

#[test]
fn align_impl_assignments_leaks() {
    assert_impl_leaks("a := 1;\nlongername := 2;", |s| {
        s.align_impl_assignments = true
    });
}

#[test]
fn align_trailing_comments_leaks() {
    assert_impl_leaks("a := 1; // one\nlongername := 2; // two", |s| {
        s.align_trailing_comments = true
    });
    assert_decl_leaks(
        "VAR\nx : INT; // one\nlonger : BOOL; // two\nEND_VAR",
        |s| s.align_trailing_comments = true,
    );
}

// The `blank_lines_*` minimums (after declarations, around block statements,
// after CASE arms, before multiline comments) raise the source's blank lines,
// so they show on a blank-free seam with a nonzero value.

#[test]
fn blank_lines_after_decl_leaks() {
    assert_decl_leaks(
        "VAR\nx : INT;\nEND_VAR\nVAR_INPUT\ny : INT;\nEND_VAR",
        |s| s.blank_lines_after_decl = 1,
    );
}

#[test]
fn blank_lines_around_block_statement_leaks() {
    assert_impl_leaks("a := 1;\nIF b THEN\nc := 1;\nEND_IF;\nd := 2;", |s| {
        s.blank_lines_around_block_statement = 1
    });
}

#[test]
fn blank_lines_after_case_leaks() {
    assert_impl_leaks("CASE x OF\n1: a := 1;\n2: b := 2;\nEND_CASE;", |s| {
        s.blank_lines_after_case = 1
    });
}

#[test]
fn blank_lines_after_multiline_case_leaks() {
    assert_impl_leaks(
        "CASE x OF\n1:\na := 1;\nb := 2;\n2: c := 3;\nEND_CASE;",
        |s| s.blank_lines_after_multiline_case = 1,
    );
}

#[test]
fn blank_lines_before_multiline_comment_leaks() {
    assert_impl_leaks("a := 1;\n(* a\nmultiline\ncomment *)\nb := 2;", |s| {
        s.blank_lines_before_multiline_comment = 1
    });
    assert_decl_leaks(
        "VAR\nx : INT;\n(* a\nmultiline\ncomment *)\ny : INT;\nEND_VAR",
        |s| s.blank_lines_before_multiline_comment = 1,
    );
}
