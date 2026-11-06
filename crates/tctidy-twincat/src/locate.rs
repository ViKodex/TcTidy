//! Locate the byte ranges of ST content (the CDATA inside `<Declaration>` and
//! `<Implementation><ST>`), recursively, anywhere in the document.
//!
//! `quick-xml` serves purely as a *reader* to discover where the ST is; the XML is never
//! re-serialized (that would not be byte-preserving). The returned
//! [`StSpan::content`] is the exact byte range of the ST *inside* the CDATA
//! delimiters.

use std::ops::Range;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::error::TwincatError;

/// Which kind of ST section a span belongs to (selects the grammar downstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StSectionKind {
    Declaration,
    Implementation,
}

/// A located ST section.
#[derive(Debug, Clone)]
pub struct StSpan {
    pub kind: StSectionKind,
    /// Byte range of the ST content, excluding the `<![CDATA[` / `]]>` delimiters.
    pub content: Range<usize>,
    /// Whether the content was CDATA-wrapped (always true: only CDATA spans are
    /// recorded, plain-text ST sections are left untouched, see crate docs).
    pub had_cdata: bool,
    /// Qualified name of the containing object (e.g. `FB_X.Init`), joined from the `Name`
    /// attributes of the enclosing elements; `None` when none is named. For diagnostics.
    pub object: Option<String>,
}

/// Find every CDATA ST span in document order.
pub fn locate_st_spans(xml: &[u8]) -> Result<Vec<StSpan>, TwincatError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().check_end_names = false;

    let mut stack: Vec<String> = Vec::new();
    // Parallel to `stack`: each element's `Name` attribute (None when it has none), so the
    // chain of named ancestors gives a span's qualified object name (e.g. FB_X.Init).
    let mut names: Vec<Option<String>> = Vec::new();
    let mut spans: Vec<StSpan> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();

    loop {
        let pos_before = reader.buffer_position() as usize;
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                stack.push(local_name(e.name().as_ref()));
                names.push(name_attr(&e));
            }
            Ok(Event::End(_)) => {
                stack.pop();
                names.pop();
            }
            Ok(Event::CData(_)) => {
                if let Some(kind) = relevant(&stack) {
                    let pos_after = reader.buffer_position() as usize;
                    let content = cdata_content_range(xml, pos_before, pos_after)?;
                    spans.push(StSpan {
                        kind,
                        content,
                        had_cdata: true,
                        object: qualified_object(&names),
                    });
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(TwincatError::Xml(e.to_string())),
        }
        buf.clear();
    }
    Ok(spans)
}

/// Strip any namespace prefix from a tag's qualified name.
fn local_name(qname: &str) -> String {
    match qname.rfind(':') {
        Some(i) => qname[i + 1..].to_owned(),
        None => qname.to_owned(),
    }
}

/// The element's `Name` attribute (`<POU Name="FB_X">` → `FB_X`), if any. Used to name the
/// containing object in diagnostics; an element without one (Declaration, Implementation, ST, …)
/// contributes nothing to the qualified name.
fn name_attr(e: &BytesStart) -> Option<String> {
    e.try_get_attribute("Name")
        .ok()
        .flatten()
        .and_then(|attr| attr.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(|value| value.into_owned())
}

/// Join the `Name` attributes of the enclosing elements into a qualified object name
/// (`POU Name="FB_X"` ▸ `Method Name="Init"` → `FB_X.Init`); `None` when no ancestor is named.
fn qualified_object(names: &[Option<String>]) -> Option<String> {
    let parts: Vec<&str> = names.iter().filter_map(Option::as_deref).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

/// Is the reader inside an ST-bearing element? `Declaration`, or `ST` within
/// `Implementation`.
fn relevant(stack: &[String]) -> Option<StSectionKind> {
    match stack.last().map(String::as_str) {
        Some("Declaration") => Some(StSectionKind::Declaration),
        Some("ST") if stack.iter().rev().nth(1).map(String::as_str) == Some("Implementation") => {
            Some(StSectionKind::Implementation)
        }
        _ => None,
    }
}

/// Compute the inner content range of a CDATA token spanning `[start, end)` in
/// `xml`, by locating the delimiters within that window (robust to any leading
/// text the reader may have included).
fn cdata_content_range(xml: &[u8], start: usize, end: usize) -> Result<Range<usize>, TwincatError> {
    const OPEN: &[u8] = b"<![CDATA[";
    const CLOSE: &[u8] = b"]]>";
    let window = &xml[start..end];
    let rel_open = memchr::memmem::find(window, OPEN)
        .ok_or_else(|| TwincatError::Xml("CDATA open delimiter not found".into()))?;
    let rel_close = memchr::memmem::rfind(window, CLOSE)
        .ok_or_else(|| TwincatError::Xml("CDATA close delimiter not found".into()))?;
    let content_start = start + rel_open + OPEN.len();
    let content_end = start + rel_close;
    if content_start > content_end {
        return Err(TwincatError::Xml("malformed CDATA range".into()));
    }
    Ok(content_start..content_end)
}
