//! Assigns the final [`SyntaxKind`] to a raw `logos` token.
//!
//! Identifiers are reclassified here: `__twincat` pseudo-identifiers, the
//! `<TYPE>_TO_<TYPE>` / `TO_<TYPE>` conversion family (recognized by pattern,
//! not an exhaustive list), reserved keywords (case-insensitive), and finally
//! ordinary identifiers. `<word>#<value>` literals are classified by prefix into
//! time / date / typed literals.

use super::callbacks::is_terminated_string;
use super::logos_token::LogosTok;
use crate::kind::keywords::keyword_kind;
use crate::kind::SyntaxKind::{self, *};

/// Map a raw token and its source text to a final kind.
pub(crate) fn remap(tok: LogosTok, text: &str) -> SyntaxKind {
    match tok {
        LogosTok::Whitespace => WHITESPACE,
        LogosTok::Newline => NEWLINE,
        LogosTok::LineComment => LINE_COMMENT,
        LogosTok::BlockComment => BLOCK_COMMENT,
        LogosTok::CBlockComment => C_BLOCK_COMMENT,
        LogosTok::Pragma => PRAGMA,
        LogosTok::TypedUnicodeString => {
            let body = text.split_once("#'").map_or("", |(_, body)| body);
            if is_terminated_string(body, b'\'') {
                TYPED_LITERAL
            } else {
                ERROR
            }
        }
        LogosTok::SqString => STRING_LITERAL,
        LogosTok::DqString => WSTRING_LITERAL,
        LogosTok::BasedInt | LogosTok::DecInt => INT_NUMBER,
        LogosTok::Real | LogosTok::RealExp => REAL_NUMBER,
        LogosTok::HashLit => classify_hash_lit(text),
        LogosTok::DirectAddress => DIRECT_ADDRESS,
        LogosTok::Ident => classify_ident(text),
        LogosTok::Assign => ASSIGN,
        LogosTok::RefAssign => REF_ASSIGN,
        LogosTok::SetAssign => SET_ASSIGN,
        LogosTok::ResetAssign => RESET_ASSIGN,
        LogosTok::Arrow => ARROW,
        LogosTok::Neq => NEQ,
        LogosTok::Lte => LTE,
        LogosTok::Gte => GTE,
        LogosTok::Pow => POW,
        LogosTok::DotDot => DOTDOT,
        LogosTok::Semi => SEMICOLON,
        LogosTok::Colon => COLON,
        LogosTok::Comma => COMMA,
        LogosTok::Dot => DOT,
        LogosTok::LParen => L_PAREN,
        LogosTok::RParen => R_PAREN,
        LogosTok::LBrack => L_BRACK,
        LogosTok::RBrack => R_BRACK,
        LogosTok::Eq => EQ,
        LogosTok::Lt => LT,
        LogosTok::Gt => GT,
        LogosTok::Plus => PLUS,
        LogosTok::Minus => MINUS,
        LogosTok::Star => STAR,
        LogosTok::Slash => SLASH,
        LogosTok::Amp => AMP,
        LogosTok::Caret => CARET,
    }
}

/// Classify a `<word>#<value>` literal by its prefix word.
fn classify_hash_lit(text: &str) -> SyntaxKind {
    if text.ends_with('#') {
        return ERROR;
    }
    let prefix = text.split('#').next().unwrap_or("");
    match prefix.to_ascii_uppercase().as_str() {
        "T" | "TIME" | "LTIME" => TIME_LITERAL,
        "D" | "DATE" | "LDATE" => DATE_LITERAL,
        "TOD" | "TIME_OF_DAY" | "LTOD" => TOD_LITERAL,
        "DT" | "DATE_AND_TIME" | "LDT" => DT_LITERAL,
        _ => TYPED_LITERAL,
    }
}

fn classify_ident(text: &str) -> SyntaxKind {
    if text.starts_with("__") {
        return keyword_kind(text).unwrap_or(TWINCAT_IDENT);
    }
    if is_conversion(text) {
        return TYPE_CONV;
    }
    if let Some(kw) = keyword_kind(text) {
        return kw;
    }
    IDENT
}

/// Recognize `TRUNC`, `TRUNC_INT`, `TO_<TYPE>`, and `<TYPE>_TO_<TYPE>` by shape.
/// Mis-classifying a user identifier here is cosmetically harmless (both are leaf
/// names); the kind mainly marks "do not rewrite".
fn is_conversion(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    if upper == "TRUNC" || upper == "TRUNC_INT" {
        return true;
    }
    if let Some(rest) = upper.strip_prefix("TO_") {
        return is_type_word(rest);
    }
    if let Some(idx) = upper.rfind("_TO_") {
        let (from, to) = (&upper[..idx], &upper[idx + 4..]);
        return is_type_word(from) && is_type_word(to);
    }
    false
}

fn is_type_word(w: &str) -> bool {
    matches!(
        w,
        "BOOL"
            | "BYTE"
            | "WORD"
            | "DWORD"
            | "LWORD"
            | "SINT"
            | "INT"
            | "DINT"
            | "LINT"
            | "USINT"
            | "UINT"
            | "UDINT"
            | "ULINT"
            | "REAL"
            | "LREAL"
            | "TIME"
            | "LTIME"
            | "DATE"
            | "DT"
            | "TOD"
            | "STRING"
            | "WSTRING"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_literals_classify_by_prefix() {
        let cases = [
            ("T#1d", TIME_LITERAL),
            ("time#2h30m", TIME_LITERAL),
            ("LTIME#100us", TIME_LITERAL),
            ("D#2026-07-05", DATE_LITERAL),
            ("DATE#2026-07-05", DATE_LITERAL),
            ("LDATE#2026-07-05", DATE_LITERAL),
            ("TOD#12:00:00", TOD_LITERAL),
            ("TIME_OF_DAY#12:00:00", TOD_LITERAL),
            ("LTOD#12:00:00", TOD_LITERAL),
            ("DT#2026-07-05-12:00:00", DT_LITERAL),
            ("DATE_AND_TIME#2026-07-05-12:00:00", DT_LITERAL),
            ("LDT#2026-07-05-12:00:00", DT_LITERAL),
            ("INT#100", TYPED_LITERAL),
            ("UDINT#42", TYPED_LITERAL),
            ("BANANA#1", TYPED_LITERAL),
            ("UTF8#", ERROR),
            ("UCHAR#", ERROR),
        ];
        for (text, want) in cases {
            assert_eq!(classify_hash_lit(text), want, "for {text:?}");
        }
    }
}
