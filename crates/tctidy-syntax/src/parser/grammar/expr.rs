//! Expression grammar: precedence climbing (Pratt) with the ST binding-power
//! table. Postfix `.`/`[]`/`^`/`()` chains build left-associative nodes via
//! checkpoints. Parentheses are preserved as `PAREN_EXPR`.

use super::super::parser_impl::Parser;
use super::{at_name, dotted_name};
use crate::error::ErrorKind;
use crate::kind::SyntaxKind::{self, *};

pub(crate) fn expr(p: &mut Parser) {
    expr_bp(p, 0, None);
}

/// Parse an expression whose top level ends before `terminator`.
pub(crate) fn expr_before(p: &mut Parser, terminator: SyntaxKind) {
    expr_bp(p, 0, Some(terminator));
}

/// Left/right binding powers; `None` if `kind` is not a binary operator.
fn binding_power(kind: SyntaxKind) -> Option<(u8, u8)> {
    Some(match kind {
        OR_KW | OR_ELSE_KW => (1, 2),
        XOR_KW => (3, 4),
        AND_KW | AND_THEN_KW | AMP => (5, 6),
        EQ | NEQ => (7, 8),
        LT | GT | LTE | GTE => (9, 10),
        PLUS | MINUS => (11, 12),
        STAR | SLASH | MOD_KW => (13, 14),
        POW => (16, 15), // right-associative
        _ => return None,
    })
}

fn expr_bp(p: &mut Parser, min_bp: u8, terminator: Option<SyntaxKind>) {
    let cp = p.checkpoint();
    unary(p);
    loop {
        let op = p.current();
        if Some(op) == terminator {
            break;
        }
        let Some((lbp, rbp)) = binding_power(op) else {
            break;
        };
        if lbp < min_bp {
            break;
        }
        p.start_at(cp, BIN_EXPR);
        p.bump(); // operator
        expr_bp(p, rbp, terminator);
        p.finish_node();
    }
}

fn unary(p: &mut Parser) {
    match p.current() {
        NOT_KW | MINUS | PLUS => {
            p.start(UNARY_EXPR);
            p.bump();
            unary(p);
            p.finish_node();
        }
        _ => postfix(p),
    }
}

fn postfix(p: &mut Parser) {
    let cp = p.checkpoint();
    primary(p);
    loop {
        match p.current() {
            DOT => {
                p.start_at(cp, QUALIFIED_REF);
                p.bump();
                // A member name, or a bit-access index (`port.0`, `flags.11`).
                if at_name(p) || p.at(INT_NUMBER) {
                    p.start(NAME_REF);
                    p.bump();
                    p.finish_node();
                } else {
                    p.error(ErrorKind::Expected, "expected name or bit index after '.'");
                }
                p.finish_node();
            }
            L_BRACK => {
                p.start_at(cp, INDEX_EXPR);
                p.bump();
                if !p.at(R_BRACK) {
                    expr(p);
                    while p.eat(COMMA) {
                        expr(p);
                    }
                }
                p.expect(R_BRACK);
                p.finish_node();
            }
            CARET => {
                p.start_at(cp, DEREF_EXPR);
                p.bump();
                p.finish_node();
            }
            L_PAREN => {
                p.start_at(cp, CALL_EXPR);
                arg_list(p);
                p.finish_node();
            }
            _ => break,
        }
    }
}

fn primary(p: &mut Parser) {
    match p.current() {
        INT_NUMBER | REAL_NUMBER | TYPED_LITERAL | TIME_LITERAL | DATE_LITERAL | TOD_LITERAL
        | DT_LITERAL | STRING_LITERAL | WSTRING_LITERAL | TRUE_KW | FALSE_KW => {
            p.start(LITERAL_EXPR);
            p.bump();
            p.finish_node();
        }
        // TwinCAT clock operators share their spelling with time types.
        TIME_KW | LTIME_KW if p.nth(1) == L_PAREN => {
            p.start(NAME_REF);
            p.bump();
            p.finish_node();
        }
        IDENT | TWINCAT_IDENT | THIS_KW | SUPER_KW | TYPE_CONV | ADR_KW | SIZEOF_KW
        | XSIZEOF_KW | INDEXOF_KW => {
            p.start(NAME_REF);
            p.bump();
            p.finish_node();
        }
        // An elementary type passed as an argument: `SIZEOF(BOOL)`, `__NEW(UINT, 1)`.
        k if super::is_type_keyword(k) && matches!(p.nth(1), R_PAREN | COMMA) => {
            p.start(NAME_REF);
            p.bump();
            p.finish_node();
        }
        L_PAREN => paren_or_init(p),
        L_BRACK => array_init(p),
        _ => p.err_and_bump("expected expression"),
    }
}

/// A bracketed array initializer `[a, b, c]` (possibly empty / multi-line).
/// Kept as a `PAREN_EXPR`; the delimiter tokens are preserved for the formatter.
fn array_init(p: &mut Parser) {
    p.start(PAREN_EXPR);
    p.bump(); // [
    if !p.at(R_BRACK) {
        init_item(p);
        while p.eat(COMMA) {
            if p.at(R_BRACK) {
                break;
            }
            init_item(p);
        }
    }
    p.expect(R_BRACK);
    p.finish_node();
}

/// `( ... )`, a parenthesized expression or an initializer list
/// `(name := expr, ...)`. Both kept as `PAREN_EXPR` (parens preserved).
fn paren_or_init(p: &mut Parser) {
    p.start(PAREN_EXPR);
    p.bump(); // (
    if !p.at(R_PAREN) {
        init_item(p);
        while p.eat(COMMA) {
            if p.at(R_PAREN) {
                break;
            }
            init_item(p);
        }
    }
    p.expect(R_PAREN);
    p.finish_node();
}

fn init_item(p: &mut Parser) {
    if at_name(p) && matches!(p.nth(1), ASSIGN | ARROW) {
        p.start(NAMED_ARG);
        p.bump(); // name
        p.bump(); // := or =>
                  // An output binding may be empty (`Q => ,` ignores the output).
        if !p.at(COMMA) && !p.at(R_PAREN) && !p.at(R_BRACK) {
            expr(p);
        }
        p.finish_node();
    } else {
        expr(p);
    }
}

/// `( arg {, arg} )` for a call. Each arg is positional or `name :=/=> expr`.
pub(crate) fn arg_list(p: &mut Parser) {
    p.start(ARG_LIST);
    p.bump(); // (
    if !p.at(R_PAREN) {
        arg(p);
        while p.eat(COMMA) {
            if p.at(R_PAREN) {
                break;
            }
            arg(p);
        }
    }
    p.expect(R_PAREN);
    p.finish_node();
}

fn arg(p: &mut Parser) {
    if at_name(p) && matches!(p.nth(1), ASSIGN | ARROW) {
        p.start(NAMED_ARG);
        p.bump();
        p.bump();
        // An output binding may be empty (`Q => ,` ignores the output).
        if !p.at(COMMA) && !p.at(R_PAREN) {
            expr(p);
        }
        p.finish_node();
    } else {
        p.start(ARG);
        expr(p);
        p.finish_node();
    }
}

/// A type reference: elementary keyword, named/qualified type, `STRING(n)`,
/// `ARRAY[..] OF`, `POINTER TO`, `REFERENCE TO`, inline enum, or subrange.
pub(crate) fn type_ref(p: &mut Parser) {
    match p.current() {
        STRING_KW | WSTRING_KW => {
            p.start(STRING_TYPE);
            p.bump();
            if p.eat(L_PAREN) {
                if !p.at(R_PAREN) {
                    expr(p);
                }
                p.expect(R_PAREN);
            } else if p.eat(L_BRACK) {
                if !p.at(R_BRACK) {
                    expr(p);
                }
                p.expect(R_BRACK);
            }
            p.finish_node();
        }
        ARRAY_KW => array_type(p),
        POINTER_KW => {
            p.start(POINTER_TYPE);
            p.bump();
            p.expect(TO_KW);
            type_ref(p);
            p.finish_node();
        }
        REFERENCE_KW => {
            p.start(REFERENCE_TYPE);
            p.bump();
            p.expect(TO_KW);
            type_ref(p);
            p.finish_node();
        }
        L_PAREN => {
            if p.nth(2) == DOTDOT {
                subrange_type(p);
            } else {
                enum_type(p);
            }
        }
        k if super::is_type_keyword(k) => {
            p.start(TYPE_REF);
            p.bump();
            subrange_suffix(p);
            p.finish_node();
        }
        _ if at_name(p) => {
            p.start(TYPE_REF);
            dotted_name(p);
            if p.at(LT) {
                generic_args(p);
            }
            subrange_suffix(p);
            p.finish_node();
        }
        _ => p.err_and_bump("expected a type"),
    }
}

fn generic_args(p: &mut Parser) {
    p.start(GENERIC_ARGS);
    p.bump(); // <
    if p.at(GT) {
        p.error(ErrorKind::Expected, "expected generic constant argument");
    } else {
        let parenthesized = p.at(L_PAREN);
        let argument_start = p.at_pos();
        expr_before(p, GT);
        if !parenthesized && p.at_pos() > argument_start + 1 {
            p.error(
                ErrorKind::Expected,
                "generic constant expressions must be parenthesized",
            );
        }
    }
    p.expect(GT);
    p.finish_node();
}

/// An optional `(lo..hi)` subrange following a base type, e.g. `UINT(0..65000)`.
fn subrange_suffix(p: &mut Parser) {
    if p.at(L_PAREN) && p.nth(2) == DOTDOT {
        p.bump(); // (
        expr(p);
        p.expect(DOTDOT);
        expr(p);
        p.expect(R_PAREN);
    }
}

fn array_type(p: &mut Parser) {
    p.start(ARRAY_TYPE);
    p.bump(); // ARRAY
    p.expect(L_BRACK);
    p.start(ARRAY_DIMS);
    array_dim(p);
    while p.eat(COMMA) {
        array_dim(p);
    }
    p.finish_node();
    p.expect(R_BRACK);
    p.expect(OF_KW);
    type_ref(p);
    p.finish_node();
}

fn array_dim(p: &mut Parser) {
    p.start(ARRAY_DIM);
    if p.at(STAR) {
        // `ARRAY[*]`, a variable-length array dimension (VAR_IN_OUT only).
        p.bump();
    } else {
        expr(p);
        p.expect(DOTDOT);
        expr(p);
    }
    p.finish_node();
}

pub(crate) fn subrange_type(p: &mut Parser) {
    p.start(SUBRANGE_TYPE);
    p.bump(); // (
    expr(p);
    p.expect(DOTDOT);
    expr(p);
    p.expect(R_PAREN);
    p.finish_node();
}

pub(crate) fn enum_type(p: &mut Parser) {
    p.start(ENUM_TYPE);
    p.bump(); // (
    while !p.at(R_PAREN) && !p.at_eof() {
        enum_variant(p);
        if !p.eat(COMMA) {
            break;
        }
    }
    p.expect(R_PAREN);
    // optional base type, e.g. `(...) UINT`
    if !p.at(SEMICOLON)
        && !p.at(END_TYPE_KW)
        && !p.at_eof()
        && (super::is_type_keyword(p.current()) || at_name(p))
    {
        type_ref(p);
    }
    p.finish_node();
}

fn enum_variant(p: &mut Parser) {
    p.start(ENUM_VARIANT);
    if at_name(p) {
        p.bump();
    } else {
        p.error(ErrorKind::Expected, "expected enum member name");
    }
    if p.at(ASSIGN) {
        p.start(INIT_EXPR);
        p.bump();
        expr(p);
        p.finish_node();
    }
    p.finish_node();
}
