//! Less common constructs that must parse cleanly: a parse error on any of them
//! makes the formatter refuse the whole file.

use tctidy_syntax::{parse_declaration, parse_implementation};

fn assert_decl_clean(src: &str) {
    let p = parse_declaration(src);
    assert!(p.debug_text_matches(src), "decl round-trip failed:\n{src}");
    assert!(
        p.errors().is_empty(),
        "decl errors in:\n{src}\n{:?}",
        p.errors()
    );
}

fn assert_impl_clean(src: &str) {
    let p = parse_implementation(src);
    assert!(p.debug_text_matches(src), "impl round-trip failed:\n{src}");
    assert!(
        p.errors().is_empty(),
        "impl errors in:\n{src}\n{:?}",
        p.errors()
    );
}

#[test]
fn variable_length_array_dimension() {
    // `ARRAY[*]`, VAR_IN_OUT variable-length array.
    assert_decl_clean("VAR_IN_OUT\n    aItems : ARRAY[*] OF ST_Item;\nEND_VAR");
    assert_decl_clean("VAR_IN_OUT\n    M : ARRAY[*, *] OF INT;\nEND_VAR");
}

#[test]
fn build_4026_long_date_time_types() {
    assert_decl_clean("VAR\n    timestamp : LDATE_AND_TIME;\n    daytime : LTIME_OF_DAY;\nEND_VAR");
}

#[test]
fn build_4026_typed_unicode_literals() {
    let assignments = "s := UTF8#'aä$'b$$';\nu := UCHAR#'⳧';";
    assert_impl_clean(assignments);
    assert_decl_clean(
        "VAR\n    s : STRING := UTF8#'aä$'b$$';\n    u : UINT := UCHAR#'⳧';\nEND_VAR",
    );
}

#[test]
fn build_4026_generic_constants_and_parameterized_types() {
    assert_decl_clean(
        "FUNCTION_BLOCK FB_Sample\n\
         VAR_GENERIC CONSTANT\n\
             nMaxLen : UDINT := 1;\n\
         END_VAR\n\
         VAR\n\
             data : ARRAY[0..nMaxLen - 1] OF BYTE;\n\
         END_VAR",
    );
    assert_decl_clean(
        "VAR\n\
             fb1 : FB_Sample<100>;\n\
             fb2 : FB_Sample<(2 * cConst)>;\n\
             fb3 : FB_Sample<(upper > lower)>;\n\
             values : ARRAY[0..5] OF FB_Sample<10>;\n\
         END_VAR",
    );
    assert_decl_clean(
        "FUNCTION_BLOCK FB_Sub\n\
         VAR_GENERIC CONSTANT\n\
             nMaxLen2 : UDINT := 1;\n\
         END_VAR\n\
         EXTENDS FB_Sample<nMaxLen2>\n\
         IMPLEMENTS I_Sample",
    );
}

#[test]
fn assignment_used_as_condition() {
    // `IF x := 1 THEN`, assignment in expression position.
    assert_impl_clean("IF nAlarm := 1 THEN\n    x := 1;\nEND_IF;");
}

#[test]
fn long_multiline_case_label_list() {
    // A CASE arm whose label list spans many lines.
    let labels: String = (0..40).map(|i| format!("    {},\n", i * 7)).collect();
    let src = format!("CASE n OF\n{labels}    999:\n        x := 1;\nEND_CASE;");
    assert_impl_clean(&src);
}

#[test]
fn typed_literals_as_case_labels() {
    assert_impl_clean(
        "CASE Qos OF\n\
             USINT#0:\n\
                 result := 0;\n\
             USINT#1, USINT#2:\n\
                 result := 1;\n\
         END_CASE",
    );
}

#[test]
fn interface_extending_several_interfaces() {
    assert_decl_clean("INTERFACE I_Child EXTENDS I_Base, I_Other");
    assert_decl_clean("INTERFACE I_Child EXTENDS Lib.I_Base, I_Other, I_Third");
}

#[test]
fn modifiers_on_a_type() {
    assert_decl_clean("TYPE INTERNAL E_Mode : (Off := 0, On := 1); END_TYPE");
    assert_decl_clean("TYPE ABSTRACT ST_Base : STRUCT a : INT; END_STRUCT END_TYPE");
    assert_decl_clean(
        "TYPE PUBLIC ABSTRACT ST_B EXTENDS ST_A : STRUCT b : INT; END_STRUCT END_TYPE",
    );
}

#[test]
fn elementary_type_as_an_argument() {
    assert_impl_clean("n := SIZEOF(BOOL);\np := __NEW(UINT, 1);");
}

#[test]
fn get_and_set_as_names() {
    assert_decl_clean("METHOD Set : BOOL");
    assert_decl_clean("METHOD Get : INT");
    assert_impl_clean("fb.Set();\nSet := TRUE;\nx := Get;");
}

#[test]
fn empty_declarations_in_blocks() {
    assert_decl_clean("VAR_INPUT\n    a : INT;;\nEND_VAR");
    assert_decl_clean("VAR\n\n;\nEND_VAR");
    assert_decl_clean("TYPE ST_X : STRUCT\n    a : INT;;\nEND_STRUCT\nEND_TYPE");
}

#[test]
fn semicolon_after_end_type() {
    assert_decl_clean("TYPE ST_X : STRUCT a : INT; END_STRUCT END_TYPE;");
    assert_decl_clean("TYPE T_Alias : INT; END_TYPE;");
}

#[test]
fn access_modifier_of_a_property_accessor() {
    // A `Get`/`Set` declaration has no header: TwinCAT writes the accessor's
    // own access modifier alone on the first line.
    for modifier in [
        "PUBLIC",
        "PRIVATE",
        "PROTECTED",
        "INTERNAL",
        "public",
        "Internal",
    ] {
        assert_decl_clean(&format!("{modifier}\nVAR\nEND_VAR"));
        assert_decl_clean(&format!(
            "// accessor\n{modifier} // c\r\nVAR\r\n    n : INT;\r\nEND_VAR"
        ));
    }
    assert_decl_clean("PUBLIC");
}

#[test]
fn access_modifier_only_opens_a_declaration() {
    assert!(!parse_declaration("VAR\nEND_VAR\nPUBLIC")
        .errors()
        .is_empty());
    assert!(!parse_declaration("PUBLIC\nPRIVATE\nVAR\nEND_VAR")
        .errors()
        .is_empty());
}
