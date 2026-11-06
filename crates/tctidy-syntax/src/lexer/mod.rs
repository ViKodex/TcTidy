//! The lexer: bytes → a flat list of [`LexToken`]s covering every byte.
//!
//! Trivia (whitespace, newlines, comments, pragmas) are **not** skipped, they
//! are emitted as tokens, which is the foundation of the lossless property. The
//! post-condition `Σ token.len == src.len()` is asserted in the tests below.
//!
//! `logos` recognizes token *shapes* ([`logos_token`]); [`remap`] then assigns
//! the final [`crate::kind::SyntaxKind`] (splitting identifiers into keyword vs
//! `__twincat` vs ordinary). Anything unrecognized becomes an `ERROR` token, so
//! the lexer is total.

mod callbacks;
mod logos_token;
mod remap;

use text_size::TextSize;

use crate::kind::SyntaxKind;

/// A lexed token: its kind and its byte length (offsets are recomputed by the
/// tree builder, keeping this list compact).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexToken {
    pub kind: SyntaxKind,
    pub len: TextSize,
}

/// Tokenize `src`. Total: every byte is covered by exactly one token.
pub fn lex(src: &str) -> Vec<LexToken> {
    use logos::Logos;

    let mut out = Vec::new();
    let mut lexer = logos_token::LogosTok::lexer(src);
    while let Some(res) = lexer.next() {
        let span = lexer.span();
        let text = &src[span];
        let kind = match res {
            Ok(tok) => remap::remap(tok, text),
            Err(_) => SyntaxKind::ERROR,
        };
        out.push(LexToken {
            kind,
            len: TextSize::of(text),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lexer-level statement of losslessness.
    #[test]
    fn byte_coverage_is_total() {
        for src in [
            "",
            "x := 1;",
            "IF a THEN\n  b := 2;\nEND_IF",
            "// hi\nVAR i:INT;END_VAR",
        ] {
            let total: usize = lex(src).iter().map(|t| u32::from(t.len) as usize).sum();
            assert_eq!(total, src.len(), "byte coverage mismatch for {src:?}");
        }
    }

    #[test]
    fn keywords_are_recognized_case_insensitively() {
        let toks = lex("if");
        assert_eq!(toks[0].kind, SyntaxKind::IF_KW);
        let toks = lex("If");
        assert_eq!(toks[0].kind, SyntaxKind::IF_KW);
    }

    #[test]
    fn exception_markers_are_keywords_but_other_pseudo_identifiers_are_not() {
        for (spelling, kind) in [
            ("__try", SyntaxKind::TRY_KW),
            ("__Catch", SyntaxKind::CATCH_KW),
            ("__FINALLY", SyntaxKind::FINALLY_KW),
            ("__endtry", SyntaxKind::ENDTRY_KW),
        ] {
            let tokens = lex(spelling);
            assert_eq!(tokens.len(), 1, "{spelling}");
            assert_eq!(tokens[0].kind, kind, "{spelling}");
        }
        assert_eq!(lex("__SYSTEM")[0].kind, SyntaxKind::TWINCAT_IDENT);
    }

    #[test]
    fn build_4026_long_date_time_types_are_keywords() {
        for (spelling, kind) in [
            ("LDATE_AND_TIME", SyntaxKind::LDT_KW),
            ("ldate_and_time", SyntaxKind::LDT_KW),
            ("LTIME_OF_DAY", SyntaxKind::LTOD_KW),
            ("ltime_of_day", SyntaxKind::LTOD_KW),
        ] {
            let tokens = lex(spelling);
            assert_eq!(tokens.len(), 1, "{spelling}");
            assert_eq!(tokens[0].kind, kind, "{spelling}");
        }
    }

    #[test]
    fn build_4026_typed_unicode_literals_are_atomic() {
        for source in ["UTF8#'aä$'b$$'", "uchar#'⳧'"] {
            let tokens = lex(source);
            assert_eq!(tokens.len(), 1, "{source}: {tokens:?}");
            assert_eq!(tokens[0].kind, SyntaxKind::TYPED_LITERAL, "{source}");
            assert_eq!(usize::from(tokens[0].len), source.len(), "{source}");
        }
    }

    #[test]
    fn hash_literal_before_case_separator_keeps_colon_separate() {
        for (source, literal_kind) in [
            ("USINT#0:", SyntaxKind::TYPED_LITERAL),
            ("TOD#12:30:00:", SyntaxKind::TOD_LITERAL),
            ("DT#2026-08-13-12:30:00:", SyntaxKind::DT_LITERAL),
        ] {
            let tokens = lex(source);
            assert_eq!(tokens.len(), 2, "{source}: {tokens:?}");
            assert_eq!(tokens[0].kind, literal_kind, "{source}");
            assert_eq!(tokens[1].kind, SyntaxKind::COLON, "{source}");
            assert_eq!(usize::from(tokens[1].len), 1, "{source}");
        }

        for source in ["USINT#0", "TIME#2h30m", "TOD#12:30:00"] {
            let tokens = lex(source);
            assert_eq!(tokens.len(), 1, "{source}: {tokens:?}");
            assert_ne!(tokens[0].kind, SyntaxKind::ERROR, "{source}");
            assert_eq!(usize::from(tokens[0].len), source.len(), "{source}");
        }

        let invalid = lex("USINT#");
        assert_eq!(invalid.len(), 1, "{invalid:?}");
        assert_eq!(invalid[0].kind, SyntaxKind::ERROR);
    }

    /// Unterminated tokens consume to end of input: total byte coverage holds
    /// and the opening delimiter still yields its intended kind.
    #[test]
    fn unterminated_tokens_stay_lossless() {
        let cases = [
            ("'unterminated string", SyntaxKind::STRING_LITERAL),
            ("\"unterminated wstring", SyntaxKind::WSTRING_LITERAL),
            ("(* unclosed paren comment", SyntaxKind::BLOCK_COMMENT),
            ("(* outer (* inner *)", SyntaxKind::BLOCK_COMMENT),
            ("/* unclosed c-style comment", SyntaxKind::C_BLOCK_COMMENT),
            ("{ unclosed pragma", SyntaxKind::PRAGMA),
            ("{msg 'unclosed", SyntaxKind::PRAGMA),
        ];
        for (src, first_kind) in cases {
            let toks = lex(src);
            let total: usize = toks.iter().map(|t| u32::from(t.len) as usize).sum();
            assert_eq!(total, src.len(), "byte coverage mismatch for {src:?}");
            assert_eq!(toks[0].kind, first_kind, "first token of {src:?}");
        }
    }

    #[test]
    fn direct_addresses_lex_as_single_tokens() {
        for src in ["%IX0.0", "%QW10", "%MD4", "%I*"] {
            let toks = lex(src);
            assert_eq!(toks.len(), 1, "expected one token for {src:?}: {toks:?}");
            assert_eq!(toks[0].kind, SyntaxKind::DIRECT_ADDRESS, "for {src:?}");
        }
    }

    /// A malformed address is not silently accepted, but the bytes survive.
    #[test]
    fn malformed_direct_address_is_refused_but_lossless() {
        let src = "%ZZ0";
        let toks = lex(src);
        assert!(
            toks.iter().all(|t| t.kind != SyntaxKind::DIRECT_ADDRESS),
            "must not lex as DIRECT_ADDRESS: {toks:?}"
        );
        let total: usize = toks.iter().map(|t| u32::from(t.len) as usize).sum();
        assert_eq!(total, src.len(), "byte coverage mismatch for {src:?}");
    }

    /// A realistic declaration must tokenize with no ERROR tokens and round-trip.
    #[test]
    fn real_declaration_fully_recognized() {
        let src = "{attribute 'strict'}\r\n\
TYPE ST_X : STRUCT\r\n\
    Start : BOOL; //not used\r\n\
    Limit : REAL := 0.001;\r\n\
    Delay : TIME := T#100MS;\r\n\
    Addr AT %IX0.0 : BOOL;\r\n\
    Hex : DWORD := 16#DEAD_BEEF;\r\n\
    Name : STRING(30) := 'hi$'there';\r\n\
END_STRUCT\r\nEND_TYPE";
        let toks = lex(src);
        let total: usize = toks.iter().map(|t| u32::from(t.len) as usize).sum();
        assert_eq!(total, src.len(), "byte coverage mismatch");
        assert!(
            toks.iter().all(|t| t.kind != SyntaxKind::ERROR),
            "unexpected ERROR token(s): {:?}",
            toks.iter()
                .filter(|t| t.kind == SyntaxKind::ERROR)
                .collect::<Vec<_>>()
        );
        // Spot-check classifications.
        let kinds: Vec<SyntaxKind> = toks.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&SyntaxKind::PRAGMA));
        assert!(kinds.contains(&SyntaxKind::LINE_COMMENT));
        assert!(kinds.contains(&SyntaxKind::REAL_NUMBER));
        assert!(kinds.contains(&SyntaxKind::TIME_LITERAL));
        assert!(kinds.contains(&SyntaxKind::DIRECT_ADDRESS));
        assert!(kinds.contains(&SyntaxKind::STRING_LITERAL));
    }
}
