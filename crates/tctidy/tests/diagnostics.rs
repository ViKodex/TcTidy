//! Facade-level parse-error diagnostics: a refused parse must reach the caller
//! naming the failing section and the source lines, including on the TwinCAT
//! XML path, where lines are relative to the ST *section*, not the file.

use tctidy::{format_st, format_twincat_file, FormatOptions, StSection};

fn opts() -> FormatOptions {
    FormatOptions::default()
}

#[test]
fn raw_st_parse_error_names_the_section_and_line() {
    // A dangling END_IF (no matching IF).
    let err = format_st("x := 1;\nEND_IF\n", StSection::Implementation, &opts())
        .expect_err("broken ST must be refused");
    assert_eq!(
        err.to_string(),
        "parse error in implementation: line 2",
        "got:\n{err}"
    );
}

#[test]
fn twincat_implementation_error_is_located_within_the_section() {
    // A POU whose Implementation ST is broken. The line must be relative to the
    // implementation body (END_IF is line 2 there), not the XML file line.
    let xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
               <TcPlcObject Version=\"1.1.0.1\">\r\n\
               \x20 <POU Name=\"FB_Test\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n\
               \x20   <Implementation>\r\n\
               \x20     <ST><![CDATA[x := 1;\r\nEND_IF]]></ST>\r\n\
               \x20   </Implementation>\r\n\
               \x20 </POU>\r\n\
               </TcPlcObject>\r\n";
    let err = format_twincat_file(xml.as_bytes(), &opts())
        .expect_err("a POU with broken ST must be refused");
    // Section-relative (line 2), surfaced as a parse error (no "twincat error"), and naming the
    // containing object (the POU) so the failure points at it rather than just the file.
    assert_eq!(
        err.to_string(),
        "parse error in FB_Test implementation: line 2",
        "got:\n{err}"
    );
}

#[test]
fn twincat_error_names_the_failing_method_not_just_the_pou() {
    // A parse error inside a METHOD must name the method (FB_X.Init), not just the POU (a bare
    // "declaration" would not say which one). The POU body is valid, so the failure is
    // attributed to the method's declaration alone.
    let xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
               <TcPlcObject Version=\"1.1.0.1\">\r\n\
               \x20 <POU Name=\"FB_X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n\
               \x20   <Declaration><![CDATA[FUNCTION_BLOCK FB_X\r\nVAR\r\nEND_VAR]]></Declaration>\r\n\
               \x20   <Implementation><ST><![CDATA[]]></ST></Implementation>\r\n\
               \x20   <Method Name=\"Init\" Id=\"{00000000-0000-0000-0000-000000000001}\">\r\n\
               \x20     <Declaration><![CDATA[METHOD Init : BOOL\r\nVAR_INPUT\r\ncafé : INT;\r\nEND_VAR]]></Declaration>\r\n\
               \x20     <Implementation><ST><![CDATA[]]></ST></Implementation>\r\n\
               \x20   </Method>\r\n\
               \x20 </POU>\r\n\
               </TcPlcObject>\r\n";
    let err = format_twincat_file(xml.as_bytes(), &opts())
        .expect_err("a method with broken ST must be refused");
    assert!(
        err.to_string()
            .starts_with("parse error in FB_X.Init declaration:"),
        "should name the method FB_X.Init; got:\n{err}"
    );
}

#[test]
fn a_clean_pou_still_formats() {
    // Guard: the error path doesn't disturb the success path.
    let xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
               <TcPlcObject Version=\"1.1.0.1\">\r\n\
               \x20 <POU Name=\"FB_Test\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n\
               \x20   <Implementation>\r\n\
               \x20     <ST><![CDATA[x:=1;]]></ST>\r\n\
               \x20   </Implementation>\r\n\
               \x20 </POU>\r\n\
               </TcPlcObject>\r\n";
    let out = format_twincat_file(xml.as_bytes(), &opts()).expect("clean POU formats");
    assert!(out.changed, "x:=1; should be reformatted to x := 1;");
}
