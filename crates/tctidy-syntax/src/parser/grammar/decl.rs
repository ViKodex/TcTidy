//! Declaration-part grammar: POU headers, `TYPE` definitions (struct / union /
//! enum / alias / subrange / array / pointer / reference), and `VAR` blocks with
//! qualifiers and `AT` located variables.

use super::super::parser_impl::Parser;
use super::expr::{arg_list, enum_type, expr, subrange_type, type_ref};
use super::{at_name, dotted_name};
use crate::error::ErrorKind;
use crate::kind::SyntaxKind::{self, *};

const VAR_SECTIONS: &[SyntaxKind] = &[
    VAR_KW,
    VAR_INPUT_KW,
    VAR_OUTPUT_KW,
    VAR_IN_OUT_KW,
    VAR_GLOBAL_KW,
    VAR_TEMP_KW,
    VAR_STAT_KW,
    VAR_EXTERNAL_KW,
    VAR_CONFIG_KW,
    VAR_PUBLIC_KW,
    VAR_INST_KW,
    VAR_GENERIC_KW,
];

const QUALIFIERS: &[SyntaxKind] = &[CONSTANT_KW, RETAIN_KW, PERSISTENT_KW];

const ACCESS_MODIFIERS: &[SyntaxKind] = &[PUBLIC_KW, PRIVATE_KW, PROTECTED_KW, INTERNAL_KW];

pub(crate) fn decl_part(p: &mut Parser) {
    // A property accessor (`Get`/`Set`) has no header line: its declaration
    // opens with its own access modifier, alone on the first line.
    if p.at_any(ACCESS_MODIFIERS) {
        p.start(ACCESS_MODIFIER);
        p.bump();
        p.finish_node();
    }
    while !p.at_eof() {
        let before = p.at_pos();
        match p.current() {
            TYPE_KW => type_decl(p),
            PROGRAM_KW => pou_header(p, PROGRAM_DECL),
            FUNCTION_BLOCK_KW => pou_header(p, FUNCTION_BLOCK_DECL),
            FUNCTION_KW => pou_header(p, FUNCTION_DECL),
            METHOD_KW => pou_header(p, METHOD_DECL),
            PROPERTY_KW => pou_header(p, PROPERTY_DECL),
            INTERFACE_KW => pou_header(p, INTERFACE_DECL),
            ACTION_KW => pou_header(p, ACTION_DECL),
            k if VAR_SECTIONS.contains(&k) => var_block(p),
            _ => p.err_and_bump("unexpected token in declaration"),
        }
        if p.at_pos() == before && !p.at_eof() {
            p.err_and_bump("no progress");
        }
    }
}

/// A POU header line (no body / no `END_*`: TwinCAT stores those separately).
fn pou_header(p: &mut Parser, kind: SyntaxKind) {
    p.start(kind);
    p.bump(); // PROGRAM / FUNCTION_BLOCK / ...
    while p.at_any(&[
        ABSTRACT_KW,
        FINAL_KW,
        PUBLIC_KW,
        PRIVATE_KW,
        PROTECTED_KW,
        INTERNAL_KW,
    ]) {
        p.bump(); // access / abstract / final
    }
    if at_name(p) {
        p.bump(); // POU name
    }
    if kind == FUNCTION_BLOCK_DECL && p.at(VAR_GENERIC_KW) {
        var_block(p);
    }
    if p.at(EXTENDS_KW) {
        extends_clause(p);
    }
    if p.at(IMPLEMENTS_KW) {
        p.start(IMPLEMENTS_CLAUSE);
        p.bump();
        dotted_name(p);
        while p.eat(COMMA) {
            dotted_name(p);
        }
        p.finish_node();
    }
    if p.at(COLON) {
        p.start(RETURN_TYPE);
        p.bump();
        type_ref(p);
        p.finish_node();
    }
    p.eat(SEMICOLON); // optional `;` terminating the header (e.g. `FUNCTION F : REAL;`)
    p.finish_node();
}

/// `EXTENDS` and its base list; an interface may extend several (`I_A, I_B`).
fn extends_clause(p: &mut Parser) {
    p.start(EXTENDS_CLAUSE);
    p.bump();
    type_ref(p);
    while p.eat(COMMA) {
        type_ref(p);
    }
    p.finish_node();
}

const TYPE_MODIFIERS: &[SyntaxKind] = &[
    ABSTRACT_KW,
    FINAL_KW,
    PUBLIC_KW,
    PRIVATE_KW,
    PROTECTED_KW,
    INTERNAL_KW,
];

fn type_decl(p: &mut Parser) {
    p.start(TYPE_DECL);
    p.bump(); // TYPE
    while p.at_any(TYPE_MODIFIERS) {
        p.bump(); // `TYPE INTERNAL E_Mode`, `TYPE ABSTRACT ST_Base`
    }
    if at_name(p) {
        p.bump(); // type name
    } else {
        p.error(ErrorKind::Expected, "expected type name");
    }
    if p.at(EXTENDS_KW) {
        p.start(EXTENDS_CLAUSE);
        p.bump();
        dotted_name(p);
        p.finish_node();
    }
    p.expect(COLON);
    type_def(p);
    if p.at(ASSIGN) {
        // type-level default, e.g. enum `(...) USINT := None`
        p.start(INIT_EXPR);
        p.bump();
        expr(p);
        p.finish_node();
    }
    p.eat(SEMICOLON); // optional
    p.expect(END_TYPE_KW);
    p.eat(SEMICOLON); // `END_TYPE;`, accepted by TwinCAT
    p.finish_node();
}

fn type_def(p: &mut Parser) {
    match p.current() {
        STRUCT_KW => struct_like(p, STRUCT_TYPE, END_STRUCT_KW),
        UNION_KW => struct_like(p, UNION_TYPE, END_UNION_KW),
        L_PAREN => {
            if p.nth(2) == DOTDOT {
                subrange_type(p);
            } else {
                enum_type(p);
            }
        }
        _ => type_ref(p),
    }
}

fn struct_like(p: &mut Parser, node: SyntaxKind, end: SyntaxKind) {
    p.start(node);
    p.bump(); // STRUCT / UNION
    while !p.at(end) && !p.at_eof() {
        let before = p.at_pos();
        member(p);
        if p.at_pos() == before && !p.at(end) && !p.at_eof() {
            p.err_and_bump("expected struct member");
        }
    }
    p.expect(end);
    p.finish_node();
}

fn var_block(p: &mut Parser) {
    p.start(VAR_BLOCK);
    let is_generic = p.at(VAR_GENERIC_KW);
    p.bump(); // VAR section keyword
    if is_generic {
        p.expect(CONSTANT_KW);
    } else {
        while p.at_any(QUALIFIERS) {
            p.bump(); // CONSTANT / RETAIN / PERSISTENT (stay on the header line)
        }
    }
    while !p.at(END_VAR_KW) && !p.at_eof() {
        let before = p.at_pos();
        member(p);
        if p.at_pos() == before && !p.at(END_VAR_KW) && !p.at_eof() {
            p.err_and_bump("expected variable declaration");
        }
    }
    p.expect(END_VAR_KW);
    p.eat(SEMICOLON); // accepted by TwinCAT although omitted from the documented syntax
    p.finish_node();
}

/// A declaration, or a stray `;` that TwinCAT accepts as an empty declaration.
fn member(p: &mut Parser) {
    if p.at(SEMICOLON) {
        p.start(EMPTY_DECL);
        p.bump();
        p.finish_node();
    } else {
        var_decl(p);
    }
}

/// One declaration line: `names [AT %addr] : type [:= init] ;`.
fn var_decl(p: &mut Parser) {
    p.start(VAR_DECL);

    p.start(VAR_NAME_LIST);
    if at_name(p) {
        p.bump();
        while p.eat(COMMA) {
            if at_name(p) {
                p.bump();
            } else {
                break;
            }
        }
    } else {
        p.error(ErrorKind::Expected, "expected variable name");
    }
    p.finish_node();

    if p.at(AT_KW) {
        p.start(AT_CLAUSE);
        p.bump();
        if p.at(DIRECT_ADDRESS) {
            p.bump();
        } else {
            p.error(ErrorKind::Expected, "expected address after AT");
        }
        p.finish_node();
    }

    p.expect(COLON);
    type_ref(p);

    // Function-block instance with default arguments: `x : FB_T(a := 1, b := 2)`.
    if p.at(L_PAREN) {
        arg_list(p);
    }

    if p.at(ASSIGN) {
        p.start(INIT_EXPR);
        p.bump();
        // A single value, or an array-of-struct initializer: `(..), (..), (..)`.
        expr(p);
        while p.eat(COMMA) {
            expr(p);
        }
        p.finish_node();
    }

    p.expect(SEMICOLON);
    p.finish_node();
}
