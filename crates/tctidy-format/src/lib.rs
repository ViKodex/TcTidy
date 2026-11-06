//! `tctidy-format`, lowers the ST typed AST to the [`tctidy_pretty`] IR.
//!
//! This crate is where Structured Text meets layout. It owns:
//!
//! * the centralized [`StyleConfig`], which stores the house defaults and every
//!   style setting; [`FormatContext`] exposes the spacing and casing helpers the
//!   rules share;
//! * the comment-attachment pass ([`trivia`]) that guarantees no comment is lost
//!   or reordered;
//! * the per-construct lowering rules ([`rules`]).
//!
//! The two entry points mirror the two ST grammars (declaration vs body).
#![forbid(unsafe_code)]

pub mod options;
pub mod style;

mod context;
mod entry;
mod error;
mod keyword;
mod rules;
mod traits;
mod trivia;
mod verbatim;

pub use context::FormatContext;
pub use error::FormatError;
pub use style::StyleConfig;
pub use traits::{Format, FormatResult, FormatRule};

use tctidy_pretty::{print, LineEnding};
use tctidy_syntax::ast::{DeclPart, ImplPart};
use tctidy_syntax::tokens::lex;
use tctidy_syntax::{Parse, SyntaxKind};

/// Format a parsed `<Declaration>` section into text.
///
/// Refuses (returns `Err`) if the parse carries any error, per the
/// non-destruction rule the caller then leaves the original untouched.
pub fn format_declaration(
    parse: &Parse<DeclPart>,
    style: &StyleConfig,
    eol: LineEnding,
) -> Result<String, FormatError> {
    format_root(parse, style, eol)
}

/// Format a parsed `<Implementation><ST>` body into text.
pub fn format_implementation(
    parse: &Parse<ImplPart>,
    style: &StyleConfig,
    eol: LineEnding,
) -> Result<String, FormatError> {
    format_root(parse, style, eol)
}

fn format_root<T: tctidy_syntax::ast::AstNode>(
    parse: &Parse<T>,
    style: &StyleConfig,
    eol: LineEnding,
) -> Result<String, FormatError> {
    if !parse.errors().is_empty() {
        return Err(FormatError::Parse(parse.errors().to_vec()));
    }
    let mut comments = trivia::Comments::attach(&parse.syntax_node());
    // Trailing-whitespace trim is unconditional; the space-after-`//` rule is the
    // configurable part (passed through).
    comments.normalize_line_comments(style.space_after_line_comment);
    let mut ctx = FormatContext::new(style, &comments);
    let doc = entry::format_node(&parse.syntax_node(), &mut ctx)?;
    let out = print(&doc, &style.print_options(eol));
    verify_trivia_kept(&parse.syntax_node(), &out)?;
    Ok(out)
}

/// The last line of defense for the non-destruction rule: every comment and
/// pragma of the input must appear in the output, in order (whitespace aside,
/// which the line-comment rule may normalize). A rule that fails to print one
/// is a bug; refusing here turns it into an error that leaves the input
/// untouched instead of silently deleting the comment.
fn verify_trivia_kept(root: &tctidy_syntax::SyntaxNode, out: &str) -> Result<(), FormatError> {
    fn is_kept_trivia(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
                | SyntaxKind::C_BLOCK_COMMENT
                | SyntaxKind::PRAGMA
        )
    }
    fn squash(text: &str) -> String {
        text.split_whitespace().collect()
    }
    let before: Vec<(String, String)> = root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| is_kept_trivia(t.kind()))
        .map(|t| (squash(t.text()), t.text().to_string()))
        .collect();
    let mut after = Vec::new();
    let mut offset = 0usize;
    for token in lex(out) {
        let len = usize::from(token.len);
        if is_kept_trivia(token.kind) {
            after.push(squash(&out[offset..offset + len]));
        }
        offset += len;
    }
    let mut remaining = after.iter();
    for (key, original) in &before {
        if !remaining.any(|kept| kept == key) {
            let shown: String = original.chars().take(60).collect();
            return Err(FormatError::Internal(format!(
                "refusing to format: the comment or pragma `{shown}` would be lost; \
                 the input was left unchanged (please report this)"
            )));
        }
    }
    Ok(())
}
