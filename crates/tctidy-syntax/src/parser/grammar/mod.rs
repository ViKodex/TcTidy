//! The ST grammar, the only place grammar rules live.
//!
//! Split into [`decl`] (declaration part), [`stmt`] (implementation statements),
//! and [`expr`] (Pratt/precedence-climbing expressions, preserving parentheses).
//! Parentheses are kept as `PAREN_EXPR` nodes and never added or removed.

pub(crate) mod decl;
pub(crate) mod expr;
pub(crate) mod stmt;

use super::parser_impl::Parser;
use crate::error::ErrorKind;
use crate::kind::SyntaxKind::{self, DOT, IDENT, SUPER_KW, THIS_KW, TWINCAT_IDENT};

/// Whole-file ST (stdin path): a statement list.
pub(crate) fn source_file(p: &mut Parser) {
    stmt::impl_part_body(p);
}

/// Is the current token the start of a name (identifier-like)?
pub(crate) fn at_name(p: &Parser) -> bool {
    matches!(p.current(), IDENT | TWINCAT_IDENT | THIS_KW | SUPER_KW)
}

/// Bump a dotted name `a.b.c` as plain tokens (no wrapper node).
pub(crate) fn dotted_name(p: &mut Parser) {
    if !at_name(p) {
        p.error(ErrorKind::Expected, "expected a name");
        return;
    }
    p.bump();
    while p.at(DOT) {
        p.bump();
        if at_name(p) {
            p.bump();
        } else {
            p.error(ErrorKind::Expected, "expected name after '.'");
            break;
        }
    }
}

/// Elementary / generic type keyword from the generated inventory.
pub(crate) fn is_type_keyword(k: SyntaxKind) -> bool {
    crate::kind::keywords::is_type_keyword(k)
}
