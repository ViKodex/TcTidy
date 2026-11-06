//! [`SyntaxKind`], the closed vocabulary of the language.
//!
//! One flat `#[repr(u16)]` enum holding every token kind and every node kind.
//! Flat (rather than nested enums) because `rowan` needs a single `u16` and
//! because exhaustive `match` over the whole vocabulary is worth more than
//! visual grouping. Categories are expressed as contiguous ranges plus the
//! predicate methods below, never as type nesting.
//!
//! Keyword *recognition* is case-insensitive (see [`keywords`]), but the
//! original source text is always preserved by the tree, so keyword-case
//! normalization is purely a formatter concern.

pub mod keywords;

use num_enum::{IntoPrimitive, TryFromPrimitive};

// Brought into scope so the predicate methods can name variants unqualified.
use SyntaxKind::*;

/// Every token and node kind. See the module docs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TryFromPrimitive, IntoPrimitive,
)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ---- 0. Sentinels ----
    TOMBSTONE = 0,
    EOF,

    // ---- 1. Trivia (always kept as tokens in the tree) ----
    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,
    BLOCK_COMMENT,   // (* ... *), possibly nested
    C_BLOCK_COMMENT, // /* ... */, possibly nested
    PRAGMA,          // { ... } verbatim, multi-line

    // ---- 2. Literals ----
    INT_NUMBER,
    REAL_NUMBER,
    TYPED_LITERAL, // INT#100, REAL#1.5
    TIME_LITERAL,  // T#.., TIME#.., LTIME#..
    DATE_LITERAL,  // D#.., DATE#..
    TOD_LITERAL,   // TOD#.., TIME_OF_DAY#..
    DT_LITERAL,    // DT#.., DATE_AND_TIME#.., LDT#..
    STRING_LITERAL,
    WSTRING_LITERAL,
    TRUE_KW,
    FALSE_KW,

    // ---- 3. Identifiers & names ----
    IDENT,
    TWINCAT_IDENT,  // __SYSTEM, __NEW, ...
    TYPE_CONV,      // INT_TO_REAL, TO_INT, TRUNC_INT
    DIRECT_ADDRESS, // %IX0.0, %QW10, %I*

    // ---- 4. Punctuation / delimiters ----
    SEMICOLON,
    COLON,
    COMMA,
    DOTDOT, // ..   (must beat DOT)
    DOT,
    L_PAREN,
    R_PAREN,
    L_BRACK,
    R_BRACK,

    // ---- 5. Operators ----
    ASSIGN,       // :=
    ARROW,        // =>
    REF_ASSIGN,   // REF=
    SET_ASSIGN,   // S=
    RESET_ASSIGN, // R=
    EQ,           // =
    NEQ,          // <>
    LT,
    GT,
    LTE,
    GTE,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    POW,   // **
    AMP,   // &
    CARET, // ^

    // ---- 6. Keyword operators (case-insensitive words) ----
    AND_KW,
    OR_KW,
    XOR_KW,
    NOT_KW,
    MOD_KW,
    AND_THEN_KW,
    OR_ELSE_KW,
    ADR_KW,
    SIZEOF_KW,
    XSIZEOF_KW,
    INDEXOF_KW,

    // ---- 7. POU / structure keywords ----
    PROGRAM_KW,
    END_PROGRAM_KW,
    FUNCTION_KW,
    END_FUNCTION_KW,
    FUNCTION_BLOCK_KW,
    END_FUNCTION_BLOCK_KW,
    METHOD_KW,
    END_METHOD_KW,
    PROPERTY_KW,
    END_PROPERTY_KW,
    INTERFACE_KW,
    END_INTERFACE_KW,
    ACTION_KW,
    END_ACTION_KW,

    // ---- 8. OOP keywords ----
    EXTENDS_KW,
    IMPLEMENTS_KW,
    ABSTRACT_KW,
    FINAL_KW,
    THIS_KW,
    SUPER_KW,
    PUBLIC_KW,
    PRIVATE_KW,
    PROTECTED_KW,
    INTERNAL_KW,

    // ---- 9. Type-definition keywords ----
    TYPE_KW,
    END_TYPE_KW,
    STRUCT_KW,
    END_STRUCT_KW,
    UNION_KW,
    END_UNION_KW,
    ARRAY_KW,
    OF_KW,
    POINTER_KW,
    REFERENCE_KW,
    TO_KW,

    // ---- 10. VAR-section keywords ----
    VAR_KW,
    END_VAR_KW,
    VAR_INPUT_KW,
    VAR_OUTPUT_KW,
    VAR_IN_OUT_KW,
    VAR_GLOBAL_KW,
    VAR_TEMP_KW,
    VAR_STAT_KW,
    VAR_EXTERNAL_KW,
    VAR_CONFIG_KW,
    VAR_PUBLIC_KW,
    VAR_INST_KW,
    VAR_GENERIC_KW,

    // ---- 11. Variable qualifiers ----
    CONSTANT_KW,
    RETAIN_KW,
    PERSISTENT_KW,
    AT_KW,

    // ---- 12. Control-flow keywords ----
    IF_KW,
    THEN_KW,
    ELSIF_KW,
    ELSE_KW,
    END_IF_KW,
    CASE_KW,
    END_CASE_KW,
    FOR_KW,
    BY_KW,
    DO_KW,
    END_FOR_KW,
    WHILE_KW,
    END_WHILE_KW,
    REPEAT_KW,
    UNTIL_KW,
    END_REPEAT_KW,
    RETURN_KW,
    EXIT_KW,
    CONTINUE_KW,
    JMP_KW,
    TRY_KW,
    CATCH_KW,
    FINALLY_KW,
    ENDTRY_KW,

    // ---- 13. Elementary & generic type keywords ----
    BOOL_KW,
    BIT_KW,
    BYTE_KW,
    WORD_KW,
    DWORD_KW,
    LWORD_KW,
    SINT_KW,
    INT_KW,
    DINT_KW,
    LINT_KW,
    USINT_KW,
    UINT_KW,
    UDINT_KW,
    ULINT_KW,
    REAL_KW,
    LREAL_KW,
    TIME_KW,
    LTIME_KW,
    DATE_KW,
    LDATE_KW,
    TIME_OF_DAY_KW,
    TOD_KW,
    LTOD_KW,
    DATE_AND_TIME_KW,
    DT_KW,
    LDT_KW,
    STRING_KW,
    WSTRING_KW,
    ANY_KW,
    ANY_BIT_KW,
    ANY_DATE_KW,
    ANY_INT_KW,
    ANY_NUM_KW,
    ANY_REAL_KW,
    ANY_STRING_KW,

    // ====================== NODES (interior tree nodes) ======================

    // roots & recovery
    SOURCE_FILE,
    DECL_PART,
    IMPL_PART,
    ERROR,

    // POU declaration headers
    PROGRAM_DECL,
    FUNCTION_DECL,
    FUNCTION_BLOCK_DECL,
    METHOD_DECL,
    PROPERTY_DECL,
    INTERFACE_DECL,
    ACTION_DECL,
    EXTENDS_CLAUSE,
    IMPLEMENTS_CLAUSE,
    RETURN_TYPE,
    ACCESS_MODIFIER,

    // declaration part
    VAR_BLOCK,
    VAR_BLOCK_QUALIFIER,
    VAR_DECL,
    /// A stray `;` among declarations (`a : INT;;`, or a `;` alone in a block).
    EMPTY_DECL,
    VAR_NAME_LIST,
    AT_CLAUSE,
    INIT_EXPR,

    // type definitions & references
    TYPE_DECL,
    STRUCT_TYPE,
    UNION_TYPE,
    ENUM_TYPE,
    SUBRANGE_TYPE,
    ARRAY_TYPE,
    ARRAY_DIMS,
    ARRAY_DIM,
    POINTER_TYPE,
    REFERENCE_TYPE,
    STRING_TYPE,
    TYPE_REF,
    GENERIC_ARGS,
    ENUM_VARIANT,

    // statements
    STMT_LIST,
    ASSIGN_STMT,
    REF_ASSIGN_STMT,
    CALL_STMT,
    IF_STMT,
    ELSIF_CLAUSE,
    ELSE_CLAUSE,
    TRY_STMT,
    CATCH_CLAUSE,
    FINALLY_CLAUSE,
    CASE_STMT,
    CASE_ARM,
    CASE_LABEL_LIST,
    CASE_LABEL,
    FOR_STMT,
    FOR_RANGE,
    WHILE_STMT,
    REPEAT_STMT,
    RETURN_STMT,
    EXIT_STMT,
    CONTINUE_STMT,
    JMP_STMT,
    LABEL,
    EMPTY_STMT,

    // expressions
    LITERAL_EXPR,
    NAME_REF,
    QUALIFIED_REF,
    INDEX_EXPR,
    DEREF_EXPR,
    CALL_EXPR,
    ARG_LIST,
    ARG,
    NAMED_ARG,
    PAREN_EXPR,
    UNARY_EXPR,
    BIN_EXPR,
    RANGE_EXPR,

    // keep last
    #[doc(hidden)]
    __LAST,
}

impl SyntaxKind {
    /// Whitespace, newlines, comments and pragmas, never semantic, always kept.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            WHITESPACE | NEWLINE | LINE_COMMENT | BLOCK_COMMENT | C_BLOCK_COMMENT | PRAGMA
        )
    }

    /// True for any reserved word (incl. word-operators and boolean literals).
    pub fn is_keyword(self) -> bool {
        matches!(self, TRUE_KW | FALSE_KW) || (AND_KW <= self && self <= ANY_STRING_KW)
    }

    /// True for a literal token.
    pub fn is_literal(self) -> bool {
        INT_NUMBER <= self && self <= FALSE_KW
    }

    /// True for structural punctuation/operator tokens.
    pub fn is_punct(self) -> bool {
        SEMICOLON <= self && self <= CARET
    }

    /// True for an interior node kind (as opposed to a leaf token).
    pub fn is_node(self) -> bool {
        SOURCE_FILE <= self && self < __LAST
    }

    /// Convert from `rowan`'s untyped kind.
    pub fn from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::try_from(raw.0).unwrap_or(ERROR)
    }

    /// Convert to `rowan`'s untyped kind.
    pub fn into_raw(self) -> rowan::SyntaxKind {
        rowan::SyntaxKind(u16::from(self))
    }
}
