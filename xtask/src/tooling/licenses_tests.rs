use super::*;

const LICENSE_FILES: [&str; 3] = ["LICENSE-MIT", "LICENSE-APACHE", "LICENSE"];

struct Fixture {
    temporary: tempfile::TempDir,
    args: Vsix,
    metadata: Value,
    about: Value,
    tree: String,
    inventory: Value,
    assets: Value,
    package: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let payload = root.join("payload");
        let packages = root.join("packages");
        let package = packages.join("toolkit/1.0.0");
        let metadata = json!({"workspace_members":["root","internal"],"packages":[
            {"id":"root","name":"tctidy-cli","version":"0.4.0","license":"MIT OR Apache-2.0"},
            {"id":"internal","name":"tctidy","version":"0.4.0","license":"MIT OR Apache-2.0"}
        ]});
        let about = json!({"crates":[
            {"package":{"id":"runtime","name":"runtime","version":"1.0.0","source":"registry+https://github.com/rust-lang/crates.io-index","license":"MIT"}},
            {"package":{"id":"build","name":"build-only","version":"1.0.0","source":"registry+index","license":"MIT"}},
            {"package":{"id":"macro","name":"proc-macro","version":"1.0.0","source":"registry+index","license":"MIT"}}
        ],"licenses":[{"name":"MIT License","id":"MIT","text":"Full license text.\r\n","used_by":[
            {"crate":{"id":"runtime","name":"runtime","version":"1.0.0","repository":"https://example.invalid/runtime"}},
            {"crate":{"id":"build","name":"build-only","version":"1.0.0"}},
            {"crate":{"id":"macro","name":"proc-macro","version":"1.0.0"}}
        ]}]});
        let tree = "tctidy-cli v0.4.0\ntctidy v0.4.0\nruntime v1.0.0\n".to_owned();
        let cargo_license = json!({"type":"expression","value":"MIT OR Apache-2.0","files":["LICENSE-MIT","LICENSE-APACHE"]});
        let nuget_license =
            json!({"type":"expression","value":"Apache-2.0","files":["LICENSE-APACHE"]});
        let inventory = json!({"schema":1,"projectFiles":["TcTidy.VsExtension.dll","TcTidy.VsExtension.Core.dll"],
            "cargoBundles":[{"file":"tctidy.exe","package":"tctidy-cli","version":"0.4.0","target":"x86_64-pc-windows-msvc","license":cargo_license,"inventoryFile":"CARGO-DEPENDENCIES.json","noticeFile":"THIRD-PARTY-NOTICES.md"}],
            "nugetFiles":[{"file":"Toolkit.dll","package":"Toolkit","version":"1.0.0","license":nuget_license,"notice":{"required":false}}]});
        let assets = json!({"targets":{"net472":{"Toolkit/1.0.0":{"type":"package","runtime":{"lib/net472/Toolkit.dll":{}}}}},
            "libraries":{"Toolkit/1.0.0":{"type":"package","path":"toolkit/1.0.0","files":["toolkit.nuspec","lib/net472/Toolkit.dll"]}},
            "packageFolders":{packages.to_string_lossy().to_string():{}}});
        let args = Vsix {
            payload: payload.clone(),
            inventory: root.join("inventory.json"),
            license: LICENSE_FILES.iter().map(|name| root.join(name)).collect(),
            project_assets: root.join("project.assets.json"),
            cargo_metadata: root.join("metadata.json"),
            cargo_about: root.join("about.json"),
            cargo_tree: root.join("tree.txt"),
        };
        for name in [
            "TcTidy.VsExtension.dll",
            "TcTidy.VsExtension.Core.dll",
            "tctidy.exe",
            "Toolkit.dll",
        ] {
            write(&payload.join(name), name.as_bytes()).unwrap();
        }
        for (source, name) in args.license.iter().zip(LICENSE_FILES) {
            write(source, name.as_bytes()).unwrap();
            write(&payload.join(name), name.as_bytes()).unwrap();
        }
        write(
            &payload.join(EXTENSION_NOTICE),
            b"# Extension notices\n\n- Package: Toolkit (see `LICENSE-APACHE`)\n",
        )
        .unwrap();
        write(&package.join("lib/net472/Toolkit.dll"), b"Toolkit.dll").unwrap();
        write(&package.join("toolkit.nuspec"),b"<package xmlns=\"http://schemas.microsoft.com/packaging/2013/05/nuspec.xsd\"><metadata><id>Toolkit</id><version>1.0.0</version><license type=\"expression\">Apache-2.0</license></metadata></package>").unwrap();
        let fixture = Self {
            temporary,
            args,
            metadata,
            about,
            tree,
            inventory,
            assets,
            package,
        };
        fixture.sync();
        let (inventory, notice) = evidence(
            &fixture.metadata,
            &fixture.about,
            &fixture.tree,
            "tctidy-cli",
            "x86_64-pc-windows-msvc",
        )
        .unwrap();
        write_json(&payload.join("CARGO-DEPENDENCIES.json"), &inventory).unwrap();
        write(&payload.join("THIRD-PARTY-NOTICES.md"), notice.as_bytes()).unwrap();
        fixture
    }

    fn sync(&self) {
        write_json(&self.args.inventory, &self.inventory).unwrap();
        write_json(
            &self.args.payload.join("THIRD-PARTY-INVENTORY.json"),
            &self.inventory,
        )
        .unwrap();
        write_json(&self.args.project_assets, &self.assets).unwrap();
        write_json(&self.args.cargo_metadata, &self.metadata).unwrap();
        write_json(&self.args.cargo_about, &self.about).unwrap();
        write(&self.args.cargo_tree, self.tree.as_bytes()).unwrap();
    }

    fn validate(&self) -> Result {
        validate_vsix(&self.args)
    }
}

#[test]
fn exact_runtime_evidence_excludes_workspace_build_and_macro_crates() {
    let fixture = Fixture::new();
    let (inventory, notice) = evidence(
        &fixture.metadata,
        &fixture.about,
        &fixture.tree,
        "tctidy-cli",
        "x86_64-pc-windows-msvc",
    )
    .unwrap();
    assert_eq!(inventory["packages"].as_array().unwrap().len(), 1);
    assert_eq!(inventory["packages"][0]["name"], "runtime");
    assert!(!notice.contains("build-only"));
    assert!(!notice.contains("proc-macro"));
    assert!(!notice.contains('\r'));
    assert_eq!(inventory["noticeSha256"], digest(notice.as_bytes()));
    fixture.validate().unwrap();
}

#[test]
fn rejects_incomplete_or_ambiguous_cargo_evidence() {
    let fixture = Fixture::new();
    for tree in [
        "",
        "invalid",
        "runtime v1.0.0\n",
        "tctidy-cli v0.4.0\nmissing v1.0.0\n",
    ] {
        assert!(evidence(
            &fixture.metadata,
            &fixture.about,
            tree,
            "tctidy-cli",
            "target"
        )
        .is_err());
    }
    for field in ["name", "version", "license", "source", "id"] {
        let mut about = fixture.about.clone();
        about["crates"][0]["package"][field] = Value::Null;
        assert!(
            evidence(
                &fixture.metadata,
                &about,
                &fixture.tree,
                "tctidy-cli",
                "target"
            )
            .is_err(),
            "{field}"
        );
    }
    let mut about = fixture.about.clone();
    about["licenses"] = json!([]);
    assert!(evidence(
        &fixture.metadata,
        &about,
        &fixture.tree,
        "tctidy-cli",
        "target"
    )
    .is_err());
    about = fixture.about.clone();
    let duplicate = about["crates"][0].clone();
    about["crates"].as_array_mut().unwrap().push(duplicate);
    assert!(evidence(
        &fixture.metadata,
        &about,
        &fixture.tree,
        "tctidy-cli",
        "target"
    )
    .is_err());
}

#[test]
fn generate_prefetches_locked_dependencies_and_uses_a_temporary_about_output() {
    let fixture = Fixture::new();
    let root = fixture.temporary.path().join("generated");
    let args = Generate {
        manifest: "Cargo.toml".into(),
        package: "tctidy-cli".into(),
        target: "x86_64-pc-windows-msvc".into(),
        inventory: root.join("inventory.json"),
        notice: root.join("notice.md"),
        metadata_output: root.join("metadata.json"),
        about_output: root.join("about.json"),
        tree_output: root.join("tree.txt"),
    };
    for invalid in [false, true] {
        let mut commands = Vec::new();
        let mut about_path = PathBuf::new();
        let result = generate_with(&args, |command| {
            commands.push(command.to_vec());
            assert!(command.iter().any(|arg| arg == "--locked"));
            match command[0].as_str() {
                "fetch" => {
                    assert!(!command.iter().any(|arg| arg == "--target"));
                    Ok(String::new())
                }
                "metadata" => Ok(fixture.metadata.to_string()),
                "about" => {
                    let index = command
                        .iter()
                        .position(|arg| arg == "--output-file")
                        .unwrap();
                    about_path = command[index + 1].clone().into();
                    let output = if invalid {
                        "invalid".to_owned()
                    } else {
                        fixture.about.to_string()
                    };
                    write(&about_path, output.as_bytes())?;
                    Ok(String::new())
                }
                "tree" => {
                    assert!(command.iter().any(|arg| arg == "normal,no-proc-macro"));
                    Ok(fixture.tree.clone())
                }
                _ => panic!("unexpected command"),
            }
        });
        assert_eq!(result.is_err(), invalid);
        assert_eq!(commands[0][0], "fetch");
        assert!(!about_path.exists());
        assert!(!about_path.parent().unwrap().exists());
        if !invalid {
            assert!(args.inventory.is_file());
        }
    }
}

#[test]
fn supplied_evidence_must_be_byte_identical() {
    let temporary = tempfile::tempdir().unwrap();
    let a = temporary.path().join("a.json");
    let b = temporary.path().join("b.json");
    write(&a, b"{\"a\":1}\n").unwrap();
    write(&b, b"{\"a\":1}\n").unwrap();
    compare(&a, &b).unwrap();
    write(&b, b"{ \"a\": 1 }\n").unwrap();
    assert!(compare(&a, &b).is_err());
}

#[test]
fn duplicate_mappings_and_package_identities_fail_across_categories() {
    for case in 0..5 {
        let mut fixture = Fixture::new();
        match case {
            0 => fixture.inventory["projectFiles"]
                .as_array_mut()
                .unwrap()
                .push(json!("TcTidy.VsExtension.dll")),
            1 => {
                let duplicate = fixture.inventory["cargoBundles"][0].clone();
                fixture.inventory["cargoBundles"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            2 => {
                let duplicate = fixture.inventory["nugetFiles"][0].clone();
                fixture.inventory["nugetFiles"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            3 => fixture.inventory["nugetFiles"][0]["file"] = json!("tctidy.exe"),
            4 => {
                fixture.inventory["nugetFiles"][0]["package"] = json!("tctidy-cli");
                fixture.inventory["nugetFiles"][0]["version"] = json!("0.4.0");
            }
            _ => unreachable!(),
        }
        fixture.sync();
        assert!(fixture.validate().is_err(), "case {case}");
    }
}

#[test]
fn every_license_file_the_inventory_names_must_ship() {
    for (group, file) in [
        ("cargoBundles", "LICENSE-MIT"),
        ("nugetFiles", "LICENSE-APACHE"),
    ] {
        let fixture = Fixture::new();
        fs::remove_file(fixture.args.payload.join(file)).unwrap();
        assert!(fixture.validate().is_err(), "{group}: {file} removed");

        let mut fixture = Fixture::new();
        fixture.inventory[group][0]["license"]["files"] = json!(["missing"]);
        fixture.sync();
        assert!(fixture.validate().is_err(), "{group}: unknown file");
    }
}

#[test]
fn the_cargo_bundle_must_declare_the_workspace_license() {
    let mut fixture = Fixture::new();
    fixture.inventory["cargoBundles"][0]["license"]["value"] = json!("Apache-2.0");
    fixture.sync();
    assert!(fixture.validate().is_err());
}

#[test]
fn inventories_and_licenses_must_match_source_bytes() {
    for name in [
        "THIRD-PARTY-INVENTORY.json",
        "LICENSE",
        "LICENSE-MIT",
        "LICENSE-APACHE",
    ] {
        let fixture = Fixture::new();
        let path = fixture.args.payload.join(name);
        let mut bytes = read(&path).unwrap();
        bytes.push(b' ');
        write(&path, &bytes).unwrap();
        assert!(fixture.validate().is_err(), "{name}");
    }
}

#[test]
fn rejects_missing_wrong_and_nonruntime_dependency_inventory_entries() {
    for case in 0..5 {
        let fixture = Fixture::new();
        let path = fixture.args.payload.join("CARGO-DEPENDENCIES.json");
        let mut data = json(&path).unwrap();
        match case {
            0 => data["packages"] = json!([]),
            1 => data["packages"][0]["license"] = json!("GPL-3.0"),
            2 => {
                let mut entry = data["packages"][0].clone();
                entry["name"] = json!("build-only");
                data["packages"].as_array_mut().unwrap().push(entry);
            }
            3 => data["root"]["version"] = json!("0.4.1"),
            4 => data["noticeSha256"] = json!("0".repeat(64)),
            _ => unreachable!(),
        }
        write_json(&path, &data).unwrap();
        assert!(fixture.validate().is_err(), "case {case}");
    }
    for (field, value) in [("package", "unknown"), ("version", "0.4.1")] {
        let mut fixture = Fixture::new();
        fixture.inventory["cargoBundles"][0][field] = json!(value);
        fixture.sync();
        assert!(fixture.validate().is_err());
    }
}

#[test]
fn notices_cannot_be_rewritten_or_reference_missing_files() {
    let fixture = Fixture::new();
    let path = fixture.args.payload.join("THIRD-PARTY-NOTICES.md");
    let source = text(&path).unwrap();
    write(&path, source.replace('\n', "\r\n").as_bytes()).unwrap();
    assert!(fixture.validate().is_err());
    for name in ["LICENSE-MIT", "LICENSE-APACHE", "CARGO-DEPENDENCIES.json"] {
        let fixture = Fixture::new();
        fs::remove_file(fixture.args.payload.join(name)).unwrap();
        assert!(notice_references(
            &fixture.args.payload,
            &fixture.args.payload.join("THIRD-PARTY-NOTICES.md")
        )
        .is_err());
    }
    for reference in ["../outside", "C:\\outside", "/outside"] {
        write(&path, format!("See `{reference}`").as_bytes()).unwrap();
        assert!(notice_references(&fixture.args.payload, &path).is_err());
    }
}

#[test]
fn extension_notice_must_ship_and_attribute_every_nuget_package() {
    for (case, notice) in [
        "- Package: OtherToolkit (see `LICENSE-APACHE`)\n",
        "- Package: Toolkit.Extra (see `LICENSE-APACHE`)\n",
        "- Package: Toolkit (see `MISSING.txt`)\n",
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = Fixture::new();
        write(
            &fixture.args.payload.join(EXTENSION_NOTICE),
            notice.as_bytes(),
        )
        .unwrap();
        assert!(fixture.validate().is_err(), "case {case}");
    }
    let fixture = Fixture::new();
    fs::remove_file(fixture.args.payload.join(EXTENSION_NOTICE)).unwrap();
    assert!(fixture.validate().is_err());
    let toolkit = json!({"package": "Toolkit", "version": "1.0.0"});
    nuget_attribution("Toolkit", &toolkit).unwrap();
    nuget_attribution("- Other (x)\n- Toolkit\n", &toolkit).unwrap();
}

#[test]
fn only_known_generated_files_may_be_unmapped() {
    let fixture = Fixture::new();
    for name in GENERATED_FILES {
        write(&fixture.args.payload.join(name), b"generated").unwrap();
    }
    fixture.validate().unwrap();
    for name in ["extra.dll", "extra.txt"] {
        write(&fixture.args.payload.join(name), b"extra").unwrap();
        assert!(fixture.validate().is_err());
        fs::remove_file(fixture.args.payload.join(name)).unwrap();
    }
}

#[test]
fn nuget_identity_license_provider_and_content_are_checked() {
    for case in 0..9 {
        let mut fixture = Fixture::new();
        match case {
            0 => fixture.inventory["nugetFiles"][0]["version"] = json!("1.0.1"),
            1 => fixture.inventory["nugetFiles"][0]["license"]["value"] = json!("MIT"),
            2 => write(&fixture.package.join("toolkit.nuspec"),b"<package><metadata><id>Other</id><version>1.0.0</version><license type=\"expression\">Apache-2.0</license></metadata></package>").unwrap(),
            3 => write(&fixture.package.join("lib/net472/Toolkit.dll"),b"different").unwrap(),
            4 => fs::remove_file(fixture.package.join("lib/net472/Toolkit.dll")).unwrap(),
            5 => fixture.assets["libraries"]["Toolkit/1.0.0"]["files"] = json!([]),
            6 => fixture.assets["targets"]["net472"]["Other/1.0.0"] = fixture.assets["targets"]["net472"]["Toolkit/1.0.0"].clone(),
            7 => fixture.assets["libraries"]["Toolkit/1.0.0"]["files"].as_array_mut().unwrap().push(json!("THIRD-PARTY-NOTICES.txt")),
            8 => fixture.inventory["nugetFiles"][0]["notice"] = json!({"required":true,"file":"missing.txt"}),
            _ => unreachable!(),
        }
        fixture.sync();
        assert!(fixture.validate().is_err(), "case {case}");
    }
}

#[test]
fn installer_accepts_only_identical_payload_and_known_generated_files() {
    let fixture = Fixture::new();
    let installed = fixture.temporary.path().join("installed");
    for name in files(&fixture.args.payload).unwrap() {
        write(
            &installed.join(&name),
            &read(&fixture.args.payload.join(name)).unwrap(),
        )
        .unwrap();
    }
    for name in ["unins000.exe", "unins000.dat", ".tctidy-install-target"] {
        write(&installed.join(name), b"generated").unwrap();
    }
    installer(&fixture.args.payload, &installed).unwrap();
    write(&installed.join("Toolkit.dll"), b"changed").unwrap();
    assert!(installer(&fixture.args.payload, &installed).is_err());
    write(&installed.join("Toolkit.dll"), b"Toolkit.dll").unwrap();
    for name in ["foreign.txt", "nested/unins000.exe"] {
        write(&installed.join(name), b"unexpected").unwrap();
        assert!(installer(&fixture.args.payload, &installed).is_err());
        fs::remove_file(installed.join(name)).unwrap();
    }
    fs::remove_file(installed.join("tctidy.exe")).unwrap();
    assert!(installer(&fixture.args.payload, &installed).is_err());
}
