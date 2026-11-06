//! Declaration-section lowering: `VAR..END_VAR` blocks, `TYPE` definitions
//! (struct / union / enum / alias / subrange / array / pointer / reference) and
//! POU headers.
//!
//! Column alignment (`:`, `AT`, and `:=` over a run of init-bearing members) is
//! computed **eagerly** here, max name/type width per group, rather than by the
//! printer, so it survives a re-format unchanged (idempotence). A group is a run
//! of declarations with no blank line, no leading comment, and no wrapped member
//! between them.

use tctidy_pretty::prelude::*;
use tctidy_pretty::{print, LineEnding};
use tctidy_syntax::ast::{
    ArrayType, AstNode, EnumType, EnumVariant, Expr, GenericArgs, NamedArg, PointerType,
    ReferenceType, StructType, Type, TypeDecl, TypeRef, UnionType, VarBlock, VarDecl,
};
use tctidy_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::context::FormatContext;
use crate::keyword::apply_keyword_case;
use crate::rules::expr::format_expr;
use crate::style::ColonAlignment;
use crate::trivia::{
    first_sig_token, gap_doc, last_sig_token, leading_doc, token_after_newline, trailing_doc,
};
use crate::verbatim::verbatim;

/// Lower a `DECL_PART`: a sequence of top-level constructs (var blocks, type
/// definitions, POU headers), each carrying its leading/trailing comments.
pub(crate) fn format_decl_part(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let items: Vec<SyntaxNode> = node.children().collect();
    let mut parts = Vec::new();
    for (i, item) in items.iter().enumerate() {
        // The construct after a declaration block gets at least
        // `blank_lines_after_decl` blank lines above it.
        let min_blank = match i.checked_sub(1).map(|p| items[p].kind()) {
            Some(SyntaxKind::VAR_BLOCK | SyntaxKind::TYPE_DECL) => f.style.blank_lines_after_decl,
            _ => 0,
        };
        if i > 0 {
            parts.push(hard_line());
        }
        parts.push(item_entry(item, i == 0, min_blank, f));
    }
    // End-of-section dangling comments (after the last construct).
    let dangling = f.comments.dangling();
    if !dangling.is_empty() {
        if !items.is_empty() {
            parts.push(hard_line());
        }
        parts.push(crate::trivia::dangling_doc(
            dangling,
            f.style.blank_lines_max,
        ));
    }
    concat(parts)
}

/// A top-level construct preceded by its blank line(s), at least `min_blank`,
/// and leading comments.
fn item_entry(node: &SyntaxNode, is_first: bool, min_blank: u16, f: &FormatContext) -> Doc {
    let cap = f.style.blank_lines_max;
    let lead = f.comments.leading_of(node);
    let block_blank = if lead.is_empty() {
        f.comments.blanks_before_of(node)
    } else {
        lead.first().map_or(0, |lp| lp.blank_before)
    };
    let mut parts = Vec::new();
    if !is_first {
        let minimum = min_blank.max(f.style.multiline_comment_minimum(lead));
        for _ in 0..f.style.ensure_blank(block_blank, minimum) {
            parts.push(hard_line());
        }
    }
    parts.push(leading_doc(lead, cap));
    parts.push(format_item(node, f));
    concat(parts)
}

/// Dispatch a top-level construct to its layout; unknown kinds fall back to
/// verbatim source text (the totality lever).
fn format_item(node: &SyntaxNode, f: &FormatContext) -> Doc {
    match node.kind() {
        SyntaxKind::VAR_BLOCK => VarBlock::cast(node.clone())
            .map(|b| format_var_block(&b, f))
            .unwrap_or_else(|| verbatim_node(node)),
        SyntaxKind::TYPE_DECL => TypeDecl::cast(node.clone())
            .map(|t| format_type_decl(&t, f))
            .unwrap_or_else(|| verbatim_node(node)),
        SyntaxKind::PROGRAM_DECL
        | SyntaxKind::FUNCTION_DECL
        | SyntaxKind::FUNCTION_BLOCK_DECL
        | SyntaxKind::METHOD_DECL
        | SyntaxKind::PROPERTY_DECL
        | SyntaxKind::INTERFACE_DECL
        | SyntaxKind::ACTION_DECL => format_pou_header(node, f),
        SyntaxKind::ACCESS_MODIFIER => format_access_modifier(node, f),
        _ => verbatim_node(node),
    }
}

/// A property accessor's own access modifier (`PUBLIC`), alone on its line.
fn format_access_modifier(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let Some(tok) = first_sig_token(node) else {
        return verbatim_node(node);
    };
    concat([
        f.kw(tok.text()),
        trailing_doc(
            f.comments.trailing(&tok),
            f.style.trailing_comment_min_spaces as usize,
        ),
    ])
}

/// A POU header line: `KEYWORD [access]* Name [EXTENDS …] [IMPLEMENTS …] [: type]`,
/// all on one line, keywords cased, identifiers verbatim.
fn format_pou_header(node: &SyntaxNode, f: &FormatContext) -> Doc {
    use tctidy_syntax::SyntaxElement;
    let mut parts = Vec::new();
    let mut multiline = false;
    for elem in node.children_with_tokens() {
        match elem {
            SyntaxElement::Token(t) => {
                if t.kind().is_trivia() {
                    continue;
                }
                // The optional terminating `;` attaches with no leading space.
                if t.kind() == SyntaxKind::SEMICOLON {
                    parts.push(text(";"));
                    continue;
                }
                if !parts.is_empty() {
                    parts.push(text(" "));
                }
                parts.push(type_token(&t, f));
            }
            SyntaxElement::Node(n) => match n.kind() {
                SyntaxKind::RETURN_TYPE => {
                    if let Some(return_type) = tctidy_syntax::ast::ReturnType::cast(n.clone()) {
                        let ty = return_type.ty();
                        let (gap, breaks) = gap_after_token(
                            return_type.colon().as_ref(),
                            ty.as_ref().map(AstNode::syntax),
                            f.space_after_colon(),
                            f,
                        );
                        parts.push(f.colon_token(true));
                        let value =
                            concat([gap, ty.as_ref().map_or_else(nil, |ty| format_type(ty, f))]);
                        parts.push(if breaks { indent(value) } else { value });
                    }
                }
                SyntaxKind::VAR_BLOCK => {
                    parts.push(hard_line());
                    parts.push(leading_doc(
                        f.comments.leading_of(&n),
                        f.style.blank_lines_max,
                    ));
                    parts.push(
                        VarBlock::cast(n.clone())
                            .map(|block| format_var_block(&block, f))
                            .unwrap_or_else(|| verbatim_node(&n)),
                    );
                    multiline = true;
                }
                SyntaxKind::EXTENDS_CLAUSE | SyntaxKind::IMPLEMENTS_CLAUSE => {
                    if multiline {
                        parts.push(hard_line());
                        parts.push(leading_doc(
                            f.comments.leading_of(&n),
                            f.style.blank_lines_max,
                        ));
                    } else {
                        parts.push(text(" "));
                    }
                    parts.push(format_clause(&n, f));
                }
                _ => {
                    parts.push(text(" "));
                    parts.push(verbatim_node(&n));
                }
            },
        }
    }
    // A trailing comment on the header line (e.g. a property's doc comment).
    if let Some(tok) = crate::trivia::last_sig_token(node) {
        // An embedded generic block already emitted its END_VAR comment.
        if !(multiline && tok.kind() == SyntaxKind::END_VAR_KW) {
            parts.push(trailing_doc(
                f.comments.trailing(&tok),
                f.style.trailing_comment_min_spaces as usize,
            ));
        }
    }
    concat(parts)
}

/// An `EXTENDS`/`IMPLEMENTS` clause: keyword + space-separated (dotted) type list.
fn format_clause(node: &SyntaxNode, f: &FormatContext) -> Doc {
    use tctidy_syntax::SyntaxElement;

    let mut parts = Vec::new();
    let mut previous: Option<SyntaxToken> = None;
    for element in node.children_with_tokens() {
        let (first, last, doc) = match element {
            SyntaxElement::Token(token) if !token.kind().is_trivia() => {
                let doc = match token.kind() {
                    SyntaxKind::EXTENDS_KW | SyntaxKind::IMPLEMENTS_KW => f.kw(token.text()),
                    _ => type_token(&token, f),
                };
                (token.clone(), token, doc)
            }
            SyntaxElement::Node(child) => {
                let Some(ty) = Type::cast(child.clone()) else {
                    continue;
                };
                let Some(first) = first_sig_token(&child) else {
                    continue;
                };
                let Some(last) = last_sig_token(&child) else {
                    continue;
                };
                (first, last, format_type(&ty, f))
            }
            _ => continue,
        };

        if let Some(previous) = &previous {
            let default = match (previous.kind(), first.kind()) {
                (_, SyntaxKind::COMMA | SyntaxKind::DOT) | (SyntaxKind::DOT, _) => "",
                (SyntaxKind::COMMA | SyntaxKind::EXTENDS_KW | SyntaxKind::IMPLEMENTS_KW, _) => " ",
                _ => "",
            };
            parts.push(
                gap_doc(
                    previous,
                    &first,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    default,
                )
                .0,
            );
        }
        parts.push(doc);
        previous = Some(last);
    }
    concat(parts)
}

// ---- VAR blocks ----

fn format_var_block(block: &VarBlock, f: &FormatContext) -> Doc {
    let node = block.syntax();
    // Header: the section keyword plus any qualifiers, all on one line.
    let header = header_keywords(node, f);
    let body = format_members(block.decls().collect(), node, SyntaxKind::END_VAR_KW, f);
    let semicolon = if has_token(node, SyntaxKind::SEMICOLON) {
        text(";")
    } else {
        nil()
    };
    let end = concat([
        f.kw("END_VAR"),
        semicolon,
        trailing_doc(
            f.comments.trailing_of(node),
            f.style.trailing_comment_min_spaces as usize,
        ),
    ]);
    group_break(concat([header, body, hard_line(), end]))
}

/// The leading keyword tokens of a block header (e.g. `VAR_GLOBAL CONSTANT`),
/// cased and space-separated, stopping at the first declaration.
fn header_keywords(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let mut parts = Vec::new();
    for tok in node
        .children_with_tokens()
        // Stop at the first declaration node or at `END_VAR` (so an empty block
        // does not swallow its terminator into the header line).
        .take_while(|e| {
            e.as_node().is_none()
                && e.as_token()
                    .is_none_or(|t| t.kind() != SyntaxKind::END_VAR_KW)
        })
        .filter_map(|e| e.into_token())
    {
        if tok.kind().is_trivia() {
            continue;
        }
        if !parts.is_empty() {
            parts.push(text(" "));
        }
        parts.push(f.kw(tok.text()));
    }
    concat(parts)
}

/// Lower a list of declarations (var block members or struct/union members) into
/// an indented body: one declaration per line, names + `AT` + colons aligned per
/// group.
fn format_members(
    decls: Vec<VarDecl>,
    node: &SyntaxNode,
    end_kind: SyntaxKind,
    f: &FormatContext,
) -> Doc {
    let separate_named_initializers = node.kind() == SyntaxKind::STRUCT_TYPE;
    let cols = align_columns(&decls, f, separate_named_initializers);
    let acols = assign_columns(&decls, f, separate_named_initializers);
    let mut body = Vec::new();
    let mut next_decl = 0;
    let mut is_first = true;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::VAR_DECL if next_decl < decls.len() => {
                let i = next_decl;
                next_decl += 1;
                body.push(hard_line());
                body.push(decl_entry(&decls[i], cols[i], acols[i], is_first, f));
                is_first = false;
            }
            SyntaxKind::EMPTY_DECL => {
                body.push(hard_line());
                body.push(empty_decl_entry(&child, f));
                is_first = false;
            }
            _ => {}
        }
    }
    // Trailing comments / blank line before the closer, indented with the members.
    body.push(dangling_inner(node, end_kind, f));
    indent(concat(body))
}

/// A stray `;` among the members, kept on its own line with its comments.
fn empty_decl_entry(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let cap = f.style.blank_lines_max;
    let lead = f.comments.leading_of(node);
    let block_blank = if lead.is_empty() {
        f.comments.blanks_before_of(node)
    } else {
        lead.first().map_or(0, |lp| lp.blank_before)
    };
    let mut parts = Vec::new();
    for _ in 0..block_blank.min(cap) {
        parts.push(hard_line());
    }
    parts.push(leading_doc(lead, cap));
    parts.push(text(";"));
    parts.push(trailing_doc(
        f.comments.trailing_of(node),
        f.style.trailing_comment_min_spaces as usize,
    ));
    concat(parts)
}

/// A single declaration line with its leading and trailing comments. A member
/// after the first gets `blank_lines_before_multiline_comment` above a leading
/// multiline comment.
fn decl_entry(
    decl: &VarDecl,
    col: (usize, usize),
    assign_col: usize,
    is_first: bool,
    f: &FormatContext,
) -> Doc {
    let cap = f.style.blank_lines_max;
    let node = decl.syntax();
    let lead = f.comments.leading_of(node);
    let block_blank = if lead.is_empty() {
        f.comments.blanks_before_of(node)
    } else {
        lead.first().map_or(0, |lp| lp.blank_before)
    };
    let minimum = if is_first {
        0
    } else {
        f.style.multiline_comment_minimum(lead)
    };
    let mut parts = Vec::new();
    for _ in 0..f.style.ensure_blank(block_blank, minimum) {
        parts.push(hard_line());
    }
    parts.push(leading_doc(lead, cap));
    parts.push(decl_line(decl, col, assign_col, f));
    concat(parts)
}

/// `names [AT %addr] : type [:= init] ;` plus any trailing comment. `col` is the
/// group's `(name_col, colon_col)`, the name is padded to align `AT`, and the
/// pre-colon segment to align the `:`. `assign_col` aligns `:=` over a run of
/// init-bearing members (0 when not aligned).
fn decl_line(decl: &VarDecl, col: (usize, usize), assign_col: usize, f: &FormatContext) -> Doc {
    let (name_col, colon_col) = col;
    let mut pre = names_text(decl);
    if let Some(addr) = at_text(decl) {
        // Pad the name to align `AT` across the group's located variables.
        pre = pad_to(&pre, name_col);
        pre.push(' ');
        pre.push_str(&apply_keyword_case("AT", f.style.keyword_case));
        pre.push(' ');
        pre.push_str(&addr);
    }
    let pad = colon_col.saturating_sub(display_width(&pre));
    let prefix = concat([text(pre.clone()), text(" ".repeat(pad))]);
    let ty = decl.ty();
    let tw = ty.as_ref().map_or(0, |t| type_width(t, f));
    let init = decl.init();
    let init_doc = init
        .as_ref()
        .map(|i| join(text(", "), i.exprs().map(|e| format_expr(&e, f))));
    let type_doc = {
        let base = ty.as_ref().map(|t| format_type(t, f)).unwrap_or_else(nil);
        // A function-block instance's default arguments: `x : FB_T(a := 1, …)`.
        match decl
            .syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::ARG_LIST)
        {
            Some(al) => concat([base, crate::rules::expr::format_arg_list(&al, f)]),
            None => base,
        }
    };
    let assign_pad = init.as_ref().map_or(0, |_| assign_col.saturating_sub(tw));

    // A comment inside the name list is preserved in place. Block comments
    // stay inline; a line comment forces the names one per line and the `:` onto
    // the next line. Such a member is not colon-aligned and is isolated from its
    // neighbours' alignment group.
    if name_list_comments(decl, f) {
        let (names, last_is_line) = names_with_comments(decl, f);
        let colon_starts_line = last_is_line || colon_on_new_line(decl);
        let mut head = vec![names];
        if let Some(addr) = at_text(decl) {
            head.push(text(" "));
            head.push(text(apply_keyword_case("AT", f.style.keyword_case)));
            head.push(text(" "));
            head.push(text(addr));
        }
        let tail = var_decl_tail(decl, type_doc, init_doc, assign_pad, !colon_starts_line, f);
        // A source `:` on its own line, or a line comment ending the names, puts
        // the tail on the next (indented) line; otherwise it stays inline.
        if colon_starts_line {
            head.push(indent(concat([hard_line(), tail])));
        } else {
            head.push(tail);
        }
        return concat(head);
    }

    // `:` on its own (continuation) line: the name stays alone, then `: type …`
    // moves to the next line at one extra indent. The member is not colon-aligned.
    if colon_on_new_line(decl) {
        let tail = var_decl_tail(decl, type_doc, init_doc, assign_pad, false, f);
        return concat([text(pre), indent(concat([hard_line(), tail]))]);
    }

    concat([
        prefix,
        var_decl_tail(decl, type_doc, init_doc, assign_pad, true, f),
    ])
}

fn var_decl_tail(
    decl: &VarDecl,
    type_doc: Doc,
    init_doc: Option<Doc>,
    assign_pad: usize,
    colon_inline: bool,
    f: &FormatContext,
) -> Doc {
    let ty = decl.ty();
    let init = decl.init();
    let (type_gap, type_breaks) = gap_after_token(
        decl.colon().as_ref(),
        ty.as_ref().map(AstNode::syntax),
        f.space_after_colon(),
        f,
    );
    let type_value = concat([type_gap, type_doc]);
    let mut parts = vec![
        f.colon_token(colon_inline),
        if type_breaks {
            indent(type_value)
        } else {
            type_value
        },
    ];

    if let (Some(init), Some(init_doc)) = (init.as_ref(), init_doc) {
        let first = init.exprs().next();
        let (value_gap, value_breaks) = gap_after_token(
            init.assign_token().as_ref(),
            first.as_ref().map(AstNode::syntax),
            f.space_after_assign(),
            f,
        );
        let value = concat([value_gap, init_doc]);
        let value_multiline = first.as_ref().is_some_and(|expr| {
            matches!(expr, Expr::Paren(_)) && crate::rules::expr::paren_is_multiline(expr.syntax())
        });
        parts.push(text(" ".repeat(assign_pad)));
        parts.push(f.assign_op_token(true));
        parts.push(if value_breaks && !value_multiline {
            indent(value)
        } else {
            value
        });
    }

    if init.is_none() {
        if let (Some(ty), Some(semicolon)) = (ty.as_ref(), decl.semicolon()) {
            if let Some(last) = last_sig_token(ty.syntax()) {
                let has_inline_comment = !f.comments.trailing(&last).is_empty();
                if has_inline_comment || token_after_newline(&semicolon) {
                    parts.push(
                        gap_doc(
                            &last,
                            &semicolon,
                            f.comments,
                            f.style.trailing_comment_min_spaces as usize,
                            f.style.blank_lines_max,
                            if has_inline_comment { " " } else { "" },
                        )
                        .0,
                    );
                }
            }
        }
    }
    parts.push(text(";"));
    parts.push(trailing_doc(
        f.comments.trailing_of(decl.syntax()),
        f.style.trailing_comment_min_spaces as usize,
    ));
    concat(parts)
}

/// The comma-separated names of a declaration (left of `AT`/`:`).
///
/// Multi-member names are always rendered on a single line: source line breaks
/// between names are dropped and the list is never re-wrapped on width (the
/// opinionated house style). The one exception is a name list carrying
/// comments, handled by `names_with_comments` so no comment is lost.
fn names_text(decl: &VarDecl) -> String {
    let names: Vec<String> = decl
        .name_list()
        .map(|nl| nl.names().map(|t| t.text().to_string()).collect())
        .unwrap_or_default();
    names.join(", ")
}

/// The `AT` located-address text (without the keyword), if any.
fn at_text(decl: &VarDecl) -> Option<String> {
    decl.at_clause()
        .and_then(|at| at.address())
        .map(|a| a.text().to_string())
}

/// The `,` token immediately following a name token (skipping trivia), if the
/// name is not the last in its list.
fn comma_after(name: &SyntaxToken) -> Option<SyntaxToken> {
    let mut cur = name.next_token();
    while let Some(t) = cur {
        match t.kind() {
            SyntaxKind::COMMA => return Some(t),
            k if k.is_trivia() => cur = t.next_token(),
            _ => return None,
        }
    }
    None
}

/// Whether the name list carries any comment attached between/around its names,
/// a comment trailing a name or its comma, or an own-line comment above a name
/// other than the first (the first name's leading is the declaration's leading,
/// emitted by `decl_entry`). Such a declaration is rendered one name per line so
/// the comment is never dropped.
fn name_list_comments(decl: &VarDecl, f: &FormatContext) -> bool {
    let Some(nl) = decl.name_list() else {
        return false;
    };
    nl.names().enumerate().any(|(i, name)| {
        (i > 0 && !f.comments.leading(&name).is_empty())
            || !f.comments.trailing(&name).is_empty()
            || comma_after(&name).is_some_and(|c| !f.comments.trailing(&c).is_empty())
    })
}

/// Whether a comment's verbatim text is a line comment (`//…`), it must end its
/// line, so it forces a break; block comments and pragmas may stay inline.
fn is_line_comment(text: &str) -> bool {
    text.starts_with("//")
}

/// Render a comment-bearing name list, preserving every comment in place. Block
/// comments and pragmas stay inline (`a, (* c *) b`); a line comment (which must
/// end its line) or an own-line comment above a name forces the names one per
/// line. The first name's leading comments belong to the declaration and are
/// emitted by the caller, so they are skipped here. Returns the names' `Doc` and
/// whether the last name ended with a line comment (so the caller puts the `:`
/// on the next line).
fn names_with_comments(decl: &VarDecl, f: &FormatContext) -> (Doc, bool) {
    let cap = f.style.blank_lines_max;
    let names: Vec<SyntaxToken> = decl
        .name_list()
        .map(|nl| nl.names().collect())
        .unwrap_or_default();
    // A line comment, or an own-line comment above a non-first name, forces one
    // name per line; a list carrying only inline block comments stays flat.
    let explode = names.iter().enumerate().any(|(i, n)| {
        (i > 0 && !f.comments.leading(n).is_empty())
            || f.comments
                .trailing(n)
                .iter()
                .any(|p| is_line_comment(&p.text))
            || comma_after(n).is_some_and(|c| {
                f.comments
                    .trailing(&c)
                    .iter()
                    .any(|p| is_line_comment(&p.text))
            })
    });
    let mut parts = Vec::new();
    let mut last_is_line = false;
    for (i, name) in names.iter().enumerate() {
        if i > 0 && !f.comments.leading(name).is_empty() {
            parts.push(leading_doc(f.comments.leading(name), cap));
        }
        parts.push(text(name.text().to_string()));
        let is_last = i + 1 == names.len();
        if !is_last {
            parts.push(text(","));
        }
        // The name's own trailing comments, then the following comma's, inline.
        let mut trailing = f.comments.trailing(name).to_vec();
        if let Some(c) = comma_after(name) {
            trailing.extend_from_slice(f.comments.trailing(&c));
        }
        let mut ended_line = false;
        for p in &trailing {
            parts.push(text(" "));
            parts.push(verbatim(&p.text));
            ended_line = is_line_comment(&p.text);
        }
        if is_last {
            last_is_line = ended_line;
        } else if explode {
            parts.push(hard_line());
        } else {
            parts.push(text(" "));
        }
    }
    (concat(parts), last_is_line)
}

/// Pad `s` with trailing spaces to at least `width` display columns.
fn pad_to(s: &str, width: usize) -> String {
    let mut out = s.to_string();
    for _ in display_width(s)..width {
        out.push(' ');
    }
    out
}

/// Per-declaration `(name_col, colon_col)` alignment columns. Within a group, the
/// name is padded to the widest name (so `AT` lines up) and the pre-colon segment
/// to the widest such segment (so `:` lines up). A group breaks at a blank line
/// or a leading comment. Within a `STRUCT`, named structure initializers and
/// scalar members form separate groups.
fn align_columns(
    decls: &[VarDecl],
    f: &FormatContext,
    separate_named_initializers: bool,
) -> Vec<(usize, usize)> {
    let name_w: Vec<usize> = decls
        .iter()
        .map(|d| display_width(&names_text(d)))
        .collect();
    let has_at: Vec<bool> = decls.iter().map(|d| d.at_clause().is_some()).collect();
    // Width contributed by the `AT %addr` clause: ` AT ` (4) + address.
    let at_w: Vec<usize> = decls
        .iter()
        .map(|d| at_text(d).map_or(0, |a| 4 + display_width(&a)))
        .collect();
    let breaks: Vec<bool> = (0..decls.len())
        .map(|i| group_break_at(decls, i, f, separate_named_initializers))
        .collect();

    let mut cols = vec![(0usize, 0usize); decls.len()];
    let mut start = 0;
    let close = |start: usize, end: usize, cols: &mut Vec<(usize, usize)>| {
        // `name_col` aligns `AT` and is measured over the located rows only; a
        // group with no `AT` leaves it at 0 (names are not pre-padded).
        let name_col = if f.style.align_at_addresses {
            (start..end)
                .filter(|&k| has_at[k])
                .map(|k| name_w[k])
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        let colon_col = if f.style.align_decl_colon == ColonAlignment::PerBlock {
            (start..end)
                .map(|k| {
                    if has_at[k] {
                        name_col.max(name_w[k]) + at_w[k]
                    } else {
                        name_w[k]
                    }
                })
                .max()
                .unwrap_or(0)
        } else {
            // Unaligned: every row keeps its own width (no padding before `:`).
            0
        };
        cols[start..end]
            .iter_mut()
            .for_each(|c| *c = (name_col, colon_col));
    };
    for (i, &brk) in breaks.iter().enumerate() {
        if i > 0 && brk {
            close(start, i, &mut cols);
            start = i;
        }
    }
    close(start, decls.len(), &mut cols);
    cols
}

/// The `:=` column for each declaration: the max type width within its
/// contiguous run of init-bearing members (0 for members without init). A run
/// breaks at a member without init, a leading comment, or a blank line.
fn assign_columns(
    decls: &[VarDecl],
    f: &FormatContext,
    separate_named_initializers: bool,
) -> Vec<usize> {
    let n = decls.len();
    let mut cols = vec![0usize; n];
    // The `:=` column builds on the aligned `:` column; without that, a padded
    // `:=` would line up with nothing.
    if f.style.align_decl_colon == ColonAlignment::Off {
        return cols;
    }
    let mut i = 0;
    while i < n {
        if decls[i].init().is_none() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n
            && decls[j].init().is_some()
            && (j == i || !group_break_at(decls, j, f, separate_named_initializers))
        {
            j += 1;
        }
        let max = decls[i..j]
            .iter()
            .map(|d| d.ty().map_or(0, |t| type_width(&t, f)))
            .max()
            .unwrap_or(0);
        cols[i..j].iter_mut().for_each(|c| *c = max);
        i = j;
    }
    cols
}

/// The display width of a type rendered flat (for `:=` column alignment).
fn type_width(ty: &Type, f: &FormatContext) -> usize {
    let doc = format_type(ty, f);
    let s = print(&doc, &f.style.print_options(LineEnding::Lf));
    s.lines().next().map_or(0, display_width)
}

// ---- TYPE definitions ----

fn format_type_decl(decl: &TypeDecl, f: &FormatContext) -> Doc {
    let name = decl
        .name()
        .map(|t| text(t.text().to_string()))
        .unwrap_or_else(nil);
    let mut head = vec![f.kw("TYPE")];
    // Access or abstract modifiers before the name (`TYPE INTERNAL E_Mode`).
    for modifier in decl
        .syntax()
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| {
            matches!(
                t.kind(),
                SyntaxKind::ABSTRACT_KW
                    | SyntaxKind::FINAL_KW
                    | SyntaxKind::PUBLIC_KW
                    | SyntaxKind::PRIVATE_KW
                    | SyntaxKind::PROTECTED_KW
                    | SyntaxKind::INTERNAL_KW
            )
        })
    {
        head.push(text(" "));
        head.push(f.kw(modifier.text()));
    }
    head.push(text(" "));
    head.push(name);
    if let Some(ext) = decl.extends() {
        head.push(text(" "));
        head.push(f.kw("EXTENDS"));
        head.push(text(" "));
        if let Some(n) = ext.name() {
            head.push(text(n.text().to_string()));
        }
    }
    let body = decl
        .type_def()
        .map(|ty| format_type(&ty, f))
        .unwrap_or_else(nil);

    let type_def = decl.type_def();
    let (type_gap, _) = gap_after_token(
        decl.colon().as_ref(),
        type_def.as_ref().map(AstNode::syntax),
        f.space_after_colon(),
        f,
    );
    let mut parts = vec![concat(head), f.colon_token(true), type_gap, body];
    if let Some(init) = decl.init() {
        let first = init.exprs().next();
        let (value_gap, value_breaks) = gap_after_token(
            init.assign_token().as_ref(),
            first.as_ref().map(AstNode::syntax),
            f.space_after_assign(),
            f,
        );
        let value = concat([
            value_gap,
            join(text(", "), init.exprs().map(|expr| format_expr(&expr, f))),
        ]);
        parts.push(f.assign_op_token(true));
        parts.push(if value_breaks { indent(value) } else { value });
    }
    // The `;` before `END_TYPE` is present for alias/subrange definitions but not
    // for struct/union/enum bodies, emit it only when the source had one. TwinCAT
    // also accepts one after `END_TYPE`; keep that one too.
    let end_type = token_of(decl.syntax(), SyntaxKind::END_TYPE_KW).map(|t| t.text_range());
    let semicolons: Vec<_> = decl
        .syntax()
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() == SyntaxKind::SEMICOLON)
        .map(|t| t.text_range())
        .collect();
    let after_end: Vec<bool> = semicolons
        .iter()
        .map(|range| end_type.is_some_and(|end| range.start() >= end.end()))
        .collect();
    if after_end.contains(&false) {
        parts.push(text(";"));
    }
    parts.push(hard_line());
    parts.push(f.kw("END_TYPE"));
    if after_end.contains(&true) {
        parts.push(text(";"));
    }
    parts.push(trailing_doc(
        f.comments.trailing_of(decl.syntax()),
        f.style.trailing_comment_min_spaces as usize,
    ));
    group_break(concat(parts))
}

// ---- types ----

fn format_type(ty: &Type, f: &FormatContext) -> Doc {
    match ty {
        Type::Ref(t) => format_type_ref(t, f),
        Type::String(t) => tokens_nospace(t.syntax(), f),
        Type::Subrange(t) => tokens_nospace(t.syntax(), f),
        Type::Array(t) => format_array(t, f),
        Type::Pointer(t) => format_pointer(t, f),
        Type::Reference(t) => format_reference(t, f),
        Type::Enum(t) => format_enum(t, f),
        Type::Struct(t) => format_struct(t, f),
        Type::Union(t) => format_union(t, f),
    }
}

fn format_type_ref(reference: &TypeRef, f: &FormatContext) -> Doc {
    use tctidy_syntax::SyntaxElement;

    let mut parts = Vec::new();
    let mut previous: Option<SyntaxToken> = None;
    for element in reference.syntax().children_with_tokens() {
        let (first, last, doc) = match element {
            SyntaxElement::Token(token) if !token.kind().is_trivia() => {
                (token.clone(), token.clone(), type_token(&token, f))
            }
            SyntaxElement::Node(child) if child.kind() == SyntaxKind::GENERIC_ARGS => {
                let Some(args) = GenericArgs::cast(child.clone()) else {
                    continue;
                };
                let Some(first) = first_sig_token(&child) else {
                    continue;
                };
                let Some(last) = last_sig_token(&child) else {
                    continue;
                };
                (first, last, format_generic_args(&args, f))
            }
            SyntaxElement::Node(child) => {
                let Some(expr) = Expr::cast(child.clone()) else {
                    continue;
                };
                let Some(first) = first_sig_token(&child) else {
                    continue;
                };
                let Some(last) = last_sig_token(&child) else {
                    continue;
                };
                (first, last, format_expr(&expr, f))
            }
            _ => continue,
        };

        if let Some(previous) = &previous {
            parts.push(
                gap_doc(
                    previous,
                    &first,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    "",
                )
                .0,
            );
        }
        parts.push(doc);
        previous = Some(last);
    }
    concat(parts)
}

fn format_generic_args(args: &GenericArgs, f: &FormatContext) -> Doc {
    let open = args
        .syntax()
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::LT);
    let close = args
        .syntax()
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::GT);
    let value = args.value();

    let mut parts = vec![text("<")];
    if let (Some(open), Some(value)) = (open.as_ref(), value.as_ref()) {
        if let Some(first) = first_sig_token(value.syntax()) {
            parts.push(
                gap_doc(
                    open,
                    &first,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    "",
                )
                .0,
            );
        }
        parts.push(format_expr(value, f));
    }
    if let (Some(value), Some(close)) = (value.as_ref(), close.as_ref()) {
        if let Some(last) = last_sig_token(value.syntax()) {
            parts.push(
                gap_doc(
                    &last,
                    close,
                    f.comments,
                    f.style.trailing_comment_min_spaces as usize,
                    f.style.blank_lines_max,
                    "",
                )
                .0,
            );
        }
    }
    parts.push(text(">"));
    concat(parts)
}

/// Render a type made only of names/keywords/punctuation and nested expressions
/// with no inserted spaces: dotted names, `STRING(n)`, `UINT(0..10)`, subranges.
fn tokens_nospace(node: &SyntaxNode, f: &FormatContext) -> Doc {
    let mut parts = Vec::new();
    for elem in node.children_with_tokens() {
        match elem {
            tctidy_syntax::SyntaxElement::Token(t) => {
                if t.kind().is_trivia() {
                    continue;
                }
                parts.push(type_token(&t, f));
            }
            tctidy_syntax::SyntaxElement::Node(n) => {
                if let Some(e) = tctidy_syntax::ast::Expr::cast(n) {
                    parts.push(format_expr(&e, f));
                }
            }
        }
    }
    concat(parts)
}

/// A single type-level token: elementary type keywords cased per
/// `type_keyword_case`, other keywords per `keyword_case`, rest verbatim.
fn type_token(tok: &tctidy_syntax::SyntaxToken, f: &FormatContext) -> Doc {
    let k = tok.kind();
    if is_type_keyword(k) {
        f.type_kw(tok.text())
    } else if k.is_keyword() {
        f.kw(tok.text())
    } else {
        text(tok.text().to_string())
    }
}

fn format_array(t: &ArrayType, f: &FormatContext) -> Doc {
    let dims = t
        .dims()
        .map(|d| {
            join(
                text(", "),
                d.dims().map(|dim| {
                    // A variable-length dimension (`ARRAY[*]`) has a `*` instead
                    // of a `lo..hi` range, reproduce it as-is.
                    if dim
                        .syntax()
                        .children_with_tokens()
                        .filter_map(|e| e.into_token())
                        .any(|t| t.kind() == SyntaxKind::STAR)
                    {
                        return text("*");
                    }
                    let lo = dim.lo().map(|e| format_expr(&e, f)).unwrap_or_else(nil);
                    let hi = dim.hi().map(|e| format_expr(&e, f)).unwrap_or_else(nil);
                    concat([lo, text(".."), hi])
                }),
            )
        })
        .unwrap_or_else(nil);
    let elem = t.elem().map(|e| format_type(&e, f)).unwrap_or_else(nil);
    concat([
        f.kw("ARRAY"),
        text("["),
        dims,
        text("] "),
        f.kw("OF"),
        text(" "),
        elem,
    ])
}

fn format_pointer(t: &PointerType, f: &FormatContext) -> Doc {
    let inner = t.inner().map(|e| format_type(&e, f)).unwrap_or_else(nil);
    concat([f.kw("POINTER"), text(" "), f.kw("TO"), text(" "), inner])
}

fn format_reference(t: &ReferenceType, f: &FormatContext) -> Doc {
    let inner = t.inner().map(|e| format_type(&e, f)).unwrap_or_else(nil);
    concat([f.kw("REFERENCE"), text(" "), f.kw("TO"), text(" "), inner])
}

fn format_struct(t: &StructType, f: &FormatContext) -> Doc {
    let body = format_members(
        t.members().collect(),
        t.syntax(),
        SyntaxKind::END_STRUCT_KW,
        f,
    );
    group_break(concat([
        f.kw("STRUCT"),
        body,
        hard_line(),
        f.kw("END_STRUCT"),
    ]))
}

fn format_union(t: &UnionType, f: &FormatContext) -> Doc {
    let body = format_members(
        t.members().collect(),
        t.syntax(),
        SyntaxKind::END_UNION_KW,
        f,
    );
    group_break(concat([
        f.kw("UNION"),
        body,
        hard_line(),
        f.kw("END_UNION"),
    ]))
}

/// An enum body: one variant per line, comma-separated (a trailing comma after
/// the last variant follows `StyleConfig::trailing_comma_enum`; house style:
/// none), followed by the optional base type. `:=` initializers are not aligned.
fn format_enum(t: &EnumType, f: &FormatContext) -> Doc {
    let cap = f.style.blank_lines_max;
    let min = f.style.trailing_comment_min_spaces as usize;
    let variants: Vec<EnumVariant> = t.variants().collect();

    // The `:=` is aligned per contiguous run of init-bearing variants; a run
    // breaks at a bare variant, a leading comment, or a blank line. The column is
    // the widest name in the run (so an isolated init keeps a single space).
    let assign_cols = enum_assign_columns(&variants, f);

    let mut body = Vec::new();
    for (i, v) in variants.iter().enumerate() {
        let node = v.syntax();
        body.push(hard_line());
        // Leading comments and blank-line separation above the variant.
        let lead = f.comments.leading_of(node);
        let blank = if lead.is_empty() {
            f.comments.blanks_before_of(node)
        } else {
            lead.first().map_or(0, |lp| lp.blank_before)
        };
        for _ in 0..blank.min(cap) {
            body.push(hard_line());
        }
        body.push(leading_doc(lead, cap));
        body.push(variant_line(v, assign_cols[i], f));

        let is_last = i + 1 == variants.len();
        if !is_last || f.emit_trailing_sep(f.style.trailing_comma_enum) {
            // The separating comma is usually inline, but the source sometimes
            // puts it on its own line (at the variant indent), preserve that.
            if enum_comma_on_new_line(v) {
                body.push(hard_line());
                body.push(text(","));
            } else {
                body.push(text(","));
            }
        }
        // The trailing comment sits after the comma (so it binds to the comma
        // token), except on the last variant where it binds to the variant.
        let trailing = if is_last {
            f.comments.trailing_of_last_item(node)
        } else {
            next_comma(node)
                .map(|comma| f.comments.trailing(&comma).to_vec())
                .unwrap_or_default()
        };
        body.push(trailing_doc(&trailing, min));
    }

    body.push(dangling_inner(t.syntax(), SyntaxKind::R_PAREN, f));
    let mut parts = vec![text("("), indent(concat(body)), hard_line(), text(")")];
    if let Some(base) = t.base() {
        parts.push(text(" "));
        parts.push(format_type(&base, f));
    }
    group_break(concat(parts))
}

/// The variant's `:=` (and value) is on a continuation line (`Name \n  := v`).
fn enum_assign_on_new_line(v: &EnumVariant) -> bool {
    v.init().is_some_and(|i| starts_on_new_line(i.syntax()))
}

/// The comma separating this variant from the next is on its own line
/// (`Name := v \n  , // comment`).
fn enum_comma_on_new_line(v: &EnumVariant) -> bool {
    next_comma(v.syntax()).is_some_and(|c| token_after_newline(&c))
}

/// Whether variant `i` begins a new `:=` alignment group: the usual blank/comment
/// break, plus a variant whose `:=` is on its own line (isolated), plus the
/// variant after any wrapped variant.
fn enum_group_break(variants: &[EnumVariant], i: usize, f: &FormatContext) -> bool {
    group_breaks(variants[i].syntax(), f)
        || enum_assign_on_new_line(&variants[i])
        || (i > 0
            && (enum_assign_on_new_line(&variants[i - 1])
                || enum_comma_on_new_line(&variants[i - 1])))
}

/// The `:=` column for each enum variant: the max name width within its
/// contiguous run of init-bearing variants (0 for bare variants). A run breaks
/// at a bare variant, a leading comment, a blank line, or a wrapped variant.
fn enum_assign_columns(variants: &[EnumVariant], f: &FormatContext) -> Vec<usize> {
    let n = variants.len();
    let mut cols = vec![0usize; n];
    let mut i = 0;
    while i < n {
        if variants[i].init().is_none() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && variants[j].init().is_some() && (j == i || !enum_group_break(variants, j, f))
        {
            j += 1;
        }
        let max = variants[i..j]
            .iter()
            .map(|v| v.name().map_or(0, |t| display_width(t.text())))
            .max()
            .unwrap_or(0);
        cols[i..j].iter_mut().for_each(|c| *c = max);
        i = j;
    }
    cols
}

/// Whether `node` starts a new alignment group (has a leading comment or a blank
/// line above it). Only a blank line that survives `blank_lines_max` counts: a
/// removed one must not split the group, or the next pass would merge it.
fn group_breaks(node: &SyntaxNode, f: &FormatContext) -> bool {
    !f.comments.leading_of(node).is_empty()
        || f.comments
            .blanks_before_of(node)
            .min(f.style.blank_lines_max)
            > 0
}

/// Whether declaration `i` begins a new alignment group. Within a `STRUCT`,
/// named structure initializers align with their peers, separately from scalar
/// members. A member whose `:` is on its own line is isolated from both runs.
fn group_break_at(
    decls: &[VarDecl],
    i: usize,
    f: &FormatContext,
    separate_named_initializers: bool,
) -> bool {
    group_breaks(decls[i].syntax(), f)
        || (separate_named_initializers
            && i > 0
            && has_named_initializer(&decls[i - 1]) != has_named_initializer(&decls[i]))
        // A name written alone on its line (`:` on the continuation) is isolated.
        || colon_on_new_line(&decls[i])
        // Any wrapped member ends its group: the next member starts a new one.
        || (i > 0 && member_wraps(&decls[i - 1], f))
}

fn has_named_initializer(decl: &VarDecl) -> bool {
    decl.init()
        .and_then(|init| init.exprs().next())
        .is_some_and(|expr| {
            matches!(expr, Expr::Paren(_))
                && expr
                    .syntax()
                    .children()
                    .any(|child| NamedArg::cast(child).is_some())
        })
}

fn variant_line(v: &EnumVariant, assign_col: usize, f: &FormatContext) -> Doc {
    let name = v.name().map(|t| t.text().to_string()).unwrap_or_default();
    let Some(init) = v.init() else {
        return text(name);
    };
    let value = join(text(", "), init.exprs().map(|e| format_expr(&e, f)));
    let first = init.exprs().next();
    let (value_gap, value_breaks) = gap_after_token(
        init.assign_token().as_ref(),
        first.as_ref().map(AstNode::syntax),
        f.space_after_assign(),
        f,
    );
    let value = concat([value_gap, value]);

    // `:=` on its own (continuation) line: name alone, then `:= value` at one
    // extra indent. Not aligned.
    if enum_assign_on_new_line(v) {
        let value = if value_breaks { indent(value) } else { value };
        return concat([
            text(name),
            indent(concat([hard_line(), f.assign_op_token(false), value])),
        ]);
    }

    let pad = assign_col.saturating_sub(display_width(&name));
    concat([
        text(name),
        text(" ".repeat(pad)),
        f.assign_op_token(true),
        if value_breaks { indent(value) } else { value },
    ])
}

/// The `COMMA` token immediately following `node` among its siblings (skipping
/// trivia), if any.
fn next_comma(node: &SyntaxNode) -> Option<tctidy_syntax::SyntaxToken> {
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

// ---- helpers ----

fn gap_after_token(
    previous: Option<&SyntaxToken>,
    next: Option<&SyntaxNode>,
    inline_prefix: &str,
    f: &FormatContext,
) -> (Doc, bool) {
    match (previous, next.and_then(first_sig_token)) {
        (Some(previous), Some(next)) => gap_doc(
            previous,
            &next,
            f.comments,
            f.style.trailing_comment_min_spaces as usize,
            f.style.blank_lines_max,
            inline_prefix,
        ),
        _ => (text(inline_prefix.to_string()), false),
    }
}

fn verbatim_node(node: &SyntaxNode) -> Doc {
    verbatim(&node.text().to_string())
}

/// Comments and blank lines between the last member and a container's closing
/// token (`END_STRUCT`/`END_VAR`/`)`), attached as that token's leading trivia.
/// Returned content is meant to live **inside** the members' indent (so dangling
/// comments line up with the members); each piece is prefixed by a hard break and
/// none is appended, so the closer stays on the next, dedented line.
fn dangling_inner(node: &SyntaxNode, end_kind: SyntaxKind, f: &FormatContext) -> Doc {
    let cap = f.style.blank_lines_max;
    let Some(end_tok) = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == end_kind)
    else {
        return nil();
    };
    let lead = f.comments.leading(&end_tok);
    let lead_blank = if lead.is_empty() {
        f.comments.blanks_before(&end_tok)
    } else {
        lead.first().map_or(0, |lp| lp.blank_before)
    };

    let mut parts = Vec::new();
    for _ in 0..f.style.cap_block_end(lead_blank) {
        parts.push(hard_line());
    }
    for (i, lp) in lead.iter().enumerate() {
        if i > 0 {
            for _ in 0..lp.blank_before.min(cap) {
                parts.push(hard_line());
            }
        }
        parts.push(hard_line());
        parts.push(verbatim(&lp.piece.text));
    }
    concat(parts)
}

fn is_type_keyword(k: SyntaxKind) -> bool {
    tctidy_syntax::kind::keywords::is_type_keyword(k)
}

/// Whether `node`'s first token is preceded (across trivia) by a newline, i.e.
/// the construct begins on its own line in the source.
fn starts_on_new_line(node: &SyntaxNode) -> bool {
    crate::trivia::first_sig_token(node).is_some_and(|t| token_after_newline(&t))
}

/// Whether a declaration's `:` sits on its own (continuation) line, the name was
/// written alone on the first line. Such a member breaks colon alignment.
fn colon_on_new_line(decl: &VarDecl) -> bool {
    decl.colon().is_some_and(|t| token_after_newline(&t))
}

/// The type is on a continuation line but the `:` stayed on the first line
/// (`name : \n    type`).
fn wrap_after_colon(decl: &VarDecl) -> bool {
    !colon_on_new_line(decl) && decl.ty().is_some_and(|t| starts_on_new_line(t.syntax()))
}

/// The initializer value is on a continuation line (`name : type := \n    value`).
fn wrap_before_init(decl: &VarDecl) -> bool {
    decl.init()
        .and_then(|i| i.exprs().next())
        .is_some_and(|e| starts_on_new_line(e.syntax()))
}

/// Whether the declaration spans more than one line (any continuation). A wrapped
/// member ends its alignment group. A name list carrying comments is rendered
/// specially (see `decl_line`) and is likewise isolated from column alignment.
fn member_wraps(decl: &VarDecl, f: &FormatContext) -> bool {
    colon_on_new_line(decl)
        || wrap_after_colon(decl)
        || wrap_before_init(decl)
        || name_list_comments(decl, f)
}

/// Whether `node` has a direct child token of the given kind.
fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    token_of(node, kind).is_some()
}

/// The first direct child token of the given kind.
fn token_of(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}
