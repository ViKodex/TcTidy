//! Deterministic keyword-table generation and validation.

use std::collections::{BTreeMap, BTreeSet};

const HEADER: &str = "spelling,syntax_kind,category,canonical,since_build";
const CATEGORIES: &[(&str, &str)] = &[
    ("literal", "Literal"),
    ("operator", "Operator"),
    ("pou", "Pou"),
    ("oop", "Oop"),
    ("type_declaration", "TypeDeclaration"),
    ("variable_section", "VariableSection"),
    ("qualifier", "Qualifier"),
    ("control_flow", "ControlFlow"),
    ("type", "Type"),
];

#[derive(Clone, Debug)]
struct Row {
    spelling: String,
    syntax_kind: String,
    category: String,
    canonical: String,
    since_build: String,
}

pub fn generate(csv: &str, kind_source: &str) -> Result<String, String> {
    let mut rows = parse_rows(csv)?;
    validate_rows(&rows, kind_source)?;
    rows.sort_by(|left, right| left.spelling.cmp(&right.spelling));

    let max_len = rows.iter().map(|row| row.spelling.len()).max().unwrap_or(0);
    let type_kinds: BTreeSet<&str> = rows
        .iter()
        .filter(|row| row.category == "type")
        .map(|row| row.syntax_kind.as_str())
        .collect();

    let mut output = String::from(
        "// @generated from keywords.csv; edit keywords.csv instead.\n\
         // The build compiles its own copy from OUT_DIR. src/kind/keywords_generated.rs\n\
         // is a committed copy for review, written by `cargo run -p xtask -- keywords`\n\
         // and checked by `cargo run -p xtask -- keywords --check`.\n\n",
    );
    output.push_str(&format!("const MAX_KEYWORD_LEN: usize = {max_len};\n\n"));
    output.push_str("pub static KEYWORDS: &[KeywordMetadata] = &[\n");
    for row in &rows {
        output.push_str("    KeywordMetadata { ");
        output.push_str(&format!(
            "spelling: \"{}\", kind: {}, category: KeywordCategory::{}, canonical: \"{}\", since_build: \"{}\"",
            row.spelling,
            row.syntax_kind,
            category_variant(&row.category).expect("validated category"),
            row.canonical,
            row.since_build
        ));
        output.push_str(" },\n");
    }
    output.push_str("];\n\n");

    output.push_str("fn generated_keyword_kind(lowercase: &[u8]) -> Option<SyntaxKind> {\n");
    output.push_str("    let kind = match lowercase {\n");
    for row in &rows {
        output.push_str(&format!(
            "        b\"{}\" => {},\n",
            row.spelling.to_ascii_lowercase(),
            row.syntax_kind
        ));
    }
    output.push_str("        _ => return None,\n    };\n    Some(kind)\n}\n\n");

    output.push_str("pub fn is_type_keyword(kind: SyntaxKind) -> bool {\n");
    output.push_str("    matches!(kind, ");
    for (index, kind) in type_kinds.iter().enumerate() {
        if index > 0 {
            output.push_str(" | ");
        }
        output.push_str(kind);
    }
    output.push_str(")\n}\n");
    Ok(output)
}

fn parse_rows(csv: &str) -> Result<Vec<Row>, String> {
    let mut lines = csv.lines();
    let header = lines.next().unwrap_or_default().trim_end_matches('\r');
    if header != HEADER {
        return Err(format!("invalid keyword CSV header: expected {HEADER:?}"));
    }

    let mut rows = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line_number = index + 2;
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() != 5 {
            return Err(format!(
                "keywords.csv:{line_number}: expected 5 comma-separated fields"
            ));
        }
        rows.push(Row {
            spelling: fields[0].to_string(),
            syntax_kind: fields[1].to_string(),
            category: fields[2].to_string(),
            canonical: fields[3].to_string(),
            since_build: fields[4].to_string(),
        });
    }
    if rows.is_empty() {
        return Err("keyword inventory is empty".to_string());
    }
    Ok(rows)
}

fn validate_rows(rows: &[Row], kind_source: &str) -> Result<(), String> {
    let variants = syntax_kind_variants(kind_source)?;
    let variant_set: BTreeSet<&str> = variants.iter().map(String::as_str).collect();
    let keyword_kinds = keyword_kinds(&variants)?;
    let type_kinds = variant_range(&variants, "BOOL_KW", "ANY_STRING_KW")?;

    let mut by_spelling = BTreeMap::new();
    let mut mapped_kinds = BTreeSet::new();
    for row in rows {
        validate_spelling(&row.spelling, "spelling")?;
        validate_spelling(&row.canonical, "canonical spelling")?;
        if !row.syntax_kind.ends_with("_KW") || !is_upper_identifier(&row.syntax_kind) {
            return Err(format!(
                "{}: invalid syntax kind {}",
                row.spelling, row.syntax_kind
            ));
        }
        if !variant_set.contains(row.syntax_kind.as_str()) {
            return Err(format!(
                "{}: syntax kind {} is missing from SyntaxKind",
                row.spelling, row.syntax_kind
            ));
        }
        if !keyword_kinds.contains(row.syntax_kind.as_str()) {
            return Err(format!(
                "{}: {} is not mapped as a keyword SyntaxKind",
                row.spelling, row.syntax_kind
            ));
        }
        if category_variant(&row.category).is_none() {
            return Err(format!(
                "{}: unknown keyword category {}",
                row.spelling, row.category
            ));
        }
        if row.since_build != "baseline"
            && (row.since_build.is_empty()
                || !row.since_build.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(format!(
                "{}: since_build must be `baseline` or a numeric TwinCAT build",
                row.spelling
            ));
        }
        if row.category == "type" && !type_kinds.contains(row.syntax_kind.as_str()) {
            return Err(format!(
                "{}: type keyword {} has no type-parser mapping",
                row.spelling, row.syntax_kind
            ));
        }
        if row.category != "type" && type_kinds.contains(row.syntax_kind.as_str()) {
            return Err(format!(
                "{}: type SyntaxKind {} must use the `type` category",
                row.spelling, row.syntax_kind
            ));
        }
        if by_spelling.insert(row.spelling.as_str(), row).is_some() {
            return Err(format!("duplicate keyword spelling: {}", row.spelling));
        }
        mapped_kinds.insert(row.syntax_kind.as_str());
    }

    for row in rows {
        let canonical = by_spelling.get(row.canonical.as_str()).ok_or_else(|| {
            format!(
                "{}: alias references missing canonical spelling {}",
                row.spelling, row.canonical
            )
        })?;
        if canonical.spelling != canonical.canonical {
            return Err(format!(
                "{}: canonical spelling {} is itself an alias",
                row.spelling, row.canonical
            ));
        }
        if canonical.category != row.category {
            return Err(format!(
                "{}: alias category {} differs from canonical category {}",
                row.spelling, row.category, canonical.category
            ));
        }
    }

    let missing: Vec<_> = keyword_kinds.difference(&mapped_kinds).copied().collect();
    if !missing.is_empty() {
        return Err(format!(
            "keyword SyntaxKind values missing from keywords.csv: {}",
            missing.join(", ")
        ));
    }
    Ok(())
}

fn validate_spelling(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!("invalid {field}: {value:?}"));
    }
    Ok(())
}

fn is_upper_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn category_variant(category: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .find_map(|(name, variant)| (*name == category).then_some(*variant))
}

fn syntax_kind_variants(source: &str) -> Result<Vec<String>, String> {
    let enum_start = source
        .find("pub enum SyntaxKind {")
        .ok_or_else(|| "could not find SyntaxKind enum".to_string())?;
    let mut variants = Vec::new();
    for raw in source[enum_start..].lines().skip(1) {
        let line = raw.split("//").next().unwrap_or_default().trim();
        if line == "}" {
            break;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = line.split([',', '=']).next().unwrap_or_default().trim();
        if is_upper_identifier(name) {
            variants.push(name.to_string());
        }
    }
    if variants.is_empty() {
        return Err("SyntaxKind enum has no parsed variants".to_string());
    }
    Ok(variants)
}

fn keyword_kinds(variants: &[String]) -> Result<BTreeSet<&str>, String> {
    let mut kinds = variant_range(variants, "AND_KW", "ANY_STRING_KW")?;
    for literal in ["TRUE_KW", "FALSE_KW"] {
        if !variants.iter().any(|variant| variant == literal) {
            return Err(format!("SyntaxKind is missing {literal}"));
        }
        kinds.insert(literal);
    }
    Ok(kinds)
}

fn variant_range<'a>(
    variants: &'a [String],
    first: &str,
    last: &str,
) -> Result<BTreeSet<&'a str>, String> {
    let start = variants
        .iter()
        .position(|variant| variant == first)
        .ok_or_else(|| format!("SyntaxKind is missing {first}"))?;
    let end = variants
        .iter()
        .position(|variant| variant == last)
        .ok_or_else(|| format!("SyntaxKind is missing {last}"))?;
    if start > end {
        return Err(format!("invalid SyntaxKind range {first}..={last}"));
    }
    Ok(variants[start..=end].iter().map(String::as_str).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = include_str!("keywords.csv");
    const KINDS: &str = include_str!("src/kind.rs");

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(generate(CSV, KINDS), generate(CSV, KINDS));
    }

    #[test]
    fn duplicate_spellings_fail() {
        let csv = format!("{CSV}TRUE,TRUE_KW,literal,TRUE,baseline\n");
        let error = generate(&csv, KINDS).expect_err("duplicate must fail");
        assert!(error.contains("duplicate keyword spelling"), "{error}");
    }

    #[test]
    fn alias_without_canonical_form_fails() {
        let csv = CSV.replace(
            "TOD,TOD_KW,type,TIME_OF_DAY,baseline",
            "TOD,TOD_KW,type,MISSING_CANONICAL,baseline",
        );
        let error = generate(&csv, KINDS).expect_err("missing canonical must fail");
        assert!(error.contains("missing canonical spelling"), "{error}");
    }

    #[test]
    fn missing_parser_mapping_fails() {
        let csv = CSV.replace(
            "BOOL,BOOL_KW,type,BOOL,baseline",
            "BOOL,IF_KW,type,BOOL,baseline",
        );
        let error = generate(&csv, KINDS).expect_err("non-type kind must fail");
        assert!(error.contains("no type-parser mapping"), "{error}");
    }
}
