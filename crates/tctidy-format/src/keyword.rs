//! The single implementation site for keyword casing.
//!
//! Reached only via [`crate::context::FormatContext::kw`] / `type_kw`, so keyword
//! casing cannot leak into individual rules.

use crate::style::KeywordCase;

/// Apply the configured casing to a keyword's raw source text.
pub(crate) fn apply_keyword_case(raw: &str, case: KeywordCase) -> String {
    match case {
        KeywordCase::Upper => raw.to_ascii_uppercase(),
        KeywordCase::Lower => raw.to_ascii_lowercase(),
        KeywordCase::Preserve => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casing() {
        assert_eq!(apply_keyword_case("if", KeywordCase::Upper), "IF");
        assert_eq!(apply_keyword_case("IF", KeywordCase::Lower), "if");
        assert_eq!(apply_keyword_case("If", KeywordCase::Preserve), "If");
    }
}
