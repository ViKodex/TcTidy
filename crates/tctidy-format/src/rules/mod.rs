//! Per-construct lowering rules: AST node → `Doc`.
//!
//! Each rule is pure (AST → Doc) and emits style only through
//! [`crate::context::FormatContext`] helpers. The full set mirrors the grammar:
//! POU headers, declaration sections (with the `:=` alignment option), type
//! definitions, the statement list and every control structure, assignments,
//! calls, expressions (operator precedence + parenthesis preservation),
//! literals (verbatim), member/index/deref access, and pragmas (verbatim).
//!
//! Pragmas/attributes are handled as trivia (see [`crate::trivia`]) and emitted
//! verbatim, so there is no dedicated pragma rule.

pub(crate) mod decl_section;
pub(crate) mod expr;
pub(crate) mod stmt;
