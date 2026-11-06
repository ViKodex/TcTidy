//! `tctidy`, the library facade and public API.
//!
//! This is what editor extensions, build tools, and the CLI link against. It
//! decides raw-ST vs TwinCAT-file, selects the right grammar, wires the XML
//! container layer to the formatter, and owns the public [`Error`] type. It adds
//! no formatting logic of its own, beyond keeping the final newline of the ST.
//!
//! Both entry points are live: [`format_st`] formats a raw `<Declaration>` or
//! `<Implementation>` ST string, and [`format_twincat_file`] reformats every ST
//! section inside a `.TcPOU`/`.TcGVL`/`.TcDUT` file while leaving the surrounding
//! XML byte-identical. On any error the caller keeps the original untouched.
#![forbid(unsafe_code)]

pub mod config;
mod error;
pub mod settings;

pub use config::{
    find_config, load_config, parse_config, update_config, ConfigError, CONFIG_FILE_NAME,
};
pub use error::{Error, Result};
pub use tctidy_format::StyleConfig;

use tctidy_format::{format_declaration, format_implementation, FormatError};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_declaration, parse_implementation};
use tctidy_twincat::{reformat_document, Eol, StSectionKind};

/// Which ST grammar to use for a raw string (the two are distinct).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StSection {
    Declaration,
    Implementation,
    /// Default to the implementation grammar.
    Auto,
}

/// Formatting options. [`FormatOptions::default`] is the house style; a
/// `.tctidy.toml` is turned into a [`StyleConfig`] by [`load_config`].
#[derive(Clone, Debug, Default)]
pub struct FormatOptions {
    pub style: StyleConfig,
}

/// Result of formatting a whole file.
#[derive(Debug)]
pub struct FormatOutput {
    pub bytes: Vec<u8>,
    pub changed: bool,
}

/// Format a raw ST string.
///
/// Its final newline is kept: when the input's last line ends with a line
/// break (blank lines and spaces after it aside), a non-empty output ends with
/// one, in the input's EOL. An input without one gets none.
pub fn format_st(src: &str, section: StSection, opts: &FormatOptions) -> Result<String> {
    let eol = Eol::detect(src.as_bytes());
    let (result, label) = match section {
        StSection::Declaration => (
            format_declaration(&parse_declaration(src), &opts.style, map_eol(eol)),
            "declaration",
        ),
        StSection::Implementation | StSection::Auto => (
            format_implementation(&parse_implementation(src), &opts.style, map_eol(eol)),
            "implementation",
        ),
    };
    // Raw ST has no surrounding object to name.
    let out = result.map_err(|e| Error::Parse(render_st_failure(&e, src, label, None)))?;
    Ok(keep_final_newline(src, out, eol))
}

/// End the formatted `out` with a line break in `eol` when the last line of
/// `src` ends with one, ignoring the blank lines and spaces after it. An output
/// that is empty (the input was blank) stays empty.
fn keep_final_newline(src: &str, mut out: String, eol: Eol) -> String {
    let terminated = src.trim_end_matches([' ', '\t']).ends_with('\n');
    if terminated && !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol.as_str());
    }
    out
}

/// Format a whole TwinCAT file (`.TcPOU`/`.TcGVL`/`.TcDUT`), preserving every
/// byte outside the ST. The XML container layer is invoked here and the
/// formatter is injected into it as a callback. Each ST section keeps the line
/// break that ends it before `]]>`, as [`format_st`] keeps a final newline.
pub fn format_twincat_file(bytes: &[u8], opts: &FormatOptions) -> Result<FormatOutput> {
    let out = reformat_document(bytes, |st, cx| {
        let eol = map_eol(cx.eol);
        let (result, label) = match cx.kind {
            StSectionKind::Declaration => (
                format_declaration(&parse_declaration(st), &opts.style, eol),
                "declaration",
            ),
            StSectionKind::Implementation => (
                format_implementation(&parse_implementation(st), &opts.style, eol),
                "implementation",
            ),
        };
        // `st` is the section text, so error offsets resolve here; render the
        // located diagnostic now (the container layer only sees the string), naming
        // the containing object (e.g. FB_X.Init) when the span has one.
        result
            .map(|out| keep_final_newline(st, out, cx.eol))
            .map_err(|e| render_st_failure(&e, st, label, cx.object).into())
    })?;
    let changed = out != bytes;
    Ok(FormatOutput {
        bytes: out,
        changed,
    })
}

fn map_eol(eol: Eol) -> LineEnding {
    match eol {
        Eol::Lf => LineEnding::Lf,
        Eol::Crlf => LineEnding::Crlf,
    }
}

/// Turn a formatting failure into a self-describing message. A parse error names the failing
/// section and the source lines involved (`section`, declaration or implementation, disambiguates
/// a multi-section file, and line numbers are relative to that section), and, on the TwinCAT file
/// path, the containing `object` (e.g. `FB_X.Init`) so a failure points at the exact method/action,
/// not just the file; the rare internal error falls back to its terse description.
fn render_st_failure(e: &FormatError, src: &str, section: &str, object: Option<&str>) -> String {
    match e {
        FormatError::Parse(errs) => {
            let location = match object {
                Some(object) => format!("{object} {section}"),
                None => section.to_string(),
            };
            format!(
                "parse error in {location}: {}",
                tctidy_syntax::diagnostics::render(src, errs)
            )
        }
        FormatError::Internal(_) => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_uses_the_opinion() {
        let opts = FormatOptions::default();
        assert_eq!(opts.style, StyleConfig::default());
    }

    /// A DUT whose declaration CDATA holds `st`, with CRLF line endings.
    fn dut(st: &str) -> String {
        format!(
            "<TcPlcObject>\r\n  <DUT Name=\"D\">\r\n    \
             <Declaration><![CDATA[{st}]]></Declaration>\r\n  </DUT>\r\n</TcPlcObject>\r\n"
        )
    }

    #[test]
    fn twincat_section_keeps_its_final_newline() {
        let messy = "TYPE D : STRUCT\r\na:INT;\r\nEND_STRUCT\r\nEND_TYPE";
        let formatted = "TYPE D : STRUCT\r\n    a : INT;\r\nEND_STRUCT\r\nEND_TYPE";
        // Kept once, blank lines and spaces after it aside; never added; and a
        // section of only blank lines formats to empty, as raw ST does.
        for (st, expected) in [
            (format!("{messy}\r\n"), format!("{formatted}\r\n")),
            (format!("{messy}\r\n\r\n  "), format!("{formatted}\r\n")),
            (messy.to_string(), formatted.to_string()),
            ("\r\n\r\n".to_string(), String::new()),
        ] {
            let out = format_twincat_file(dut(&st).as_bytes(), &FormatOptions::default())
                .expect("formats cleanly");
            assert_eq!(
                String::from_utf8(out.bytes).unwrap(),
                dut(&expected),
                "{st:?}"
            );
        }
    }
}
