//! The formatting traits.
//!
//! A [`FormatRule`] lowers one AST node category to a `Doc` and contains **no**
//! comment logic. Dispatch and comment-wrapping happen in [`crate::entry`],
//! which looks up the rule for a node's kind, wraps the result with that node's
//! leading/trailing/dangling comments, and falls back to verbatim text for
//! `ERROR` (and not-yet-handled) nodes, the totality lever.

use tctidy_pretty::Doc;

use crate::context::FormatContext;
use crate::error::FormatError;

/// Result of lowering: a layout document, or a refusal.
pub type FormatResult = Result<Doc, FormatError>;

/// Lowers one node category to a `Doc`. Pure AST → Doc.
pub trait FormatRule {
    type Node;
    fn fmt(node: &Self::Node, f: &mut FormatContext) -> FormatResult;
}

/// The uniform entry every node goes through (comments handled around it).
///
/// Implemented for nodes via [`crate::entry`] dispatch rather than a blanket
/// impl, because the rule for a node is selected by its `SyntaxKind` at runtime.
pub trait Format {
    fn format(&self, f: &mut FormatContext) -> FormatResult;
}
