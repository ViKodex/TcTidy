//! Ergonomic constructors for [`Doc`] values.
//!
//! Lowering code composes documents exclusively through these helpers so the
//! shape of the IR stays an implementation detail of [`crate::doc`].

use crate::doc::{Doc, GroupId, IndentSpec, LineKind};
use unicode_width::UnicodeWidthStr;

/// Verbatim text. Panics in debug if `s` contains a newline (invariant of
/// [`Doc::Text`]); callers must split multi-line content into [`hard_literal`]
/// joined fragments.
pub fn text(s: impl Into<Box<str>>) -> Doc {
    let s = s.into();
    debug_assert!(
        !s.contains('\n'),
        "Doc::Text must not contain a newline: {s:?}"
    );
    let width = UnicodeWidthStr::width(&*s).min(u16::MAX as usize) as u16;
    Doc::Text(s, width)
}

/// The empty document.
pub fn nil() -> Doc {
    Doc::Concat(Vec::new())
}

/// Concatenate documents.
pub fn concat(parts: impl IntoIterator<Item = Doc>) -> Doc {
    Doc::Concat(parts.into_iter().collect())
}

/// `" "` flat / newline broken.
pub fn line() -> Doc {
    Doc::Line(LineKind::Space)
}

/// `""` flat / newline broken.
pub fn soft_line() -> Doc {
    Doc::Line(LineKind::Soft)
}

/// Always a newline; forces enclosing groups to break.
pub fn hard_line() -> Doc {
    // A hard line implies BreakParent; both are emitted so callers needn't remember.
    Doc::Concat(vec![Doc::Line(LineKind::Hard), Doc::BreakParent])
}

/// A newline that never injects indentation (verbatim multi-line content).
pub fn hard_literal() -> Doc {
    Doc::Concat(vec![Doc::Line(LineKind::HardLiteral), Doc::BreakParent])
}

/// Group whose content is rendered flat if it fits, else broken.
pub fn group(content: Doc) -> Doc {
    Doc::Group {
        id: None,
        content: Box::new(content),
        should_break: false,
        expanded_states: None,
    }
}

/// Group carrying an id so [`if_break_group`] / [`indent_if_break`] can reference it.
pub fn group_with_id(id: GroupId, content: Doc) -> Doc {
    Doc::Group {
        id: Some(id),
        content: Box::new(content),
        should_break: false,
        expanded_states: None,
    }
}

/// Group forced to break.
pub fn group_break(content: Doc) -> Doc {
    Doc::Group {
        id: None,
        content: Box::new(content),
        should_break: true,
        expanded_states: None,
    }
}

/// Conditional group: the first alternative that fits wins, else the last.
pub fn conditional_group(states: Vec<Doc>) -> Doc {
    let content = states.first().cloned().unwrap_or_else(nil);
    Doc::Group {
        id: None,
        content: Box::new(content),
        should_break: false,
        expanded_states: Some(states),
    }
}

/// Indent the content one configured unit.
pub fn indent(content: Doc) -> Doc {
    Doc::Indent(Box::new(content))
}

/// Indent the content by an explicit amount.
pub fn align(spec: IndentSpec, content: Doc) -> Doc {
    Doc::Align {
        spec,
        content: Box::new(content),
    }
}

/// Align the content's line breaks to the current output column (e.g. multi-line
/// call arguments under their opening `(`).
pub fn align_current(content: Doc) -> Doc {
    Doc::AlignCurrent(Box::new(content))
}

/// Render `broke` when the nearest enclosing group breaks, else `flat`.
pub fn if_break(broke: Doc, flat: Doc) -> Doc {
    Doc::IfBreak {
        broke: Box::new(broke),
        flat: Box::new(flat),
        group_id: None,
    }
}

/// As [`if_break`] but keyed to a specific group's decision.
pub fn if_break_group(group_id: GroupId, broke: Doc, flat: Doc) -> Doc {
    Doc::IfBreak {
        broke: Box::new(broke),
        flat: Box::new(flat),
        group_id: Some(group_id),
    }
}

/// Indent only when the referenced group breaks.
pub fn indent_if_break(group_id: GroupId, content: Doc) -> Doc {
    Doc::IndentIfBreak {
        content: Box::new(content),
        group_id,
    }
}

/// Defer content to end of line (trailing comments).
pub fn line_suffix(content: Doc) -> Doc {
    Doc::LineSuffix(Box::new(content))
}

/// Flush pending line suffixes.
pub fn line_suffix_boundary() -> Doc {
    Doc::LineSuffixBoundary
}

/// Force the enclosing group to break.
pub fn break_parent() -> Doc {
    Doc::BreakParent
}

/// Packing fill of `[content, separator, …]`.
pub fn fill(items: Vec<Doc>) -> Doc {
    Doc::Fill(items)
}

/// Tag a sub-document.
pub fn label(name: impl Into<Box<str>>, content: Doc) -> Doc {
    Doc::Label(name.into(), Box::new(content))
}

/// Interleave `sep` between `items`.
pub fn join(sep: Doc, items: impl IntoIterator<Item = Doc>) -> Doc {
    let mut out = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(sep.clone());
        }
        out.push(item);
    }
    Doc::Concat(out)
}
