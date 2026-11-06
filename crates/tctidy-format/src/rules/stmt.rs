//! Statement-list and control-structure lowering (IF/CASE/FOR/WHILE/REPEAT,
//! exception blocks, assignments, calls). Each statement is one logical line
//! (its own line breaks, e.g. multi-line calls, are preserved by the expression
//! rules); blocks indent their bodies one level. Runs of assignments align their
//! `:=` only under `align_impl_assignments`.

use tctidy_pretty::prelude::*;
use tctidy_syntax::ast::{AstNode, Expr};
use tctidy_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use crate::context::FormatContext;
use crate::rules::expr::format_expr;
use crate::style::BlockTokenPlacement;
use crate::trivia::{
    first_sig_token, gap_doc, last_sig_token, leading_doc, token_after_newline, trailing_doc,
};
use crate::verbatim::verbatim;

/// Lower an `IMPL_PART`: its single top-level `STMT_LIST`, flush left.
pub(crate) fn format_impl_part(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let Some(list) = node.children().find(|n| n.kind() == SyntaxKind::STMT_LIST) else {
        return verbatim(&node.text().to_string());
    };
    let items = stmt_items(&list, true, f);
    let mut parts = Vec::new();
    for (i, it) in items.into_iter().enumerate() {
        if i > 0 {
            parts.push(hard_line());
        }
        parts.push(it);
    }
    // Dangling comments after the last statement (end of body). The separating
    // line break is only needed when a statement precedes them, a body that is
    // *only* comments must not gain a spurious leading blank line.
    let dangling = f.comments.dangling();
    if !dangling.is_empty() {
        if !parts.is_empty() {
            parts.push(hard_line());
        }
        parts.push(crate::trivia::dangling_doc(
            dangling,
            f.style.blank_lines_max,
        ));
    }
    concat(parts)
}

/// One Doc per statement in a list, each carrying its blank-line separation,
/// leading comments, the statement, and any trailing comment. `skip_first_blank`
/// suppresses the blank above the first statement (used at the top level, where
/// there is no preceding line; a block body keeps it, after `THEN`/`:`).
fn stmt_items(list: &SyntaxNode, skip_first_blank: bool, f: &FormatContext) -> Vec<Doc> {
    let cap = f.style.blank_lines_max;
    let min = f.style.trailing_comment_min_spaces as usize;
    let stmts: Vec<SyntaxNode> = list.children().collect();
    let pads = assignment_pads(&stmts, f);
    let mut out = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        let lead = f.comments.leading_of(s);
        let blank = if lead.is_empty() {
            f.comments.blanks_before_of(s)
        } else {
            lead.first().map_or(0, |lp| lp.blank_before)
        };
        // The first body statement (right after `THEN`/`DO`/`:`/`REPEAT`) is a
        // block start: its leading blanks are capped by `blank_lines_at_block_start`.
        // At the top level (`skip_first_blank`) the first statement keeps no blank.
        // Between two statements, a block statement on either side wants at
        // least `blank_lines_around_block_statement` blank lines, and a leading
        // multiline comment `blank_lines_before_multiline_comment`.
        let n = if i == 0 {
            if skip_first_blank {
                0
            } else {
                f.style.cap_block_start(blank)
            }
        } else {
            let around_block = if is_block_stmt(s) || is_block_stmt(&stmts[i - 1]) {
                f.style.blank_lines_around_block_statement
            } else {
                0
            };
            let minimum = around_block.max(f.style.multiline_comment_minimum(lead));
            f.style.ensure_blank(blank, minimum)
        };
        let mut parts = Vec::new();
        for _ in 0..n {
            parts.push(hard_line());
        }
        parts.push(leading_doc(lead, cap));
        parts.push(if pads[i] > 0 {
            assign_stmt(s, pads[i], f)
        } else {
            format_stmt(s, f)
        });
        if let Some(tok) = last_sig_token(s) {
            parts.push(trailing_doc(f.comments.trailing(&tok), min));
        }
        out.push(concat(parts));
    }
    out
}

/// Whether `node` is a statement with a body: `IF` (even one written on a single
/// line), `CASE`, `FOR`, `WHILE`, `REPEAT` or `__TRY`.
fn is_block_stmt(node: &SyntaxNode) -> bool {
    use SyntaxKind::*;
    matches!(
        node.kind(),
        IF_STMT | CASE_STMT | FOR_STMT | WHILE_STMT | REPEAT_STMT | TRY_STMT
    )
}

/// The padding before the `:=` of each statement under
/// [`StyleConfig::align_impl_assignments`]: a run of plain assignments with no
/// blank line or comment line between them pads each target to the widest one.
/// Any other statement ends the run.
///
/// [`StyleConfig::align_impl_assignments`]: crate::StyleConfig::align_impl_assignments
fn assignment_pads(stmts: &[SyntaxNode], f: &FormatContext) -> Vec<usize> {
    let mut pads = vec![0; stmts.len()];
    if !f.style.align_impl_assignments {
        return pads;
    }
    let widths: Vec<Option<usize>> = stmts.iter().map(|s| aligned_target_width(s, f)).collect();
    let mut start = 0;
    while start < stmts.len() {
        if widths[start].is_none() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < stmts.len() && widths[end].is_some() && !starts_new_line_group(&stmts[end], f) {
            end += 1;
        }
        let widest = widths[start..end]
            .iter()
            .flatten()
            .max()
            .copied()
            .unwrap_or(0);
        for (pad, width) in pads[start..end].iter_mut().zip(&widths[start..end]) {
            *pad = widest - width.unwrap_or(widest);
        }
        start = end;
    }
    pads
}

/// Whether a comment line or a kept blank line separates `node` from the
/// statement above it.
fn starts_new_line_group(node: &SyntaxNode, f: &FormatContext) -> bool {
    !f.comments.leading_of(node).is_empty()
        || f.comments
            .blanks_before_of(node)
            .min(f.style.blank_lines_max)
            > 0
}

/// The printed width of the target of a plain `target := value;` assignment,
/// or `None` for any other statement (another operator, a chained assignment, a
/// comment before the `:=`, a target that would not stay on one line).
fn aligned_target_width(node: &SyntaxNode, f: &FormatContext) -> Option<usize> {
    if node.kind() != SyntaxKind::ASSIGN_STMT {
        return None;
    }
    let mut ops = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| {
            matches!(
                t.kind(),
                SyntaxKind::ASSIGN
                    | SyntaxKind::REF_ASSIGN
                    | SyntaxKind::SET_ASSIGN
                    | SyntaxKind::RESET_ASSIGN
            )
        });
    let op = ops.next()?;
    if op.kind() != SyntaxKind::ASSIGN || ops.next().is_some() {
        return None;
    }
    let commented = node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .skip_while(|t| t.kind().is_trivia())
        .take_while(|t| *t != op)
        .any(|t| {
            t.kind().is_trivia()
                && !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE)
        });
    if commented {
        return None;
    }
    let target = node.children().find_map(Expr::cast)?;
    let printed = tctidy_pretty::print(
        &format_expr(&target, f),
        &f.style.print_options(tctidy_pretty::LineEnding::Lf),
    );
    (!printed.contains('\n')).then(|| printed.chars().count())
}

/// The indented body of a block (a `STMT_LIST`), or `nil` if empty.
fn block_body(list: &SyntaxNode, f: &FormatContext) -> Doc {
    let items = stmt_items(list, false, f);
    if items.is_empty() {
        return nil();
    }
    let mut body = Vec::new();
    for it in items {
        body.push(hard_line());
        body.push(it);
    }
    indent(concat(body))
}

fn format_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    use SyntaxKind::*;
    match node.kind() {
        ASSIGN_STMT | REF_ASSIGN_STMT => assign_stmt(node, 0, f),
        CALL_STMT => call_stmt(node, f),
        IF_STMT => if_stmt(node, f),
        TRY_STMT => try_stmt(node, f),
        FOR_STMT => for_stmt(node, f),
        WHILE_STMT => while_stmt(node, f),
        REPEAT_STMT => repeat_stmt(node, f),
        CASE_STMT => case_stmt(node, f),
        RETURN_STMT | EXIT_STMT | CONTINUE_STMT => simple_kw_stmt(node, f),
        JMP_STMT => jmp_stmt(node, f),
        LABEL => label_stmt(node, f),
        EMPTY_STMT => text(";"),
        _ => verbatim(&node.text().to_string()),
    }
}

/// The trailing comment bound to the direct child token of `kind` (e.g. a
/// comment after `THEN`/`DO`/`OF` on a block-header line), or `nil`.
fn header_trailing(node: &SyntaxNode, kind: SyntaxKind, f: &FormatContext) -> Doc {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
        .map(|t| {
            trailing_doc(
                f.comments.trailing(&t),
                f.style.trailing_comment_min_spaces as usize,
            )
        })
        .unwrap_or_else(nil)
}

/// A block keyword (`THEN`/`DO`/`OF`) with its leading separator. With the
/// `SameLine` placement: a space normally, but a line break (keeping the keyword
/// flush with the block) when the source put it on its own line because the
/// condition wrapped. With `OwnLine`: always a line break, the keyword indented
/// one level under `indent_block_token`. A comment between the header and the
/// keyword keeps its source layout under either placement.
fn keyword_after_previous(node: &SyntaxNode, kind: SyntaxKind, kw: &str, f: &FormatContext) -> Doc {
    let tok = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind);
    let previous = tok.as_ref().and_then(prev_sig_token);
    let placement = match kind {
        SyntaxKind::THEN_KW => f.style.then_placement,
        SyntaxKind::DO_KW => f.style.do_placement,
        _ => f.style.of_placement,
    };
    if placement == BlockTokenPlacement::OwnLine {
        let commented = previous.as_ref().zip(tok.as_ref()).is_some_and(|(p, t)| {
            !f.comments.trailing(p).is_empty() || !f.comments.leading(t).is_empty()
        });
        if !commented {
            let own_line = concat([hard_line(), f.kw(kw)]);
            return if f.style.indent_block_token {
                indent(own_line)
            } else {
                own_line
            };
        }
    }
    let gap = previous
        .zip(tok.as_ref())
        .map(|(previous, next)| {
            gap_doc(
                &previous,
                next,
                f.comments,
                f.style.trailing_comment_min_spaces as usize,
                f.style.blank_lines_max,
                " ",
            )
            .0
        })
        .unwrap_or_else(|| text(" "));
    concat([gap, f.kw(kw)])
}

/// The nearest significant (non-trivia) token before `tok` in the whole stream.
fn prev_sig_token(tok: &tctidy_syntax::SyntaxToken) -> Option<tctidy_syntax::SyntaxToken> {
    let mut cur = tok.prev_token();
    while let Some(t) = cur {
        if !t.kind().is_trivia() {
            return Some(t);
        }
        cur = t.prev_token();
    }
    None
}

/// Blank lines (and any dangling comments) between a block's last statement and
/// its `END_*` keyword, attached as that token's leading trivia. Blank lines are
/// emitted at the block indent; comments at the body indent.
fn block_tail(node: &SyntaxNode, end_kind: SyntaxKind, f: &FormatContext) -> Doc {
    let cap = f.style.blank_lines_max;
    let Some(end_tok) = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == end_kind)
    else {
        return nil();
    };
    let lead = f.comments.leading(&end_tok);
    let blank = if lead.is_empty() {
        f.comments.blanks_before(&end_tok)
    } else {
        lead.first().map_or(0, |lp| lp.blank_before)
    };
    let mut parts = Vec::new();
    for _ in 0..f.style.cap_block_end(blank) {
        parts.push(hard_line());
    }
    for (i, lp) in lead.iter().enumerate() {
        if i > 0 {
            for _ in 0..lp.blank_before.min(cap) {
                parts.push(hard_line());
            }
        }
        parts.push(indent(concat([hard_line(), verbatim(&lp.piece.text)])));
    }
    concat(parts)
}

/// The optional `;` after a block's `END_*` keyword, if the source had one.
fn end_semi(node: &SyntaxNode, f: &FormatContext) -> Doc {
    semicolon_token(node).map_or_else(nil, |semi| semicolon_doc(&semi, f))
}

/// A statement's terminating `;`.
fn semicolon(node: &SyntaxNode, f: &FormatContext) -> Doc {
    semicolon_token(node).map_or_else(|| text(";"), |semi| semicolon_doc(&semi, f))
}

fn semicolon_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::SEMICOLON)
}

/// The `;` together with any comment or pragma between the statement's last
/// token and it (`x := a (* why *);`). No other rule prints those pieces: they
/// trail the statement's last token, or lead a `;` written on its own line.
fn semicolon_doc(semi: &SyntaxToken, f: &FormatContext) -> Doc {
    let Some(previous) = prev_sig_token(semi) else {
        return text(";");
    };
    let trailing = f.comments.trailing(&previous);
    if trailing.is_empty() && f.comments.leading(semi).is_empty() {
        return text(";");
    }
    if token_after_newline(semi) {
        let (gap, _) = gap_doc(
            &previous,
            semi,
            f.comments,
            f.style.trailing_comment_min_spaces as usize,
            f.style.blank_lines_max,
            "",
        );
        return concat([gap, text(";")]);
    }
    let mut parts = Vec::new();
    for piece in trailing {
        parts.push(text(" "));
        parts.push(verbatim(&piece.text));
    }
    parts.push(text(";"));
    concat(parts)
}

/// The first expression child of `node` (a condition, selector, …).
fn first_expr(node: &SyntaxNode, f: &FormatContext) -> Doc {
    node.children()
        .find_map(Expr::cast)
        .map(|e| format_expr(&e, f))
        .unwrap_or_else(nil)
}

/// `pad` spaces go between the target and the first operator (assignment
/// alignment).
fn assign_stmt(node: &SyntaxNode, pad: usize, f: &FormatContext) -> Doc {
    // `lhs := rhs [:= rhs] ;`, chained assignment stays flat. When the source
    // put a right-hand side on its own line (`lhs :=` then the value), the break
    // is preserved with the value indented one level.
    let mut parts = Vec::new();
    let mut pending: Option<SyntaxToken> = None;
    for elem in node.children_with_tokens() {
        match elem {
            SyntaxElement::Node(n) => {
                let Some(e) = Expr::cast(n) else { continue };
                match pending.take() {
                    None => {
                        parts.push(format_expr(&e, f));
                        if parts.len() == 1 && pad > 0 {
                            parts.push(text(" ".repeat(pad)));
                        }
                    }
                    Some(op) => {
                        parts.push(assign_op_doc(node, &op, f));
                        let (gap, breaks) = first_sig_token(e.syntax()).map_or_else(
                            || (text(" "), false),
                            |next| {
                                gap_doc(
                                    &op,
                                    &next,
                                    f.comments,
                                    f.style.trailing_comment_min_spaces as usize,
                                    f.style.blank_lines_max,
                                    " ",
                                )
                            },
                        );
                        let value = concat([gap, format_expr(&e, f)]);
                        parts.push(if breaks { indent(value) } else { value });
                    }
                }
            }
            SyntaxElement::Token(t) => match t.kind() {
                SyntaxKind::ASSIGN
                | SyntaxKind::REF_ASSIGN
                | SyntaxKind::SET_ASSIGN
                | SyntaxKind::RESET_ASSIGN => pending = Some(t),
                SyntaxKind::SEMICOLON => parts.push(semicolon(node, f)),
                _ => {}
            },
        }
    }
    concat(parts)
}

/// An assignment operator (`:=`, `REF=`, `S=`, `R=`) with the space before it.
/// A comment between the operator and the expression before it keeps its
/// place: inline (`b (* note *) := 3;`), or with the source line break and the
/// operator continuing on the next line, indented one level (`b // note` then
/// `:= 3;`).
fn assign_op_doc(node: &SyntaxNode, op: &SyntaxToken, f: &FormatContext) -> Doc {
    let previous = prev_sig_token(op)
        .filter(|previous| node.text_range().contains_range(previous.text_range()));
    if let Some(previous) = previous {
        if !f.comments.trailing(&previous).is_empty() || !f.comments.leading(op).is_empty() {
            let (gap, breaks) = gap_doc(
                &previous,
                op,
                f.comments,
                f.style.trailing_comment_min_spaces as usize,
                f.style.blank_lines_max,
                " ",
            );
            let doc = concat([gap, text(op.text().to_string())]);
            return if breaks { indent(doc) } else { doc };
        }
    }
    text(format!(" {}", op.text()))
}

fn call_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let call = node
        .children()
        .find_map(Expr::cast)
        .map(|e| format_expr(&e, f))
        .unwrap_or_else(nil);
    concat([call, semicolon(node, f)])
}

fn simple_kw_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    // The keyword, cased, plus the trailing `;`.
    let kw = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map(|t| f.kw(t.text()))
        .unwrap_or_else(nil);
    concat([kw, semicolon(node, f)])
}

fn jmp_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let mut parts = vec![f.kw("JMP")];
    if let Some(name) = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| matches!(t.kind(), SyntaxKind::IDENT | SyntaxKind::TWINCAT_IDENT))
    {
        parts.push(text(" "));
        parts.push(text(name.text().to_string()));
    }
    parts.push(semicolon(node, f));
    concat(parts)
}

fn label_stmt(node: &SyntaxNode, _f: &FormatContext) -> Doc {
    // `name:`, verbatim name plus the colon (no surrounding spaces).
    let name = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| matches!(t.kind(), SyntaxKind::IDENT | SyntaxKind::TWINCAT_IDENT))
        .map(|t| t.text().to_string())
        .unwrap_or_default();
    concat([text(name), text(":")])
}

/// Whether `node` is an `IF` that [`StyleConfig::place_simple_if_on_single_line`]
/// may put on one line: a single non-block statement as its only branch, and no
/// comment or pragma from `IF` to its end.
///
/// [`StyleConfig::place_simple_if_on_single_line`]: crate::StyleConfig::place_simple_if_on_single_line
fn is_simple_if(node: &SyntaxNode) -> bool {
    use SyntaxKind::*;
    let mut lists = 0;
    for child in node.children() {
        match child.kind() {
            STMT_LIST => {
                lists += 1;
                let mut stmts = child.children();
                let simple = stmts.next().is_some_and(|stmt| {
                    matches!(
                        stmt.kind(),
                        ASSIGN_STMT
                            | REF_ASSIGN_STMT
                            | CALL_STMT
                            | RETURN_STMT
                            | EXIT_STMT
                            | CONTINUE_STMT
                            | JMP_STMT
                    )
                });
                if !simple || stmts.next().is_some() {
                    return false;
                }
            }
            ELSIF_CLAUSE | ELSE_CLAUSE => return false,
            _ => {}
        }
    }
    let Some(last) = last_sig_token(node) else {
        return false;
    };
    let commented = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .skip_while(|token| token.kind().is_trivia())
        .take_while(|token| *token != last)
        .any(|token| {
            matches!(
                token.kind(),
                LINE_COMMENT | BLOCK_COMMENT | C_BLOCK_COMMENT | PRAGMA
            )
        });
    lists == 1 && !commented
}

/// A simple `IF` as one group: `IF cond THEN stmt END_IF;` on a single line when
/// it fits, else the usual block layout.
fn single_line_if(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let body = node
        .children()
        .find(|n| n.kind() == SyntaxKind::STMT_LIST)
        .map(|list| concat(stmt_items(&list, false, f)))
        .unwrap_or_else(nil);
    group(concat([
        f.kw("IF"),
        text(" "),
        first_expr(node, f),
        keyword_after_previous(node, SyntaxKind::THEN_KW, "THEN", f),
        indent(concat([line(), body])),
        block_tail(node, SyntaxKind::END_IF_KW, f),
        line(),
        f.kw("END_IF"),
        end_semi(node, f),
    ]))
}

fn if_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    if f.style.place_simple_if_on_single_line && is_simple_if(node) {
        return single_line_if(node, f);
    }
    let mut parts = vec![
        f.kw("IF"),
        text(" "),
        first_expr(node, f),
        keyword_after_previous(node, SyntaxKind::THEN_KW, "THEN", f),
    ];
    parts.push(header_trailing(node, SyntaxKind::THEN_KW, f));
    if let Some(then_list) = node.children().find(|n| n.kind() == SyntaxKind::STMT_LIST) {
        parts.push(block_body(&then_list, f));
    }
    let cap = f.style.blank_lines_max;
    for clause in node.children() {
        if matches!(
            clause.kind(),
            SyntaxKind::ELSIF_CLAUSE | SyntaxKind::ELSE_CLAUSE
        ) {
            parts.push(block_clause_leading(&clause, cap, f));
        }
        match clause.kind() {
            SyntaxKind::ELSIF_CLAUSE => {
                parts.push(f.kw("ELSIF"));
                parts.push(text(" "));
                parts.push(first_expr(&clause, f));
                parts.push(keyword_after_previous(
                    &clause,
                    SyntaxKind::THEN_KW,
                    "THEN",
                    f,
                ));
                parts.push(header_trailing(&clause, SyntaxKind::THEN_KW, f));
                if let Some(list) = clause
                    .children()
                    .find(|n| n.kind() == SyntaxKind::STMT_LIST)
                {
                    parts.push(block_body(&list, f));
                }
            }
            SyntaxKind::ELSE_CLAUSE => {
                parts.push(f.kw("ELSE"));
                parts.push(header_trailing(&clause, SyntaxKind::ELSE_KW, f));
                if let Some(list) = clause
                    .children()
                    .find(|n| n.kind() == SyntaxKind::STMT_LIST)
                {
                    parts.push(block_body(&list, f));
                }
            }
            _ => {}
        }
    }
    parts.push(block_tail(node, SyntaxKind::END_IF_KW, f));
    parts.push(hard_line());
    parts.push(f.kw("END_IF"));
    parts.push(end_semi(node, f));
    group_break(concat(parts))
}

fn try_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let mut parts = vec![f.kw("__TRY"), header_trailing(node, SyntaxKind::TRY_KW, f)];
    if let Some(body) = node
        .children()
        .find(|child| child.kind() == SyntaxKind::STMT_LIST)
    {
        parts.push(block_body(&body, f));
    }

    let cap = f.style.blank_lines_max;
    for clause in node.children().filter(|child| {
        matches!(
            child.kind(),
            SyntaxKind::CATCH_CLAUSE | SyntaxKind::FINALLY_CLAUSE
        )
    }) {
        parts.push(block_clause_leading(&clause, cap, f));
        match clause.kind() {
            SyntaxKind::CATCH_CLAUSE => {
                parts.push(catch_header(&clause, f));
                parts.push(header_trailing(&clause, SyntaxKind::R_PAREN, f));
            }
            SyntaxKind::FINALLY_CLAUSE => {
                parts.push(f.kw("__FINALLY"));
                parts.push(header_trailing(&clause, SyntaxKind::FINALLY_KW, f));
            }
            _ => unreachable!(),
        }
        if let Some(body) = clause
            .children()
            .find(|child| child.kind() == SyntaxKind::STMT_LIST)
        {
            parts.push(block_body(&body, f));
        }
    }

    parts.push(block_tail(node, SyntaxKind::ENDTRY_KW, f));
    parts.push(hard_line());
    parts.push(f.kw("__ENDTRY"));
    parts.push(end_semi(node, f));
    group_break(concat(parts))
}

fn block_clause_leading(node: &SyntaxNode, cap: u16, f: &FormatContext) -> Doc {
    let lead = f.comments.leading_of(node);
    let blank = if lead.is_empty() {
        f.comments.blanks_before_of(node)
    } else {
        lead.first().map_or(0, |piece| piece.blank_before)
    };
    let mut parts = vec![hard_line()];
    for _ in 0..blank.min(cap) {
        parts.push(hard_line());
    }
    parts.push(leading_doc(lead, cap));
    concat(parts)
}

fn catch_header(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let tokens: Vec<_> = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let mut parts = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            parts.push(
                gap_doc(
                    &tokens[index - 1],
                    token,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    "",
                )
                .0,
            );
        }
        parts.push(match token.kind() {
            SyntaxKind::CATCH_KW => f.kw("__CATCH"),
            _ => text(token.text().to_string()),
        });
    }
    concat(parts)
}

fn for_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let range = node
        .children()
        .find(|n| n.kind() == SyntaxKind::FOR_RANGE)
        .map(|r| for_range(&r, f))
        .unwrap_or_else(nil);
    let body = block_or_empty(node, f);
    group_break(concat([
        f.kw("FOR"),
        text(" "),
        range,
        keyword_after_previous(node, SyntaxKind::DO_KW, "DO", f),
        header_trailing(node, SyntaxKind::DO_KW, f),
        body,
        block_tail(node, SyntaxKind::END_FOR_KW, f),
        hard_line(),
        f.kw("END_FOR"),
        end_semi(node, f),
    ]))
}

fn for_range(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let mut parts = Vec::new();
    for elem in node.children_with_tokens() {
        match elem {
            SyntaxElement::Node(n) => {
                if let Some(e) = Expr::cast(n) {
                    parts.push(format_expr(&e, f));
                }
            }
            SyntaxElement::Token(t) => match t.kind() {
                SyntaxKind::ASSIGN => parts.push(text(" := ")),
                SyntaxKind::TO_KW => parts.push(concat([text(" "), f.kw("TO"), text(" ")])),
                SyntaxKind::BY_KW => parts.push(concat([text(" "), f.kw("BY"), text(" ")])),
                _ => {}
            },
        }
    }
    concat(parts)
}

fn while_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let body = block_or_empty(node, f);
    group_break(concat([
        f.kw("WHILE"),
        text(" "),
        first_expr(node, f),
        keyword_after_previous(node, SyntaxKind::DO_KW, "DO", f),
        header_trailing(node, SyntaxKind::DO_KW, f),
        body,
        block_tail(node, SyntaxKind::END_WHILE_KW, f),
        hard_line(),
        f.kw("END_WHILE"),
        end_semi(node, f),
    ]))
}

fn repeat_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let body = block_or_empty(node, f);
    group_break(concat([
        f.kw("REPEAT"),
        body,
        hard_line(),
        f.kw("UNTIL"),
        text(" "),
        first_expr(node, f),
        block_tail(node, SyntaxKind::END_REPEAT_KW, f),
        hard_line(),
        f.kw("END_REPEAT"),
        end_semi(node, f),
    ]))
}

fn case_stmt(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let selector = first_expr(node, f);
    let mut head = vec![
        f.kw("CASE"),
        text(" "),
        selector,
        keyword_after_previous(node, SyntaxKind::OF_KW, "OF", f),
    ];
    head.push(header_trailing(node, SyntaxKind::OF_KW, f));

    // Arms (and a trailing `ELSE`) are indented one level under `CASE … OF`; each
    // arm body indents one level further. Blank lines and leading comments between
    // arms are preserved (capped), like statements in a list.
    let cap = f.style.blank_lines_max;
    let mut arms = Vec::new();
    let mut previous_arm: Option<SyntaxNode> = None;
    for child in node.children() {
        if !matches!(child.kind(), SyntaxKind::CASE_ARM | SyntaxKind::ELSE_CLAUSE) {
            continue;
        }
        arms.push(hard_line());
        let lead = f.comments.leading_of(&child);
        let blank = if lead.is_empty() {
            f.comments.blanks_before_of(&child)
        } else {
            lead.first().map_or(0, |lp| lp.blank_before)
        };
        // The first arm (right after `CASE … OF`) is a block start; later arms
        // keep their separation, raised to the minimum after the previous arm
        // and capped by `blank_lines_max`.
        let n = match &previous_arm {
            None => f.style.cap_block_start(blank),
            Some(previous) => {
                let after_arm = if is_multiline_arm(previous) {
                    f.style.blank_lines_after_multiline_case
                } else {
                    f.style.blank_lines_after_case
                };
                let minimum = after_arm.max(f.style.multiline_comment_minimum(lead));
                f.style.ensure_blank(blank, minimum)
            }
        };
        for _ in 0..n {
            arms.push(hard_line());
        }
        previous_arm = Some(child.clone());
        arms.push(leading_doc(lead, cap));
        if child.kind() == SyntaxKind::CASE_ARM {
            arms.push(case_arm(&child, f));
        } else {
            arms.push(f.kw("ELSE"));
            arms.push(header_trailing(&child, SyntaxKind::ELSE_KW, f));
            if let Some(list) = child.children().find(|n| n.kind() == SyntaxKind::STMT_LIST) {
                arms.push(block_body(&list, f));
            }
        }
    }
    group_break(concat([
        concat(head),
        indent(concat(arms)),
        block_tail(node, SyntaxKind::END_CASE_KW, f),
        hard_line(),
        f.kw("END_CASE"),
        end_semi(node, f),
    ]))
}

/// Whether a `CASE` arm's body holds more than one statement or a block
/// statement (it then takes `blank_lines_after_multiline_case`).
fn is_multiline_arm(arm: &SyntaxNode) -> bool {
    arm.children()
        .find(|n| n.kind() == SyntaxKind::STMT_LIST)
        .is_some_and(|list| {
            let mut stmts = list.children();
            match (stmts.next(), stmts.next()) {
                (Some(only), None) => is_block_stmt(&only),
                (Some(_), Some(_)) => true,
                (None, _) => false,
            }
        })
}

fn case_arm(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let labels = node
        .children()
        .find(|n| n.kind() == SyntaxKind::CASE_LABEL_LIST)
        .map(|l| case_label_list(&l, f))
        .unwrap_or_else(nil);
    let body = node
        .children()
        .find(|n| n.kind() == SyntaxKind::STMT_LIST)
        .map(|l| block_body(&l, f))
        .unwrap_or_else(nil);
    concat([
        labels,
        text(":"),
        header_trailing(node, SyntaxKind::COLON, f),
        body,
    ])
}

fn case_label_list(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let labels = node
        .children()
        .filter(|n| n.kind() == SyntaxKind::CASE_LABEL);
    join(text(", "), labels.map(|l| case_label(&l, f)))
}

fn case_label(node: &SyntaxNode, f: &FormatContext) -> Doc {
    // A constant or a `lo..hi` range.
    let mut parts = Vec::new();
    for elem in node.children_with_tokens() {
        match elem {
            SyntaxElement::Node(n) => {
                if let Some(e) = Expr::cast(n) {
                    parts.push(format_expr(&e, f));
                }
            }
            SyntaxElement::Token(t) if t.kind() == SyntaxKind::DOTDOT => parts.push(text("..")),
            _ => {}
        }
    }
    concat(parts)
}

/// The body of a block statement that owns a single `STMT_LIST`.
fn block_or_empty(node: &SyntaxNode, f: &FormatContext) -> Doc {
    node.children()
        .find(|n| n.kind() == SyntaxKind::STMT_LIST)
        .map(|l| block_body(&l, f))
        .unwrap_or_else(nil)
}
