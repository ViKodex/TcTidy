//! Behavior of the style settings, one section per setting: the output at a
//! non-default value, pinned with inline snapshots, plus the cases the setting
//! must leave alone. `style_leak.rs` only proves that a setting changes the
//! output; this file shows how.

use insta::assert_snapshot;

use tctidy_format::style::CollectionBreak;
use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_declaration, parse_implementation};

/// Format `src` with the default style changed by `mutate`, and check that a
/// second pass changes nothing.
fn imp(src: &str, mutate: impl FnOnce(&mut StyleConfig)) -> String {
    let mut style = StyleConfig::default();
    mutate(&mut style);
    let format = |src: &str| {
        format_implementation(&parse_implementation(src), &style, LineEnding::Lf)
            .expect("implementation formats cleanly")
    };
    let once = format(src);
    assert_eq!(format(&once), once, "not idempotent for {src:?}");
    once
}

/// [`imp`] for a declaration section.
fn dec(src: &str, mutate: impl FnOnce(&mut StyleConfig)) -> String {
    let mut style = StyleConfig::default();
    mutate(&mut style);
    let format = |src: &str| {
        format_declaration(&parse_declaration(src), &style, LineEnding::Lf)
            .expect("declaration formats cleanly")
    };
    let once = format(src);
    assert_eq!(format(&once), once, "not idempotent for {src:?}");
    once
}

// ---- place_simple_if_on_single_line ----

fn single_line_if(src: &str) -> String {
    imp(src, |s| s.place_simple_if_on_single_line = true)
}

#[test]
fn simple_if_goes_on_one_line() {
    assert_snapshot!(
        single_line_if("if bReset then\nnCount:=0;\nend_if"),
        @"IF bReset THEN nCount := 0; END_IF"
    );
    assert_snapshot!(
        single_line_if("IF bDone THEN RETURN; END_IF;"),
        @"IF bDone THEN RETURN; END_IF;"
    );
    assert_snapshot!(
        single_line_if("IF bStart THEN fbMotor(bEnable := TRUE); END_IF;"),
        @"IF bStart THEN fbMotor(bEnable := TRUE); END_IF;"
    );
}

#[test]
fn simple_if_is_off_by_default() {
    assert_snapshot!(
        imp("IF bReset THEN nCount := 0; END_IF;", |_| {}),
        @r"
    IF bReset THEN
        nCount := 0;
    END_IF;
    "
    );
}

#[test]
fn simple_if_nested_in_a_block() {
    assert_snapshot!(
        single_line_if("FOR i := 1 TO 10 DO\nIF a[i] < 0 THEN\na[i] := 0;\nEND_IF;\nEND_FOR;"),
        @r"
    FOR i := 1 TO 10 DO
        IF a[i] < 0 THEN a[i] := 0; END_IF;
    END_FOR;
    "
    );
}

#[test]
fn simple_if_keeps_a_trailing_comment_after_end_if() {
    assert_snapshot!(
        single_line_if("IF bReset THEN\nnCount := 0;\nEND_IF; // clear"),
        @"IF bReset THEN nCount := 0; END_IF; // clear"
    );
}

#[test]
fn an_if_that_does_not_fit_keeps_its_block() {
    let src = "IF bFirstVeryLongConditionName AND bSecondVeryLongConditionName THEN nResultValue := nResultValue + 1; END_IF;";
    assert_snapshot!(
        imp(src, |s| {
            s.place_simple_if_on_single_line = true;
            s.line_width = 80;
        }),
        @r"
    IF bFirstVeryLongConditionName AND bSecondVeryLongConditionName THEN
        nResultValue := nResultValue + 1;
    END_IF;
    "
    );
}

#[test]
fn only_simple_ifs_go_on_one_line() {
    // Two statements, an ELSE, an ELSIF, a nested block, a comment inside, a
    // call that always breaks, and THEN on its own line all keep the block.
    let cases = [
        "IF a THEN b := 1; c := 2; END_IF;",
        "IF a THEN b := 1; ELSE b := 2; END_IF;",
        "IF a THEN b := 1; ELSIF c THEN b := 2; END_IF;",
        "IF a THEN IF c THEN b := 1; END_IF; END_IF;",
        "IF a THEN b := 1; (* why *) END_IF;",
        "IF a (* why *) THEN b := 1; END_IF;",
        "IF a THEN // why\nb := 1; END_IF;",
        "IF a THEN fb(x := 1, y := 2); END_IF;",
        "IF a THEN ; END_IF;",
    ];
    for src in cases {
        let output = single_line_if(src);
        assert!(
            output.lines().count() > 1,
            "{src:?} went on one line: {output}"
        );
    }
    assert_snapshot!(
        imp("IF a THEN b := 1; END_IF;", |s| {
            s.place_simple_if_on_single_line = true;
            s.then_placement = tctidy_format::style::BlockTokenPlacement::OwnLine;
        }),
        @r"
    IF a
    THEN
        b := 1;
    END_IF;
    "
    );
}

// ---- collection_max_inline ----

fn max_args(src: &str, limit: u16) -> String {
    imp(src, |s| s.collection_max_inline = limit)
}

#[test]
fn a_call_over_the_argument_limit_breaks() {
    assert_snapshot!(max_args("nMax := MAX(nA, nB, nC, nD);", 3), @r"
    nMax := MAX(nA,
                nB,
                nC,
                nD
                );
    ");
    assert_snapshot!(
        imp("nMax := MAX(nA, nB, nC, nD);", |s| {
            s.collection_max_inline = 3;
            s.call_argument_layout = tctidy_format::style::CallArgumentLayout::Vertical;
        }),
        @r"
    nMax := MAX(
        nA,
        nB,
        nC,
        nD
    );
    "
    );
}

#[test]
fn a_call_within_the_argument_limit_stays_inline() {
    assert_snapshot!(max_args("nMax := MAX(nA, nB, nC);", 3), @"nMax := MAX(nA, nB, nC);");
    assert_snapshot!(max_args("Reset();", 1), @"Reset();");
}

#[test]
fn no_argument_limit_by_default() {
    assert_snapshot!(
        max_args("nMax := MAX(nA, nB, nC, nD, nE, nF, nG, nH);", 0),
        @"nMax := MAX(nA, nB, nC, nD, nE, nF, nG, nH);"
    );
}

#[test]
fn the_argument_limit_counts_each_list_on_its_own() {
    // The inner call has two arguments: within the limit, it stays inline
    // while the outer list breaks.
    assert_snapshot!(max_args("x := LIMIT(nLow, MAX(nA, nB), nHigh);", 2), @r"
    x := LIMIT(nLow,
               MAX(nA, nB),
               nHigh
               );
    ");
}

// ---- arg_break ----

fn fit(src: &str) -> String {
    imp(src, |s| s.arg_break = CollectionBreak::Fit)
}

#[test]
fn a_source_broken_argument_list_stays_broken_by_default() {
    assert_snapshot!(imp("Move(fTarget,\nfVelocity,\nbExecute);", |_| {}), @r"
    Move(fTarget,
         fVelocity,
         bExecute
         );
    ");
}

#[test]
fn fit_joins_a_source_broken_argument_list() {
    assert_snapshot!(
        fit("Move(fTarget,\nfVelocity,\nbExecute);"),
        @"Move(fTarget, fVelocity, bExecute);"
    );
    assert_snapshot!(
        fit("x := LIMIT(\n0,\nMAX(a,\nb),\n100\n);"),
        @"x := LIMIT(0, MAX(a, b), 100);"
    );
}

#[test]
fn fit_still_breaks_on_width_limit_and_named_arguments() {
    assert_snapshot!(
        imp("Move(fFirstPosition,\nfSecondPosition);", |s| {
            s.arg_break = CollectionBreak::Fit;
            s.line_width = 30;
        }),
        @r"
    Move(fFirstPosition,
         fSecondPosition
         );
    "
    );
    assert_snapshot!(
        imp("Move(fTarget,\nfVelocity,\nbExecute);", |s| {
            s.arg_break = CollectionBreak::Fit;
            s.collection_max_inline = 2;
        }),
        @r"
    Move(fTarget,
         fVelocity,
         bExecute
         );
    "
    );
    assert_snapshot!(fit("fbMove(Execute := TRUE,\nPosition := 10);"), @r"
    fbMove(Execute  := TRUE,
           Position := 10);
    ");
}

#[test]
fn fit_keeps_a_commented_argument_list() {
    assert_snapshot!(fit("Move(fTarget, // where\nfVelocity);"), @r"
    Move(fTarget, // where
         fVelocity);
    ");
}

// ---- align_impl_assignments ----

fn aligned(src: &str) -> String {
    imp(src, |s| s.align_impl_assignments = true)
}

#[test]
fn consecutive_assignments_align() {
    assert_snapshot!(
        aligned("nCount := 0;\nbDone := FALSE;\nstAxis.fPosition := 10.0;\naValues[i] := x;"),
        @r"
    nCount           := 0;
    bDone            := FALSE;
    stAxis.fPosition := 10.0;
    aValues[i]       := x;
    "
    );
}

#[test]
fn assignment_runs_break_at_blank_lines_comments_and_other_statements() {
    assert_snapshot!(
        aligned("a := 1;\nbb := 2;\n\nccc := 3;\nd := 4;\n// note\neeeee := 5;\nf := 6;\nReset();\ngggggg := 7;\nh := 8;"),
        @r"
    a  := 1;
    bb := 2;

    ccc := 3;
    d   := 4;
    // note
    eeeee := 5;
    f     := 6;
    Reset();
    gggggg := 7;
    h      := 8;
    "
    );
}

#[test]
fn only_plain_assignments_align() {
    // `REF=`, `S=`, `R=` and chained assignments keep their spacing and end
    // the run.
    assert_snapshot!(
        aligned("a := 1;\npRef REF= x;\nbb := 2;\nbSet S= TRUE;\nccc := 3;\nd := e := 4;\nffff := 5;"),
        @r"
    a := 1;
    pRef REF= x;
    bb := 2;
    bSet S= TRUE;
    ccc := 3;
    d := e := 4;
    ffff := 5;
    "
    );
}

#[test]
fn aligned_assignments_keep_their_comments() {
    assert_snapshot!(
        aligned("a := 1; // one\nlongname := 2; (* two *)\nb := (* value *) 3;\nc := 4;"),
        @r"
    a        := 1; // one
    longname := 2; (* two *)
    b        := (* value *) 3;
    c        := 4;
    "
    );
}

#[test]
fn each_block_aligns_its_own_assignments() {
    assert_snapshot!(
        aligned("x := 1;\nIF bStart THEN\nnLongerName := 1;\nn := 2;\nEND_IF;\nyy := 2;"),
        @r"
    x := 1;
    IF bStart THEN
        nLongerName := 1;
        n           := 2;
    END_IF;
    yy := 2;
    "
    );
}

#[test]
fn assignments_do_not_align_by_default() {
    assert_snapshot!(imp("a := 1;\nlongname := 2;", |_| {}), @r"
    a := 1;
    longname := 2;
    ");
}

// ---- align_trailing_comments ----

fn comments_aligned(src: &str) -> String {
    imp(src, |s| s.align_trailing_comments = true)
}

#[test]
fn trailing_comments_of_consecutive_lines_align() {
    assert_snapshot!(
        comments_aligned("nCount := 0; // reset\nbDone := TRUE; (* finished *)\nfTargetPosition := 10.0; // mm"),
        @r"
    nCount := 0;             // reset
    bDone := TRUE;           (* finished *)
    fTargetPosition := 10.0; // mm
    "
    );
}

#[test]
fn trailing_comment_runs_break_at_blank_and_uncommented_lines() {
    assert_snapshot!(
        comments_aligned("a := 1; // one\nbbb := 2; // two\n\nc := 3; // three\ndddd := 4;\ne := 5; // five\nff := 6; // six"),
        @r"
    a := 1;   // one
    bbb := 2; // two

    c := 3; // three
    dddd := 4;
    e := 5;  // five
    ff := 6; // six
    "
    );
}

#[test]
fn trailing_comments_align_across_block_lines() {
    assert_snapshot!(
        comments_aligned("IF bStart THEN // requested\nnCount := nCount + 1; // count it\nEND_IF; // start"),
        @r"
    IF bStart THEN            // requested
        nCount := nCount + 1; // count it
    END_IF;                   // start
    "
    );
}

#[test]
fn trailing_comments_align_in_declarations() {
    assert_snapshot!(
        dec(
            "VAR\nbStart : BOOL; // start request\nnCycleCount : INT := 0; // cycles\n// a note\nfSpeed : LREAL; // mm/s\nfAcc : LREAL; // mm/s2\nEND_VAR",
            |s| s.align_trailing_comments = true,
        ),
        @r"
    VAR
        bStart      : BOOL;     // start request
        nCycleCount : INT := 0; // cycles
        // a note
        fSpeed : LREAL; // mm/s
        fAcc   : LREAL; // mm/s2
    END_VAR
    "
    );
}

#[test]
fn trailing_comments_keep_the_minimum_gap() {
    assert_snapshot!(
        imp("a := 1; // one\nlonger := 2; // two", |s| {
            s.align_trailing_comments = true;
            s.trailing_comment_min_spaces = 3;
        }),
        @r"
    a := 1;        // one
    longer := 2;   // two
    "
    );
}

#[test]
fn trailing_comments_do_not_align_by_default() {
    assert_snapshot!(imp("a := 1; // one\nlonger := 2; // two", |_| {}), @r"
    a := 1; // one
    longer := 2; // two
    ");
}

// ---- blank_lines_after_decl ----

#[test]
fn declaration_blocks_get_the_minimum_blank_lines() {
    assert_snapshot!(
        dec(
            "FUNCTION_BLOCK FB_Motor\nVAR_INPUT\nbEnable : BOOL;\nEND_VAR\nVAR_OUTPUT\nbReady : BOOL;\nEND_VAR\n// state\nVAR\nnState : INT;\nEND_VAR",
            |s| s.blank_lines_after_decl = 1,
        ),
        @r"
    FUNCTION_BLOCK FB_Motor
    VAR_INPUT
        bEnable : BOOL;
    END_VAR

    VAR_OUTPUT
        bReady : BOOL;
    END_VAR

    // state
    VAR
        nState : INT;
    END_VAR
    "
    );
}

#[test]
fn blank_lines_after_decl_is_a_minimum_under_the_maximum() {
    let src = "TYPE T_A : INT; END_TYPE\n\n\nTYPE T_B : INT; END_TYPE\nTYPE T_C : INT; END_TYPE";
    // More blank lines than the minimum stay, up to blank_lines_max.
    assert_snapshot!(dec(src, |s| s.blank_lines_after_decl = 1), @r"
    TYPE T_A : INT;
    END_TYPE


    TYPE T_B : INT;
    END_TYPE

    TYPE T_C : INT;
    END_TYPE
    ");
    // blank_lines_max wins over the minimum.
    assert_snapshot!(
        dec(src, |s| {
            s.blank_lines_after_decl = 3;
            s.blank_lines_max = 1;
        }),
        @r"
    TYPE T_A : INT;
    END_TYPE

    TYPE T_B : INT;
    END_TYPE

    TYPE T_C : INT;
    END_TYPE
    "
    );
}

// ---- blank_lines_around_block_statement ----

fn around_blocks(src: &str) -> String {
    imp(src, |s| s.blank_lines_around_block_statement = 1)
}

#[test]
fn block_statements_get_blank_lines_around_them() {
    assert_snapshot!(
        around_blocks("nCount := 0;\nIF bStart THEN\nnCount := 1;\nEND_IF;\n// then loop\nFOR i := 1 TO 3 DO\na[i] := 0;\nEND_FOR;\nWHILE bRun DO\nStep();\nEND_WHILE;\nbDone := TRUE;"),
        @r"
    nCount := 0;

    IF bStart THEN
        nCount := 1;
    END_IF;

    // then loop
    FOR i := 1 TO 3 DO
        a[i] := 0;
    END_FOR;

    WHILE bRun DO
        Step();
    END_WHILE;

    bDone := TRUE;
    "
    );
}

#[test]
fn no_blank_line_at_block_start_or_end() {
    assert_snapshot!(
        around_blocks("IF bStart THEN\nCASE nState OF\n0: nState := 10;\nEND_CASE;\nEND_IF;"),
        @r"
    IF bStart THEN
        CASE nState OF
            0:
                nState := 10;
        END_CASE;
    END_IF;
    "
    );
}

#[test]
fn blank_lines_around_blocks_respect_the_maximum() {
    assert_snapshot!(
        imp("a := 1;\nREPEAT\nb := b + 1;\nUNTIL b > 3\nEND_REPEAT;\nc := 2;", |s| {
            s.blank_lines_around_block_statement = 2;
            s.blank_lines_max = 1;
        }),
        @r"
    a := 1;

    REPEAT
        b := b + 1;
    UNTIL b > 3
    END_REPEAT;

    c := 2;
    "
    );
}

// ---- blank_lines_after_case, blank_lines_after_multiline_case ----

const CASE_ARMS: &str = "CASE nState OF\n0: nState := 10;\n10:\nnCount := 0;\nnState := 20;\n20: IF bDone THEN nState := 0; END_IF;\n// waiting\n30: nState := 0;\nELSE\nnState := 0;\nEND_CASE;";

#[test]
fn case_arms_get_the_minimum_blank_lines() {
    assert_snapshot!(
        imp(CASE_ARMS, |s| {
            s.blank_lines_after_case = 1;
            s.blank_lines_after_multiline_case = 2;
        }),
        @r"
    CASE nState OF
        0:
            nState := 10;

        10:
            nCount := 0;
            nState := 20;


        20:
            IF bDone THEN
                nState := 0;
            END_IF;


        // waiting
        30:
            nState := 0;

        ELSE
            nState := 0;
    END_CASE;
    "
    );
}

#[test]
fn the_two_case_arm_minimums_are_independent() {
    assert_snapshot!(imp(CASE_ARMS, |s| s.blank_lines_after_multiline_case = 1), @r"
    CASE nState OF
        0:
            nState := 10;
        10:
            nCount := 0;
            nState := 20;

        20:
            IF bDone THEN
                nState := 0;
            END_IF;

        // waiting
        30:
            nState := 0;
        ELSE
            nState := 0;
    END_CASE;
    ");
}

// ---- blank_lines_before_multiline_comment ----

#[test]
fn a_multiline_comment_gets_blank_lines_above_it() {
    assert_snapshot!(
        imp(
            "nCount := 0;\n(* Start the motor\n   once the guard is closed. *)\nbStart := bGuardClosed;\n// one line\nbBusy := bStart;\n(* one line too *)\nbDone := FALSE;\nIF bStart THEN\n(* opens the block\n   so no blank *)\nnCount := 1;\nEND_IF;",
            |s| s.blank_lines_before_multiline_comment = 1,
        ),
        @r"
    nCount := 0;

    (* Start the motor
       once the guard is closed. *)
    bStart := bGuardClosed;
    // one line
    bBusy := bStart;
    (* one line too *)
    bDone := FALSE;
    IF bStart THEN
        (* opens the block
       so no blank *)
        nCount := 1;
    END_IF;
    "
    );
}

#[test]
fn a_multiline_comment_in_declarations_gets_blank_lines_above_it() {
    assert_snapshot!(
        dec(
            "VAR\n(* first member:\n   no blank *)\nbStart : BOOL;\n(* Cycle\n   counter *)\nnCount : INT;\nEND_VAR\n(* The outputs\n   block *)\nVAR_OUTPUT\nbDone : BOOL;\nEND_VAR",
            |s| s.blank_lines_before_multiline_comment = 1,
        ),
        @r"
    VAR
        (* first member:
       no blank *)
        bStart : BOOL;

        (* Cycle
       counter *)
        nCount : INT;
    END_VAR

    (* The outputs
       block *)
    VAR_OUTPUT
        bDone : BOOL;
    END_VAR
    "
    );
}

#[test]
fn a_commented_argument_list_keeps_its_layout() {
    assert_snapshot!(
        max_args("x := foo(a, (* b *) b, c);", 2),
        @"x := foo(a, (* b *) b, c);"
    );
}
