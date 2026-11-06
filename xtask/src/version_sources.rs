//! Shared workspace version reading and VSIX source transformations.

use std::fs;
use std::path::Path;

use crate::version_projection::VersionProjection;

pub(crate) const ROOT_MANIFEST: &str = "Cargo.toml";
pub(crate) const VSIX_MANIFEST: &str =
    "editors/vs/TcTidy.VsExtension/source.extension.vsixmanifest";
pub(crate) const VSIX_MANIFEST_XAE15: &str =
    "editors/vs/TcTidy.VsExtension/source.xae15.extension.vsixmanifest";
pub(crate) const ASSEMBLY_INFO: &str = "editors/vs/TcTidy.VsExtension/Properties/AssemblyInfo.cs";
pub(crate) const PACKAGE_SOURCE: &str = "editors/vs/TcTidy.VsExtension/TcTidyPackage.cs";

pub(crate) fn workspace_version(input: &str) -> Result<String, String> {
    section_value(input, "workspace.package", "version")
}

pub(crate) fn read_workspace_version(root: &Path) -> Result<String, String> {
    let path = root.join(ROOT_MANIFEST);
    let input =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    workspace_version(&input)
}

pub(crate) fn section_bounds(input: &str, section: &str) -> Result<(usize, usize), String> {
    let header = format!("[{section}]");
    let mut start = None;
    let mut end = None;
    let mut offset = 0;
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == header {
            if start.is_some() {
                return Err(format!("duplicate [{section}] section"));
            }
            start = Some(offset + line.len());
        } else if trimmed.starts_with('[') && start.is_some() && end.is_none() {
            end = Some(offset);
        }
        offset += line.len();
    }
    let start = start.ok_or_else(|| format!("missing [{section}] section"))?;
    Ok((start, end.unwrap_or(input.len())))
}

pub(crate) fn section_value(input: &str, section: &str, key: &str) -> Result<String, String> {
    let (start, end) = section_bounds(input, section)?;
    let mut values = input[start..end]
        .lines()
        .filter_map(|line| parse_quoted_assignment(line, key));
    let value = values
        .next()
        .ok_or_else(|| format!("missing {key} in [{section}]"))?;
    if values.next().is_some() {
        return Err(format!("duplicate {key} in [{section}]"));
    }
    Ok(value)
}

pub(crate) fn parse_quoted_assignment(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let value = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some(value.to_string())
}

pub(crate) fn unique_line<F>(input: &str, mut predicate: F) -> Result<&str, String>
where
    F: FnMut(&str) -> bool,
{
    let mut matches = input.lines().filter(|line| predicate(line));
    let line = matches
        .next()
        .ok_or_else(|| "expected line was not found".to_string())?;
    if matches.next().is_some() {
        return Err("expected exactly one matching line".to_string());
    }
    Ok(line)
}

pub(crate) fn replace_once(
    input: &str,
    old: &str,
    new: &str,
    label: &str,
) -> Result<String, String> {
    let count = input.matches(old).count();
    if count != 1 {
        return Err(format!("expected one {label}, found {count}"));
    }
    Ok(input.replacen(old, new, 1))
}

pub(crate) fn attribute_value(
    input: &str,
    line_marker: &str,
    attribute: &str,
) -> Result<String, String> {
    unique_marker(input, line_marker)?;
    let line = unique_line(input, |line| line.contains(line_marker))?;
    let marker = format!("{attribute}=\"");
    unique_marker(line, &marker)?;
    let start = line
        .find(&marker)
        .map(|index| index + marker.len())
        .ok_or_else(|| format!("missing {attribute} on {line_marker} line"))?;
    let end = line[start..]
        .find('"')
        .map(|index| start + index)
        .ok_or_else(|| format!("unterminated {attribute} on {line_marker} line"))?;
    Ok(line[start..end].to_string())
}

pub(crate) fn replace_attribute(
    input: &str,
    line_marker: &str,
    attribute: &str,
    value: &str,
) -> Result<String, String> {
    let old = attribute_value(input, line_marker, attribute)?;
    let line = unique_line(input, |line| line.contains(line_marker))?;
    let old_fragment = format!("{attribute}=\"{old}\"");
    let new_fragment = format!("{attribute}=\"{value}\"");
    let new_line = replace_once(line, &old_fragment, &new_fragment, attribute)?;
    replace_once(input, line, &new_line, line_marker)
}

pub(crate) fn call_value(input: &str, marker: &str) -> Result<String, String> {
    unique_marker(input, marker)?;
    let line = unique_line(input, |line| line.contains(marker))?;
    let start = line
        .find(marker)
        .map(|index| index + marker.len())
        .ok_or_else(|| format!("missing {marker}"))?;
    let end = line[start..]
        .find('"')
        .map(|index| start + index)
        .ok_or_else(|| format!("unterminated string after {marker}"))?;
    Ok(line[start..end].to_string())
}

pub(crate) fn replace_call_value(input: &str, marker: &str, value: &str) -> Result<String, String> {
    let old = call_value(input, marker)?;
    replace_once(
        input,
        &format!("{marker}{old}\""),
        &format!("{marker}{value}\""),
        marker,
    )
}

pub(crate) fn installed_product_version(input: &str) -> Result<String, String> {
    unique_marker(input, "[InstalledProductRegistration(")?;
    let line = unique_line(input, |line| {
        line.contains("[InstalledProductRegistration(")
    })?;
    let ranges = quoted_ranges(line)?;
    let range = ranges
        .get(2)
        .ok_or_else(|| "InstalledProductRegistration needs three string arguments".to_string())?;
    Ok(line[range.clone()].to_string())
}

pub(crate) fn replace_installed_product_version(
    input: &str,
    value: &str,
) -> Result<String, String> {
    installed_product_version(input)?;
    let line = unique_line(input, |line| {
        line.contains("[InstalledProductRegistration(")
    })?;
    let ranges = quoted_ranges(line)?;
    let range = ranges
        .get(2)
        .ok_or_else(|| "InstalledProductRegistration needs three string arguments".to_string())?;
    let new_line = format!("{}{}{}", &line[..range.start], value, &line[range.end..]);
    replace_once(input, line, &new_line, "InstalledProductRegistration")
}

fn unique_marker(input: &str, marker: &str) -> Result<(), String> {
    match input.matches(marker).count() {
        1 => Ok(()),
        0 => Err(format!("missing version marker `{marker}`")),
        _ => Err(format!("ambiguous version marker `{marker}`")),
    }
}

fn quoted_ranges(input: &str) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut ranges = Vec::new();
    let mut start = None;
    let mut characters = input.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        if let Some(open) = start {
            match character {
                '\\' => {
                    characters.next().ok_or("unterminated C# string escape")?;
                }
                '"' => {
                    ranges.push(open..index);
                    start = None;
                }
                _ => {}
            }
        } else if character == '/' && characters.peek().is_some_and(|(_, next)| *next == '/') {
            break;
        } else if character == '"' {
            if input[..index].ends_with(['@', '$']) || input[index..].starts_with("\"\"\"") {
                return Err("version stamping requires ordinary C# string literals".into());
            }
            start = Some(index + 1);
        }
    }
    if start.is_some() {
        return Err("unterminated C# string literal".into());
    }
    Ok(ranges)
}

pub(crate) fn stamp_manifest(source: &str, version: &VersionProjection) -> Result<String, String> {
    replace_attribute(source, "<Identity ", "Version", &version.system)
}

pub(crate) fn stamp_assembly_info(
    source: &str,
    version: &VersionProjection,
) -> Result<String, String> {
    let source = replace_call_value(source, "AssemblyVersion(\"", &version.assembly)?;
    let source = replace_call_value(&source, "AssemblyFileVersion(\"", &version.assembly)?;
    replace_call_value(&source, "AssemblyInformationalVersion(\"", &version.full)
}

pub(crate) fn stamp_package(source: &str, version: &VersionProjection) -> Result<String, String> {
    replace_installed_product_version(source, &version.full)
}

/// Calculate every source update before callers write any file.
pub(crate) fn vsix_updates(
    root: &Path,
    version: &VersionProjection,
) -> Result<Vec<(&'static str, String)>, String> {
    type Transform = fn(&str, &VersionProjection) -> Result<String, String>;
    let transforms: [(&str, Transform); 4] = [
        (VSIX_MANIFEST, stamp_manifest),
        (VSIX_MANIFEST_XAE15, stamp_manifest),
        (ASSEMBLY_INFO, stamp_assembly_info),
        (PACKAGE_SOURCE, stamp_package),
    ];
    transforms
        .into_iter()
        .map(|(relative, transform)| {
            let path = root.join(relative);
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            Ok((relative, transform(&source, version)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_version_rejects_duplicate_assignments() {
        let source = "[package]\nversion = \"9.9.9\"\n\n[workspace.package]\nversion = \"1.2.3\"\nversion = \"1.2.4\"\n";
        assert!(workspace_version(source).unwrap_err().contains("duplicate"));
    }

    #[test]
    fn workspace_tables_ignore_comments_and_keep_indented_sections_separate() {
        let source = "# [workspace.package]\r\nversion = \"9.9.9\"\r\n\r\n  [workspace.package]\r\nversion = \"1.2.3\"\r\n\t[other.package]\r\nversion = \"4.5.6\"\r\n";
        assert_eq!(workspace_version(source).unwrap(), "1.2.3");
        let (start, end) = section_bounds(source, "workspace.package").unwrap();
        assert_eq!(&source[start..end], "version = \"1.2.3\"\r\n");
        assert!(workspace_version("# [workspace.package]\nversion = \"9.9.9\"\n").is_err());
        assert!(workspace_version(
            "[workspace.package]\nversion = \"1.2.3\"\n  [workspace.package]\nversion = \"4.5.6\"\n"
        ).unwrap_err().contains("duplicate"));
        assert!(
            workspace_version("[workspace.package]\n  [other.package]\nversion = \"4.5.6\"\n")
                .is_err()
        );
    }

    #[test]
    fn package_stamping_honors_escaped_quotes_and_backslashes() {
        let version = VersionProjection::parse("1.2.3-rc.4").unwrap();
        let source = r#"[InstalledProductRegistration("Product \"X\"", "Path C:\\Tools \"quoted\" café", "0.1.0")]"#;
        assert_eq!(installed_product_version(source).unwrap(), "0.1.0");
        assert_eq!(
            stamp_package(source, &version).unwrap(),
            source.replace("\"0.1.0\"", "\"1.2.3-rc.4\"")
        );
    }

    #[test]
    fn package_stamping_rejects_unsupported_or_unterminated_literals() {
        let version = VersionProjection::parse("1.2.3").unwrap();
        for source in [
            r#"[InstalledProductRegistration(@"Title", "Description", "0.1.0")]"#,
            r#"[InstalledProductRegistration($"Title", "Description", "0.1.0")]"#,
            r#"[InstalledProductRegistration("""Title""", "Description", "0.1.0")]"#,
            r#"[InstalledProductRegistration("Title", "Description", "0.1.0)]"#,
        ] {
            assert!(stamp_package(source, &version).is_err(), "{source}");
        }
    }

    #[test]
    fn stamping_rejects_ambiguous_version_markers() {
        let version = VersionProjection::parse("1.2.3").unwrap();
        assert!(
            stamp_manifest("<Identity Version=\"0.1.0\" Version=\"0.2.0\" />", &version,).is_err()
        );
        assert!(stamp_assembly_info(
            "[assembly: AssemblyVersion(\"0.1.0.0\")] [assembly: AssemblyVersion(\"0.2.0.0\")]",
            &version,
        )
        .is_err());
        assert!(stamp_package(
            "[InstalledProductRegistration(\"Title\", \"Description\", \"0.1.0\")] [InstalledProductRegistration(\"Title\", \"Description\", \"0.2.0\")]",
            &version,
        ).is_err());
    }

    #[test]
    fn stamping_preserves_other_metadata_and_windows_line_endings() {
        let version = VersionProjection::parse("1.2.3-rc.4+build.5").unwrap();
        let source = "[InstalledProductRegistration(\"Renamed product\", \"New description\", \"0.1.0\")]\r\n";
        assert_eq!(
            stamp_package(source, &version).unwrap(),
            "[InstalledProductRegistration(\"Renamed product\", \"New description\", \"1.2.3-rc.4+build.5\")]\r\n"
        );
        let source = "<Identity Id=\"TcTidy\" Version=\"0.1.0\" Language=\"en-US\" />\r\n";
        assert_eq!(
            stamp_manifest(source, &version).unwrap(),
            "<Identity Id=\"TcTidy\" Version=\"1.2.3\" Language=\"en-US\" />\r\n"
        );
        let source = "[assembly: AssemblyVersion(\"0.1.0.0\")]\r\n[assembly: AssemblyFileVersion(\"0.1.0.0\")]\r\n[assembly: AssemblyInformationalVersion(\"0.1.0\")]\r\n";
        assert_eq!(
            stamp_assembly_info(source, &version).unwrap(),
            "[assembly: AssemblyVersion(\"1.2.3.0\")]\r\n[assembly: AssemblyFileVersion(\"1.2.3.0\")]\r\n[assembly: AssemblyInformationalVersion(\"1.2.3-rc.4+build.5\")]\r\n"
        );
    }
}
