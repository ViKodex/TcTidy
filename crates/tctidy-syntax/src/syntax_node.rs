//! Concrete `rowan` tree types specialized to [`TcStLang`].

use crate::lang::TcStLang;

/// An interior node of the lossless tree.
pub type SyntaxNode = rowan::SyntaxNode<TcStLang>;
/// A leaf token (including trivia and pragmas).
pub type SyntaxToken = rowan::SyntaxToken<TcStLang>;
/// Either a node or a token.
pub type SyntaxElement = rowan::SyntaxElement<TcStLang>;
