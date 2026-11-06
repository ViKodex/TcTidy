//! Errors from the TwinCAT container layer. Any error means "leave the file
//! untouched", never a partial or corrupted write.

use std::fmt;

#[derive(Debug)]
pub enum TwincatError {
    /// The XML could not be parsed; the file is left untouched.
    Xml(String),
    /// The injected formatter failed on a section.
    Format(String),
    /// The file was not valid UTF-8.
    NotUtf8,
    /// Formatted ST contained the byte sequence `]]>`, which would terminate the
    /// CDATA early and corrupt the XML. Refused.
    CdataDelimiterInOutput,
}

impl fmt::Display for TwincatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TwincatError::Xml(m) => write!(f, "XML error: {m}"),
            TwincatError::Format(m) => write!(f, "formatting error: {m}"),
            TwincatError::NotUtf8 => write!(f, "the file is not valid UTF-8"),
            TwincatError::CdataDelimiterInOutput => {
                write!(
                    f,
                    "formatted ST contained `]]>` which would break the CDATA section"
                )
            }
        }
    }
}

impl std::error::Error for TwincatError {}
