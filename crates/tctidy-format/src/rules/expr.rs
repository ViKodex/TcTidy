//! Expression lowering: width-sensitive binary chains, structured parenthesized
//! expressions, operator spacing from style, and lossless comment handling.
//! Parentheses are reproduced exactly from `PAREN_EXPR` nodes, never added or
//! removed. Expressions carrying comments use a source-preserving fallback so
//! layout normalization cannot reorder comment attachments.

use tctidy_pretty::prelude::*;
use tctidy_syntax::ast::{AstNode, BinExpr, Expr, NamedArg};
use tctidy_syntax::{SyntaxKind, SyntaxToken};

use crate::context::FormatContext;
use crate::style::{
    CallArgumentLayout, CollectionBreak, LogicalExpressionLayout, ParenthesizedExpressionLayout,
};
use crate::trivia::{
    first_sig_token, gap_doc, last_sig_token, leading_doc, token_after_newline, trailing_doc,
};
use crate::verbatim::verbatim;

/// Lower any expression to a `Doc`, starting a fresh continuation context (a
/// multi-line operator chain inside indents one level from here).
pub(crate) fn format_expr(expr: &Expr, f: &FormatContext) -> Doc {
    render_expr(expr, f, false)
}

#[derive(Clone, Copy, Default)]
struct ExprLayout {
    force_break: bool,
    break_parenthesized_with: Option<GroupId>,
}

/// Render `expr`. `indented` is true when an enclosing operator chain has already
/// added the one-level continuation indent, so nested chains never compound it.
fn render_expr(expr: &Expr, f: &FormatContext, indented: bool) -> Doc {
    render_expr_with_layout(expr, f, indented, ExprLayout::default())
}

fn render_expr_with_layout(
    expr: &Expr,
    f: &FormatContext,
    indented: bool,
    layout: ExprLayout,
) -> Doc {
    match expr {
        Expr::Literal(_) | Expr::Name(_) => atom(expr, f),
        Expr::Paren(p) => format_paren(p.syntax(), f, indented, layout),
        Expr::Bin(b) => {
            if !requires_source_layout(b.syntax(), f) {
                return format_bin_chain(b, f, layout.force_break);
            }
            format_bin_source_layout(b, f, indented)
        }
        Expr::Unary(u) => {
            let operand = u.operand();
            let operand_doc = operand
                .as_ref()
                .map(|e| render_expr(e, f, indented))
                .unwrap_or_else(nil);
            match (u.op_token(), operand.as_ref()) {
                (Some(op), Some(value)) => {
                    let default = if op.kind() == SyntaxKind::NOT_KW {
                        " "
                    } else {
                        ""
                    };
                    let op_doc = op_text(&op, f);
                    let gap = first_sig_token(value.syntax())
                        .map(|next| {
                            gap_doc(
                                &op,
                                &next,
                                f.comments,
                                f.style.trailing_comment_min_spaces as usize,
                                f.style.blank_lines_max,
                                default,
                            )
                            .0
                        })
                        .unwrap_or_else(|| text(default));
                    concat([op_doc, gap, operand_doc])
                }
                (Some(op), None) => op_text(&op, f),
                (None, _) => operand_doc,
            }
        }
        Expr::Qualified(q) => {
            let base_expr = q.base();
            let base = base_expr
                .as_ref()
                .map(|e| render_expr(e, f, indented))
                .unwrap_or_else(nil);
            let seg = q
                .segment()
                .map(|n| atom(&Expr::Name(n), f))
                .unwrap_or_else(nil);
            // A comment before the `.` of a method chain (`a.B() // note` then
            // `.C()` on the next line) keeps its place and the source line break;
            // the continuation is indented one level. Chains without comments
            // keep their usual layout.
            let dot = q
                .syntax()
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == SyntaxKind::DOT);
            let previous = base_expr.as_ref().and_then(|e| last_sig_token(e.syntax()));
            if let (Some(previous), Some(dot)) = (previous, dot) {
                if !f.comments.trailing(&previous).is_empty()
                    || !f.comments.leading(&dot).is_empty()
                {
                    let (gap, breaks) = gap_doc(
                        &previous,
                        &dot,
                        f.comments,
                        f.style.trailing_comment_min_spaces as usize,
                        f.style.blank_lines_max,
                        " ",
                    );
                    let rest = concat([gap, text("."), seg]);
                    return concat([base, if breaks { indent(rest) } else { rest }]);
                }
            }
            concat([base, text("."), seg])
        }
        Expr::Index(i) => {
            let base = i
                .base()
                .map(|e| render_expr(&e, f, indented))
                .unwrap_or_else(nil);
            let indices: Vec<_> = i.indices().collect();
            let open = i
                .syntax()
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == SyntaxKind::L_BRACK);
            let mut parts = vec![base, text("[")];
            if let (Some(open), Some(first)) = (&open, indices.first()) {
                parts.push(gap_to_node(open, first.syntax(), f, "").0);
            }
            for (index, value) in indices.iter().enumerate() {
                if index > 0 {
                    parts.push(text(","));
                    let gap = next_comma(indices[index - 1].syntax())
                        .map(|comma| gap_to_node(&comma, value.syntax(), f, " ").0)
                        .unwrap_or_else(|| text(" "));
                    parts.push(gap);
                }
                parts.push(format_expr(value, f));
            }
            parts.push(text("]"));
            concat(parts)
        }
        Expr::Deref(d) => {
            let base = d
                .base()
                .map(|e| render_expr(&e, f, indented))
                .unwrap_or_else(nil);
            concat([base, text("^")])
        }
        Expr::Call(c) => {
            let callee = c
                .callee()
                .map(|e| render_expr(&e, f, indented))
                .unwrap_or_else(nil);
            match c.arg_list() {
                Some(al) => format_call_args(callee, al.syntax(), f),
                None => concat([callee, text("("), text(")")]),
            }
        }
    }
}

/// Render a binary expression with width-sensitive breaks. Operators at the
/// same precedence form one group; higher-precedence children keep independent
/// groups so an `OR` chain can break without needlessly exploding every `AND`.
fn format_bin_chain(b: &BinExpr, f: &FormatContext, force_break: bool) -> Doc {
    let Some(level) = b.op_token().and_then(|token| binary_level(token.kind())) else {
        return format_bin_source_layout(b, f, false);
    };
    let group_id = f.fresh_group_id();
    let mut parts = Vec::new();
    flatten_bin_level(b, level, group_id, f, &mut parts);
    let content = align_current(concat(parts));
    let max_inline = usize::from(f.style.logical_chain_max_inline);
    let too_many_operands =
        level <= 3 && max_inline > 0 && bin_level_operand_count(b, level) > max_inline;
    if force_break || too_many_operands {
        group_with_id(group_id, concat([break_parent(), content]))
    } else {
        group_with_id(group_id, content)
    }
}

/// Operands of the chain at `level` that `b` heads: `a OR b AND c OR d` has
/// three at the OR level (`b AND c` is one operand).
fn bin_level_operand_count(b: &BinExpr, level: u8) -> usize {
    let lhs = match b.lhs() {
        Some(Expr::Bin(inner))
            if inner
                .op_token()
                .and_then(|token| binary_level(token.kind()))
                == Some(level) =>
        {
            bin_level_operand_count(&inner, level)
        }
        _ => 1,
    };
    lhs + 1
}

fn flatten_bin_level(
    b: &BinExpr,
    level: u8,
    group_id: GroupId,
    f: &FormatContext,
    parts: &mut Vec<Doc>,
) {
    match b.lhs() {
        Some(Expr::Bin(inner))
            if inner
                .op_token()
                .and_then(|token| binary_level(token.kind()))
                == Some(level) =>
        {
            flatten_bin_level(&inner, level, group_id, f, parts);
        }
        Some(value) => parts.push(render_bin_operand(&value, level, group_id, f)),
        None => {}
    }

    let op_token = b.op_token();
    let required_spacing = op_token
        .as_ref()
        .is_some_and(|token| token.kind().is_keyword());
    let operator = op_token
        .as_ref()
        .map(|token| op_text(token, f))
        .unwrap_or_else(nil);
    let spacing = if required_spacing || f.style.space_around_binary_op {
        " "
    } else {
        ""
    };
    if f.style.wrap_before_operator {
        parts.push(f.binary_op_space(required_spacing));
        parts.push(operator);
        parts.push(text(spacing));
    } else {
        parts.push(text(spacing));
        parts.push(operator);
        parts.push(f.binary_op_space(required_spacing));
    }

    if let Some(value) = b.rhs() {
        parts.push(render_bin_operand(&value, level, group_id, f));
    }
}

fn render_bin_operand(expr: &Expr, level: u8, group_id: GroupId, f: &FormatContext) -> Doc {
    let break_parenthesized_with = (f.style.logical_expression_layout
        == LogicalExpressionLayout::ExpandedBranches
        && matches!(level, 1 | 2))
    .then_some(group_id);
    render_expr_with_layout(
        expr,
        f,
        false,
        ExprLayout {
            force_break: false,
            break_parenthesized_with,
        },
    )
}

fn binary_level(kind: SyntaxKind) -> Option<u8> {
    Some(match kind {
        SyntaxKind::OR_KW | SyntaxKind::OR_ELSE_KW => 1,
        SyntaxKind::XOR_KW => 2,
        SyntaxKind::AND_KW | SyntaxKind::AND_THEN_KW | SyntaxKind::AMP => 3,
        SyntaxKind::EQ | SyntaxKind::NEQ => 4,
        SyntaxKind::LT | SyntaxKind::GT | SyntaxKind::LTE | SyntaxKind::GTE => 5,
        SyntaxKind::PLUS | SyntaxKind::MINUS => 6,
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::MOD_KW => 7,
        SyntaxKind::POW => 8,
        _ => return None,
    })
}

fn requires_source_layout(node: &tctidy_syntax::SyntaxNode, f: &FormatContext) -> bool {
    let mut has_line_break = false;
    for token in node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        match token.kind() {
            SyntaxKind::LINE_COMMENT
            | SyntaxKind::BLOCK_COMMENT
            | SyntaxKind::C_BLOCK_COMMENT
            | SyntaxKind::PRAGMA => return true,
            SyntaxKind::NEWLINE => has_line_break = true,
            _ => {}
        }
    }
    f.style.preserve_line_breaks && has_line_break
}

fn format_bin_source_layout(b: &BinExpr, f: &FormatContext, indented: bool) -> Doc {
    // The operand chain breaks before any operator the source put on a new
    // line (operator-leads-continuation). The continuation is indented one
    // level, but only once for the whole expression, so nested broken
    // chains (e.g. inside parentheses) never compound the indent.
    let breaks = bin_chain_breaks(b);
    let child_indented = indented || breaks;
    let mut parts = Vec::new();
    flatten_bin(b, f, &mut parts, child_indented);
    if breaks && !indented {
        indent(concat(parts))
    } else {
        concat(parts)
    }
}

/// A leaf expression (literal or name): the token text, with boolean-literal
/// tokens cased as keywords, everything else verbatim (identifier case is
/// preserved).
fn atom(expr: &Expr, f: &FormatContext) -> Doc {
    let node = expr.syntax();
    let tok = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia());
    match tok {
        Some(t) if matches!(t.kind(), SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW) => f.kw(t.text()),
        Some(t) => text(t.text().to_string()),
        None => nil(),
    }
}

/// Whether any operator in this left-associative chain sits on a new line.
fn bin_chain_breaks(b: &BinExpr) -> bool {
    let lhs_breaks = matches!(b.lhs(), Some(Expr::Bin(_)))
        && match b.lhs() {
            Some(Expr::Bin(inner)) => bin_chain_breaks(&inner),
            _ => false,
        };
    let op_breaks = b.op_token().is_some_and(|t| token_after_newline(&t));
    let rhs_breaks = b
        .rhs()
        .and_then(|rhs| first_sig_token(rhs.syntax()))
        .is_some_and(|token| token_after_newline(&token));
    lhs_breaks || op_breaks || rhs_breaks
}

/// Flatten a binary chain into `parts`, breaking before any operator the source
/// wrote on a new line (the operator then leads its continuation line). Operands
/// render with the chain's `indented` state so nested chains don't re-indent.
fn flatten_bin(b: &BinExpr, f: &FormatContext, parts: &mut Vec<Doc>, indented: bool) {
    match b.lhs() {
        Some(Expr::Bin(inner)) => flatten_bin(&inner, f, parts, indented),
        Some(e) => parts.push(render_expr(&e, f, indented)),
        None => {}
    }
    let op_token = b.op_token();
    let op = op_token.as_ref().map(|t| op_text(t, f)).unwrap_or_else(nil);
    let lhs_last = b.lhs().and_then(|e| last_sig_token(e.syntax()));
    let op_breaks = op_token.as_ref().is_some_and(token_after_newline);
    if op_breaks {
        // The operator leads its continuation line. A trailing comment on the
        // left operand sits on the line about to break, emit it before
        // the break so it is never dropped (non-destruction of comments).
        if let Some(tok) = &lhs_last {
            let min = f.style.trailing_comment_min_spaces as usize;
            parts.push(trailing_doc(f.comments.trailing(tok), min));
        }
        parts.push(hard_line());
        // A comment on its own line before the operator (leading of the operator
        // token) is preserved on its own line, just above the operator.
        if let Some(opt) = &op_token {
            let lead = f.comments.leading(opt);
            if !lead.is_empty() {
                parts.push(leading_doc(lead, f.style.blank_lines_max));
            }
        }
        parts.push(op);
    } else {
        parts.push(text(" "));
        // An inline block comment trailing the left operand (`a (*c*) AND b`)
        // stays in place, it cannot be a line comment here (the operator is on
        // the same line), so it is safe to emit verbatim inline.
        if let Some(tok) = &lhs_last {
            for p in f.comments.trailing(tok) {
                parts.push(verbatim(&p.text));
                parts.push(text(" "));
            }
        }
        parts.push(op);
    }
    if let Some(e) = b.rhs() {
        let gap = match (&op_token, first_sig_token(e.syntax())) {
            (Some(previous), Some(next)) => {
                gap_doc(
                    previous,
                    &next,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    " ",
                )
                .0
            }
            _ => text(" "),
        };
        parts.push(gap);
        parts.push(render_expr(&e, f, indented));
    }
}

/// A binary/unary operator token: keyword operators (`AND`, `OR`, `MOD`, …) cased
/// like keywords; symbolic operators verbatim.
fn op_text(tok: &SyntaxToken, f: &FormatContext) -> Doc {
    if tok.kind().is_keyword() {
        f.kw(tok.text())
    } else {
        text(tok.text().to_string())
    }
}

/// A `PAREN_EXPR`: a parenthesized expression or an initializer list. The
/// delimiter is taken from the actual tokens (`( )` or `[ ]`) so it is preserved.
/// An initializer the source wrote across several lines is reproduced one item
/// per line, with named-field `:=` aligned. A trailing separator after the last
/// item follows `StyleConfig::trailing_comma_collection` (house style: none, so a
/// redundant `,)` is dropped).
fn format_paren(
    node: &tctidy_syntax::SyntaxNode,
    f: &FormatContext,
    indented: bool,
    layout: ExprLayout,
) -> Doc {
    let open = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| matches!(token.kind(), SyntaxKind::L_PAREN | SyntaxKind::L_BRACK));
    let items: Vec<_> = node.children().collect();

    if open
        .as_ref()
        .is_some_and(|token| token.kind() == SyntaxKind::L_PAREN)
        && items.len() == 1
        && !requires_source_layout(node, f)
    {
        if let Some(expression) = Expr::cast(items[0].clone()) {
            let normal = expression_paren(&expression, f, false);
            if matches!(expression, Expr::Bin(_)) {
                let broken = expression_paren(&expression, f, true);
                if layout.force_break {
                    return broken;
                }
                if let Some(group_id) = layout.break_parenthesized_with {
                    return if_break_group(group_id, broken, normal);
                }
            }
            return normal;
        }
    }

    format_paren_source_layout(node, f, indented)
}

fn expression_paren(expression: &Expr, f: &FormatContext, force_break: bool) -> Doc {
    match f.style.parenthesized_expression_layout {
        ParenthesizedExpressionLayout::Hanging => {
            hanging_expression_paren(expression, f, force_break)
        }
        ParenthesizedExpressionLayout::Vertical => {
            vertical_expression_paren(expression, f, force_break)
        }
    }
}

fn hanging_expression_paren(expression: &Expr, f: &FormatContext, force_break: bool) -> Doc {
    let group_id = f.fresh_group_id();
    group_with_id(
        group_id,
        concat([
            text("("),
            if force_break { break_parent() } else { nil() },
            if_break_group(group_id, nil(), text(f.paren_pad())),
            align_current(concat([
                render_expr_with_layout(
                    expression,
                    f,
                    false,
                    ExprLayout {
                        force_break,
                        break_parenthesized_with: None,
                    },
                ),
                if_break_group(group_id, line(), text(f.paren_pad())),
                text(")"),
            ])),
        ]),
    )
}

fn vertical_expression_paren(expression: &Expr, f: &FormatContext, force_break: bool) -> Doc {
    let content = concat([
        text("("),
        indent(concat([
            f.paren_soft_line(),
            render_expr_with_layout(
                expression,
                f,
                false,
                ExprLayout {
                    force_break,
                    break_parenthesized_with: None,
                },
            ),
        ])),
        f.paren_soft_line(),
        text(")"),
    ]);
    if force_break {
        group_break(content)
    } else {
        group(content)
    }
}

fn format_paren_source_layout(
    node: &tctidy_syntax::SyntaxNode,
    f: &FormatContext,
    indented: bool,
) -> Doc {
    let open = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| matches!(t.kind(), SyntaxKind::L_PAREN | SyntaxKind::L_BRACK));
    let (lhs, rhs) = match open.as_ref().map(|t| t.kind()) {
        Some(SyntaxKind::L_BRACK) => ("[", "]"),
        _ => ("(", ")"),
    };
    let items: Vec<_> = node.children().collect();

    let is_named_initializer = !items.is_empty()
        && items
            .iter()
            .all(|item| NamedArg::cast(item.clone()).is_some());
    let too_many_named_fields =
        is_named_initializer && items.len() > usize::from(f.style.struct_initializer_max_inline);
    let array_max_inline = usize::from(f.style.array_initializer_max_inline);
    let too_many_array_values = lhs == "["
        && !is_named_initializer
        && array_max_inline > 0
        && items.len() > array_max_inline;
    let too_many_items = too_many_named_fields || too_many_array_values;
    if too_many_items {
        // A comment after `(` or before the first field needs its source line.
        // Comments on later fields can follow the hanging layout.
        let first_field_can_hang = open.as_ref().is_some_and(|token| {
            f.comments.trailing(token).is_empty()
                && items
                    .first()
                    .is_some_and(|first| f.comments.leading_of(first).is_empty())
        });
        let close_has_leading_comment = node
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(), SyntaxKind::R_PAREN | SyntaxKind::R_BRACK))
            .is_some_and(|token| !f.comments.leading(&token).is_empty());
        let preserve_open_break = f.style.preserve_line_breaks && paren_is_multiline(node);
        if first_field_can_hang && !close_has_leading_comment && !preserve_open_break {
            let name_col = items
                .iter()
                .filter_map(|item| NamedArg::cast(item.clone()))
                .filter_map(|arg| arg.name())
                .map(|name| name.text().chars().count())
                .max()
                .unwrap_or(0);
            return hanging_named_initializer(lhs, rhs, &items, name_col, f);
        }
    }

    if !paren_is_multiline(node) && !too_many_items {
        // Brackets (`[ … ]` array initializers) are not parentheses.
        let pad = if lhs == "(" && !items.is_empty() {
            f.paren_pad()
        } else {
            ""
        };
        let mut out = vec![text(lhs)];
        if let (Some(open), Some(first)) = (&open, items.first()) {
            out.push(gap_to_node(open, first, f, pad).0);
        }
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push(text(","));
                let gap = next_comma(&items[i - 1])
                    .map(|comma| gap_to_node(&comma, item, f, " ").0)
                    .unwrap_or_else(|| text(" "));
                out.push(gap);
            }
            // A parenthesized expression shares the enclosing continuation
            // indent; an initializer item starts fresh.
            out.push(match Expr::cast(item.clone()) {
                Some(expr) => render_expr(&expr, f, indented),
                None => item_doc(item, 0, f),
            });
        }
        out.push(text(pad));
        out.push(text(rhs));
        return concat(out);
    }

    // The `:=` column aligns the named fields by their widest name.
    let name_col = items
        .iter()
        .filter_map(|n| NamedArg::cast(n.clone()))
        .filter_map(|na| na.name())
        .map(|t| t.text().chars().count())
        .max()
        .unwrap_or(0);

    if is_named_initializer && !requires_source_layout(node, f) {
        return hanging_named_initializer(lhs, rhs, &items, name_col, f);
    }

    let min = f.style.trailing_comment_min_spaces as usize;
    let (open_gap, open_breaks) = match (&open, items.first()) {
        (Some(open), Some(first)) => gap_to_node(open, first, f, ""),
        _ => (nil(), false),
    };
    let mut body = Vec::new();
    for (i, n) in items.iter().enumerate() {
        if i > 0 || !open_breaks {
            body.push(hard_line());
        }
        // Comments on their own line above the item.
        let lead = f.comments.leading_of(n);
        if !(lead.is_empty() || i == 0 && open_breaks) {
            body.push(leading_doc(lead, f.style.blank_lines_max));
        }
        body.push(item_doc(n, name_col, f));
        let is_last = i + 1 == items.len();
        if !is_last || f.emit_trailing_sep(f.style.trailing_comma_collection) {
            body.push(text(","));
        }
        // A trailing comment on the item line (bound to the following comma, or
        // for the last item to its own last token) is preserved.
        let trailing = if is_last {
            f.comments.trailing_of_last_item(n)
        } else {
            next_comma(n)
                .map(|comma| f.comments.trailing(&comma).to_vec())
                .unwrap_or_default()
        };
        body.push(trailing_doc(&trailing, min));
    }
    group_break(concat([
        text(lhs),
        indent(concat([open_gap, concat(body)])),
        hard_line(),
        text(rhs),
    ]))
}

/// A multiline structure or array initializer with its first item beside the
/// opening delimiter. Following items align below the first one, while the
/// closing delimiter aligns below the opening delimiter.
fn hanging_named_initializer(
    lhs: &str,
    rhs: &str,
    items: &[tctidy_syntax::SyntaxNode],
    name_col: usize,
    f: &FormatContext,
) -> Doc {
    let mut body = Vec::new();
    let min = f.style.trailing_comment_min_spaces as usize;
    let mut previous_comment_moved = false;
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            body.push(hard_line());
            if !previous_comment_moved {
                let leading = f.comments.leading_of(item);
                let blanks_before = leading
                    .first()
                    .map_or(f.comments.blanks_before_of(item), |comment| {
                        comment.blank_before
                    });
                for _ in 0..blanks_before.min(f.style.blank_lines_max) {
                    body.push(hard_line());
                }
                body.push(leading_doc(leading, f.style.blank_lines_max));
                if !leading.is_empty() {
                    for _ in 0..f
                        .comments
                        .blanks_before_of(item)
                        .min(f.style.blank_lines_max)
                    {
                        body.push(hard_line());
                    }
                }
            }
        }
        body.push(item_doc(item, name_col, f));
        let is_last = index + 1 == items.len();
        if !is_last || f.emit_trailing_sep(f.style.trailing_comma_collection) {
            body.push(text(","));
        }
        let trailing = if is_last {
            f.comments.trailing_of_last_item(item)
        } else {
            next_comma(item)
                .map(|comma| f.comments.trailing(&comma).to_vec())
                .unwrap_or_default()
        };
        previous_comment_moved = false;
        {
            let trailing = trailing.as_slice();
            body.push(trailing_doc(trailing, min));
            if trailing.is_empty() {
                if let Some(next) = items.get(index + 1) {
                    let leading = f.comments.leading_of(next);
                    // An isolated comment immediately after a field comma
                    // annotates that field. Blank lines or stacked comments
                    // keep their source placement above the next field.
                    if leading.len() == 1
                        && leading[0].blank_before == 0
                        && leading[0].piece.text.starts_with("//")
                        && f.comments.blanks_before_of(next) == 0
                    {
                        body.push(trailing_doc(std::slice::from_ref(&leading[0].piece), min));
                        previous_comment_moved = true;
                    }
                }
            }
        }
    }

    group_break(align_current(concat([
        text(lhs.to_string()),
        align(IndentSpec::Spaces(1), concat(body)),
        hard_line(),
        text(rhs.to_string()),
    ])))
}

/// Whether the list is *internally* broken, i.e. a newline sits strictly
/// between the opening and closing delimiter.
///
/// Only newlines inside the delimiters count. A leading newline attached to the
/// node as trivia (e.g. the `:=`-on-the-next-line break of the enclosing
/// assignment, parsed as a leading child of the `PAREN_EXPR`) belongs to the
/// enclosing construct, not to the paren's own layout. Counting it would break
/// the paren on the first pass and then break one more nested paren on every
/// subsequent pass, a non-idempotence bug.
pub(crate) fn paren_is_multiline(node: &tctidy_syntax::SyntaxNode) -> bool {
    let mut inside = false;
    for tok in node.children_with_tokens().filter_map(|e| e.into_token()) {
        match tok.kind() {
            SyntaxKind::L_PAREN | SyntaxKind::L_BRACK => inside = true,
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACK => inside = false,
            SyntaxKind::NEWLINE if inside => return true,
            _ => {}
        }
    }
    false
}

/// One initializer-list item: a named field (`name := value`, optionally aligned
/// to `name_col`) or a bare expression.
fn item_doc(node: &tctidy_syntax::SyntaxNode, name_col: usize, f: &FormatContext) -> Doc {
    if let Some(na) = NamedArg::cast(node.clone()) {
        named_arg_doc(&na, name_col, f)
    } else if let Some(e) = Expr::cast(node.clone()) {
        format_expr(&e, f)
    } else {
        nil()
    }
}

/// A call expression: the callee followed by its argument list.
fn format_call_args(callee: Doc, al: &tctidy_syntax::SyntaxNode, f: &FormatContext) -> Doc {
    concat([callee, format_arg_list(al, f)])
}

/// A parenthesized argument list `( … )` (a call's args, or a function-block
/// instance's default arguments). The complete list makes one width-sensitive
/// layout decision, while lists with at least two named arguments, or with more
/// arguments than `collection_max_inline` (when not 0), always break, and so do
/// lists the source wrote across lines unless `arg_break` is `Fit`.
/// An argument expression can never break while the list itself remains flat.
/// Broken lists use one argument per line and align named argument operators.
/// Lists carrying comments or explicitly preserved source breaks use the lossless
/// source-layout path below.
pub(crate) fn format_arg_list(al: &tctidy_syntax::SyntaxNode, f: &FormatContext) -> Doc {
    if requires_source_layout(al, f) {
        return format_arg_list_source_layout(al, f);
    }

    let args: Vec<_> = al.children().collect();
    if args.is_empty() {
        return text("()");
    }

    let named_arg_count = args
        .iter()
        .filter(|node| NamedArg::cast((*node).clone()).is_some())
        .count();
    let name_col = args
        .iter()
        .filter_map(|node| NamedArg::cast(node.clone()))
        .filter_map(|arg| arg.name())
        .map(|token| token.text().chars().count())
        .max()
        .unwrap_or(0);
    let group_id = f.fresh_group_id();
    let mut items = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        items.push(if_break_group(
            group_id,
            call_arg(arg, name_col, f),
            call_arg(arg, 0, f),
        ));
        if index + 1 < args.len() {
            items.push(f.comma());
        } else if f.emit_trailing_sep(f.style.trailing_comma_collection) {
            items.push(if_break_group(group_id, text(","), nil()));
        }
    }

    // Under `AlwaysWhenMultiline`, keep an explicitly multiline list multiline,
    // but normalize its shape; the same group also gives a single-line source a
    // canonical first-pass wrap. Under `Fit`, only the width decides.
    let force_source_break = f.style.arg_break == CollectionBreak::AlwaysWhenMultiline
        && al.text().to_string().contains('\n');
    let force_named_break = named_arg_count >= 2;
    let max_inline = usize::from(f.style.collection_max_inline);
    let too_many_args = max_inline > 0 && args.len() > max_inline;
    let force_break = force_source_break || force_named_break || too_many_args;
    match f.style.call_argument_layout {
        CallArgumentLayout::Hanging => {
            hanging_arg_list(group_id, items, force_break, force_named_break, f)
        }
        CallArgumentLayout::Vertical => vertical_arg_list(group_id, items, force_break, f),
    }
}

fn hanging_arg_list(
    group_id: GroupId,
    items: Vec<Doc>,
    force_break: bool,
    close_after_last_arg: bool,
    f: &FormatContext,
) -> Doc {
    group_with_id(
        group_id,
        concat([
            text("("),
            if force_break { break_parent() } else { nil() },
            if_break_group(group_id, nil(), text(f.paren_pad())),
            align_current(concat([
                concat(items),
                if close_after_last_arg {
                    if_break_group(group_id, nil(), text(f.paren_pad()))
                } else {
                    if_break_group(group_id, line(), text(f.paren_pad()))
                },
                text(")"),
            ])),
        ]),
    )
}

fn vertical_arg_list(
    group_id: GroupId,
    items: Vec<Doc>,
    force_break: bool,
    f: &FormatContext,
) -> Doc {
    group_with_id(
        group_id,
        concat([
            text("("),
            if force_break { break_parent() } else { nil() },
            indent(concat([f.paren_soft_line(), concat(items)])),
            f.paren_soft_line(),
            text(")"),
        ]),
    )
}

/// Lossless argument-list layout for comments, pragmas, or explicit source-break
/// preservation. This keeps the prior source-sensitive behavior because comment
/// attachment takes precedence over canonical wrapping.
fn format_arg_list_source_layout(al: &tctidy_syntax::SyntaxNode, f: &FormatContext) -> Doc {
    let args: Vec<_> = al.children().collect();
    let open = al
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| matches!(t.kind(), SyntaxKind::L_PAREN | SyntaxKind::L_BRACK));
    if !al.text().to_string().contains('\n') {
        // A single-line list stays flat, but any inline block comment (after the
        // `(` or trailing an argument) is kept in place, it can only be a block
        // comment here, so it is safe to emit verbatim inline.
        let mut out = vec![text("(")];
        if let (Some(open), Some(first)) = (&open, args.first()) {
            out.push(gap_to_node(open, first, f, f.paren_pad()).0);
        }
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                out.push(text(","));
                let gap = next_comma(&args[i - 1])
                    .map(|comma| gap_to_node(&comma, a, f, " ").0)
                    .unwrap_or_else(|| text(" "));
                out.push(gap);
            }
            out.push(call_arg(a, 0, f));
            if let Some(tok) = last_sig_token(a) {
                for p in f.comments.trailing(&tok) {
                    out.push(text(" "));
                    out.push(verbatim(&p.text));
                }
            }
        }
        if !args.is_empty() {
            out.push(text(f.paren_pad()));
        }
        out.push(text(")"));
        return concat(out);
    }
    let name_col = args
        .iter()
        .filter_map(|n| NamedArg::cast(n.clone()))
        .filter_map(|na| na.name())
        .map(|t| t.text().chars().count())
        .max()
        .unwrap_or(0);
    let min = f.style.trailing_comment_min_spaces as usize;
    let (open_gap, open_breaks) = match (&open, args.first()) {
        (Some(open), Some(first)) => gap_to_node(open, first, f, ""),
        _ => (nil(), false),
    };
    let mut inner = Vec::new();
    for (i, a) in args.iter().enumerate() {
        // Comments on their own line above the argument. For the first argument
        // this also pushes it off the `(` line, so a re-parse keeps the comment
        // leading (idempotence).
        let lead = f.comments.leading_of(a);
        if !(lead.is_empty() || i == 0 && open_breaks) {
            if i == 0 {
                inner.push(hard_line());
            }
            inner.push(leading_doc(lead, f.style.blank_lines_max));
        }
        inner.push(call_arg(a, name_col, f));
        let is_last = i + 1 == args.len();
        if !is_last || f.emit_trailing_sep(f.style.trailing_comma_collection) {
            inner.push(text(","));
        }
        // A trailing comment on the argument line binds to the comma (or, for the
        // last argument, the argument's last token).
        let trailing = if is_last {
            f.comments.trailing_of_last_item(a)
        } else {
            next_comma(a)
                .map(|comma| f.comments.trailing(&comma).to_vec())
                .unwrap_or_default()
        };
        inner.push(trailing_doc(&trailing, min));
        // A comment after an emitted trailing comma keeps the `)` on the next
        // line: next to it, the comment would be flushed past the `)` and the
        // next pass would lay the list out without it (not idempotent).
        let close_on_own_line = is_last
            && !trailing.is_empty()
            && f.emit_trailing_sep(f.style.trailing_comma_collection);
        if !is_last || close_on_own_line {
            inner.push(hard_line());
        }
    }
    concat([
        text("("),
        align_current(concat([open_gap, concat(inner)])),
        text(")"),
    ])
}

/// The `COMMA` token immediately following `node` among its siblings.
fn next_comma(node: &tctidy_syntax::SyntaxNode) -> Option<SyntaxToken> {
    let mut cur = node.next_sibling_or_token();
    while let Some(el) = cur {
        match el.as_token() {
            Some(t) if t.kind() == SyntaxKind::COMMA => return Some(t.clone()),
            Some(t) if t.kind().is_trivia() => {}
            _ => return None,
        }
        cur = el.next_sibling_or_token();
    }
    None
}

fn gap_to_node(
    previous: &SyntaxToken,
    next: &tctidy_syntax::SyntaxNode,
    f: &FormatContext,
    inline_prefix: &str,
) -> (Doc, bool) {
    first_sig_token(next).map_or_else(
        || (text(inline_prefix.to_string()), false),
        |token| {
            if f.comments.trailing(previous).is_empty() && f.comments.leading(&token).is_empty() {
                return (text(inline_prefix.to_string()), false);
            }
            gap_doc(
                previous,
                &token,
                f.comments,
                f.style.trailing_comment_min_spaces as usize,
                f.style.blank_lines_max,
                inline_prefix,
            )
        },
    )
}

/// One call argument: positional or `name := value` / `name => value` (the name
/// padded to `name_col` for alignment; the value may be empty for `=>`).
fn call_arg(node: &tctidy_syntax::SyntaxNode, name_col: usize, f: &FormatContext) -> Doc {
    if let Some(na) = NamedArg::cast(node.clone()) {
        named_arg_doc(&na, name_col, f)
    } else if let Some(e) = node.children().find_map(Expr::cast) {
        format_expr(&e, f)
    } else {
        nil()
    }
}

fn named_arg_doc(arg: &NamedArg, name_col: usize, f: &FormatContext) -> Doc {
    let name = arg.name().map(|t| t.text().to_string()).unwrap_or_default();
    let pad = name_col.saturating_sub(name.chars().count());
    let assign = arg.assign_token();
    let value = arg.value();
    let operator = match assign.as_ref() {
        Some(token) if token.kind() == SyntaxKind::ARROW => " =>",
        _ => " :=",
    };

    let mut parts = vec![text(name), text(" ".repeat(pad)), text(operator)];
    if let Some(value) = value {
        let gap = match (assign.as_ref(), first_sig_token(value.syntax())) {
            (Some(previous), Some(next)) => {
                gap_doc(
                    previous,
                    &next,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    " ",
                )
                .0
            }
            _ => text(" "),
        };
        parts.push(gap);
        parts.push(format_expr(&value, f));
    }
    concat(parts)
}
