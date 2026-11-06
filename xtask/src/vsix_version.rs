//! Single-source VSIX version validation and source stamping.

use std::fs;
use std::path::Path;

use crate::version_projection::VersionProjection;
use crate::version_sources::{read_workspace_version, vsix_updates};
#[cfg(test)]
use crate::version_sources::{
    stamp_assembly_info, stamp_manifest, stamp_package, workspace_version,
};

pub(crate) fn run(root: &Path, mode: &[String]) -> Result<(), String> {
    let version = match mode {
        [flag] if flag == "--workspace" => {
            let workspace = read_workspace_version(root)?;
            VersionProjection::parse(&workspace)?
        }
        [flag, tag] if flag == "--tag" => {
            let workspace = read_workspace_version(root)?;
            VersionProjection::from_tag(tag, &workspace)?
        }
        _ => {
            return Err(
                "usage: xtask vsix-version <--workspace | --tag vX.Y.Z[-prerelease]>".to_owned(),
            )
        }
    };

    for (relative, stamped) in vsix_updates(root, &version)? {
        let path = root.join(relative);
        fs::write(&path, stamped).map_err(|error| format!("write {}: {error}", path.display()))?;
    }
    print_outputs(&version);
    Ok(())
}

fn print_outputs(version: &VersionProjection) {
    println!("full={}", version.full);
    println!("system={}", version.system);
    println!("assembly={}", version.assembly);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version_sources::{
        ASSEMBLY_INFO, PACKAGE_SOURCE, ROOT_MANIFEST, VSIX_MANIFEST, VSIX_MANIFEST_XAE15,
    };

    fn fixture(root: &Path) -> Vec<(&'static str, String)> {
        let files = vec![
            (ROOT_MANIFEST, "[workspace.package]\nversion = \"1.2.3-rc.4\"\n".to_owned()),
            (VSIX_MANIFEST, "<Identity Id=\"TcTidy\" Version=\"0.1.0\" />\n".to_owned()),
            (VSIX_MANIFEST_XAE15, "<Identity Id=\"TcTidy\" Version=\"0.1.0\" />\n".to_owned()),
            (ASSEMBLY_INFO, "[assembly: AssemblyVersion(\"0.1.0.0\")]\n[assembly: AssemblyFileVersion(\"0.1.0.0\")]\n[assembly: AssemblyInformationalVersion(\"0.1.0\")]\n".to_owned()),
            (PACKAGE_SOURCE, "[InstalledProductRegistration(\"Renamed product\", \"New description\", \"0.1.0\")]\n".to_owned()),
        ];
        for (relative, source) in &files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        files
    }

    #[test]
    fn failed_stamping_or_tag_validation_leaves_sources_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut files = fixture(root);
        assert!(run(root, &["--tag".into(), "v1.2.4".into()]).is_err());
        for (relative, source) in &files {
            assert_eq!(fs::read_to_string(root.join(relative)).unwrap(), *source);
        }

        // Fail the last transformation after the other three have been staged.
        let invalid = "[InstalledProductRegistration(\"Missing version\")]\n";
        fs::write(root.join(PACKAGE_SOURCE), invalid).unwrap();
        files.last_mut().unwrap().1 = invalid.into();
        assert!(run(root, &["--workspace".into()]).is_err());
        for (relative, source) in &files {
            assert_eq!(fs::read_to_string(root.join(relative)).unwrap(), *source);
        }
    }

    #[test]
    fn workspace_stamping_updates_both_shells_and_full_display_version() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fixture(root);
        run(root, &["--workspace".into()]).unwrap();
        for relative in [VSIX_MANIFEST, VSIX_MANIFEST_XAE15] {
            assert_eq!(
                crate::version_sources::attribute_value(
                    &fs::read_to_string(root.join(relative)).unwrap(),
                    "<Identity ",
                    "Version",
                )
                .unwrap(),
                "1.2.3"
            );
        }
        assert_eq!(
            crate::version_sources::installed_product_version(
                &fs::read_to_string(root.join(PACKAGE_SOURCE)).unwrap(),
            )
            .unwrap(),
            "1.2.3-rc.4"
        );
        let expected = stamp_assembly_info(
            "[assembly: AssemblyVersion(\"0.1.0.0\")]\n[assembly: AssemblyFileVersion(\"0.1.0.0\")]\n[assembly: AssemblyInformationalVersion(\"0.1.0\")]\n",
            &VersionProjection::parse("1.2.3-rc.4").unwrap(),
        ).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(ASSEMBLY_INFO)).unwrap(),
            expected
        );
    }

    #[test]
    fn stable_version_maps_to_system_version_fields() {
        let version = VersionProjection::parse("1.2.3").unwrap();
        assert_eq!(version.full, "1.2.3");
        assert_eq!(version.system, "1.2.3");
        assert_eq!(version.assembly, "1.2.3.0");
    }

    #[test]
    fn prerelease_suffix_is_kept_for_display_but_removed_from_system_version() {
        let version = VersionProjection::parse("1.2.3-rc.4").unwrap();
        assert_eq!(version.full, "1.2.3-rc.4");
        assert_eq!(version.system, "1.2.3");
        assert_eq!(version.assembly, "1.2.3.0");
    }

    #[test]
    fn build_metadata_is_kept_out_of_system_version_fields() {
        let version = VersionProjection::parse("1.2.3-rc-feature.4+build.05").unwrap();
        assert_eq!(version.full, "1.2.3-rc-feature.4+build.05");
        assert_eq!(version.system, "1.2.3");
        assert_eq!(version.assembly, "1.2.3.0");
    }

    #[test]
    fn tag_must_match_the_workspace_version() {
        assert_eq!(
            VersionProjection::from_tag("v1.2.3-rc.4", "1.2.3-rc.4")
                .unwrap()
                .full,
            "1.2.3-rc.4"
        );
        let error = VersionProjection::from_tag("v1.2.4", "1.2.3").unwrap_err();
        assert!(
            error.contains("does not match workspace version"),
            "{error}"
        );
    }

    #[test]
    fn invalid_semver_is_rejected() {
        for value in [
            "1.2",
            "01.2.3",
            "1.2.3-",
            "1.2.3-rc..1",
            "1.2.3-01",
            "1.2.3+",
            "v1.2.3",
            "65535.2.3",
        ] {
            assert!(VersionProjection::parse(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn workspace_version_comes_only_from_workspace_package() {
        let manifest = r#"
[package]
version = "9.9.9"

[workspace.package]
version = "1.2.3-rc.4"
"#;
        assert_eq!(workspace_version(manifest).unwrap(), "1.2.3-rc.4");
    }

    #[test]
    fn stamping_updates_every_exposed_source_value() {
        let version = VersionProjection::parse("1.2.3-rc.4").unwrap();
        let manifest = stamp_manifest(
            r#"<Identity Id="TcTidy" Version="0.0.0" Language="en-US" />"#,
            &version,
        )
        .unwrap();
        assert!(manifest.contains("Version=\"1.2.3\""));

        let assembly = stamp_assembly_info(
            r#"[assembly: AssemblyVersion("0.0.0.0")]
[assembly: AssemblyFileVersion("0.0.0.0")]
[assembly: AssemblyInformationalVersion("0.0.0-dev")]"#,
            &version,
        )
        .unwrap();
        assert!(assembly.contains("AssemblyVersion(\"1.2.3.0\")"));
        assert!(assembly.contains("AssemblyFileVersion(\"1.2.3.0\")"));
        assert!(assembly.contains("AssemblyInformationalVersion(\"1.2.3-rc.4\")"));

        let package = stamp_package(
            r#"[InstalledProductRegistration("TcTidy for TwinCAT", "Format TwinCAT Structured Text with tctidy.", "0.0.0-dev")]"#,
            &version,
        )
        .unwrap();
        assert!(package.ends_with("\"1.2.3-rc.4\")]"));
    }
}
