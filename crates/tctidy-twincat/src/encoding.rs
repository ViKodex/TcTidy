//! Encoding (UTF-8 ± BOM) and end-of-line detection/preservation.
//!
//! The BOM is stripped before XML parsing (so byte offsets are computed on
//! BOM-free bytes) and re-applied after splicing. The document's own line
//! endings are never touched, they live outside the ST spans, but the detected
//! EOL is passed to the formatter so any *newly inserted* lines match.

/// Whether the file carried a UTF-8 BOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
}

/// The dominant line ending of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
}

impl Encoding {
    /// Detect and strip a UTF-8 BOM, returning the body and the encoding.
    pub fn detect_and_strip_bom(input: &[u8]) -> (&[u8], Encoding) {
        if input.starts_with(&[0xEF, 0xBB, 0xBF]) {
            (&input[3..], Encoding::Utf8Bom)
        } else {
            (input, Encoding::Utf8)
        }
    }

    /// Re-prepend the BOM if the original had one.
    pub fn reapply_bom(self, body: Vec<u8>) -> Vec<u8> {
        match self {
            Encoding::Utf8Bom => {
                let mut v = Vec::with_capacity(body.len() + 3);
                v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
                v.extend_from_slice(&body);
                v
            }
            Encoding::Utf8 => body,
        }
    }
}

impl Eol {
    /// The first newline decides; default CRLF (the TwinCAT convention) if none.
    pub fn detect(body: &[u8]) -> Eol {
        match memchr::memchr(b'\n', body) {
            Some(i) if i > 0 && body[i - 1] == b'\r' => Eol::Crlf,
            Some(_) => Eol::Lf,
            None => Eol::Crlf,
        }
    }

    /// The literal bytes for this EOL.
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_round_trip() {
        let with_bom = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        let (body, enc) = Encoding::detect_and_strip_bom(&with_bom);
        assert_eq!(body, b"hi");
        assert_eq!(enc, Encoding::Utf8Bom);
        assert_eq!(enc.reapply_bom(body.to_vec()), with_bom);
    }

    #[test]
    fn eol_detection() {
        assert_eq!(Eol::detect(b"a\r\nb"), Eol::Crlf);
        assert_eq!(Eol::detect(b"a\nb"), Eol::Lf);
        assert_eq!(Eol::detect(b"no newline"), Eol::Crlf);
    }

    #[test]
    fn eol_as_str_matches_the_variant() {
        assert_eq!(Eol::Lf.as_str(), "\n");
        assert_eq!(Eol::Crlf.as_str(), "\r\n");
    }
}
