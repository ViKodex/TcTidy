//! Typed token wrappers, including the trivia the formatter cares about.

use super::{ast_token, AstToken};
use crate::kind::SyntaxKind::{self, *};
use crate::syntax_node::SyntaxToken;

ast_token!(Whitespace, WHITESPACE);
ast_token!(Newline, NEWLINE);
ast_token!(Pragma, PRAGMA);

impl Newline {
    /// Whether this newline is a CRLF (`\r\n`) as opposed to a bare LF.
    pub fn is_crlf(&self) -> bool {
        self.text() == "\r\n"
    }
}

/// The four pragma families, classified from the verbatim text without ever
/// changing the token kind (the token stays a single `PRAGMA`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PragmaKind {
    Attribute,
    RegionOpen,
    RegionClose,
    Conditional,
    Message,
    Unknown,
}

impl Pragma {
    /// Read-only classification of the pragma's content.
    pub fn classify(&self) -> PragmaKind {
        classify_pragma_text(self.text())
    }
    pub fn is_region_open(&self) -> bool {
        self.classify() == PragmaKind::RegionOpen
    }
    pub fn is_region_close(&self) -> bool {
        self.classify() == PragmaKind::RegionClose
    }
}

fn classify_pragma_text(text: &str) -> PragmaKind {
    let inner = text.trim().trim_start_matches('{').trim_end_matches('}');
    let lower = inner.trim_start().to_ascii_lowercase();
    if lower.starts_with("attribute") {
        PragmaKind::Attribute
    } else if lower.starts_with("endregion") {
        PragmaKind::RegionClose
    } else if lower.starts_with("region") {
        PragmaKind::RegionOpen
    } else if ["if", "elsif", "else", "end_if", "define", "undefine"]
        .iter()
        .any(|k| lower.starts_with(k))
    {
        PragmaKind::Conditional
    } else if ["warning", "info", "text"]
        .iter()
        .any(|k| lower.starts_with(k))
    {
        PragmaKind::Message
    } else {
        PragmaKind::Unknown
    }
}

/// Any of the three comment syntaxes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Comment {
    Line(SyntaxToken),
    Paren(SyntaxToken),
    C(SyntaxToken),
}

impl AstToken for Comment {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, LINE_COMMENT | BLOCK_COMMENT | C_BLOCK_COMMENT)
    }
    fn cast(token: SyntaxToken) -> Option<Self> {
        match token.kind() {
            LINE_COMMENT => Some(Comment::Line(token)),
            BLOCK_COMMENT => Some(Comment::Paren(token)),
            C_BLOCK_COMMENT => Some(Comment::C(token)),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxToken {
        match self {
            Comment::Line(t) | Comment::Paren(t) | Comment::C(t) => t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::AstToken;
    use crate::parser::parse_implementation;

    fn first_token(src: &str, kind: SyntaxKind) -> SyntaxToken {
        parse_implementation(src)
            .syntax_node()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == kind)
            .unwrap_or_else(|| panic!("no {kind:?} token in {src:?}"))
    }

    #[test]
    fn pragma_classification_covers_every_family() {
        use PragmaKind::*;
        let cases = [
            ("{attribute 'qualified_only'}", Attribute),
            ("{region init}", RegionOpen),
            ("{endregion}", RegionClose),
            ("{IF defined (simulation)}", Conditional),
            ("{elsif defined (hardware)}", Conditional),
            ("{else}", Conditional),
            ("{end_if}", Conditional),
            ("{define machine 1}", Conditional),
            ("{undefine machine}", Conditional),
            ("{warning 'not converted'}", Message),
            ("{info 'checked'}", Message),
            ("{text 'free text'}", Message),
            ("{analysis -33}", Unknown),
            ("{}", Unknown),
        ];
        for (text, want) in cases {
            assert_eq!(classify_pragma_text(text), want, "for {text:?}");
        }
    }

    #[test]
    fn pragma_token_classifies_and_answers_region_queries() {
        let open = Pragma::cast(first_token("{region io}\nx := 1;", PRAGMA)).unwrap();
        assert_eq!(open.classify(), PragmaKind::RegionOpen);
        assert!(open.is_region_open());
        assert!(!open.is_region_close());

        let close = Pragma::cast(first_token("{endregion}\nx := 1;", PRAGMA)).unwrap();
        assert_eq!(close.classify(), PragmaKind::RegionClose);
        assert!(!close.is_region_open());
        assert!(close.is_region_close());
    }

    #[test]
    fn comment_casts_distinguish_the_three_syntaxes() {
        let line = Comment::cast(first_token("// note\nx := 1;", LINE_COMMENT)).unwrap();
        assert!(matches!(line, Comment::Line(_)));
        assert_eq!(line.text(), "// note");

        let paren = Comment::cast(first_token("(* note *)\nx := 1;", BLOCK_COMMENT)).unwrap();
        assert!(matches!(paren, Comment::Paren(_)));

        let c = Comment::cast(first_token("/* note */\nx := 1;", C_BLOCK_COMMENT)).unwrap();
        assert!(matches!(c, Comment::C(_)));
    }

    #[test]
    fn comment_cast_refuses_non_comment_tokens() {
        assert!(!Comment::can_cast(IDENT));
        assert!(Comment::cast(first_token("x := 1;", IDENT)).is_none());
    }

    #[test]
    fn newline_distinguishes_crlf_from_lf() {
        let crlf = Newline::cast(first_token("x := 1;\r\ny := 2;", NEWLINE)).unwrap();
        assert!(crlf.is_crlf());

        let lf = Newline::cast(first_token("x := 1;\ny := 2;", NEWLINE)).unwrap();
        assert!(!lf.is_crlf());
    }
}
