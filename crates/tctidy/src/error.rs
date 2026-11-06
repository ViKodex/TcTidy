//! The facade's public error type.

use std::fmt;

/// Anything that can go wrong formatting via the facade. On any error the caller
/// keeps the original input untouched (non-destruction).
#[derive(Debug)]
pub enum Error {
    /// A rendered ST diagnostic, a parse error with location and source context
    /// (the common case), or the rare internal formatting failure. Already
    /// self-describing, so it is printed verbatim.
    Parse(String),
    /// The TwinCAT XML container could not be processed.
    Xml(String),
    /// I/O failure.
    Io(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "{m}"),
            Error::Xml(m) => write!(f, "twincat error: {m}"),
            Error::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tctidy_twincat::TwincatError> for Error {
    fn from(e: tctidy_twincat::TwincatError) -> Self {
        match e {
            // A formatting/parse failure inside the ST is already rendered (with
            // location) by the facade closure, surface it verbatim, not as an
            // XML-container error. Genuine container failures keep the XML prefix.
            tctidy_twincat::TwincatError::Format(m) => Error::Parse(m),
            other => Error::Xml(other.to_string()),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
