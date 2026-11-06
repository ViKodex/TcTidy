//! Bind candidate halves to one commit and uploaded files to their exact hashes.

use super::{array, file_digest, json, relative, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub(crate) fn provenance(tag: &str, sha: &str, linux: &Path, windows: &Path) -> Result {
    if sha.len() != 40
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected source SHA must be a full lowercase 40-hex commit ID".into());
    }
    let version = tag
        .strip_prefix('v')
        .filter(|v| !v.is_empty())
        .ok_or("tag must start with v")?;
    for (component, path) in [("linux", linux), ("windows", windows)] {
        let actual = json(path)?;
        let expected =
            json!({"schema":1,"component":component,"tag":tag,"version":version,"source_sha":sha});
        if actual != expected {
            return Err(format!(
                "{component} provenance must be {expected}, got {actual}"
            ));
        }
    }
    println!("candidate provenance OK: {tag} from {sha}");
    Ok(())
}

pub(crate) fn uploaded(tag: &str, directory: &Path, response: &Path) -> Result {
    validate_uploaded(tag, directory, &json(response)?)?;
    println!("uploaded release assets match the candidate: {tag}");
    Ok(())
}

fn validate_uploaded(tag: &str, directory: &Path, response: &Value) -> Result {
    if !tag.starts_with('v') || tag.len() == 1 {
        return Err("release tag must start with v".into());
    }
    let pages = response
        .as_array()
        .ok_or("release response must be a paginated array")?;
    let mut matches = Vec::new();
    for page in pages {
        for release in page
            .as_array()
            .ok_or("each release page must be an array")?
        {
            if release["tag_name"] == tag {
                matches.push(release);
            }
        }
    }
    if matches.len() != 1 {
        return Err(format!("expected exactly one release for {tag}"));
    }
    let release = matches[0];
    if release["draft"] != true {
        return Err("release must still be a draft".into());
    }
    let mut local = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("candidate contains a non-file entry".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "invalid asset filename")?;
        local.insert(name, entry.path());
    }
    if local.is_empty() {
        return Err("candidate directory is empty".into());
    }
    let mut assets = BTreeMap::new();
    for asset in array(release, "assets")? {
        let name = super::string(asset, "name")?;
        if relative(name)? != name || name.contains('/') {
            return Err("invalid asset name".into());
        }
        if assets.insert(name.to_owned(), asset).is_some() {
            return Err(format!("duplicate asset {name}"));
        }
    }
    if assets.keys().collect::<BTreeSet<_>>() != local.keys().collect() {
        return Err("uploaded asset set mismatch".into());
    }
    for (name, path) in local {
        let asset = assets[&name];
        if asset["state"] != "uploaded" {
            return Err(format!("{name} is not fully uploaded"));
        }
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if asset["size"].as_u64() != Some(size) {
            return Err(format!("asset size mismatch: {name}"));
        }
        let expected = format!("sha256:{}", file_digest(&path)?);
        if asset["digest"].as_str() != Some(expected.as_str()) {
            return Err(format!("asset digest mismatch: {name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provenance_rejects_missing_mixed_and_malformed_halves() {
        let temp = tempfile::tempdir().unwrap();
        let linux = temp.path().join("linux.json");
        let windows = temp.path().join("windows.json");
        let sha = "a".repeat(40);
        for (path, component) in [(&linux, "linux"), (&windows, "windows")] {
            super::super::write_json(path, &json!({"schema":1,"component":component,"tag":"v0.4.0","version":"0.4.0","source_sha":sha})).unwrap();
        }
        provenance("v0.4.0", &sha, &linux, &windows).unwrap();
        let mut data = json(&windows).unwrap();
        for (field, wrong) in [
            ("schema", json!(true)),
            ("source_sha", json!("b".repeat(40))),
            ("component", json!("linux")),
            ("tag", json!("v0.4.1")),
            ("version", json!("0.4.1")),
        ] {
            let old = data[field].clone();
            data[field] = wrong;
            super::super::write_json(&windows, &data).unwrap();
            assert!(provenance("v0.4.0", &sha, &linux, &windows).is_err());
            data[field] = old;
        }
        fs::remove_file(&windows).unwrap();
        assert!(provenance("v0.4.0", &sha, &linux, &windows).is_err());
        assert!(provenance("v0.4.0", "bad", &linux, &windows).is_err());
    }

    #[test]
    fn uploaded_assets_require_exact_draft_bytes_and_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("asset.txt");
        fs::write(&path, "payload").unwrap();
        let asset = json!({"name":"asset.txt","state":"uploaded","size":7,"digest":format!("sha256:{}", file_digest(&path).unwrap())});
        let release = json!({"tag_name":"v0.4.0","draft":true,"assets":[asset]});
        let response = json!([[release]]);
        validate_uploaded("v0.4.0", temp.path(), &response).unwrap();
        for (field, wrong) in [
            ("size", json!(true)),
            ("size", json!(8)),
            ("state", json!("starter")),
            ("digest", Value::Null),
            ("digest", json!("sha256:invalid")),
            ("name", json!("../asset.txt")),
        ] {
            let mut response = response.clone();
            response[0][0]["assets"][0][field] = wrong;
            assert!(
                validate_uploaded("v0.4.0", temp.path(), &response).is_err(),
                "{field}"
            );
        }
        for response in [
            json!([[{"tag_name":"v0.4.0","draft":false,"assets":[asset]}]]),
            json!([[release, release]]),
            json!([[{"tag_name":"v0.4.0","draft":true,"assets":[asset,asset]}]]),
            json!([[{"tag_name":"v0.4.0","draft":true,"assets":[]}]]),
            json!([release]),
            json!([[{"tag_name":"v0.4.1","draft":true,"assets":[asset]}]]),
        ] {
            assert!(validate_uploaded("v0.4.0", temp.path(), &response).is_err());
        }
        fs::write(path, "changed").unwrap();
        assert!(validate_uploaded("v0.4.0", temp.path(), &response).is_err());
    }
}
