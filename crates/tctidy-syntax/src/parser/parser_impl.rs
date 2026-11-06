//! The hand-written recursive-descent parser.
//!
//! Direct-build: the parser walks the
//! significant (non-trivia) tokens and drives a `GreenNodeBuilder` directly,
//! using `checkpoint`/`start_node_at` for left-associative constructs. Trivia
//! (whitespace, newlines, comments, pragmas) is emitted verbatim around the
//! significant tokens it precedes, so the tree is lossless by construction:
//! every raw token is emitted exactly once with its original text, hence
//! `tree.to_string() == src`.
//!
//! Resilience: there is no recursion-free guarantee needed because every loop
//! advances by at least one token (callers use `err_and_bump` on no progress),
//! and lookahead past EOF yields `EOF`, so parsing always terminates and never
//! panics. Unmodelled input becomes `ERROR` nodes; the caller (the formatter)
//! refuses to format when any error is present, leaving the file untouched.

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder};
use text_size::{TextRange, TextSize};

use crate::error::{ErrorKind, SyntaxError};
use crate::kind::SyntaxKind::{self, EOF, ERROR};
use crate::lexer::LexToken;

pub(crate) struct Parser<'a> {
    text: &'a str,
    tokens: &'a [LexToken],
    /// Indices into `tokens` of the significant (non-trivia) tokens.
    sig: Vec<usize>,
    /// Cursor into `sig`.
    pos: usize,
    /// Next raw token index to emit.
    raw_pos: usize,
    /// Byte offset of `tokens[raw_pos]`.
    byte_pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(text: &'a str, tokens: &'a [LexToken]) -> Self {
        let sig = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.kind.is_trivia())
            .map(|(i, _)| i)
            .collect();
        Parser {
            text,
            tokens,
            sig,
            pos: 0,
            raw_pos: 0,
            byte_pos: 0,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
        }
    }

    // --- lookahead over significant tokens ---

    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        match self.sig.get(self.pos + n) {
            Some(&i) => self.tokens[i].kind,
            None => EOF,
        }
    }
    pub(crate) fn current(&self) -> SyntaxKind {
        self.nth(0)
    }
    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }
    pub(crate) fn at_any(&self, set: &[SyntaxKind]) -> bool {
        set.contains(&self.current())
    }
    pub(crate) fn at_eof(&self) -> bool {
        self.pos >= self.sig.len()
    }
    /// Cursor position over significant tokens, for loop progress checks.
    pub(crate) fn at_pos(&self) -> usize {
        self.pos
    }

    // --- low-level token emission ---

    fn emit_raw(&mut self) {
        let t = self.tokens[self.raw_pos];
        let len = u32::from(t.len) as usize;
        let text = &self.text[self.byte_pos..self.byte_pos + len];
        self.builder.token(t.kind.into_raw(), text);
        self.byte_pos += len;
        self.raw_pos += 1;
    }

    /// Emit trivia tokens up to (not including) the next significant token.
    fn emit_leading_trivia(&mut self) {
        let target = self.sig.get(self.pos).copied().unwrap_or(self.tokens.len());
        while self.raw_pos < target {
            self.emit_raw();
        }
    }

    // --- consumption ---

    /// Consume the current significant token (with any preceding trivia).
    pub(crate) fn bump(&mut self) {
        self.emit_leading_trivia();
        if self.pos < self.sig.len() {
            debug_assert_eq!(self.raw_pos, self.sig[self.pos]);
            self.emit_raw();
            self.pos += 1;
        }
    }
    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }
    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error(ErrorKind::Expected, format!("expected {kind:?}"));
            false
        }
    }

    // --- nodes ---

    pub(crate) fn start(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into_raw());
    }
    pub(crate) fn finish_node(&mut self) {
        self.builder.finish_node();
    }
    pub(crate) fn checkpoint(&self) -> Checkpoint {
        self.builder.checkpoint()
    }
    pub(crate) fn start_at(&mut self, cp: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(cp, kind.into_raw());
    }

    // --- errors / recovery ---

    pub(crate) fn error(&mut self, kind: ErrorKind, msg: impl Into<String>) {
        let at = TextSize::from(self.byte_pos as u32);
        self.errors
            .push(SyntaxError::new(kind, msg, TextRange::empty(at)));
    }

    /// Wrap the current significant token in an `ERROR` node and advance. A no-op
    /// at EOF (callers must check progress / EOF to terminate).
    pub(crate) fn err_and_bump(&mut self, msg: impl Into<String>) {
        if self.at_eof() {
            self.error(ErrorKind::Unexpected, msg);
            return;
        }
        self.start(ERROR);
        self.error(ErrorKind::Unexpected, msg);
        self.bump();
        self.finish_node();
    }

    // --- finishing ---

    /// Emit any trailing trivia (after the last significant token).
    pub(crate) fn emit_remaining(&mut self) {
        while self.raw_pos < self.tokens.len() {
            self.emit_raw();
        }
    }
    pub(crate) fn into_green(self) -> (GreenNode, Vec<SyntaxError>) {
        (self.builder.finish(), self.errors)
    }
}
