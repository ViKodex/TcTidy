//! The pretty-printing intermediate representation: the [`Doc`] tree.
//!
//! A `Doc` is an abstract description of layout, text, line breaks that may or
//! may not be taken, groups that break as a unit, indentation, independent of
//! the final width. The [`crate::print`] engine turns a `Doc` plus a width into
//! a concrete string.

/// Dense identifier for a [`Doc::Group`], so that [`Doc::IfBreak`] /
/// [`Doc::IndentIfBreak`] can refer to a *non-enclosing* group's break decision.
pub type GroupId = u32;

/// Mints dense [`GroupId`]s without global state.
#[derive(Debug, Default)]
pub struct GroupIdBuilder(u32);

impl GroupIdBuilder {
    /// Allocate a fresh, unique group id.
    pub fn fresh(&mut self) -> GroupId {
        self.0 += 1;
        self.0
    }
}

/// The behaviour of a line break in the two printing modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// `""` when flat, newline + indent when broken.
    Soft,
    /// `" "` when flat, newline + indent when broken.
    Space,
    /// Always a newline + indent; also forces the enclosing group to break.
    Hard,
    /// Like [`LineKind::Hard`] but emits no indentation on the new line, used to
    /// reproduce verbatim multi-line content (e.g. pragma interiors) without
    /// ever re-indenting it.
    HardLiteral,
    /// Trims trailing whitespace from the current line.
    Trim,
}

/// How much to indent: either N logical levels (scaled by the configured indent
/// width) or an absolute number of spaces (used for `:=` alignment padding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentSpec {
    Levels(i16),
    Spaces(u16),
}

/// The document IR. Build it with the helpers in [`crate::builders`].
///
/// Invariant: a [`Doc::Text`] never contains a newline, newlines only ever come
/// from a [`Doc::Line`]. Multi-line verbatim spans are encoded as `Text`
/// fragments joined by [`LineKind::HardLiteral`] lines.
#[derive(Debug, Clone)]
pub enum Doc {
    /// Verbatim text together with its pre-computed display width (so the fitting
    /// check never re-scans the string). See [`crate::builders::text`].
    Text(Box<str>, u16),

    /// Sequence of documents rendered back to back.
    Concat(Vec<Doc>),

    /// A line break whose rendering depends on the enclosing group's mode.
    Line(LineKind),

    /// Render the content flat if it fits the remaining width, otherwise broken.
    Group {
        id: Option<GroupId>,
        content: Box<Doc>,
        /// Force broken mode regardless of fit (set by `BreakParent`/hardline
        /// propagation).
        should_break: bool,
        /// Ordered alternative layouts; the first that fits wins. `None` means
        /// an ordinary group.
        expanded_states: Option<Vec<Doc>>,
    },

    /// Increase indentation by one configured unit for the enclosed content.
    Indent(Box<Doc>),

    /// Indent the enclosed content by an explicit amount (alignment/dedent).
    Align { spec: IndentSpec, content: Box<Doc> },

    /// Align the enclosed content's line breaks to the **current output column**.
    /// Used to line up multi-line call arguments under their opening `(`.
    AlignCurrent(Box<Doc>),

    /// Render `broke` if the governing group is broken, else `flat`.
    IfBreak {
        broke: Box<Doc>,
        flat: Box<Doc>,
        /// The group whose mode decides; `None` = nearest enclosing group.
        group_id: Option<GroupId>,
    },

    /// Behave as [`Doc::Indent`] only when the referenced group breaks.
    IndentIfBreak {
        content: Box<Doc>,
        group_id: GroupId,
    },

    /// Defer content to the end of the current line, where trailing comments
    /// live, so they never land before following code.
    LineSuffix(Box<Doc>),

    /// Flush any pending [`Doc::LineSuffix`] now.
    LineSuffixBoundary,

    /// Force the nearest enclosing group into broken mode.
    BreakParent,

    /// `[content, separator, content, …]` where each separator independently
    /// breaks only if the following content does not fit.
    Fill(Vec<Doc>),

    /// A no-op wrapper tagging a sub-document for later passes/debugging.
    Label(Box<str>, Box<Doc>),
}
