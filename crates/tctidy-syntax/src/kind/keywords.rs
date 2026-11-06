//! Case-insensitive keyword recognition generated from `keywords.csv`.

use super::SyntaxKind::{self, *};

/// Stable keyword categories recorded by the versioned inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordCategory {
    Literal,
    Operator,
    Pou,
    Oop,
    TypeDeclaration,
    VariableSection,
    Qualifier,
    ControlFlow,
    Type,
}

/// One validated row from the keyword inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeywordMetadata {
    pub spelling: &'static str,
    pub kind: SyntaxKind,
    pub category: KeywordCategory,
    pub canonical: &'static str,
    pub since_build: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/keywords_generated.rs"));

/// Returns the keyword kind if `text` (in any ASCII case) is reserved.
pub fn keyword_kind(text: &str) -> Option<SyntaxKind> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_KEYWORD_LEN {
        return None;
    }
    let mut buffer = [0u8; MAX_KEYWORD_LEN];
    for (index, byte) in bytes.iter().enumerate() {
        buffer[index] = byte.to_ascii_lowercase();
    }
    generated_keyword_kind(&buffer[..bytes.len()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_inventory_row_is_recognized_case_insensitively() {
        for keyword in KEYWORDS {
            assert_eq!(keyword_kind(keyword.spelling), Some(keyword.kind));
            assert_eq!(
                keyword_kind(&keyword.spelling.to_ascii_lowercase()),
                Some(keyword.kind)
            );
        }
    }

    #[test]
    fn inventory_is_sorted_and_unique() {
        let spellings: Vec<_> = KEYWORDS.iter().map(|keyword| keyword.spelling).collect();
        let unique: BTreeSet<_> = spellings.iter().copied().collect();
        assert_eq!(unique.len(), spellings.len());
        assert!(spellings.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn aliases_reference_canonical_rows_in_the_same_category() {
        for keyword in KEYWORDS {
            let canonical = KEYWORDS
                .iter()
                .find(|candidate| candidate.spelling == keyword.canonical)
                .expect("generator validates canonical spelling");
            assert_eq!(canonical.spelling, canonical.canonical);
            assert_eq!(canonical.category, keyword.category);
        }
    }

    #[test]
    fn build_4026_long_type_names_have_versioned_metadata() {
        for (spelling, kind) in [("LDATE_AND_TIME", LDT_KW), ("LTIME_OF_DAY", LTOD_KW)] {
            let keyword = KEYWORDS
                .iter()
                .find(|candidate| candidate.spelling == spelling)
                .expect("build 4026 keyword is present");
            assert_eq!(keyword.kind, kind);
            assert_eq!(keyword.category, KeywordCategory::Type);
            assert_eq!(keyword.canonical, spelling);
            assert_eq!(keyword.since_build, "4026");
        }
    }

    #[test]
    fn build_4026_generic_section_has_versioned_metadata() {
        let keyword = KEYWORDS
            .iter()
            .find(|candidate| candidate.spelling == "VAR_GENERIC")
            .expect("build 4026 keyword is present");
        assert_eq!(keyword.kind, VAR_GENERIC_KW);
        assert_eq!(keyword.category, KeywordCategory::VariableSection);
        assert_eq!(keyword.canonical, "VAR_GENERIC");
        assert_eq!(keyword.since_build, "4026");
    }

    #[test]
    fn exception_markers_have_build_4024_metadata() {
        for (spelling, kind) in [
            ("__TRY", TRY_KW),
            ("__CATCH", CATCH_KW),
            ("__FINALLY", FINALLY_KW),
            ("__ENDTRY", ENDTRY_KW),
        ] {
            let keyword = KEYWORDS
                .iter()
                .find(|candidate| candidate.spelling == spelling)
                .expect("exception marker is present");
            assert_eq!(keyword.kind, kind);
            assert_eq!(keyword.category, KeywordCategory::ControlFlow);
            assert_eq!(keyword.canonical, spelling);
            assert_eq!(keyword.since_build, "4024");
        }
    }

    #[test]
    fn alternative_spellings_remain_distinct() {
        assert_ne!(keyword_kind("TOD"), keyword_kind("TIME_OF_DAY"));
        assert_ne!(keyword_kind("DT"), keyword_kind("DATE_AND_TIME"));
    }

    #[test]
    fn non_keywords_are_none() {
        assert_eq!(keyword_kind("myVariable"), None);
        assert_eq!(keyword_kind(""), None);
    }
}
