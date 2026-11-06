//! Property accessors whose declaration opens with an access modifier: TwinCAT
//! compiles them, so the formatter must accept them and keep the modifier.

use tctidy::{format_st, format_twincat_file, FormatOptions, StSection};

fn opts() -> FormatOptions {
    FormatOptions::default()
}

/// A function block with a property whose `Get` declares its own `PUBLIC`
/// access modifier, as TwinCAT writes it.
const POU: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
<TcPlcObject Version=\"1.1.0.1\">\r\n\
\x20 <POU Name=\"FB_Sample\" Id=\"{00000000-0000-0000-0000-000000000000}\" SpecialFunc=\"None\">\r\n\
\x20   <Declaration><![CDATA[FUNCTION_BLOCK FB_Sample\r\nVAR\r\n    nCount : INT;\r\nEND_VAR]]></Declaration>\r\n\
\x20   <Implementation>\r\n\
\x20     <ST><![CDATA[]]></ST>\r\n\
\x20   </Implementation>\r\n\
\x20   <Property Name=\"Status\" Id=\"{00000000-0000-0000-0000-000000000001}\">\r\n\
\x20     <Declaration><![CDATA[PROPERTY PUBLIC Status : INT]]></Declaration>\r\n\
\x20     <Get Name=\"Get\" Id=\"{00000000-0000-0000-0000-000000000002}\">\r\n\
\x20       <Declaration><![CDATA[PUBLIC\r\nVAR\r\nEND_VAR]]></Declaration>\r\n\
\x20       <Implementation>\r\n\
\x20         <ST><![CDATA[Status:=nCount;]]></ST>\r\n\
\x20       </Implementation>\r\n\
\x20     </Get>\r\n\
\x20   </Property>\r\n\
\x20 </POU>\r\n\
</TcPlcObject>\r\n";

#[test]
fn getter_with_an_access_modifier_formats_and_keeps_it() {
    let out = format_twincat_file(POU.as_bytes(), &opts()).expect("the POU formats");
    let formatted = String::from_utf8(out.bytes).unwrap();
    assert_eq!(
        formatted,
        POU.replace("Status:=nCount;", "Status := nCount;"),
        "only the implementation changes"
    );
    let again = format_twincat_file(formatted.as_bytes(), &opts()).expect("reformats");
    assert!(!again.changed, "not idempotent");
}

#[test]
fn every_access_modifier_is_kept() {
    for modifier in ["PUBLIC", "PRIVATE", "PROTECTED", "INTERNAL"] {
        let src = format!("{modifier}\nVAR\nEND_VAR");
        assert_eq!(
            format_st(&src, StSection::Declaration, &opts()).expect("formats"),
            src
        );
    }
}

#[test]
fn access_modifier_follows_keyword_case_and_keeps_its_comments() {
    let src = "// accessor\npublic  // visible\nvar\nn:int;\nend_var";
    let out = format_st(src, StSection::Declaration, &opts()).expect("formats");
    assert_eq!(
        out,
        "// accessor\nPUBLIC // visible\nVAR\n    n : INT;\nEND_VAR"
    );
    assert_eq!(
        format_st(&out, StSection::Declaration, &opts()).expect("reformats"),
        out
    );
}
