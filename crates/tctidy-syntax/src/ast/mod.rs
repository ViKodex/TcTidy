//! Typed, read-only views over the untyped `rowan` tree.
//!
//! Each wrapper is a newtype around a [`SyntaxNode`]/[`SyntaxToken`] with
//! `Option`-returning accessors, following the rust-analyzer pattern. Casting is
//! zero-copy. The [`ast_node`]/[`ast_token`] macros remove the boilerplate.

use crate::kind::SyntaxKind;
use crate::syntax_node::{SyntaxNode, SyntaxToken};

mod nodes;
mod operators;
mod tokens;

pub use nodes::*;
pub use operators::*;
pub use tokens::*;

/// A typed view over an interior node.
pub trait AstNode {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;
    fn cast(node: SyntaxNode) -> Option<Self>
    where
        Self: Sized;
    fn syntax(&self) -> &SyntaxNode;
}

/// A typed view over a leaf token.
pub trait AstToken {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;
    fn cast(token: SyntaxToken) -> Option<Self>
    where
        Self: Sized;
    fn syntax(&self) -> &SyntaxToken;
    fn text(&self) -> &str {
        self.syntax().text()
    }
}

/// First child node castable to `N`.
pub(crate) fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    parent.children().find_map(N::cast)
}

/// All child nodes castable to `N`, in order.
pub(crate) fn children<N: AstNode>(parent: &SyntaxNode) -> impl Iterator<Item = N> {
    parent.children().filter_map(N::cast)
}

/// First direct child token of the given `kind`.
pub(crate) fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name($crate::syntax_node::SyntaxNode);

        impl $crate::ast::AstNode for $name {
            fn can_cast(kind: $crate::kind::SyntaxKind) -> bool {
                kind == $crate::kind::SyntaxKind::$kind
            }
            fn cast(node: $crate::syntax_node::SyntaxNode) -> ::core::option::Option<Self> {
                if <Self as $crate::ast::AstNode>::can_cast(node.kind()) {
                    ::core::option::Option::Some($name(node))
                } else {
                    ::core::option::Option::None
                }
            }
            fn syntax(&self) -> &$crate::syntax_node::SyntaxNode {
                &self.0
            }
        }
    };
}

macro_rules! ast_token {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name($crate::syntax_node::SyntaxToken);

        impl $crate::ast::AstToken for $name {
            fn can_cast(kind: $crate::kind::SyntaxKind) -> bool {
                kind == $crate::kind::SyntaxKind::$kind
            }
            fn cast(token: $crate::syntax_node::SyntaxToken) -> ::core::option::Option<Self> {
                if <Self as $crate::ast::AstToken>::can_cast(token.kind()) {
                    ::core::option::Option::Some($name(token))
                } else {
                    ::core::option::Option::None
                }
            }
            fn syntax(&self) -> &$crate::syntax_node::SyntaxToken {
                &self.0
            }
        }
    };
}

pub(crate) use {ast_node, ast_token};
