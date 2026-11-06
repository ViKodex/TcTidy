//! Implementation-body grammar: the statement language.
//!
//! Recursive descent over the significant tokens, reusing the expression parser
//! ([`super::expr`]) for conditions, call statements and assignment sides. A
//! statement list runs until a caller-supplied terminator predicate (the `END_*`
//! of the enclosing block, the next `CASE` label, or EOF at the top level). Every
//! loop advances by at least one token (via `err_and_bump` on no progress), so
//! parsing always terminates.

use super::super::parser_impl::Parser;
use super::at_name;
use super::expr::expr;
use crate::error::ErrorKind;
use crate::kind::SyntaxKind::{self, *};

pub(crate) fn impl_part(p: &mut Parser) {
    impl_part_body(p);
}

/// A whole implementation body / raw-ST file: one top-level statement list.
pub(crate) fn impl_part_body(p: &mut Parser) {
    stmt_list(p, |_| false);
}

/// `STMT_LIST` of statements until `is_end(p)` holds (or EOF).
fn stmt_list(p: &mut Parser, is_end: impl Fn(&Parser) -> bool) {
    p.start(STMT_LIST);
    while !p.at_eof() && !is_end(p) {
        let before = p.at_pos();
        statement(p);
        if p.at_pos() == before && !p.at_eof() && !is_end(p) {
            p.err_and_bump("expected a statement");
        }
    }
    p.finish_node();
}

fn statement(p: &mut Parser) {
    match p.current() {
        IF_KW => if_stmt(p),
        TRY_KW => try_stmt(p),
        CASE_KW => case_stmt(p),
        FOR_KW => for_stmt(p),
        WHILE_KW => while_stmt(p),
        REPEAT_KW => repeat_stmt(p),
        RETURN_KW => kw_stmt(p, RETURN_STMT),
        EXIT_KW => kw_stmt(p, EXIT_STMT),
        CONTINUE_KW => kw_stmt(p, CONTINUE_STMT),
        JMP_KW => jmp_stmt(p),
        SEMICOLON => {
            p.start(EMPTY_STMT);
            p.bump();
            p.finish_node();
        }
        // `label:`, a bare name immediately followed by a colon.
        _ if at_name(p) && p.nth(1) == COLON => {
            p.start(LABEL);
            p.bump(); // name
            p.bump(); // :
            p.finish_node();
        }
        _ => expr_stmt(p),
    }
}

/// `RETURN` / `EXIT` / `CONTINUE` (`;` optional).
fn kw_stmt(p: &mut Parser, kind: SyntaxKind) {
    p.start(kind);
    p.bump();
    p.eat(SEMICOLON);
    p.finish_node();
}

/// `JMP label ;`
fn jmp_stmt(p: &mut Parser) {
    p.start(JMP_STMT);
    p.bump(); // JMP
    if at_name(p) {
        p.bump();
    }
    p.eat(SEMICOLON);
    p.finish_node();
}

/// An assignment or a call/expression statement. Chained assignment
/// (`a := b := c;`) is kept flat inside one node.
fn expr_stmt(p: &mut Parser) {
    let cp = p.checkpoint();
    expr(p);
    let kind = match p.current() {
        ASSIGN | SET_ASSIGN | RESET_ASSIGN => ASSIGN_STMT,
        REF_ASSIGN => REF_ASSIGN_STMT,
        _ => {
            p.start_at(cp, CALL_STMT);
            p.eat(SEMICOLON);
            p.finish_node();
            return;
        }
    };
    p.start_at(cp, kind);
    while p.at(ASSIGN) || p.at(REF_ASSIGN) || p.at(SET_ASSIGN) || p.at(RESET_ASSIGN) {
        p.bump(); // := / REF= / S= / R=
        expr(p);
    }
    p.eat(SEMICOLON);
    p.finish_node();
}

/// A condition expression. Tolerates an assignment used in condition position
/// (non-standard, but present in real code: `IF x := 1 THEN`), folding the `:=`
/// into a binary expression so the file round-trips and formats instead of
/// erroring. Plain `expr` is unchanged, so statement-level assignments are
/// unaffected.
fn cond(p: &mut Parser) {
    let cp = p.checkpoint();
    expr(p);
    while p.at(ASSIGN) {
        p.start_at(cp, BIN_EXPR);
        p.bump(); // :=
        expr(p);
        p.finish_node();
    }
}

fn if_stmt(p: &mut Parser) {
    p.start(IF_STMT);
    p.bump(); // IF
    cond(p);
    p.expect(THEN_KW);
    stmt_list(p, at_if_end);
    while p.at(ELSIF_KW) {
        p.start(ELSIF_CLAUSE);
        p.bump();
        cond(p);
        p.expect(THEN_KW);
        stmt_list(p, at_if_end);
        p.finish_node();
    }
    if p.at(ELSE_KW) {
        p.start(ELSE_CLAUSE);
        p.bump();
        stmt_list(p, |p| p.at(END_IF_KW));
        p.finish_node();
    }
    p.expect(END_IF_KW);
    p.eat(SEMICOLON); // optional trailing `;`
    p.finish_node();
}

fn at_if_end(p: &Parser) -> bool {
    matches!(p.current(), ELSIF_KW | ELSE_KW | END_IF_KW)
}

fn try_stmt(p: &mut Parser) {
    p.start(TRY_STMT);
    p.bump(); // __TRY
    stmt_list(p, at_try_boundary);

    let mut has_handler = false;
    if p.at(CATCH_KW) {
        has_handler = true;
        p.start(CATCH_CLAUSE);
        p.bump(); // __CATCH
        p.expect(L_PAREN);
        if at_name(p) {
            p.bump();
        } else {
            p.error(ErrorKind::Expected, "expected an exception variable");
        }
        p.expect(R_PAREN);
        stmt_list(p, |p| matches!(p.current(), FINALLY_KW | ENDTRY_KW));
        p.finish_node();
    }

    if p.at(FINALLY_KW) {
        has_handler = true;
        p.start(FINALLY_CLAUSE);
        p.bump(); // __FINALLY
        stmt_list(p, |p| p.at(ENDTRY_KW));
        p.finish_node();
    }

    if !has_handler {
        p.error(
            ErrorKind::Expected,
            "expected __CATCH or __FINALLY in exception block",
        );
    }
    p.expect(ENDTRY_KW);
    p.eat(SEMICOLON);
    p.finish_node();
}

fn at_try_boundary(p: &Parser) -> bool {
    matches!(p.current(), CATCH_KW | FINALLY_KW | ENDTRY_KW)
}

fn for_stmt(p: &mut Parser) {
    p.start(FOR_STMT);
    p.bump(); // FOR
    p.start(FOR_RANGE);
    expr(p); // loop variable
    p.expect(ASSIGN);
    expr(p); // start
    p.expect(TO_KW);
    expr(p); // end
    if p.eat(BY_KW) {
        expr(p); // step
    }
    p.finish_node(); // FOR_RANGE
    p.expect(DO_KW);
    stmt_list(p, |p| p.at(END_FOR_KW));
    p.expect(END_FOR_KW);
    p.eat(SEMICOLON);
    p.finish_node();
}

fn while_stmt(p: &mut Parser) {
    p.start(WHILE_STMT);
    p.bump(); // WHILE
    cond(p);
    p.expect(DO_KW);
    stmt_list(p, |p| p.at(END_WHILE_KW));
    p.expect(END_WHILE_KW);
    p.eat(SEMICOLON);
    p.finish_node();
}

fn repeat_stmt(p: &mut Parser) {
    p.start(REPEAT_STMT);
    p.bump(); // REPEAT
    stmt_list(p, |p| p.at(UNTIL_KW));
    p.expect(UNTIL_KW);
    expr(p);
    p.expect(END_REPEAT_KW);
    p.eat(SEMICOLON);
    p.finish_node();
}

fn case_stmt(p: &mut Parser) {
    p.start(CASE_STMT);
    p.bump(); // CASE
    expr(p); // selector
    p.expect(OF_KW);
    while !p.at(ELSE_KW) && !p.at(END_CASE_KW) && !p.at_eof() {
        let before = p.at_pos();
        case_arm(p);
        if p.at_pos() == before {
            p.err_and_bump("expected a CASE label");
        }
    }
    if p.at(ELSE_KW) {
        p.start(ELSE_CLAUSE);
        p.bump();
        stmt_list(p, |p| p.at(END_CASE_KW));
        p.finish_node();
    }
    p.expect(END_CASE_KW);
    p.eat(SEMICOLON);
    p.finish_node();
}

/// One `CASE` arm: `label {, label} : statements`. The body runs until the next
/// label, `ELSE`, or `END_CASE`.
fn case_arm(p: &mut Parser) {
    p.start(CASE_ARM);
    p.start(CASE_LABEL_LIST);
    case_label(p);
    while p.eat(COMMA) {
        case_label(p);
    }
    p.finish_node(); // CASE_LABEL_LIST
    p.expect(COLON);
    stmt_list(p, |p| {
        p.at(ELSE_KW) || p.at(END_CASE_KW) || at_case_label(p)
    });
    p.finish_node();
}

/// A single `CASE` label: a constant, or a `lo..hi` range.
fn case_label(p: &mut Parser) {
    p.start(CASE_LABEL);
    expr(p);
    if p.eat(DOTDOT) {
        expr(p);
    }
    p.finish_node();
}

/// Heuristic: do the upcoming tokens form a `CASE` label list (a constant list
/// terminated by `:`)? Used to end an arm body, since labels are not keywords.
/// Scans label-shaped atoms; a `:` first means a label, an assignment/`(`/`[`/`^`
/// first means a statement.
fn at_case_label(p: &Parser) -> bool {
    if !matches!(
        p.current(),
        INT_NUMBER | TYPED_LITERAL | IDENT | TWINCAT_IDENT | MINUS | PLUS | TRUE_KW | FALSE_KW
    ) {
        return false;
    }
    // Scan the label-shaped run to its terminator. No fixed bound: a `:` means it
    // is a label list, and the first non-label token (or EOF) means it is not,
    // both terminate the loop. A fixed cap here truncated long label lists (real
    // code has CASE arms with dozens of error-code labels) into a parse error.
    let mut k = 0;
    loop {
        match p.nth(k) {
            COLON => return true,
            INT_NUMBER | TYPED_LITERAL | IDENT | TWINCAT_IDENT | DOT | DOTDOT | COMMA | MINUS
            | PLUS | TRUE_KW | FALSE_KW => k += 1,
            _ => return false,
        }
    }
}
