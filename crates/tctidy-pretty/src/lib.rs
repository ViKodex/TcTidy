//! `tctidy-pretty`, a generic, language-agnostic pretty-printer.
//!
//! This crate implements a group-based document IR ([`Doc`]) and a
//! printing engine ([`print`]) that decides where to break lines based on the
//! available width. It has **zero** knowledge of Structured Text (or any other
//! language): callers build a [`Doc`] and hand it to [`print`].
//!
//! Keeping the IR and the line-breaking algorithm in their own crate means they
//! can be unit-tested in complete isolation from the parser and the ST rules,
//! and it localizes every line-breaking decision in one place.
#![forbid(unsafe_code)]

pub mod builders;
pub mod doc;
pub mod options;
pub mod print;

mod fits;

#[cfg(any(test, feature = "debug"))]
pub mod debug;

pub use doc::{Doc, GroupId, GroupIdBuilder, IndentSpec, LineKind};
pub use options::{IndentStyle, LineEnding, PrintOptions};
pub use print::print;

/// Convenience re-export of every builder, so downstream lowering code can write
/// `use tctidy_pretty::prelude::*;` and compose documents declaratively.
pub mod prelude {
    pub use crate::builders::*;
    pub use crate::doc::{Doc, GroupId, GroupIdBuilder, IndentSpec, LineKind};
}
