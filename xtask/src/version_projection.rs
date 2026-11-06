//! Shared projections for versions exposed through CLR and Visual Studio metadata.

use semver::Version;

const CLR_COMPONENT_MAX: u64 = 65_534;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VersionProjection {
    pub(crate) full: String,
    pub(crate) system: String,
    pub(crate) assembly: String,
}

impl VersionProjection {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        if value.is_empty() || value.starts_with('v') || value.chars().any(char::is_whitespace) {
            return Err(format!("invalid semantic version `{value}`"));
        }
        let version = Version::parse(value)
            .map_err(|error| format!("invalid semantic version `{value}`: {error}"))?;
        Self::from_semver_with_full(&version, value)
    }

    pub(crate) fn from_semver(version: &Version) -> Result<Self, String> {
        Self::from_semver_with_full(version, &version.to_string())
    }

    pub(crate) fn from_tag(tag: &str, workspace: &str) -> Result<Self, String> {
        let tagged = tag
            .strip_prefix('v')
            .ok_or_else(|| format!("release tag `{tag}` must start with `v`"))?;
        let version = Self::parse(tagged)?;
        Self::parse(workspace)?;
        if tagged != workspace {
            return Err(format!(
                "release tag `{tag}` does not match workspace version `{workspace}`"
            ));
        }
        Ok(version)
    }

    fn from_semver_with_full(version: &Version, full: &str) -> Result<Self, String> {
        for (name, component) in [
            ("major", version.major),
            ("minor", version.minor),
            ("patch", version.patch),
        ] {
            if component > CLR_COMPONENT_MAX {
                return Err(format!(
                    "{name} version component `{component}` exceeds the CLR limit {CLR_COMPONENT_MAX}"
                ));
            }
        }

        let system = format!("{}.{}.{}", version.major, version.minor, version.patch);
        Ok(Self {
            full: full.to_owned(),
            assembly: format!("{system}.0"),
            system,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clr_projection_accepts_65534_and_rejects_65535() {
        let accepted = VersionProjection::parse("65534.65534.65534").unwrap();
        assert_eq!(accepted.assembly, "65534.65534.65534.0");

        let error = VersionProjection::parse("65535.0.0").unwrap_err();
        assert!(error.contains("CLR limit 65534"), "{error}");
    }
}
