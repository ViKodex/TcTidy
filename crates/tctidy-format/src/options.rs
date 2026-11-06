//! The user-configurable surface of [`StyleConfig`]: one [`StyleOption`] per
//! public setting, with its key, documentation, accepted values, and typed
//! accessors.
//!
//! This table is the single source of truth for everything that reads or writes
//! style settings from the outside: the `.tctidy.toml` loader in the facade and
//! the editor integrations that render a settings UI from it. Every listed
//! setting changes the output (`tests/style_leak.rs`), so a config file can
//! never contain a setting that silently does nothing, and keeps the
//! non-destruction guarantees at every value (`tests/configurable_options.rs`
//! drives each one to its boundaries). Two `StyleConfig` fields are not listed:
//! `preserve_line_breaks` stays internal because keeping a width-wrapped
//! layout's breaks reshapes it on the next run, and `comment_style` has a
//! single value.
//!
//! Values are validated here, not by callers: [`StyleOption::set`] rejects a
//! wrong type, an unknown choice, or an out-of-range integer, leaving the style
//! untouched.

use std::fmt;

use tctidy_pretty::IndentStyle;

use crate::style::{
    BlockTokenPlacement, CallArgumentLayout, CollectionBreak, ColonAlignment, KeywordCase,
    LogicalExpressionLayout, ParenthesizedExpressionLayout, StyleConfig, TrailingSeparator,
};

/// The section a setting belongs to, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OptionGroup {
    Indentation,
    Casing,
    Spacing,
    Blocks,
    Wrapping,
    Collections,
    Declarations,
    Comments,
    BlankLines,
}

impl OptionGroup {
    /// Every group, in display order.
    pub const ALL: &'static [OptionGroup] = &[
        OptionGroup::Indentation,
        OptionGroup::Casing,
        OptionGroup::Spacing,
        OptionGroup::Blocks,
        OptionGroup::Wrapping,
        OptionGroup::Collections,
        OptionGroup::Declarations,
        OptionGroup::Comments,
        OptionGroup::BlankLines,
    ];

    /// A stable machine-readable identifier.
    pub fn id(self) -> &'static str {
        match self {
            OptionGroup::Indentation => "indentation",
            OptionGroup::Casing => "casing",
            OptionGroup::Spacing => "spacing",
            OptionGroup::Blocks => "blocks",
            OptionGroup::Wrapping => "wrapping",
            OptionGroup::Collections => "collections",
            OptionGroup::Declarations => "declarations",
            OptionGroup::Comments => "comments",
            OptionGroup::BlankLines => "blank_lines",
        }
    }

    /// A human-readable title.
    pub fn label(self) -> &'static str {
        match self {
            OptionGroup::Indentation => "Indentation & width",
            OptionGroup::Casing => "Casing",
            OptionGroup::Spacing => "Spacing",
            OptionGroup::Blocks => "Control structures",
            OptionGroup::Wrapping => "Line wrapping",
            OptionGroup::Collections => "Lists & initializers",
            OptionGroup::Declarations => "Declarations",
            OptionGroup::Comments => "Comments",
            OptionGroup::BlankLines => "Blank lines",
        }
    }
}

/// The values a setting accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Bool,
    /// An integer in `min..=max`.
    Integer {
        min: u16,
        max: u16,
    },
    /// One of a fixed set of (snake_case) names.
    Choice(&'static [&'static str]),
}

/// A setting's value, independent of any file format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionValue {
    Bool(bool),
    Integer(i64),
    Choice(String),
}

impl fmt::Display for OptionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionValue::Bool(b) => write!(f, "{b}"),
            OptionValue::Integer(n) => write!(f, "{n}"),
            OptionValue::Choice(s) => write!(f, "\"{s}\""),
        }
    }
}

/// Why a value was refused by [`StyleOption::set`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionError {
    /// The value has the wrong type (e.g. a string for a boolean).
    WrongType { expected: &'static str },
    /// The string is not one of the accepted names.
    UnknownChoice { allowed: &'static [&'static str] },
    /// The integer is outside the accepted range.
    OutOfRange { min: u16, max: u16 },
}

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionError::WrongType { expected } => write!(f, "expected {expected}"),
            OptionError::UnknownChoice { allowed } => {
                write!(f, "expected one of ")?;
                for (i, a) in allowed.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{a}\"")?;
                }
                Ok(())
            }
            OptionError::OutOfRange { min, max } => {
                write!(f, "expected an integer between {min} and {max}")
            }
        }
    }
}

impl std::error::Error for OptionError {}

/// One user-configurable style setting.
pub struct StyleOption {
    /// The key used in `.tctidy.toml` (the `StyleConfig` field name).
    pub key: &'static str,
    pub group: OptionGroup,
    /// A short title for settings UIs.
    pub label: &'static str,
    /// One or two sentences explaining the effect.
    pub description: &'static str,
    pub kind: OptionKind,
    get: fn(&StyleConfig) -> OptionValue,
    // Called only with a value already validated against `kind`.
    apply: fn(&mut StyleConfig, &OptionValue),
}

impl fmt::Debug for StyleOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StyleOption")
            .field("key", &self.key)
            .field("group", &self.group)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl StyleOption {
    /// The setting's current value in `style`.
    pub fn get(&self, style: &StyleConfig) -> OptionValue {
        (self.get)(style)
    }

    /// The house-style default.
    pub fn default_value(&self) -> OptionValue {
        self.get(&StyleConfig::default())
    }

    /// Validate `value` against this setting and store it in `style`. On error
    /// `style` is left unchanged.
    pub fn set(&self, style: &mut StyleConfig, value: &OptionValue) -> Result<(), OptionError> {
        match (self.kind, value) {
            (OptionKind::Bool, OptionValue::Bool(_)) => {}
            (OptionKind::Bool, _) => {
                return Err(OptionError::WrongType {
                    expected: "a boolean",
                })
            }
            (OptionKind::Integer { min, max }, OptionValue::Integer(n)) => {
                if *n < i64::from(min) || *n > i64::from(max) {
                    return Err(OptionError::OutOfRange { min, max });
                }
            }
            (OptionKind::Integer { .. }, _) => {
                return Err(OptionError::WrongType {
                    expected: "an integer",
                })
            }
            (OptionKind::Choice(allowed), OptionValue::Choice(s)) => {
                if !allowed.contains(&s.as_str()) {
                    return Err(OptionError::UnknownChoice { allowed });
                }
            }
            (OptionKind::Choice(_), _) => {
                return Err(OptionError::WrongType {
                    expected: "a string",
                })
            }
        }
        (self.apply)(style, value);
        Ok(())
    }
}

/// Every user-configurable setting, grouped and in display order.
pub fn style_options() -> &'static [StyleOption] {
    OPTIONS
}

/// Look up a setting by its `.tctidy.toml` key.
pub fn find_option(key: &str) -> Option<&'static StyleOption> {
    OPTIONS.iter().find(|o| o.key == key)
}

// ---- value conversions -----------------------------------------------------

/// Mapping between a style enum and its snake_case names.
trait Choice: Sized + Copy + 'static {
    const NAMES: &'static [&'static str];
    fn name(self) -> &'static str;
    fn from_name(name: &str) -> Self;
}

macro_rules! choice {
    ($ty:ty { $($variant:path => $name:literal),+ $(,)? }) => {
        impl Choice for $ty {
            const NAMES: &'static [&'static str] = &[$($name),+];
            fn name(self) -> &'static str {
                match self { $($variant => $name),+ }
            }
            fn from_name(name: &str) -> Self {
                match name {
                    $($name => $variant,)+
                    // Unreachable: `StyleOption::set` validated the name.
                    _ => unreachable!("unvalidated choice {name:?}"),
                }
            }
        }
    };
}

choice!(IndentStyle {
    IndentStyle::Spaces => "spaces",
    IndentStyle::Tabs => "tabs",
});
choice!(KeywordCase {
    KeywordCase::Upper => "upper",
    KeywordCase::Lower => "lower",
    KeywordCase::Preserve => "preserve",
});
choice!(BlockTokenPlacement {
    BlockTokenPlacement::SameLine => "same_line",
    BlockTokenPlacement::OwnLine => "own_line",
});
choice!(LogicalExpressionLayout {
    LogicalExpressionLayout::Compact => "compact",
    LogicalExpressionLayout::ExpandedBranches => "expanded_branches",
});
choice!(ParenthesizedExpressionLayout {
    ParenthesizedExpressionLayout::Hanging => "hanging",
    ParenthesizedExpressionLayout::Vertical => "vertical",
});
choice!(CallArgumentLayout {
    CallArgumentLayout::Hanging => "hanging",
    CallArgumentLayout::Vertical => "vertical",
});
choice!(ColonAlignment {
    ColonAlignment::PerBlock => "per_block",
    ColonAlignment::Off => "off",
});
choice!(CollectionBreak {
    CollectionBreak::Fit => "fit",
    CollectionBreak::AlwaysWhenMultiline => "always_when_multiline",
});
choice!(TrailingSeparator {
    TrailingSeparator::Never => "never",
    TrailingSeparator::WhenBroken => "when_broken",
});

fn choice_value<T: Choice>(v: T) -> OptionValue {
    OptionValue::Choice(v.name().to_owned())
}

fn as_choice<T: Choice>(v: &OptionValue) -> T {
    match v {
        OptionValue::Choice(s) => T::from_name(s),
        _ => unreachable!("unvalidated choice value"),
    }
}

fn as_bool(v: &OptionValue) -> bool {
    match v {
        OptionValue::Bool(b) => *b,
        _ => unreachable!("unvalidated bool value"),
    }
}

fn as_u16(v: &OptionValue) -> u16 {
    match v {
        // In range: `StyleOption::set` checked it against a u16 range.
        OptionValue::Integer(n) => u16::try_from(*n).expect("validated range"),
        _ => unreachable!("unvalidated integer value"),
    }
}

macro_rules! bool_option {
    ($group:ident, $field:ident, $label:literal, $desc:literal) => {
        StyleOption {
            key: stringify!($field),
            group: OptionGroup::$group,
            label: $label,
            description: $desc,
            kind: OptionKind::Bool,
            get: |s| OptionValue::Bool(s.$field),
            apply: |s, v| s.$field = as_bool(v),
        }
    };
}

macro_rules! int_option {
    ($group:ident, $field:ident, $min:literal ..= $max:literal, $label:literal, $desc:literal) => {
        StyleOption {
            key: stringify!($field),
            group: OptionGroup::$group,
            label: $label,
            description: $desc,
            kind: OptionKind::Integer {
                min: $min,
                max: $max,
            },
            get: |s| OptionValue::Integer(i64::from(s.$field)),
            apply: |s, v| s.$field = as_u16(v),
        }
    };
}

macro_rules! choice_option {
    ($group:ident, $field:ident, $ty:ty, $label:literal, $desc:literal) => {
        StyleOption {
            key: stringify!($field),
            group: OptionGroup::$group,
            label: $label,
            description: $desc,
            kind: OptionKind::Choice(<$ty as Choice>::NAMES),
            get: |s| choice_value(s.$field),
            apply: |s, v| s.$field = as_choice::<$ty>(v),
        }
    };
}

static OPTIONS: &[StyleOption] = &[
    // --- indentation & width ---
    choice_option!(
        Indentation,
        indent_style,
        IndentStyle,
        "Indent with",
        "Indent nested code with spaces or with tab characters."
    ),
    int_option!(
        Indentation,
        indent_width,
        1..=16,
        "Indent width",
        "Columns per indentation level (also the width of one tab when indenting with tabs)."
    ),
    int_option!(
        Indentation,
        line_width,
        20..=500,
        "Line width",
        "Long expressions and calls wrap once they would extend past this column."
    ),
    // --- casing ---
    choice_option!(
        Casing,
        keyword_case,
        KeywordCase,
        "Keyword case",
        "Casing of language keywords such as IF, THEN, VAR and END_IF."
    ),
    choice_option!(
        Casing,
        type_keyword_case,
        KeywordCase,
        "Type keyword case",
        "Casing of built-in type names such as BOOL, INT, TIME and STRING."
    ),
    // --- spacing ---
    bool_option!(
        Spacing,
        space_around_binary_op,
        "Spaces around operators",
        "Put a space on each side of symbolic binary operators such as +, *, = and <>. Word operators such as AND and MOD always keep their spaces."
    ),
    bool_option!(
        Spacing,
        space_around_assign,
        "Spaces around :=",
        "Put a space on each side of the := assignment operator in declarations."
    ),
    bool_option!(
        Spacing,
        align_impl_assignments,
        "Align assignments",
        "Line up the := of consecutive assignments in code. A blank line, a comment line or any other statement starts a new group."
    ),
    bool_option!(
        Spacing,
        space_after_comma,
        "Space after comma",
        "Put a space after each comma in argument lists."
    ),
    bool_option!(
        Spacing,
        space_inside_parens,
        "Spaces inside parentheses",
        "Put a space just inside the parentheses of calls and parenthesized expressions kept on one line: foo( a, b ), ( a + b ) * c."
    ),
    bool_option!(
        Spacing,
        space_before_colon,
        "Space before colon",
        "Put a space before the : that separates a declared name from its type."
    ),
    bool_option!(
        Spacing,
        space_after_colon,
        "Space after colon",
        "Put a space after the : that separates a declared name from its type."
    ),
    // --- control structures ---
    choice_option!(
        Blocks,
        then_placement,
        BlockTokenPlacement,
        "THEN placement",
        "Keep THEN at the end of the IF / ELSIF condition line, or put it on its own line. With same_line, a THEN you placed under a wrapped condition stays there."
    ),
    choice_option!(
        Blocks,
        do_placement,
        BlockTokenPlacement,
        "DO placement",
        "Keep DO at the end of the FOR / WHILE header line, or put it on its own line."
    ),
    choice_option!(
        Blocks,
        of_placement,
        BlockTokenPlacement,
        "OF placement",
        "Keep OF at the end of the CASE line, or put it on its own line."
    ),
    bool_option!(
        Blocks,
        indent_block_token,
        "Indent own-line THEN / DO / OF",
        "Indent THEN, DO and OF one level when they are placed on their own line."
    ),
    bool_option!(
        Blocks,
        place_simple_if_on_single_line,
        "Single-line simple IF",
        "Write an IF whose only branch is one simple statement on a single line when it fits, as in IF bReset THEN nCount := 0; END_IF. An IF with ELSIF, ELSE, a nested block or a comment keeps its block layout."
    ),
    // --- line wrapping ---
    bool_option!(
        Wrapping,
        wrap_before_operator,
        "Wrap before operator",
        "When a long expression wraps, start each continuation line with the operator instead of ending the previous line with it."
    ),
    choice_option!(
        Wrapping,
        logical_expression_layout,
        LogicalExpressionLayout,
        "Logical expression layout",
        "How a wrapped OR / XOR chain lays out its parenthesized branches: expand each branch onto its own lines, or keep each branch compact."
    ),
    int_option!(
        Wrapping,
        logical_chain_max_inline,
        0..=32,
        "Inline logical operands",
        "Maximum operands an OR, XOR or AND chain keeps on one line; longer chains put one operand per line. 0 leaves it to the line width."
    ),
    choice_option!(
        Wrapping,
        parenthesized_expression_layout,
        ParenthesizedExpressionLayout,
        "Parenthesized expression layout",
        "How a parenthesized expression that does not fit wraps: hang the operands under the first one, or place them on indented lines."
    ),
    choice_option!(
        Wrapping,
        call_argument_layout,
        CallArgumentLayout,
        "Call argument layout",
        "How a call whose arguments do not fit wraps: hang the arguments under the first one, or place each on its own indented line."
    ),
    // --- lists & initializers ---
    int_option!(
        Collections,
        collection_max_inline,
        0..=16,
        "Inline call arguments",
        "Maximum arguments a call or a function block instance keeps on one line; longer argument lists put one argument per line. 0 leaves it to the line width."
    ),
    choice_option!(
        Collections,
        arg_break,
        CollectionBreak,
        "Argument lists written on several lines",
        "Keep an argument list you wrote across several lines broken (always_when_multiline), or join it onto one line whenever it fits (fit)."
    ),
    int_option!(
        Collections,
        struct_initializer_max_inline,
        1..=32,
        "Inline structure fields",
        "Maximum named fields a structure initializer keeps on one line; longer initializers put one field per line."
    ),
    int_option!(
        Collections,
        array_initializer_max_inline,
        0..=64,
        "Inline array values",
        "Maximum values an array initializer [ ] keeps on one line; longer initializers put one value per line. 0 keeps every array initializer on one line."
    ),
    choice_option!(
        Collections,
        trailing_comma_collection,
        TrailingSeparator,
        "Trailing comma in lists",
        "Whether a multi-line argument or initializer list ends with a comma after its last item."
    ),
    choice_option!(
        Collections,
        trailing_comma_enum,
        TrailingSeparator,
        "Trailing comma in enums",
        "Whether an enumeration ends with a comma after its last value."
    ),
    // --- declarations ---
    choice_option!(
        Declarations,
        align_decl_colon,
        ColonAlignment,
        "Align declarations",
        "Line up the : (and the := of initial values) of the declarations in a block, or write each declaration with single spaces."
    ),
    bool_option!(
        Declarations,
        align_at_addresses,
        "Align AT addresses",
        "Line up the AT keyword of located variables (x AT %IX0.0 : BOOL) in a block."
    ),
    // --- comments ---
    bool_option!(
        Comments,
        space_after_line_comment,
        "Space after //",
        "Ensure exactly one space after the // of a line comment. The comment text itself is never changed."
    ),
    int_option!(
        Comments,
        trailing_comment_min_spaces,
        1..=16,
        "Gap before trailing comment",
        "Minimum spaces between code and a comment at the end of the same line."
    ),
    bool_option!(
        Comments,
        align_trailing_comments,
        "Align trailing comments",
        "Line up the comments that end consecutive lines in one column, the one of the rightmost. A blank line or a line without such a comment starts a new group."
    ),
    // --- blank lines ---
    int_option!(
        BlankLines,
        blank_lines_max,
        0..=10,
        "Maximum blank lines",
        "Runs of blank lines longer than this are collapsed."
    ),
    int_option!(
        BlankLines,
        blank_lines_at_block_start,
        0..=10,
        "Blank lines at block start",
        "Blank lines kept right after IF … THEN, CASE … OF, a loop header or a CASE label."
    ),
    int_option!(
        BlankLines,
        blank_lines_at_block_end,
        0..=10,
        "Blank lines at block end",
        "Blank lines kept right before END_IF, END_CASE, END_VAR and the other block closers."
    ),
    int_option!(
        BlankLines,
        blank_lines_after_decl,
        0..=10,
        "Blank lines after a declaration block",
        "Minimum blank lines after END_VAR or END_TYPE when another block follows. Never more than the maximum blank lines; 0 keeps the blank lines you wrote."
    ),
    int_option!(
        BlankLines,
        blank_lines_around_block_statement,
        0..=10,
        "Blank lines around block statements",
        "Minimum blank lines between an IF, CASE, FOR, WHILE, REPEAT or __TRY statement and the statement before or after it. Never more than the maximum blank lines; 0 keeps the blank lines you wrote."
    ),
    int_option!(
        BlankLines,
        blank_lines_after_case,
        0..=10,
        "Blank lines after a CASE arm",
        "Minimum blank lines after a CASE arm with at most one simple statement, before the next arm or ELSE. Never more than the maximum blank lines; 0 keeps the blank lines you wrote."
    ),
    int_option!(
        BlankLines,
        blank_lines_after_multiline_case,
        0..=10,
        "Blank lines after a longer CASE arm",
        "Minimum blank lines after a CASE arm with several statements or a nested block, before the next arm or ELSE. Never more than the maximum blank lines; 0 keeps the blank lines you wrote."
    ),
    int_option!(
        BlankLines,
        blank_lines_before_multiline_comment,
        0..=10,
        "Blank lines before a multiline comment",
        "Minimum blank lines above a (* … *) comment spanning several lines that heads a statement, a CASE arm or a declaration, unless it opens its block. Never more than the maximum blank lines; 0 keeps the blank lines you wrote."
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_grouped_in_order() {
        let mut seen = std::collections::HashSet::new();
        let mut last_group = OptionGroup::ALL[0];
        for o in style_options() {
            assert!(seen.insert(o.key), "duplicate key {}", o.key);
            assert!(o.group >= last_group, "{} breaks group order", o.key);
            last_group = o.group;
            assert!(!o.label.is_empty() && !o.description.is_empty());
        }
    }

    #[test]
    fn defaults_satisfy_their_own_kind() {
        for o in style_options() {
            let mut style = StyleConfig::default();
            o.set(&mut style, &o.default_value())
                .unwrap_or_else(|e| panic!("{}: default rejected: {e}", o.key));
            assert_eq!(style, StyleConfig::default());
        }
    }

    #[test]
    fn every_value_round_trips() {
        for o in style_options() {
            let values: Vec<OptionValue> = match o.kind {
                OptionKind::Bool => vec![OptionValue::Bool(true), OptionValue::Bool(false)],
                OptionKind::Integer { min, max } => vec![
                    OptionValue::Integer(min.into()),
                    OptionValue::Integer(max.into()),
                ],
                OptionKind::Choice(names) => names
                    .iter()
                    .map(|n| OptionValue::Choice((*n).to_owned()))
                    .collect(),
            };
            for v in values {
                let mut style = StyleConfig::default();
                o.set(&mut style, &v).unwrap();
                assert_eq!(o.get(&style), v, "{}", o.key);
            }
        }
    }

    #[test]
    fn invalid_values_are_rejected_without_change() {
        let mut style = StyleConfig::default();
        let width = find_option("indent_width").unwrap();
        assert_eq!(
            width.set(&mut style, &OptionValue::Integer(0)),
            Err(OptionError::OutOfRange { min: 1, max: 16 })
        );
        assert_eq!(
            width.set(&mut style, &OptionValue::Bool(true)),
            Err(OptionError::WrongType {
                expected: "an integer"
            })
        );
        let case = find_option("keyword_case").unwrap();
        assert!(matches!(
            case.set(&mut style, &OptionValue::Choice("camel".into())),
            Err(OptionError::UnknownChoice { .. })
        ));
        assert_eq!(style, StyleConfig::default());
    }

    #[test]
    fn error_messages_are_readable() {
        let e = OptionError::UnknownChoice {
            allowed: &["upper", "lower"],
        };
        assert_eq!(e.to_string(), "expected one of \"upper\", \"lower\"");
        let e = OptionError::OutOfRange { min: 0, max: 10 };
        assert_eq!(e.to_string(), "expected an integer between 0 and 10");
    }
}
