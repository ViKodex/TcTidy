//! Node dispatch + uniform comment wrapping.
//!
//! [`format_node`] matches a node's [`SyntaxKind`] and calls the matching
//! [`crate::traits::FormatRule`], wrapping the result with the node's
//! leading/trailing/dangling comments. `ERROR` nodes, and any kind not yet
//! handled, fall back to **verbatim** source text, which is the totality lever:
//! anything the formatter does not understand is reproduced byte-for-byte rather than dropped.
//!
use tctidy_syntax::{SyntaxKind, SyntaxNode};

use crate::context::FormatContext;
use crate::rules::decl_section::format_decl_part;
use crate::rules::stmt::format_impl_part;
use crate::traits::FormatResult;
use crate::verbatim::verbatim;

/// Lower a tree root to a `Doc`: the declaration part or the implementation body
/// (both have real rules). Any unrecognized kind falls back to verbatim source.
pub(crate) fn format_node(node: &SyntaxNode, f: &mut FormatContext) -> FormatResult {
    let doc = match node.kind() {
        SyntaxKind::DECL_PART => format_decl_part(node, f),
        SyntaxKind::IMPL_PART | SyntaxKind::SOURCE_FILE => format_impl_part(node, f),
        _ => verbatim(&node.text().to_string()),
    };
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::StyleConfig;
    use crate::trivia::Comments;
    use tctidy_pretty::{print, LineEnding};

    /// The totality lever: an ERROR node reproduces its source byte-for-byte.
    /// Unreachable through the public entry points (they refuse a parse with
    /// errors), so exercised directly.
    #[test]
    fn error_node_falls_back_to_verbatim() {
        let src = "IF ?? mangled ;;\nEND_WHAT";
        let parse = tctidy_syntax::parse_implementation(src);
        assert!(!parse.errors().is_empty(), "input must be broken");

        let error_node = parse
            .syntax_node()
            .descendants()
            .find(|n| n.kind() == SyntaxKind::ERROR)
            .expect("broken input must produce an ERROR node");

        let style = StyleConfig::default();
        let comments = Comments::attach(&error_node);
        let mut ctx = FormatContext::new(&style, &comments);
        let doc = format_node(&error_node, &mut ctx).expect("verbatim fallback is total");

        let out = print(&doc, &style.print_options(LineEnding::Lf));
        assert_eq!(out, error_node.text().to_string());
    }
}
