//! Per-construct snapshots: feed a deliberately *messy* fragment of each
//! construct and pin the formatted result. These read as living documentation
//! of the house style and make any layout change show up as a reviewable
//! snapshot diff.
//!
//! Inputs use lower-case keywords and ragged spacing on purpose, so each
//! snapshot demonstrates the formatter actually normalizing, not echoing.

use insta::assert_snapshot;

use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_declaration, parse_implementation, SyntaxKind, SyntaxNode};

fn imp(src: &str) -> String {
    format_implementation(
        &parse_implementation(src),
        &StyleConfig::default(),
        LineEnding::Lf,
    )
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

// ---- expressions ----------------------------------------------------------

#[test]
fn expr_binary_precedence() {
    assert_snapshot!(imp("x:=a+b*c-d;"), @"x := a + b * c - d;");
}

#[test]
fn expr_boolean() {
    assert_snapshot!(imp("b:=a and not c or d;"), @"b := a AND NOT c OR d;");
}

#[test]
fn expr_boolean_expanded_branches() {
    assert_snapshot!(
        imp("VentOpenRequest := (Climate.TempHigh OR Climate.HumidityHigh) AND NOT RainDetected AND NOT WindAlarm AND ((Co2LevelHigh AND Climate.InAutoMode) OR (Panel.OpenVent AND NOT Panel.CloseVent) OR (ScheduleActive AND (Season.Summer OR Season.Spring) AND NOT Climate.FrostRisk));"),
        @r###"
    VentOpenRequest := (Climate.TempHigh OR Climate.HumidityHigh)
                       AND NOT RainDetected
                       AND NOT WindAlarm
                       AND ((Co2LevelHigh
                             AND Climate.InAutoMode
                             )
                            OR (Panel.OpenVent
                                AND NOT Panel.CloseVent
                                )
                            OR (ScheduleActive
                                AND (Season.Summer OR Season.Spring)
                                AND NOT Climate.FrostRisk
                                )
                            );
    "###
    );
}

#[test]
fn expr_literals() {
    assert_snapshot!(imp("x:=16#FF+ 2#1010 + 1.5e3;"), @"x := 16#FF + 2#1010 + 1.5e3;");
}

#[test]
fn expr_parens_preserved() {
    assert_snapshot!(imp("x:=(a+b)*(c-d);"), @"x := (a + b) * (c - d);");
}

// ---- assignments & calls ---------------------------------------------------

#[test]
fn assign_chained() {
    assert_snapshot!(imp("a:=b:=c;"), @"a := b := c;");
}

#[test]
fn call_positional_args() {
    assert_snapshot!(imp("foo(a,b,c);"), @"foo(a, b, c);");
}

#[test]
fn call_named_and_output_args() {
    assert_snapshot!(imp("fb(in:=1, out=>x);"), @"
    fb(in  := 1,
       out => x);
    ");
}

// ---- trailing comma: a redundant `,)` is normalized away ------------------
//
// House style drops the redundant trailing separator of a list, in every
// paren-delimited context and whether the list is flat or broken. Driven by
// `StyleConfig::trailing_comma_collection` (default: none), see style_leak.rs.

#[test]
fn call_trailing_comma_dropped() {
    assert_snapshot!(imp("foo(a,b,);"), @"foo(a, b);");
}

#[test]
fn call_trailing_comma_dropped_multiline() {
    assert_snapshot!(imp("foo(\na,\nb,\n);"), @"
    foo(a,
        b
        );
    ");
}

#[test]
fn fb_instance_trailing_comma_dropped() {
    assert_snapshot!(dec("VAR\nx:FB_Foo(a:=1,);\nEND_VAR"), @"
    VAR
        x : FB_Foo(a := 1);
    END_VAR
    ");
}

#[test]
fn fb_instance_trailing_comma_dropped_multiline() {
    assert_snapshot!(dec("VAR\nx:FB_Foo(\na:=1,\nb:=2,\n);\nEND_VAR"), @"
    VAR
        x : FB_Foo(a := 1,
                   b := 2);
    END_VAR
    ");
}

#[test]
fn initializer_list_trailing_comma_dropped() {
    assert_snapshot!(dec("VAR\nx:ARRAY[0..2] OF INT:=(\n1,\n2,\n3,\n);\nEND_VAR"), @"
    VAR
        x : ARRAY[0..2] OF INT := (
            1,
            2,
            3
        );
    END_VAR
    ");
}

#[test]
fn named_initializer_over_two_fields_uses_hanging_layout() {
    let source = "VAR\nfbDisplay:FB_Display;\nstStation:ST_WeatherStation:=(\nLabel:='Rooftop',\nLatitude:=46.2,\nLongitude:=6.1,\nHeated:=TRUE\n);\nnSamples:UDINT;\nEND_VAR";
    let once = dec(source);
    assert_eq!(once, dec(&once));

    assert_snapshot!(once, @"
    VAR
        fbDisplay : FB_Display;
        stStation : ST_WeatherStation := (Label     := 'Rooftop',
                                          Latitude  := 46.2,
                                          Longitude := 6.1,
                                          Heated    := TRUE
                                         );
        nSamples  : UDINT;
    END_VAR
    ");
}

#[test]
fn array_initializer_keeps_four_values_inline_and_breaks_five() {
    let source = "VAR\naLevels:ARRAY[1..4] OF E_Level:=[E_Level.Low, E_Level.Mid, E_Level.High, E_Level.Max];\naSteps:ARRAY[1..5] OF E_Step:=[E_Step.Fill, E_Step.Heat, E_Step.Stir, E_Step.Cool, E_Step.Drain];\naPoints:ARRAY[1..5] OF ST_Point:=[(x:=1, y:=2), (x:=3, y:=4), (x:=5, y:=6), (x:=7, y:=8), (x:=9, y:=10)];\nEND_VAR";
    let once = dec(source);
    assert_eq!(once, dec(&once));
    assert_snapshot!(once, @"
    VAR
        aLevels : ARRAY[1..4] OF E_Level  := [E_Level.Low, E_Level.Mid, E_Level.High, E_Level.Max];
        aSteps  : ARRAY[1..5] OF E_Step   := [E_Step.Fill,
                                              E_Step.Heat,
                                              E_Step.Stir,
                                              E_Step.Cool,
                                              E_Step.Drain
                                             ];
        aPoints : ARRAY[1..5] OF ST_Point := [(x := 1, y := 2),
                                              (x := 3, y := 4),
                                              (x := 5, y := 6),
                                              (x := 7, y := 8),
                                              (x := 9, y := 10)
                                             ];
    END_VAR
    ");
}

#[test]
fn array_initializer_from_several_lines_hangs_past_the_limit() {
    let source = "VAR\naSteps:ARRAY[1..5] OF INT:=[\n10, 20,\n30, 40, 50];\nEND_VAR";
    let once = dec(source);
    assert_eq!(once, dec(&once));
    assert_snapshot!(once, @"
    VAR
        aSteps : ARRAY[1..5] OF INT := [10,
                                        20,
                                        30,
                                        40,
                                        50
                                       ];
    END_VAR
    ");
}

#[test]
fn structure_initializer_keeps_two_fields_inline_and_breaks_three() {
    let source = "TYPE ST_Test : STRUCT\nInlet : ST_Limits := (Enabled := FALSE, MinLevel := 100.0);\nOutlet : ST_Limits := (MinLevel := 500.0, MaxLevel := 2500.0, Hysteresis := 1.0);\nEND_STRUCT\nEND_TYPE";
    let once = dec(source);
    assert_eq!(once, dec(&once));
    assert_snapshot!(once, @"
    TYPE ST_Test : STRUCT
        Inlet  : ST_Limits := (Enabled := FALSE, MinLevel := 100.0);
        Outlet : ST_Limits := (MinLevel   := 500.0,
                               MaxLevel   := 2500.0,
                               Hysteresis := 1.0
                              );
    END_STRUCT
    END_TYPE
    ");
}

#[test]
fn structure_initializers_align_separately_from_scalar_members() {
    let source = "TYPE ST_Test : STRUCT\nInlet : ST_Limits := (Low := 1.0, High := 2.0, Window := 3.0);\nOutlet : ST_Limits := (Low := 4.0, High := 5.0, Window := 6.0);\nOverflow : ST_Limits := (Low := 7.0, High := 8.0, Window := 9.0);\nOverflowAlarmThresholdPercent : LREAL := 80.0;\nEND_STRUCT\nEND_TYPE";
    let once = dec(source);
    assert_eq!(once, dec(&once));
    assert!(once.contains("Inlet    : ST_Limits"), "{once}");
    assert!(once.contains("Outlet   : ST_Limits"), "{once}");
    assert!(once.contains("Overflow : ST_Limits"), "{once}");
    assert!(
        once.contains("OverflowAlarmThresholdPercent : LREAL"),
        "{once}"
    );
}

// ---- IEC assignment operators ----------------------------------------------

#[test]
fn assign_ref() {
    assert_snapshot!(imp("r REF= x;"), @"r REF= x;");
}

#[test]
fn assign_set_and_reset() {
    assert_snapshot!(imp("q S= a;\nq R= b;"), @"
    q S= a;
    q R= b;
    ");
}

// ---- control-flow statements ------------------------------------------------

#[test]
fn stmt_return_exit_continue() {
    assert_snapshot!(
        imp("for i:=0 to 3 do\nif a then\ncontinue;\nend_if;\nexit;\nend_for;\nreturn;"),
        @"
    FOR i := 0 TO 3 DO
        IF a THEN
            CONTINUE;
        END_IF;
        EXIT;
    END_FOR;
    RETURN;
    "
    );
}

#[test]
fn stmt_jmp_and_label() {
    assert_snapshot!(imp("start:\nx:=x+1;\njmp start;"), @"
    start:
    x := x + 1;
    JMP start;
    ");
}

#[test]
fn stmt_empty() {
    assert_snapshot!(imp(";"), @";");
}

#[test]
fn stmt_empty_if_body() {
    assert_snapshot!(imp("if a then\nend_if;"), @"
    IF a THEN
    END_IF;
    ");
}

// ---- expression shapes -------------------------------------------------------

#[test]
fn expr_unary_not_and_minus() {
    assert_snapshot!(imp("b:=not a;\nx:=-y;"), @"
    b := NOT a;
    x := -y;
    ");
}

#[test]
fn expr_qualified_name_chain() {
    assert_snapshot!(imp("x:=a.b.c;"), @"x := a.b.c;");
}

#[test]
fn expr_chained_indices_and_deref() {
    assert_snapshot!(imp("x:=a[i][j];\ny:=p^;\nz:=q^.val;"), @"
    x := a[i][j];
    y := p^;
    z := q^.val;
    ");
}

#[test]
fn expr_empty_call() {
    assert_snapshot!(imp("foo();"), @"foo();");
}

#[test]
fn decl_empty_initializer() {
    assert_snapshot!(dec("VAR\nx:ST_Point:=();\nEND_VAR"), @"
    VAR
        x : ST_Point := ();
    END_VAR
    ");
}

// ---- type shapes --------------------------------------------------------------

#[test]
fn decl_pointer_and_reference_to() {
    assert_snapshot!(dec("VAR\np:pointer to int;\nr:reference to bool;\nEND_VAR"), @"
    VAR
        p : POINTER TO INT;
        r : REFERENCE TO BOOL;
    END_VAR
    ");
}

#[test]
fn type_union() {
    assert_snapshot!(dec("type u : union\nw:word;\nb:byte;\nend_union\nend_type"), @"
    TYPE u : UNION
        w : WORD;
        b : BYTE;
    END_UNION
    END_TYPE
    ");
}

#[test]
fn decl_sized_string_and_subrange() {
    assert_snapshot!(dec("VAR\ns:string(80);\nn:uint(0..10);\nEND_VAR"), @"
    VAR
        s : STRING(80);
        n : UINT(0..10);
    END_VAR
    ");
}

#[test]
fn type_enum_with_base_type() {
    assert_snapshot!(dec("type e : (red, green) uint := red; end_type"), @"
    TYPE e : (
        red,
        green
    ) UINT := red;
    END_TYPE
    ");
}

#[test]
fn decl_empty_var_block() {
    assert_snapshot!(dec("VAR\nEND_VAR"), @"
    VAR
    END_VAR
    ");
}

// ---- declarations & alignment ---------------------------------------------

#[test]
fn var_block_colon_alignment() {
    assert_snapshot!(dec("VAR\nshort:int;\naMuchLongerName:bool:=true;\nEND_VAR"), @"
    VAR
        short           : INT;
        aMuchLongerName : BOOL := TRUE;
    END_VAR
    ");
}

#[test]
fn var_block_at_address() {
    assert_snapshot!(dec("VAR\nx at %IX0.0:bool;\ny:int;\nEND_VAR"), @"
    VAR
        x AT %IX0.0 : BOOL;
        y           : INT;
    END_VAR
    ");
}

// Column alignment of the AT, colon and comment columns. These pin the *mixed*
// runs the simpler cases above don't exercise, differing name and `AT %addr`
// widths, `:=` alignment on AT rows, a run split by a blank line, and
// end-of-line comments (which are not aligned by default). Verified
// column-correct and idempotent.

#[test]
fn decl_at_addresses_and_assign_aligned_mixed_widths() {
    // `AT` keyword, `:` and `:=` each align even though names, addresses and
    // types all differ in width.
    assert_snapshot!(dec("VAR\nx AT %IX0.0 : BOOL := TRUE;\nyy AT %IW10 : UINT := 300;\nEND_VAR"), @"
    VAR
        x  AT %IX0.0 : BOOL := TRUE;
        yy AT %IW10  : UINT := 300;
    END_VAR
    ");
}

#[test]
fn decl_alignment_group_breaks_on_blank_line() {
    // A blank line starts a fresh alignment group: each half aligns on its own,
    // not across the gap.
    assert_snapshot!(dec("VAR\naa AT %IX0.0 : BOOL;\nb : INT;\n\nlongone : INT;\nc AT %IX1.0 : BOOL;\nEND_VAR"), @"
    VAR
        aa AT %IX0.0 : BOOL;
        b            : INT;

        longone     : INT;
        c AT %IX1.0 : BOOL;
    END_VAR
    ");
}

#[test]
fn decl_struct_at_colon_aligned_with_trailing_comments() {
    // Colons align across a STRUCT run mixing plain and `AT` members; the
    // end-of-line comments keep their single-space gap (not column-aligned).
    assert_snapshot!(dec("TYPE t : STRUCT\na : INT; // first\nbb : BOOL; // second\nccc AT %IX0.0 : BOOL; // third\nEND_STRUCT\nEND_TYPE"), @"
    TYPE t : STRUCT
        a             : INT; // first
        bb            : BOOL; // second
        ccc AT %IX0.0 : BOOL; // third
    END_STRUCT
    END_TYPE
    ");
}

// A comment inside a name list forces the names one per line so nothing is
// dropped; each name keeps its own comment. A no-comment list stays flat (above).

#[test]
fn decl_name_list_comment_explodes_names() {
    assert_snapshot!(dec("VAR\na, // first\nb, c : INT;\nEND_VAR"), @"
    VAR
        a, // first
        b,
        c : INT;
    END_VAR
    ");
}

#[test]
fn decl_per_name_comments_stay_one_per_line() {
    assert_snapshot!(dec("VAR\nnX, // X axis\nnY, // Y axis\nnZ : INT;\nEND_VAR"), @"
    VAR
        nX, // X axis
        nY, // Y axis
        nZ : INT;
    END_VAR
    ");
}

// Multi-member declarations (`a, b, c : T;`) are always rendered on one line,
// source line breaks between names are dropped and the list is never width-
// wrapped (opinionated house style). The names participate in the group's
// `:` alignment as a single unit. (Comment-bearing lists are the exception,
// above.)

#[test]
fn multi_member_names_stay_on_one_line() {
    assert_snapshot!(dec("VAR\na, b, c : INT;\nEND_VAR"), @"
    VAR
        a, b, c : INT;
    END_VAR
    ");
}

#[test]
fn multi_member_source_wrapped_names_collapse() {
    assert_snapshot!(dec("VAR\na,\nb,\nc : INT;\nEND_VAR"), @"
    VAR
        a, b, c : INT;
    END_VAR
    ");
}

#[test]
fn multi_member_participates_in_colon_alignment() {
    assert_snapshot!(dec("VAR\na, b, c : BOOL;\nlongName : REAL;\nEND_VAR"), @"
    VAR
        a, b, c  : BOOL;
        longName : REAL;
    END_VAR
    ");
}

#[test]
fn multi_member_with_initializer_stays_inline() {
    assert_snapshot!(dec("VAR\na, b, c : INT := 1, 2, 3;\nEND_VAR"), @"
    VAR
        a, b, c : INT := 1, 2, 3;
    END_VAR
    ");
}

#[test]
fn type_struct() {
    assert_snapshot!(dec("type t :\nstruct\na:int;\nbb:bool;\nend_struct\nend_type"), @"
    TYPE t :
    STRUCT
        a  : INT;
        bb : BOOL;
    END_STRUCT
    END_TYPE
    ");
}

#[test]
fn type_enum() {
    // Enums are deliberately exploded one variant per line, even from a
    // single-line source, an opinionated exception to source-break preservation
    // (see format_enum's "one variant per line"). Idempotent.
    assert_snapshot!(dec("type e : (red,green,blue); end_type"), @"
    TYPE e : (
        red,
        green,
        blue
    );
    END_TYPE
    ");
}

#[test]
fn type_array() {
    assert_snapshot!(dec("VAR\nx:array[0..9] of int;\nEND_VAR"), @"
    VAR
        x : ARRAY[0..9] OF INT;
    END_VAR
    ");
}

#[test]
fn pou_header_with_extends() {
    assert_snapshot!(dec("function_block fb_foo extends fb_base\nVAR_INPUT\nx:int;\nEND_VAR"), @"
    FUNCTION_BLOCK fb_foo EXTENDS fb_base
    VAR_INPUT
        x : INT;
    END_VAR
    ");
}

// ---- statements ------------------------------------------------------------

#[test]
fn stmt_if_elsif_else() {
    assert_snapshot!(imp("if a then\nx:=1;\nelsif b then\nx:=2;\nelse\nx:=3;\nend_if;"), @"
    IF a THEN
        x := 1;
    ELSIF b THEN
        x := 2;
    ELSE
        x := 3;
    END_IF;
    ");
}

#[test]
fn stmt_case() {
    assert_snapshot!(imp("case n of\n1: x:=1;\n2,3: x:=2;\nelse\nx:=0;\nend_case;"), @"
    CASE n OF
        1:
            x := 1;
        2, 3:
            x := 2;
        ELSE
            x := 0;
    END_CASE;
    ");
}

#[test]
fn stmt_case_with_typed_literal_labels_is_idempotent() {
    let source = "case qos of\nUSINT#0: result:=0;\nUSINT#1,USINT#2: result:=1;\nend_case;";
    let before = parse_implementation(source);
    assert!(before.errors().is_empty(), "{:?}", before.errors());

    let once = imp(source);
    let twice = imp(&once);
    assert_eq!(once, twice);

    let after = parse_implementation(&once);
    assert!(after.errors().is_empty(), "{:?}", after.errors());
    assert_eq!(
        semantic_tokens(&before.syntax_node()),
        semantic_tokens(&after.syntax_node())
    );

    assert_snapshot!(once, @"
    CASE qos OF
        USINT#0:
            result := 0;
        USINT#1, USINT#2:
            result := 1;
    END_CASE;
    ");
}

#[test]
fn stmt_for() {
    assert_snapshot!(imp("for i:=0 to 10 by 2 do\nx:=x+i;\nend_for;"), @"
    FOR i := 0 TO 10 BY 2 DO
        x := x + i;
    END_FOR;
    ");
}

#[test]
fn stmt_while() {
    assert_snapshot!(imp("while a<10 do\na:=a+1;\nend_while;"), @"
    WHILE a < 10 DO
        a := a + 1;
    END_WHILE;
    ");
}

#[test]
fn stmt_repeat() {
    assert_snapshot!(imp("repeat\na:=a+1;\nuntil a>=10\nend_repeat;"), @"
    REPEAT
        a := a + 1;
    UNTIL a >= 10
    END_REPEAT;
    ");
}

// ---- blank lines at the start of a block -----------------------------------

#[test]
fn blanks_after_if_then_collapse() {
    assert_snapshot!(imp("if a then\n\n\nx:=1;\nend_if;"), @"
    IF a THEN
        x := 1;
    END_IF;
    ");
}

#[test]
fn blanks_after_elsif_and_else_collapse() {
    assert_snapshot!(
        imp("if a then\nx:=1;\nelsif b then\n\n\nx:=2;\nelse\n\n\nx:=3;\nend_if;"),
        @"
    IF a THEN
        x := 1;
    ELSIF b THEN
        x := 2;
    ELSE
        x := 3;
    END_IF;
    "
    );
}

#[test]
fn blanks_after_case_of_collapse() {
    assert_snapshot!(imp("case n of\n\n\n1: x:=1;\nend_case;"), @"
    CASE n OF
        1:
            x := 1;
    END_CASE;
    ");
}

#[test]
fn blanks_after_case_arm_colon_collapse() {
    assert_snapshot!(imp("case n of\n1:\n\n\nx:=1;\nend_case;"), @"
    CASE n OF
        1:
            x := 1;
    END_CASE;
    ");
}

#[test]
fn blanks_after_for_do_collapse() {
    assert_snapshot!(imp("for i:=0 to 10 do\n\n\nx:=i;\nend_for;"), @"
    FOR i := 0 TO 10 DO
        x := i;
    END_FOR;
    ");
}

#[test]
fn blanks_after_while_do_collapse() {
    assert_snapshot!(imp("while a<10 do\n\n\na:=a+1;\nend_while;"), @"
    WHILE a < 10 DO
        a := a + 1;
    END_WHILE;
    ");
}

#[test]
fn blanks_after_repeat_collapse() {
    assert_snapshot!(imp("repeat\n\n\na:=a+1;\nuntil a>=10\nend_repeat;"), @"
    REPEAT
        a := a + 1;
    UNTIL a >= 10
    END_REPEAT;
    ");
}

#[test]
fn blanks_after_nested_headers_collapse() {
    // A block opener inside a CASE arm inside an IF; every header's leading
    // blanks collapse, at every nesting level.
    assert_snapshot!(
        imp("if a then\n\n\ncase n of\n\n\n1:\n\n\nfor i:=0 to 2 do\n\n\nx:=i;\nend_for;\nend_case;\nend_if;"),
        @"
    IF a THEN
        CASE n OF
            1:
                FOR i := 0 TO 2 DO
                    x := i;
                END_FOR;
        END_CASE;
    END_IF;
    "
    );
}

#[test]
fn comment_after_header_survives_blank_above_collapses() {
    // The blank between the header and an own-line comment collapses, but the
    // comment itself keeps its place (it is not a blank line).
    assert_snapshot!(imp("if a then\n\n// note\nx:=1;\nend_if;"), @"
    IF a THEN
        // note
        x := 1;
    END_IF;
    ");
}

// ---- blank lines at the end of a block -------------------------------------

#[test]
fn blanks_before_end_if_collapse() {
    assert_snapshot!(imp("if a then\nx:=1;\n\n\nend_if;"), @"
    IF a THEN
        x := 1;
    END_IF;
    ");
}

#[test]
fn blanks_before_end_case_collapse() {
    assert_snapshot!(imp("case n of\n1: x:=1;\n\n\nend_case;"), @"
    CASE n OF
        1:
            x := 1;
    END_CASE;
    ");
}

#[test]
fn blanks_before_end_for_collapse() {
    assert_snapshot!(imp("for i:=0 to 10 do\nx:=i;\n\n\nend_for;"), @"
    FOR i := 0 TO 10 DO
        x := i;
    END_FOR;
    ");
}

#[test]
fn blanks_before_end_while_collapse() {
    assert_snapshot!(imp("while a<10 do\na:=a+1;\n\n\nend_while;"), @"
    WHILE a < 10 DO
        a := a + 1;
    END_WHILE;
    ");
}

#[test]
fn blanks_before_end_repeat_collapse() {
    assert_snapshot!(imp("repeat\na:=a+1;\nuntil a>=10\n\n\nend_repeat;"), @"
    REPEAT
        a := a + 1;
    UNTIL a >= 10
    END_REPEAT;
    ");
}

#[test]
fn blanks_before_end_var_collapse() {
    assert_snapshot!(dec("VAR\nx:int;\n\n\nEND_VAR"), @"
    VAR
        x : INT;
    END_VAR
    ");
}

#[test]
fn blanks_before_end_struct_collapse() {
    assert_snapshot!(dec("type t:\nstruct\nx:int;\n\n\nend_struct\nend_type"), @"
    TYPE t :
    STRUCT
        x : INT;
    END_STRUCT
    END_TYPE
    ");
}

#[test]
fn blanks_before_enum_close_collapse() {
    assert_snapshot!(dec("type e : (red,green\n\n\n); end_type"), @"
    TYPE e : (
        red,
        green
    );
    END_TYPE
    ");
}

#[test]
fn comment_before_end_if_survives_blank_collapses() {
    // The blank between the last statement and an own-line trailing comment
    // collapses, but the comment keeps its place at the body indent.
    assert_snapshot!(imp("if a then\nx:=1;\n\n// trailing\nend_if;"), @"
    IF a THEN
        x := 1;
        // trailing
    END_IF;
    ");
}

#[test]
fn comment_before_end_var_survives_blank_collapses() {
    assert_snapshot!(dec("VAR\nx:int;\n\n// trailing\nEND_VAR"), @"
    VAR
        x : INT;
        // trailing
    END_VAR
    ");
}

// ---- comments & pragmas ----------------------------------------------------

#[test]
fn comments_trailing_and_leading() {
    assert_snapshot!(imp("// header\nx:=1; // trailing"), @"
    // header
    x := 1; // trailing
    ");
}

#[test]
fn comment_before_assignment_operator_keeps_its_place() {
    let block = imp("b(* note *):=3;");
    assert_eq!(imp(&block), block);
    assert_snapshot!(block, @"b (* note *) := 3;");

    let line = imp("b // note\n:=3;");
    assert_eq!(imp(&line), line);
    assert_snapshot!(line, @"
    b // note
        := 3;
    ");
}

#[test]
fn pragma_in_var_block() {
    assert_snapshot!(dec("VAR\n{attribute 'hide'}\nx:int;\nEND_VAR"), @"
    VAR
        {attribute 'hide'}
        x : INT;
    END_VAR
    ");
}

// ---- constructs found in public TwinCAT projects ----------------------------

#[test]
fn decl_interface_extending_several_interfaces() {
    assert_snapshot!(
        dec("interface I_Child extends I_Base,I_Other"),
        @"INTERFACE I_Child EXTENDS I_Base, I_Other"
    );
}

#[test]
fn decl_type_modifiers_and_semicolon_after_end_type() {
    assert_snapshot!(dec("type internal E_Mode : (Off := 0, On := 1); end_type"), @"
    TYPE INTERNAL E_Mode : (
        Off := 0,
        On  := 1
    );
    END_TYPE
    ");
    assert_snapshot!(dec("type abstract ST_Base : struct a:int; end_struct end_type; // base"), @"
    TYPE ABSTRACT ST_Base : STRUCT
        a : INT;
    END_STRUCT
    END_TYPE; // base
    ");
}

#[test]
fn decl_stray_semicolon_keeps_its_own_line() {
    assert_snapshot!(dec("var_input\na:int;; // stray\nend_var"), @"
    VAR_INPUT
        a : INT;
        ; // stray
    END_VAR
    ");
}

#[test]
fn expr_method_chain_keeps_a_comment_before_the_dot() {
    assert_snapshot!(imp("x := a.B() // note\n    .C();"), @"
    x := a.B() // note
        .C();
    ");
}

#[test]
fn expr_elementary_type_argument_and_set_as_a_name() {
    assert_snapshot!(imp("n:=sizeof(bool);\nfb.set();"), @"
    n := sizeof(bool);
    fb.set();
    ");
}
