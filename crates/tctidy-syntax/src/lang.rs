//! The `rowan::Language` binding for TcTidy's [`SyntaxKind`].

use crate::kind::SyntaxKind;

/// Marker type tying the [`SyntaxKind`] to `rowan`'s generic tree machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TcStLang {}

impl rowan::Language for TcStLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into_raw()
    }
}
