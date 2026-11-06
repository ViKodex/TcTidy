//! The raw `logos` token machine.
//!
//! `logos` recognizes token *shapes*; [`super::remap`] assigns the final
//! [`crate::kind::SyntaxKind`]. The variable-length, context-sensitive shapes,
//! nested `(* *)` / `/* */` comments, brace-matched multi-line pragmas, and
//! `$`-escaped strings, are matched by the hand-written scanners in
//! [`super::callbacks`], invoked here as `logos` callbacks that extend the match
//! via `lex.bump(..)`.
//!
//! The `logos` derive expands to `unsafe` code; this is the only place it is
//! permitted in the crate.
#![allow(unsafe_code)]

use logos::{Lexer, Logos};

use super::callbacks::{scan_c_block_comment, scan_paren_block_comment, scan_pragma, scan_string};

fn paren_comment(lex: &mut Lexer<LogosTok>) {
    let n = scan_paren_block_comment(lex.remainder());
    lex.bump(n);
}
fn c_block_comment(lex: &mut Lexer<LogosTok>) {
    let n = scan_c_block_comment(lex.remainder());
    lex.bump(n);
}
fn pragma(lex: &mut Lexer<LogosTok>) {
    let n = scan_pragma(lex.remainder());
    lex.bump(n);
}
fn sq_string(lex: &mut Lexer<LogosTok>) {
    let n = scan_string(lex.remainder(), b'\'');
    lex.bump(n);
}
fn dq_string(lex: &mut Lexer<LogosTok>) {
    let n = scan_string(lex.remainder(), b'"');
    lex.bump(n);
}
fn typed_unicode_string(lex: &mut Lexer<LogosTok>) {
    let n = scan_string(lex.remainder(), b'\'');
    lex.bump(n);
}

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogosTok {
    // --- trivia (kept, never skipped) ---
    #[regex(r"[ \t]+")]
    Whitespace,
    #[regex(r"\r\n|\n|\r")]
    Newline,
    // The repetition stops at the end of the line, not at the end of the input.
    #[regex(r"//[^\n\r]*", allow_greedy = true)]
    LineComment,
    #[token("(*", paren_comment)]
    BlockComment,
    #[token("/*", c_block_comment)]
    CBlockComment,
    #[token("{", pragma)]
    Pragma,

    // --- strings ($-escaped; callbacks find the closing quote) ---
    #[regex(r"([Uu][Tt][Ff]8|[Uu][Cc][Hh][Aa][Rr])#'", typed_unicode_string)]
    TypedUnicodeString,
    #[token("'", sq_string)]
    SqString,
    #[token("\"", dq_string)]
    DqString,

    // --- numbers & literals (longest match wins over DecInt / Ident) ---
    #[regex(r"[0-9][0-9_]*#[0-9A-Fa-f][0-9A-Fa-f_]*")]
    BasedInt,
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9]+)?")]
    Real,
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9]+")]
    RealExp,
    #[regex(r"[0-9][0-9_]*")]
    DecInt,
    // typed / time / date literal: `<word>#<value>` (e.g. INT#100, T#1d2h, DT#...).
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*#[0-9A-Za-z_.+\-]*(?::[0-9A-Za-z_.+\-]+)*")]
    HashLit,
    // located variable address: %IX0.0, %QW10, %MD100, %I*, %Q*.
    #[regex(r"%[IQM][XBWDL]?[0-9.]*\*?")]
    DirectAddress,

    // --- identifier shape (ASCII) ---
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,

    // --- multi-char operators before single-char (longest match) ---
    #[token(":=")]
    Assign,
    // IEC assignment operators (longest-match beats the `Ident` regex on the
    // leading `REF`/`S`/`R` because of the trailing `=`).
    #[token("REF=")]
    RefAssign,
    #[token("S=")]
    SetAssign,
    #[token("R=")]
    ResetAssign,
    #[token("=>")]
    Arrow,
    #[token("<>")]
    Neq,
    #[token("<=")]
    Lte,
    #[token(">=")]
    Gte,
    #[token("**")]
    Pow,
    #[token("..")]
    DotDot,

    // --- single-char punctuation / operators ---
    #[token(";")]
    Semi,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBrack,
    #[token("]")]
    RBrack,
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("&")]
    Amp,
    #[token("^")]
    Caret,
}
