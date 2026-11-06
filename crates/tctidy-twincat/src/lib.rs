//! `tctidy-twincat`, the TwinCAT XML container layer.
//!
//! `.TcPOU` / `.TcGVL` / `.TcDUT` files are XML with the Structured Text living
//! in `<![CDATA[ ... ]]>` inside `<Declaration>` and `<Implementation><ST>`
//! elements. The hard requirement is that **everything outside the ST stays
//! byte-identical** (attributes, GUIDs, node order, XML indentation, the CDATA
//! delimiters).
//!
//! That holds because XML is never written: [`reformat_document`] uses an XML reader
//! only to find the ST byte ranges, hands each to an injected formatting
//! callback, and splices the results back into the original buffer. Bytes outside
//! the ST are copied verbatim, so byte-identity is structural.
//!
//! This crate depends on neither the parser nor the formatter, the formatting
//! function is injected (dependency inversion), so it can be tested with a
//! trivial identity callback.
#![forbid(unsafe_code)]

mod encoding;
mod error;
mod locate;
mod splice;

use std::error::Error;

pub use encoding::{Encoding, Eol};
pub use error::TwincatError;
pub use locate::{locate_st_spans, StSectionKind, StSpan};

/// Context handed to the formatting callback for each ST section.
#[derive(Debug, Clone, Copy)]
pub struct StContext<'a> {
    /// Declaration vs implementation, selects the grammar.
    pub kind: StSectionKind,
    /// The file's detected end-of-line, for any newly inserted lines.
    pub eol: Eol,
    /// Qualified name of the containing object (e.g. `FB_X.Init`), for diagnostics; `None`
    /// when the section has no named ancestor.
    pub object: Option<&'a str>,
}

/// Reformat the ST inside a TwinCAT document, preserving every other byte.
///
/// `format_st` receives each ST section (decoded, BOM/EOL handled) and returns
/// the formatted text. If it fails for any section, the whole call returns `Err`
/// and the caller must not write (non-destruction).
pub fn reformat_document<F>(input: &[u8], mut format_st: F) -> Result<Vec<u8>, TwincatError>
where
    F: FnMut(&str, StContext<'_>) -> Result<String, Box<dyn Error + Send + Sync>>,
{
    let (body, encoding) = Encoding::detect_and_strip_bom(input);
    let eol = Eol::detect(body);
    // TwinCAT files are UTF-8; say so plainly instead of reporting the XML
    // reader's decoding error.
    if std::str::from_utf8(body).is_err() {
        return Err(TwincatError::NotUtf8);
    }

    let spans = locate_st_spans(body)?;
    let mut edits = Vec::with_capacity(spans.len());
    for span in &spans {
        let raw =
            std::str::from_utf8(&body[span.content.clone()]).map_err(|_| TwincatError::NotUtf8)?;
        let formatted = format_st(
            raw,
            StContext {
                kind: span.kind,
                eol,
                object: span.object.as_deref(),
            },
        )
        .map_err(|e| TwincatError::Format(e.to_string()))?;
        if formatted.contains("]]>") {
            return Err(TwincatError::CdataDelimiterInOutput);
        }
        edits.push((span.content.clone(), formatted));
    }

    let spliced = splice::splice(body, &edits);
    Ok(encoding.reapply_bom(spliced))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal but representative .TcPOU, CRLF line endings.
    const SAMPLE: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
<TcPlcObject Version=\"1.1.0.1\">\r\n\
  <POU Name=\"P\" Id=\"{abc-123}\">\r\n\
    <Declaration><![CDATA[PROGRAM P\r\nVAR\r\nx:INT;\r\nEND_VAR]]></Declaration>\r\n\
    <Implementation>\r\n\
      <ST><![CDATA[x:=1;]]></ST>\r\n\
    </Implementation>\r\n\
  </POU>\r\n\
</TcPlcObject>\r\n";

    // The same document with LF line endings (Unix checkout / mixed tooling).
    const SAMPLE_LF: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<TcPlcObject Version=\"1.1.0.1\">\n\
  <POU Name=\"P\" Id=\"{abc-123}\">\n\
    <Declaration><![CDATA[PROGRAM P\nVAR\nx:INT;\nEND_VAR]]></Declaration>\n\
    <Implementation>\n\
      <ST><![CDATA[x:=1;]]></ST>\n\
    </Implementation>\n\
  </POU>\n\
</TcPlcObject>\n";

    #[test]
    fn identity_callback_is_byte_identical() {
        let out = reformat_document(SAMPLE.as_bytes(), |s, _| Ok(s.to_string())).unwrap();
        assert_eq!(
            out,
            SAMPLE.as_bytes(),
            "identity reformat must be byte-identical"
        );
    }

    // A POU carrying a method, so the qualified object name has two levels (P.M).
    const SAMPLE_METHOD: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
<TcPlcObject Version=\"1.1.0.1\">\r\n\
  <POU Name=\"P\" Id=\"{abc-123}\">\r\n\
    <Declaration><![CDATA[FUNCTION_BLOCK P]]></Declaration>\r\n\
    <Implementation>\r\n\
      <ST><![CDATA[x:=1;]]></ST>\r\n\
    </Implementation>\r\n\
    <Method Name=\"M\" Id=\"{def-456}\">\r\n\
      <Declaration><![CDATA[METHOD M : BOOL]]></Declaration>\r\n\
      <Implementation>\r\n\
        <ST><![CDATA[y:=2;]]></ST>\r\n\
      </Implementation>\r\n\
    </Method>\r\n\
  </POU>\r\n\
</TcPlcObject>\r\n";

    #[test]
    fn locates_declaration_and_implementation() {
        let spans = locate_st_spans(SAMPLE.as_bytes()).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].kind, StSectionKind::Declaration);
        assert_eq!(spans[1].kind, StSectionKind::Implementation);
        // each span is named by its containing POU
        assert_eq!(spans[0].object.as_deref(), Some("P"));
        assert_eq!(spans[1].object.as_deref(), Some("P"));
        // exact content extraction (no delimiter bleed)
        let body = SAMPLE.as_bytes();
        assert_eq!(&body[spans[1].content.clone()], b"x:=1;");
    }

    #[test]
    fn names_a_nested_method_with_its_qualified_object() {
        let spans = locate_st_spans(SAMPLE_METHOD.as_bytes()).unwrap();
        assert_eq!(spans.len(), 4);
        // POU body sections are named by the POU; the method's sections by POU.Method.
        assert_eq!(spans[0].object.as_deref(), Some("P")); // POU declaration
        assert_eq!(spans[1].object.as_deref(), Some("P")); // POU implementation
        assert_eq!(spans[2].object.as_deref(), Some("P.M")); // method declaration
        assert_eq!(spans[3].object.as_deref(), Some("P.M")); // method implementation
    }

    #[test]
    fn context_carries_the_object_name_to_the_callback() {
        let mut seen = Vec::new();
        reformat_document(SAMPLE_METHOD.as_bytes(), |s, cx| {
            seen.push(cx.object.map(str::to_string));
            Ok(s.to_string())
        })
        .unwrap();
        assert_eq!(
            seen,
            vec![
                Some("P".to_string()),
                Some("P".to_string()),
                Some("P.M".to_string()),
                Some("P.M".to_string()),
            ]
        );
    }

    #[test]
    fn only_st_content_changes() {
        let out = reformat_document(SAMPLE.as_bytes(), |s, _| Ok(s.to_uppercase())).unwrap();
        let out = String::from_utf8(out).unwrap();
        // XML structure and attributes untouched...
        assert!(out.contains("<TcPlcObject Version=\"1.1.0.1\">"));
        assert!(out.contains("Id=\"{abc-123}\""));
        // ...only the ST changed.
        assert!(out.contains("X:=1;"));
    }

    #[test]
    fn cdata_delimiter_in_output_is_refused() {
        let err = reformat_document(SAMPLE.as_bytes(), |_, _| Ok("]]>".to_string()));
        assert!(matches!(err, Err(TwincatError::CdataDelimiterInOutput)));
    }

    #[test]
    fn lf_document_identity_is_byte_identical() {
        // LF-only documents round-trip byte-for-byte too: the EOL lives outside
        // the ST spans and is never rewritten, and LF is reported to the callback.
        let out = reformat_document(SAMPLE_LF.as_bytes(), |s, cx| {
            assert_eq!(
                cx.eol,
                Eol::Lf,
                "an LF document must report LF to the callback"
            );
            Ok(s.to_string())
        })
        .unwrap();
        assert_eq!(
            out,
            SAMPLE_LF.as_bytes(),
            "LF identity reformat must be byte-identical"
        );
    }

    #[test]
    fn callback_error_is_propagated() {
        let err = reformat_document(SAMPLE.as_bytes(), |_, _| Err("engine exploded".into()));
        match err {
            Err(TwincatError::Format(msg)) => assert_eq!(msg, "engine exploded"),
            other => panic!("expected a Format error, got {other:?}"),
        }
    }

    #[test]
    fn callback_error_on_a_later_section_still_fails_the_whole_document() {
        // The first section formats fine; the second fails. The whole call must
        // fail so the caller never writes a half-formatted document.
        let mut calls = 0;
        let err = reformat_document(SAMPLE.as_bytes(), |s, _| {
            calls += 1;
            if calls == 1 {
                Ok(s.to_uppercase())
            } else {
                Err("second section broken".into())
            }
        });
        assert!(matches!(err, Err(TwincatError::Format(_))));
    }

    #[test]
    fn invalid_utf8_in_cdata_is_refused() {
        let mut doc = Vec::new();
        doc.extend_from_slice(b"<TcPlcObject>\r\n  <Declaration><![CDATA[");
        doc.extend_from_slice(&[0xFF, 0xFE, 0x00]);
        doc.extend_from_slice(b"]]></Declaration>\r\n</TcPlcObject>\r\n");
        let err = reformat_document(&doc, |s, _| Ok(s.to_string()));
        assert!(matches!(err, Err(TwincatError::NotUtf8)), "got {err:?}");
    }

    #[test]
    fn declaration_only_document() {
        let doc = "<TcPlcObject>\r\n  <DUT Name=\"D\">\r\n    <Declaration><![CDATA[TYPE D : INT; END_TYPE]]></Declaration>\r\n  </DUT>\r\n</TcPlcObject>\r\n";
        let spans = locate_st_spans(doc.as_bytes()).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, StSectionKind::Declaration);
        let out = reformat_document(doc.as_bytes(), |s, _| Ok(s.to_string())).unwrap();
        assert_eq!(out, doc.as_bytes());
    }

    #[test]
    fn implementation_only_document() {
        let doc = "<TcPlcObject>\r\n  <POU Name=\"P\">\r\n    <Implementation>\r\n      <ST><![CDATA[x:=1;]]></ST>\r\n    </Implementation>\r\n  </POU>\r\n</TcPlcObject>\r\n";
        let spans = locate_st_spans(doc.as_bytes()).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, StSectionKind::Implementation);
        let out = reformat_document(doc.as_bytes(), |s, _| Ok(s.to_string())).unwrap();
        assert_eq!(out, doc.as_bytes());
    }

    #[test]
    fn document_without_st_sections_is_identity() {
        let doc =
            "<TcPlcObject Version=\"1.1.0.1\">\r\n  <Misc Id=\"{xyz}\"/>\r\n</TcPlcObject>\r\n";
        assert!(locate_st_spans(doc.as_bytes()).unwrap().is_empty());
        let out = reformat_document(doc.as_bytes(), |_, _| {
            panic!("callback must not run without ST sections")
        })
        .unwrap();
        assert_eq!(out, doc.as_bytes());
    }

    #[test]
    fn empty_cdata_section_reaches_the_callback_empty() {
        let doc = "<TcPlcObject>\r\n  <POU Name=\"P\">\r\n    <Declaration><![CDATA[]]></Declaration>\r\n  </POU>\r\n</TcPlcObject>\r\n";
        let mut seen = Vec::new();
        let out = reformat_document(doc.as_bytes(), |s, _| {
            seen.push(s.to_string());
            Ok(s.to_string())
        })
        .unwrap();
        assert_eq!(seen, vec![String::new()]);
        assert_eq!(out, doc.as_bytes());
    }

    #[test]
    fn every_section_of_a_method_bearing_pou_is_spliced() {
        // The appended marker proves each section went through the callback;
        // an uppercasing callback would show nothing on the declaration
        // payloads, which are already uppercase.
        let out = reformat_document(SAMPLE_METHOD.as_bytes(), |s, _| {
            Ok(format!("{s}(*spliced*)"))
        })
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text.matches("(*spliced*)").count(),
            4,
            "all four ST sections must be spliced"
        );
        for kept in ["FUNCTION_BLOCK P", "x:=1;", "METHOD M : BOOL", "y:=2;"] {
            assert!(text.contains(kept), "missing section content {kept:?}");
        }
        // Container untouched.
        assert!(text.contains("Id=\"{def-456}\""));
    }

    #[test]
    fn plain_text_sections_are_left_untouched() {
        // ST outside CDATA is not recorded as a span (see StSpan::had_cdata) and
        // must survive byte-for-byte, without the callback ever running.
        let doc = "<TcPlcObject>\r\n  <POU Name=\"P\">\r\n    <Declaration>PROGRAM P</Declaration>\r\n    <Implementation>\r\n      <ST>x:=1;</ST>\r\n    </Implementation>\r\n  </POU>\r\n</TcPlcObject>\r\n";
        assert!(locate_st_spans(doc.as_bytes()).unwrap().is_empty());
        let out = reformat_document(doc.as_bytes(), |_, _| {
            panic!("callback must not run for plain-text sections")
        })
        .unwrap();
        assert_eq!(out, doc.as_bytes());
    }

    #[test]
    fn bom_document_is_preserved_and_only_st_changes() {
        // A UTF-8 BOM is stripped for parsing and re-applied verbatim; the whole
        // document (BOM included) survives both an identity and a real reformat.
        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice(SAMPLE.as_bytes());

        // Identity: byte-for-byte, BOM still first.
        let identity = reformat_document(&with_bom, |s, _| Ok(s.to_string())).unwrap();
        assert_eq!(
            identity, with_bom,
            "BOM document identity must be byte-identical"
        );

        // A real edit changes only the ST and keeps the BOM at the front.
        let edited = reformat_document(&with_bom, |s, _| Ok(s.to_uppercase())).unwrap();
        assert_eq!(
            &edited[..3],
            &[0xEF, 0xBB, 0xBF],
            "the BOM must be preserved at the front"
        );
        let text = String::from_utf8(edited).unwrap();
        assert!(
            text.contains("Id=\"{abc-123}\""),
            "XML container must be untouched"
        );
        assert!(text.contains("X:=1;"), "ST must be uppercased");
    }
}
