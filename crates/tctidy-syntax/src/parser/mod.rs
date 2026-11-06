//! The parser: tokens → a structured, lossless `rowan` green tree.
//!
//! A hand-written recursive-descent parser (see [`parser_impl`]) drives a
//! `GreenNodeBuilder` directly. Three entry points mirror the way TwinCAT stores
//! ST in separate sections: the declaration part, the implementation body, and a
//! whole raw-ST file (stdin).

mod grammar;
mod parser_impl;

use crate::ast::{AstNode, DeclPart, ImplPart, SourceFile};
use crate::kind::SyntaxKind::{self, DECL_PART, IMPL_PART, SOURCE_FILE};
use crate::lexer::lex;
use crate::parse::Parse;
use parser_impl::Parser;

/// Parse a whole raw-ST file (stdin path). Root: `SOURCE_FILE`.
pub fn parse_source_file(src: &str) -> Parse<SourceFile> {
    run(src, SOURCE_FILE, grammar::source_file)
}

/// Parse a TwinCAT `<Declaration>` section. Root: `DECL_PART`.
pub fn parse_declaration(src: &str) -> Parse<DeclPart> {
    run(src, DECL_PART, grammar::decl::decl_part)
}

/// Parse a TwinCAT `<Implementation><ST>` body. Root: `IMPL_PART`.
pub fn parse_implementation(src: &str) -> Parse<ImplPart> {
    run(src, IMPL_PART, grammar::stmt::impl_part)
}

fn run<T: AstNode>(src: &str, root: SyntaxKind, grammar: fn(&mut Parser)) -> Parse<T> {
    let tokens = lex(src);
    let mut p = Parser::new(src, &tokens);
    p.start(root);
    grammar(&mut p);
    p.emit_remaining(); // trailing trivia
    p.finish_node();
    let (green, errors) = p.into_green();
    Parse::new(green, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SyntaxKind;

    fn kinds(src: &str) -> Vec<SyntaxKind> {
        parse_declaration(src)
            .syntax_node()
            .descendants()
            .map(|n| n.kind())
            .collect()
    }

    #[test]
    fn declaration_round_trips() {
        let samples = [
            "TYPE ST_X : STRUCT\r\n    A : INT;\r\n    B : BOOL := TRUE;\r\nEND_STRUCT\r\nEND_TYPE",
            "TYPE E : (\r\n    Red,\r\n    Green := 2,\r\n    Blue\r\n);\r\nEND_TYPE",
            "FUNCTION_BLOCK FB_X EXTENDS FB_Base IMPLEMENTS I_A, I_B\r\nVAR_INPUT\r\n    x : INT;\r\nEND_VAR",
            "VAR_GLOBAL CONSTANT\r\n    MAX : INT := 10;\r\n    P : POINTER TO BYTE;\r\n    A : ARRAY[0..9] OF REAL;\r\nEND_VAR",
        ];
        for src in samples {
            let parse = parse_declaration(src);
            assert!(parse.debug_text_matches(src), "round-trip failed:\n{src}");
            assert!(
                parse.errors().is_empty(),
                "unexpected errors in:\n{src}\n{:?}",
                parse.errors()
            );
        }
    }

    #[test]
    fn declaration_has_structure() {
        let ks = kinds("TYPE ST_X : STRUCT\r\n    A : INT;\r\nEND_STRUCT\r\nEND_TYPE");
        assert!(ks.contains(&SyntaxKind::TYPE_DECL));
        assert!(ks.contains(&SyntaxKind::STRUCT_TYPE));
        assert!(ks.contains(&SyntaxKind::VAR_DECL));
        assert!(ks.contains(&SyntaxKind::TYPE_REF));
    }
}
