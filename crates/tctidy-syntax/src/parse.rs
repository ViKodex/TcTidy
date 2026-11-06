//! [`Parse<T>`], the result of parsing: a green tree plus any errors.
//!
//! A `Parse` is always produced (resilience), even when `errors()` is non-empty.
//! The typed root is obtained via [`Parse::tree`]; the lossless check
//! [`Parse::debug_text_matches`] is the one-line statement of the whole promise.

use std::marker::PhantomData;
use std::sync::Arc;

use rowan::GreenNode;

use crate::ast::AstNode;
use crate::error::SyntaxError;
use crate::syntax_node::SyntaxNode;

/// Parse result for a tree whose root casts to `T`.
#[derive(Debug, Clone)]
pub struct Parse<T> {
    green: GreenNode,
    errors: Arc<[SyntaxError]>,
    _ty: PhantomData<fn() -> T>,
}

impl<T: AstNode> Parse<T> {
    pub(crate) fn new(green: GreenNode, errors: Vec<SyntaxError>) -> Self {
        Parse {
            green,
            errors: errors.into(),
            _ty: PhantomData,
        }
    }

    /// The untyped root node.
    pub fn syntax_node(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The typed root (the root cast is total by construction).
    pub fn tree(&self) -> T {
        T::cast(self.syntax_node()).expect("root node should cast to the parse's root type")
    }

    /// Non-fatal errors collected during parsing.
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    /// `Ok(tree)` if error-free, else the errors. Used by callers that must
    /// refuse to format on any error.
    pub fn ok(self) -> Result<T, Arc<[SyntaxError]>> {
        if self.errors.is_empty() {
            Ok(self.tree())
        } else {
            Err(self.errors)
        }
    }

    /// The lossless invariant: the tree re-renders to exactly the input.
    pub fn debug_text_matches(&self, src: &str) -> bool {
        self.syntax_node().to_string() == src
    }
}
