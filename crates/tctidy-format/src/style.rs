//! [`StyleConfig`], the house defaults and the configurable surface in one
//! place.
//!
//! With no configuration this is [`StyleConfig::default`]; a `.tctidy.toml`
//! fills the same struct through the setting table in [`crate::options`]. The
//! formatting engine is identical either way. The "no style leaked" test
//! (`tests/style_leak.rs`) checks that each field changes the output.
//!
//! Every field is read by the rules, and all but two are settings:
//! `preserve_line_breaks` stays internal (keeping the breaks of a width-wrapped
//! layout is not idempotent), and `comment_style` is fixed (its only variant is
//! `Preserve`).
//!
//! [`StyleConfig::default`] encodes the project's house style, see the field
//! defaults below and the "Default style" section of `ARCHITECTURE.md`.

use tctidy_pretty::{IndentStyle, LineEnding, PrintOptions};

/// Casing applied to language keywords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordCase {
    Upper,
    Lower,
    Preserve,
}

/// Whether a block token (`THEN`/`DO`/`OF`) sits on the same line as its header
/// or on its own line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTokenPlacement {
    SameLine,
    OwnLine,
}

/// Column alignment of the `:` within a declaration block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColonAlignment {
    Off,
    PerBlock,
}

/// Comment handling. Preserve-only is the safe opinion (never rewrap/convert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    Preserve,
}

/// Whether the source's line breaks decide that an argument list breaks. Width,
/// [`StyleConfig::collection_max_inline`] and two or more named arguments break
/// a list under either policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionBreak {
    /// Ignore the source's line breaks: a list that fits goes on one line.
    Fit,
    /// Keep a list broken when the source wrote it across several lines.
    AlwaysWhenMultiline,
}

/// Whether a trailing separator is emitted when a collection breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailingSeparator {
    Never,
    WhenBroken,
}

/// Layout used when a logical expression is too wide for one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalExpressionLayout {
    /// Keep each parenthesized branch on one line whenever that branch fits.
    Compact,
    /// Expand parenthesized binary branches of a wrapped `OR` or `XOR` chain.
    ExpandedBranches,
}

/// Layout used when a parenthesized expression does not fit on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParenthesizedExpressionLayout {
    /// Keep the first operand beside the opening delimiter and align all
    /// following operators and the closing delimiter under it.
    Hanging,
    /// Put the expression on indented lines and keep the closing delimiter at
    /// the enclosing expression's indentation.
    Vertical,
}

/// Layout used when a call argument list does not fit on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallArgumentLayout {
    /// Keep the first argument beside the opening delimiter and align all
    /// following arguments and the closing delimiter under it.
    Hanging,
    /// Put every argument on its own indented line and keep the closing
    /// delimiter at the call's indentation.
    Vertical,
}

/// The complete set of style dimensions. Flat on purpose: it is the single
/// source of the opinion, and `tests/style_leak.rs` proves that every field
/// with more than one value changes the output.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleConfig {
    // --- indentation & width ---
    pub indent_style: IndentStyle,
    pub indent_width: u16,
    /// Wrap long lines beyond this column.
    pub line_width: u16,
    /// Reflow line breaks within statements (vs. keeping the author's).
    pub preserve_line_breaks: bool,

    // --- casing ---
    pub keyword_case: KeywordCase,
    pub type_keyword_case: KeywordCase,

    // --- block-token placement ---
    pub then_placement: BlockTokenPlacement,
    pub do_placement: BlockTokenPlacement,
    pub of_placement: BlockTokenPlacement,
    /// Indent the block token when it is on its own line.
    pub indent_block_token: bool,
    /// Put an `IF` whose only branch is one simple (non-block) statement, with
    /// no `ELSIF`, `ELSE` or comment, on a single line when it fits.
    pub place_simple_if_on_single_line: bool,

    // --- spacing ---
    pub space_around_binary_op: bool,
    pub space_after_comma: bool,
    pub space_before_colon: bool,
    pub space_after_colon: bool,
    pub space_inside_parens: bool,
    pub space_around_assign: bool,

    // --- line breaking / wrapping ---
    /// Place the operator at the start of a wrapped continuation line.
    pub wrap_before_operator: bool,
    /// Shape of parenthesized branches in wrapped logical expressions.
    pub logical_expression_layout: LogicalExpressionLayout,
    /// Shape of a wrapped parenthesized expression.
    pub parenthesized_expression_layout: ParenthesizedExpressionLayout,
    /// Maximum arguments an argument list (a call's, or a function block
    /// instance's default arguments) keeps on one line before it breaks one
    /// argument per line; 0 leaves it to the line width. A list carrying a
    /// comment keeps its source layout.
    pub collection_max_inline: u16,
    /// Maximum named fields kept inline in a structure initializer.
    pub struct_initializer_max_inline: u16,
    /// Maximum values an array initializer `[ … ]` keeps on one line before it
    /// breaks one value per line; 0 keeps every array initializer on one line.
    pub array_initializer_max_inline: u16,
    /// Maximum operands an OR / XOR / AND chain keeps on one line before it
    /// breaks one operand per line; 0 leaves it to the line width.
    pub logical_chain_max_inline: u16,
    /// Shape of a wrapped call argument list.
    pub call_argument_layout: CallArgumentLayout,
    /// Whether an argument list written across several lines stays broken.
    pub arg_break: CollectionBreak,
    pub trailing_comma_enum: TrailingSeparator,
    /// Trailing separator after the last item of a *broken* (multi-line)
    /// argument or initializer list, a call's args, an FB instance's default
    /// args, or an array/struct initializer `( … )` / `[ … ]`. House style:
    /// never (a redundant `,)` is normalized away). A flat single-line list
    /// never gains one, whatever the policy.
    pub trailing_comma_collection: TrailingSeparator,

    // --- column alignment (declarations only by default) ---
    pub align_decl_colon: ColonAlignment,
    pub align_at_addresses: bool,
    /// Align the `:=` of consecutive plain assignments in implementation code.
    /// A blank line, a comment line or any other statement ends a run.
    pub align_impl_assignments: bool,
    /// Align the comments that end consecutive lines (declarations and code
    /// alike) on the column of the rightmost one. A blank line or a line
    /// without a trailing comment ends a run.
    pub align_trailing_comments: bool,

    // --- comments ---
    /// Fixed to [`CommentStyle::Preserve`]: comment text is never rewrapped or
    /// converted, so it is not a setting.
    pub comment_style: CommentStyle,
    pub trailing_comment_min_spaces: u16,
    /// Ensure exactly one space after the `//` of a line comment, adding a
    /// missing space (`//x` → `// x`) and collapsing an excess run of spaces/tabs
    /// (`//   x` → `// x`). Content is otherwise preserved verbatim.
    pub space_after_line_comment: bool,

    // --- blank lines ---
    pub blank_lines_max: u16,
    /// Blank lines kept at the very start of a block body, right after
    /// `IF ... THEN`, `CASE ... OF`, `FOR/WHILE/REPEAT ... DO`, or a `CASE` label
    /// `:`, before the first statement / arm. Never exceeds `blank_lines_max`.
    /// Opinionated default: collapse them entirely.
    pub blank_lines_at_block_start: u16,
    /// Blank lines kept at the very end of a block / container, right before
    /// `END_IF`/`END_CASE`/`END_FOR`/`END_WHILE`/`END_REPEAT`/`END_STRUCT`/
    /// `END_UNION`/`END_VAR` or the enum `)`. Never exceeds `blank_lines_max`.
    /// Opinionated default: collapse them entirely.
    pub blank_lines_at_block_end: u16,
    /// Minimum blank lines after a declaration block (`VAR … END_VAR`,
    /// `TYPE … END_TYPE`) that another construct follows. Never exceeds
    /// `blank_lines_max`. House default: 0, the source's blank lines are kept.
    pub blank_lines_after_decl: u16,
    /// Minimum blank lines between a block statement (`IF`, `CASE`, `FOR`,
    /// `WHILE`, `REPEAT`, `__TRY`) and the statement before or after it in the
    /// same list. Never exceeds `blank_lines_max`; block start and end keep
    /// their own caps. House default: 0, the source's blank lines are kept.
    pub blank_lines_around_block_statement: u16,
    /// Minimum blank lines after a `CASE` arm whose body is at most one simple
    /// statement, before the next arm or `ELSE`. Never exceeds
    /// `blank_lines_max`. House default: 0, the source's blank lines are kept.
    pub blank_lines_after_case: u16,
    /// Minimum blank lines after a `CASE` arm whose body holds more than one
    /// statement or a block statement. Never exceeds `blank_lines_max`. House
    /// default: 0, the source's blank lines are kept.
    pub blank_lines_after_multiline_case: u16,
    /// Minimum blank lines above a statement, `CASE` arm, declaration or
    /// declaration block whose leading comments start with a block comment
    /// spanning several lines, unless it is the first in its list. Never
    /// exceeds `blank_lines_max`. House default: 0, the source's blank lines
    /// are kept.
    pub blank_lines_before_multiline_comment: u16,
}

impl Default for StyleConfig {
    /// The project's house-style values.
    fn default() -> Self {
        StyleConfig {
            indent_style: IndentStyle::Spaces,
            indent_width: 4,
            line_width: 160,
            preserve_line_breaks: false,

            keyword_case: KeywordCase::Upper,
            type_keyword_case: KeywordCase::Upper,

            // Same-line is the house style; it keeps a source break before
            // THEN/DO/OF that follows a wrapped header.
            then_placement: BlockTokenPlacement::SameLine,
            do_placement: BlockTokenPlacement::SameLine,
            of_placement: BlockTokenPlacement::SameLine,
            indent_block_token: false,
            place_simple_if_on_single_line: false,

            space_around_binary_op: true,
            space_after_comma: true,
            space_before_colon: true,
            space_after_colon: true,
            space_inside_parens: false,
            space_around_assign: true,

            wrap_before_operator: true,
            logical_expression_layout: LogicalExpressionLayout::ExpandedBranches,
            parenthesized_expression_layout: ParenthesizedExpressionLayout::Hanging,
            collection_max_inline: 0,
            struct_initializer_max_inline: 2,
            array_initializer_max_inline: 4,
            logical_chain_max_inline: 3,
            call_argument_layout: CallArgumentLayout::Hanging,
            arg_break: CollectionBreak::AlwaysWhenMultiline,
            trailing_comma_enum: TrailingSeparator::Never,
            trailing_comma_collection: TrailingSeparator::Never,

            align_decl_colon: ColonAlignment::PerBlock,
            align_at_addresses: true,
            align_impl_assignments: false,
            align_trailing_comments: false,

            comment_style: CommentStyle::Preserve,
            trailing_comment_min_spaces: 1,
            space_after_line_comment: true,

            blank_lines_max: 2,
            blank_lines_at_block_start: 0,
            blank_lines_at_block_end: 0,
            blank_lines_after_decl: 0,
            blank_lines_around_block_statement: 0,
            blank_lines_after_case: 0,
            blank_lines_after_multiline_case: 0,
            blank_lines_before_multiline_comment: 0,
        }
    }
}

impl StyleConfig {
    /// Blank lines to keep at the start of a block body (right after the header),
    /// clamped to the block-start budget and the global maximum.
    pub(crate) fn cap_block_start(&self, blank: u16) -> u16 {
        blank
            .min(self.blank_lines_at_block_start)
            .min(self.blank_lines_max)
    }

    /// Blank lines to keep at the end of a block / container (right before its
    /// `END_*` keyword or `)`), clamped to the block-end budget and the global
    /// maximum.
    pub(crate) fn cap_block_end(&self, blank: u16) -> u16 {
        blank
            .min(self.blank_lines_at_block_end)
            .min(self.blank_lines_max)
    }

    /// Blank lines to keep at a seam that wants at least `minimum` of them: the
    /// source's count, raised to `minimum`, capped by the global maximum.
    pub(crate) fn ensure_blank(&self, blank: u16, minimum: u16) -> u16 {
        blank.max(minimum).min(self.blank_lines_max)
    }

    /// The minimum blank lines above a construct with these leading comments:
    /// `blank_lines_before_multiline_comment` when they start with a multiline
    /// block comment, else 0.
    pub(crate) fn multiline_comment_minimum(&self, lead: &[crate::trivia::LeadingPiece]) -> u16 {
        if crate::trivia::starts_with_multiline_comment(lead) {
            self.blank_lines_before_multiline_comment
        } else {
            0
        }
    }

    /// Build the printer options for a run, given the resolved EOL for the file.
    pub fn print_options(&self, eol: LineEnding) -> PrintOptions {
        PrintOptions {
            width: self.line_width,
            indent_width: self.indent_width,
            indent_style: self.indent_style,
            eol,
            align_line_suffixes: self.align_trailing_comments,
        }
    }
}
