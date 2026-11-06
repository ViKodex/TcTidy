//! Knobs the printer reads. Every field originates from the formatter's
//! `StyleConfig`; this crate stays unaware of where they come from.

/// Whether indentation is rendered with spaces or tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    Spaces,
    Tabs,
}

/// The newline sequence the printer emits for line breaks it introduces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    /// The literal bytes for this line ending.
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
        }
    }
}

/// Configuration for a single [`crate::print`] run.
#[derive(Debug, Clone, Copy)]
pub struct PrintOptions {
    /// Target maximum line width (columns).
    pub width: u16,
    /// Number of columns per indentation level.
    pub indent_width: u16,
    /// Spaces vs tabs.
    pub indent_style: IndentStyle,
    /// Newline emitted for introduced breaks.
    pub eol: LineEnding,
    /// Pad the line suffixes of consecutive lines so they all start in the
    /// column of the rightmost one. Applied after layout, so it never changes
    /// where lines break.
    pub align_line_suffixes: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        PrintOptions {
            width: 100,
            indent_width: 4,
            indent_style: IndentStyle::Spaces,
            eol: LineEnding::Lf,
            align_line_suffixes: false,
        }
    }
}
