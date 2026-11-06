//! [`FormatContext`], carries the style and comment state through lowering, and
//! exposes helpers for the spacing and casing settings the rules share.

use std::cell::RefCell;

use tctidy_pretty::prelude::*;

use crate::keyword::apply_keyword_case;
use crate::style::{StyleConfig, TrailingSeparator};
use crate::trivia::Comments;

/// Mutable context threaded through the lowering of a single tree.
pub struct FormatContext<'a> {
    pub style: &'a StyleConfig,
    pub comments: &'a Comments,
    group_ids: RefCell<GroupIdBuilder>,
}

impl<'a> FormatContext<'a> {
    pub fn new(style: &'a StyleConfig, comments: &'a Comments) -> Self {
        FormatContext {
            style,
            comments,
            group_ids: RefCell::new(GroupIdBuilder::default()),
        }
    }

    /// Allocate an id for a layout group that another document can reference.
    pub(crate) fn fresh_group_id(&self) -> GroupId {
        self.group_ids.borrow_mut().fresh()
    }

    /// A language keyword, cased per [`StyleConfig::keyword_case`].
    pub fn kw(&self, raw: &str) -> Doc {
        text(apply_keyword_case(raw, self.style.keyword_case))
    }

    /// A type keyword, cased per [`StyleConfig::type_keyword_case`].
    pub fn type_kw(&self, raw: &str) -> Doc {
        text(apply_keyword_case(raw, self.style.type_keyword_case))
    }

    /// The break-or-space between binary operands.
    pub fn op_space(&self) -> Doc {
        if self.style.space_around_binary_op {
            line()
        } else {
            soft_line()
        }
    }

    /// Binary-operator spacing, retaining mandatory spaces around word operators.
    pub(crate) fn binary_op_space(&self, required: bool) -> Doc {
        if required {
            line()
        } else {
            self.op_space()
        }
    }

    /// A comma separator with the configured trailing spacing.
    pub fn comma(&self) -> Doc {
        let sep = if self.style.space_after_comma {
            line()
        } else {
            soft_line()
        };
        concat([text(","), sep])
    }

    /// A `:` with the configured surrounding spacing.
    pub fn colon(&self) -> Doc {
        concat([
            self.colon_token(true),
            text(self.space_after_colon().to_string()),
        ])
    }

    /// A `:` without trailing spacing. `inline` controls whether configured
    /// pre-colon spacing applies; a colon starting a continuation line has none.
    pub fn colon_token(&self, inline: bool) -> Doc {
        text(if inline && self.style.space_before_colon {
            " :"
        } else {
            ":"
        })
    }

    /// Configured inline spacing between `:` and the following type.
    pub fn space_after_colon(&self) -> &'static str {
        if self.style.space_after_colon {
            " "
        } else {
            ""
        }
    }

    /// The `:=` assignment operator with the configured surrounding spacing.
    pub fn assign_op(&self) -> Doc {
        concat([
            self.assign_op_token(true),
            text(self.space_after_assign().to_string()),
        ])
    }

    /// A `:=` without trailing spacing. `inline` controls whether configured
    /// pre-operator spacing applies.
    pub fn assign_op_token(&self, inline: bool) -> Doc {
        text(if inline && self.style.space_around_assign {
            " :="
        } else {
            ":="
        })
    }

    /// Configured inline spacing between `:=` and its value.
    pub fn space_after_assign(&self) -> &'static str {
        if self.style.space_around_assign {
            " "
        } else {
            ""
        }
    }

    /// The padding just inside a pair of parentheses kept on one line, per
    /// [`StyleConfig::space_inside_parens`]. A broken pair never gets one.
    pub(crate) fn paren_pad(&self) -> &'static str {
        if self.style.space_inside_parens {
            " "
        } else {
            ""
        }
    }

    /// The break just inside a pair of parentheses in a vertical layout: a
    /// space when flat if [`StyleConfig::space_inside_parens`], else nothing.
    pub(crate) fn paren_soft_line(&self) -> Doc {
        if self.style.space_inside_parens {
            line()
        } else {
            soft_line()
        }
    }

    /// Whether a *broken* (multi-line) collection emits a trailing separator
    /// after its last item, per `policy`. Callers use this only in a broken
    /// branch, a flat single-line list never gets one.
    pub fn emit_trailing_sep(&self, policy: TrailingSeparator) -> bool {
        matches!(policy, TrailingSeparator::WhenBroken)
    }
}
