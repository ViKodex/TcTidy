//! `tctidy-syntax`, bytes to a lossless concrete syntax tree to a typed AST.
//!
//! This crate is the parsing front-end. It is deliberately ignorant of
//! formatting: its sole job is to turn Structured Text into a tree that
//! preserves **every** byte (whitespace, newlines, all comment syntaxes, and
//! pragmas are tokens living in the tree) and to expose convenient typed
//! accessors over it.
//!
//! Two invariants are guaranteed here by construction and checked in tests:
//!
//! * **Lossless round-trip**, `parse(x).syntax_node().to_string() == x`.
//! * **Totality**, parsing never panics; unparsable input yields a tree with
//!   `ERROR` nodes, and the caller decides whether to proceed.
//!
//! Pipeline: [`lexer`] (bytes → flat tokens) → [`parser`] (tokens → events →
//! `rowan` green tree, with trivia re-attached) → [`ast`] (typed views).
//!
//! `deny` rather than `forbid` for unsafe: the `logos` derive expands to
//! `unsafe` code in the lexer module, which carries a narrow `allow`. No
//! hand-written `unsafe` is permitted anywhere in this crate.
#![deny(unsafe_code)]

pub mod ast;
pub mod diagnostics;
pub mod kind;

mod error;
mod lang;
mod lexer;
mod parse;
mod parser;
mod syntax_node;

pub use error::{ErrorKind, SyntaxError};
pub use kind::SyntaxKind;
pub use lang::TcStLang;
pub use parse::Parse;
pub use parser::{parse_declaration, parse_implementation, parse_source_file};
pub use syntax_node::{SyntaxElement, SyntaxNode, SyntaxToken};

/// Raw token access for tooling (syntax highlighting, range formatting).
pub mod tokens {
    pub use crate::lexer::{lex, LexToken};
}
