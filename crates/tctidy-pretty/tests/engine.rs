//! Engine isolation tests: the printer is exercised with hand-built `Doc`
//! values, no parser involved.

use tctidy_pretty::prelude::*;
use tctidy_pretty::{print, IndentStyle, LineEnding, PrintOptions};

fn opts(width: u16) -> PrintOptions {
    PrintOptions {
        width,
        indent_width: 4,
        indent_style: IndentStyle::Spaces,
        eol: LineEnding::Lf,
        align_line_suffixes: false,
    }
}

#[test]
fn text_and_concat() {
    assert_eq!(print(&concat([text("a"), text("bc")]), &opts(80)), "abc");
}

#[test]
fn group_breaks_at_width_boundary() {
    // "aaaa bbbb" is 9 columns.
    let doc = group(concat([text("aaaa"), line(), text("bbbb")]));
    assert_eq!(print(&doc, &opts(9)), "aaaa bbbb"); // fits
    assert_eq!(print(&doc, &opts(8)), "aaaa\nbbbb"); // breaks
}

#[test]
fn hard_line_forces_break_regardless_of_width() {
    let doc = group(concat([text("a"), hard_line(), text("b")]));
    assert_eq!(print(&doc, &opts(1000)), "a\nb");
}

#[test]
fn crlf_is_emitted() {
    let doc = group(concat([text("a"), hard_line(), text("b")]));
    let o = PrintOptions {
        eol: LineEnding::Crlf,
        ..opts(1000)
    };
    assert_eq!(print(&doc, &o), "a\r\nb");
    assert!(!print(&doc, &o).contains("\n\n")); // no lone LF either
}

#[test]
fn blank_lines_have_no_trailing_whitespace() {
    // Indented context so a naive printer would leave the indent on the blank line.
    let doc = indent(concat([text("x"), hard_line(), hard_line(), text("y")]));
    assert_eq!(print(&doc, &opts(80)), "x\n\n    y");
}

#[test]
fn line_suffix_stays_at_end_of_line() {
    let doc = concat([
        text("a;"),
        line_suffix(concat([text(" "), text("// c")])),
        hard_line(),
        text("b"),
    ]);
    assert_eq!(print(&doc, &opts(80)), "a; // c\nb");
}

#[test]
fn line_suffix_boundary_flushes_inline() {
    let doc = concat([
        text("a"),
        line_suffix(text("X")),
        line_suffix_boundary(),
        text("b"),
    ]);
    assert_eq!(print(&doc, &opts(80)), "aXb");
}

#[test]
fn if_break_trailing_comma() {
    let doc = group(concat([
        text("["),
        indent(concat([
            soft_line(),
            text("1"),
            text(","),
            soft_line(),
            text("2"),
            if_break(text(","), nil()),
        ])),
        soft_line(),
        text("]"),
    ]));
    assert_eq!(print(&doc, &opts(80)), "[1,2]"); // flat: no trailing comma
    assert_eq!(print(&doc, &opts(3)), "[\n    1,\n    2,\n]"); // broken: trailing comma
}

#[test]
fn if_break_group_selects_by_referenced_group() {
    let mut ids = GroupIdBuilder::default();

    let g = ids.fresh();
    let broken = group_with_id(
        g,
        concat([
            break_parent(),
            if_break_group(g, text("BROKE"), text("FLAT")),
        ]),
    );
    assert_eq!(print(&broken, &opts(80)), "BROKE");

    let g2 = ids.fresh();
    let flat = group_with_id(g2, if_break_group(g2, text("BROKE"), text("FLAT")));
    assert_eq!(print(&flat, &opts(80)), "FLAT");
}

#[test]
fn indent_if_break_indents_only_when_broken() {
    let mut ids = GroupIdBuilder::default();
    let g = ids.fresh();
    let doc = group_with_id(
        g,
        concat([
            text("x"),
            break_parent(),
            indent_if_break(g, concat([hard_line(), text("y")])),
        ]),
    );
    assert_eq!(print(&doc, &opts(80)), "x\n    y");
}

#[test]
fn align_adds_exact_columns() {
    let doc = concat([
        text("a"),
        align(IndentSpec::Spaces(3), concat([hard_line(), text("b")])),
    ]);
    assert_eq!(print(&doc, &opts(80)), "a\n   b");
}

#[test]
fn conditional_group_picks_first_fitting() {
    let doc = conditional_group(vec![
        text("short"),
        concat([text("a"), hard_line(), text("b")]),
    ]);
    assert_eq!(print(&doc, &opts(80)), "short"); // first state fits
    assert_eq!(print(&doc, &opts(2)), "a\nb"); // falls back to expanded
}

#[test]
fn deterministic() {
    let doc = group(concat([text("aaaa"), line(), text("bbbb")]));
    assert_eq!(print(&doc, &opts(8)), print(&doc, &opts(8)));
}

fn tab_opts(width: u16) -> PrintOptions {
    PrintOptions {
        indent_style: IndentStyle::Tabs,
        ..opts(width)
    }
}

#[test]
fn tab_indentation_renders_tabs() {
    let doc = indent(concat([hard_line(), text("x")]));
    assert_eq!(print(&doc, &tab_opts(80)), "\n\tx");

    let nested = indent(indent(concat([hard_line(), text("x")])));
    assert_eq!(print(&nested, &tab_opts(80)), "\n\t\tx");
}

#[test]
fn tab_indentation_mixes_spaces_for_partial_columns() {
    // indent (4 columns) + align 2 = 6 columns = one tab unit + 2 spaces.
    let doc = indent(align(
        IndentSpec::Spaces(2),
        concat([hard_line(), text("x")]),
    ));
    assert_eq!(print(&doc, &tab_opts(80)), "\n\t  x");
}

#[test]
fn tab_indentation_counts_columns_for_fitting() {
    // At 4 columns of indent, "aaaa bbbb" (9 cols) needs width 13.
    let doc = indent(concat([
        hard_line(),
        group(concat([text("aaaa"), line(), text("bbbb")])),
    ]));
    assert_eq!(print(&doc, &tab_opts(13)), "\n\taaaa bbbb");
    assert_eq!(print(&doc, &tab_opts(12)), "\n\taaaa\n\tbbbb");
}

#[test]
fn width_one_breaks_every_group() {
    let doc = group(concat([text("aa"), line(), text("bb")]));
    assert_eq!(print(&doc, &opts(1)), "aa\nbb");
}

#[test]
fn extreme_width_never_breaks() {
    let doc = group(concat([text("aa"), line(), text("bb")]));
    assert_eq!(print(&doc, &opts(u16::MAX)), "aa bb");
}

#[test]
fn indent_width_zero_does_not_misindent() {
    let o = PrintOptions {
        indent_width: 0,
        ..opts(80)
    };
    let doc = indent(indent(concat([hard_line(), text("x")])));
    assert_eq!(print(&doc, &o), "\nx");

    let tabs = PrintOptions {
        indent_width: 0,
        indent_style: IndentStyle::Tabs,
        ..opts(80)
    };
    assert_eq!(print(&doc, &tabs), "\nx");
}

#[test]
fn group_entered_past_the_width_breaks_immediately() {
    // The leading text already overflows: the fitting check starts with a
    // negative remainder and must bail out rather than scan the group.
    let doc = concat([text("aaaaaa"), group(concat([line(), text("b")]))]);
    assert_eq!(print(&doc, &opts(3)), "aaaaaa\nb");
}

#[test]
fn if_break_group_referencing_an_unprinted_group_uses_its_own_mode() {
    let mut ids = GroupIdBuilder::default();

    // `never` is referenced but no group with that id is ever printed, so the
    // mode lookup misses and falls back to the enclosing mode.
    let never = ids.fresh();
    let doc = group(concat([
        text("aa"),
        line(),
        if_break_group(never, text("BROKE"), text("FLAT")),
    ]));
    assert_eq!(print(&doc, &opts(80)), "aa FLAT");
    assert_eq!(print(&doc, &opts(3)), "aa\nBROKE");
}

#[test]
fn line_suffixes_at_end_of_document_flush_in_order() {
    let doc = concat([
        text("a"),
        line_suffix(text("X")),
        line_suffix(concat([text("Y"), text("Z")])),
    ]);
    assert_eq!(print(&doc, &opts(80)), "aXYZ");
}

#[test]
fn multiline_line_suffix_at_end_of_document_uses_normal_rendering() {
    let suffix = concat([text(" (*alpha"), hard_literal(), text("beta*)")]);
    let at_eof = concat([text("x := 1;"), line_suffix(suffix.clone())]);
    let before_content = concat([
        text("x := 1;"),
        line_suffix(suffix),
        hard_line(),
        text("y := 2;"),
    ]);

    assert_eq!(print(&at_eof, &opts(80)), "x := 1; (*alpha\nbeta*)");
    assert_eq!(
        print(&before_content, &opts(80)),
        "x := 1; (*alpha\nbeta*)\ny := 2;"
    );
}

#[test]
fn multiline_line_suffix_at_eof_uses_configured_crlf() {
    let doc = concat([
        text("x := 1;"),
        line_suffix(concat([text(" /*alpha"), hard_literal(), text("beta*/")])),
    ]);
    let o = PrintOptions {
        eol: LineEnding::Crlf,
        ..opts(80)
    };

    assert_eq!(print(&doc, &o), "x := 1; /*alpha\r\nbeta*/");
}

// The remaining fits() walks: each of these drives a Doc shape through the
// width measurement itself, not just through printing.

#[test]
fn trailing_commands_after_a_group_count_toward_its_fit() {
    // The `;` queued after the group must be part of the measurement.
    let doc = concat([group(concat([text("aa"), line(), text("bb")])), text(";")]);
    assert_eq!(print(&doc, &opts(6)), "aa bb;");
    assert_eq!(print(&doc, &opts(5)), "aa\nbb;");
}

#[test]
fn space_line_overflowing_by_itself_breaks_the_group() {
    // The text exactly fills the width; the flat space of the line is the
    // overflowing column.
    let doc = group(concat([text("aaaaa"), line(), text("b")]));
    assert_eq!(print(&doc, &opts(5)), "aaaaa\nb");
}

#[test]
fn aligned_content_is_measured_inside_a_group() {
    let doc = group(concat([
        text("a"),
        line(),
        align(IndentSpec::Spaces(2), text("bb")),
        align_current(text("c")),
    ]));
    assert_eq!(print(&doc, &opts(5)), "a bbc");
    assert_eq!(print(&doc, &opts(4)), "a\nbbc");
}

#[test]
fn indent_if_break_content_is_measured_inside_a_group() {
    let mut ids = GroupIdBuilder::default();
    let g = ids.fresh();
    let doc = group_with_id(
        g,
        concat([text("aa"), indent_if_break(g, concat([line(), text("bb")]))]),
    );
    assert_eq!(print(&doc, &opts(5)), "aa bb");
    assert_eq!(print(&doc, &opts(4)), "aa\n    bb");
}

#[test]
fn line_suffix_is_zero_width_for_fitting() {
    // "a; b" is exactly 4 columns; the deferred comment must not break the group.
    let doc = group(concat([
        text("a;"),
        line_suffix(text(" // long trailing comment")),
        line(),
        text("b"),
    ]));
    assert_eq!(print(&doc, &opts(4)), "a; b // long trailing comment");
}

#[test]
fn nested_conditional_group_is_measured_by_its_first_state() {
    let inner = conditional_group(vec![text("bb"), text("cccc")]);
    let doc = group(concat([text("a"), line(), inner]));
    assert_eq!(print(&doc, &opts(4)), "a bb");
}

#[test]
fn forced_break_group_inside_an_if_break_arm_is_seen_by_the_fit() {
    // propagate_breaks stops at IfBreak, so this is the one way a should_break
    // group reaches the measurement: it flips the walk to break mode, where the
    // soft line ends the check early.
    let forced = group(concat([break_parent(), line(), text("x")]));
    let doc = group(concat([text("aaaa"), line(), if_break(nil(), forced)]));
    // Double space is expected: the outer group prints flat (its own line is a
    // space) and the forced group's line also flattens to a space, because
    // flat mode wins over should_break at the print stage.
    assert_eq!(print(&doc, &opts(80)), "aaaa  x");
}

#[test]
fn print_options_default_values_are_pinned() {
    // Change detector on the type's own Default. Production never uses it:
    // StyleConfig builds PrintOptions explicitly, and its house default is
    // line_width 160, not this 100.
    let d = PrintOptions::default();
    assert_eq!(d.width, 100);
    assert_eq!(d.indent_width, 4);
    assert_eq!(d.indent_style, IndentStyle::Spaces);
    assert_eq!(d.eol, LineEnding::Lf);
    assert!(!d.align_line_suffixes);
}

fn aligned(width: u16) -> PrintOptions {
    PrintOptions {
        align_line_suffixes: true,
        ..opts(width)
    }
}

/// A line of code followed by a deferred ` // c` suffix.
fn suffixed(code: &str, comment: &str) -> Doc {
    concat([
        text(code.to_string()),
        line_suffix(concat([text(" "), text(comment.to_string())])),
    ])
}

#[test]
fn line_suffixes_of_consecutive_lines_align() {
    let doc = concat([
        suffixed("a", "// one"),
        hard_line(),
        suffixed("longer", "// two"),
        hard_line(),
        indent(concat([hard_line(), suffixed("mid", "// three")])),
    ]);
    // The blank line ends the run; a lone suffix keeps its gap.
    assert_eq!(
        print(&doc, &aligned(80)),
        "a      // one\nlonger // two\n\n    mid // three"
    );
    assert_eq!(
        print(&doc, &opts(80)),
        "a // one\nlonger // two\n\n    mid // three"
    );
}

#[test]
fn a_line_without_suffix_ends_the_run() {
    let doc = concat([
        suffixed("a", "// one"),
        hard_line(),
        text("plain"),
        hard_line(),
        suffixed("bb", "// two"),
        hard_line(),
        suffixed("c", "// three"),
    ]);
    assert_eq!(
        print(&doc, &aligned(80)),
        "a // one\nplain\nbb // two\nc  // three"
    );
}

#[test]
fn a_multiline_suffix_ends_the_run() {
    // The suffix's own continuation line is not a suffixed line: the run of
    // `bbbb` and `c` starts after it, and `a` keeps its gap.
    let doc = concat([
        text("a"),
        line_suffix(concat([text(" (* x"), hard_literal(), text("y *)")])),
        hard_line(),
        suffixed("bbbb", "// two"),
        hard_line(),
        suffixed("c", "// three"),
    ]);
    assert_eq!(
        print(&doc, &aligned(80)),
        "a (* x\ny *)\nbbbb // two\nc    // three"
    );
}

#[test]
fn suffix_alignment_never_moves_line_breaks() {
    // The suffix is ignored by the fit, so the group stays flat even though the
    // aligned comment ends past the width.
    let doc = concat([
        suffixed("a", "// one"),
        hard_line(),
        group(concat([text("long"), line(), text("name")])),
        line_suffix(text(" // two")),
    ]);
    assert_eq!(
        print(&doc, &aligned(9)),
        "a         // one\nlong name // two"
    );
}

#[test]
fn conditional_group_falls_back_to_the_last_state() {
    // No state fits in 2 columns: the most-expanded (last) state is taken.
    let doc = conditional_group(vec![text("aaaa"), text("bbbbb")]);
    assert_eq!(print(&doc, &opts(2)), "bbbbb");
}
