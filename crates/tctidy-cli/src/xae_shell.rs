//! XAE Shell discovery used by the installer.
//!
//! Beckhoff publishes separate automation ProgIDs for the 15.0 32-bit and 17.0
//! 64-bit shells. Windows COM registration resolves each ProgID to a CLSID and
//! its `LocalServer32`, which is the authoritative executable path and therefore
//! survives custom install locations.
//!
//! ProgIDs: <https://infosys.beckhoff.com/content/1033/tc3_automationinterface/242746251.html>
//! COM path contract: <https://learn.microsoft.com/windows/win32/com/localserver32>

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const PROG_ID_15: &str = "TcXaeShell.DTE.15.0";
const PROG_ID_17: &str = "TcXaeShell.DTE.17.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RegistryView {
    Registry32,
    Registry64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellGeneration {
    V15,
    V17,
}

impl ShellGeneration {
    fn version(self) -> &'static str {
        match self {
            Self::V15 => "15.0",
            Self::V17 => "17.0",
        }
    }

    fn architecture(self) -> &'static str {
        match self {
            Self::V15 => "x86",
            Self::V17 => "x64",
        }
    }
}

fn component_cache_path(local_app_data: &Path, version: &str) -> Result<PathBuf, String> {
    if !local_app_data.is_absolute() {
        return Err("LOCALAPPDATA must be an absolute path".to_owned());
    }
    let generation = match version {
        "15.0" => ShellGeneration::V15,
        "17.0" => ShellGeneration::V17,
        _ => return Err(format!("unsupported XAE Shell version `{version}`")),
    };
    Ok(local_app_data
        .join("Beckhoff")
        .join("TcXaeShell")
        .join(format!("{}_IsoShell", generation.version()))
        .join("ComponentModelCache"))
}

fn clear_component_cache(local_app_data: &Path, version: &str) -> Result<(), String> {
    let cache = component_cache_path(local_app_data, version)?;
    if cache.is_dir() {
        std::fs::remove_dir_all(&cache)
            .map_err(|error| format!("remove XAE Shell cache `{}`: {error}", cache.display()))?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShellInstance {
    generation: ShellGeneration,
    executable: String,
    extension_dir: String,
    merge_marker: String,
    cache_relative: String,
}

impl ShellInstance {
    fn installer_record(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.generation.version(),
            self.generation.architecture(),
            self.executable,
            self.extension_dir,
            self.merge_marker,
            self.cache_relative
        )
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Discovery {
    instances: Vec<ShellInstance>,
    warnings: Vec<String>,
}

trait RegistryReader {
    fn read_value(
        &self,
        view: RegistryView,
        key: &str,
        value: &str,
    ) -> Result<Option<String>, String>;
}

fn discover<R, F>(registry: &R, is_file: F) -> Result<Discovery, String>
where
    R: RegistryReader,
    F: Fn(&str) -> bool,
{
    let candidates = [
        (ShellGeneration::V17, RegistryView::Registry64, PROG_ID_17),
        (ShellGeneration::V15, RegistryView::Registry32, PROG_ID_15),
    ];
    let mut discovery = Discovery::default();

    for (generation, view, prog_id) in candidates {
        let prog_id_key = format!(r"SOFTWARE\Classes\{prog_id}\CLSID");
        let Some(clsid) = read_registry(registry, view, &prog_id_key, "", prog_id)? else {
            continue;
        };
        if !valid_clsid(&clsid) {
            discovery
                .warnings
                .push(format!("{prog_id} has invalid CLSID `{clsid}`"));
            continue;
        }

        let server_key = format!(r"SOFTWARE\Classes\CLSID\{clsid}\LocalServer32");
        let server_executable =
            read_registry(registry, view, &server_key, "ServerExecutable", prog_id)?;
        let server = match server_executable {
            Some(executable) => Some(executable),
            None => read_registry(registry, view, &server_key, "", prog_id)?,
        };
        let Some(server) = server else {
            discovery.warnings.push(format!(
                "{prog_id} has no LocalServer32 executable registration"
            ));
            continue;
        };

        let executable = match executable_from_command(&server) {
            Some(path) => path,
            None => {
                discovery.warnings.push(format!(
                    "{prog_id} has invalid LocalServer32 command `{server}`"
                ));
                continue;
            }
        };
        let Some(ide_dir) = shell_ide_dir(&executable) else {
            discovery.warnings.push(format!(
                "{prog_id} executable must end in Common7\\IDE\\TcXaeShell.exe: `{executable}`"
            ));
            continue;
        };
        if !is_file(&executable) {
            discovery.warnings.push(format!(
                "{prog_id} registered executable does not exist: `{executable}`"
            ));
            continue;
        }

        if discovery.instances.iter().any(|instance| {
            instance
                .executable
                .eq_ignore_ascii_case(executable.as_str())
        }) {
            discovery.warnings.push(format!(
                "{prog_id} duplicates an already discovered executable: `{executable}`"
            ));
            continue;
        }

        discovery.instances.push(ShellInstance {
            generation,
            executable,
            extension_dir: format!(r"{ide_dir}\Extensions\TcTidy"),
            merge_marker: format!(r"{ide_dir}\extension.configurationchanged"),
            cache_relative: format!(
                r"Beckhoff\TcXaeShell\{}_IsoShell\ComponentModelCache",
                generation.version()
            ),
        });
    }

    Ok(discovery)
}

fn read_registry<R: RegistryReader>(
    registry: &R,
    view: RegistryView,
    key: &str,
    value: &str,
    prog_id: &str,
) -> Result<Option<String>, String> {
    registry
        .read_value(view, key, value)
        .map_err(|error| format!("read {prog_id} registration `{key}`: {error}"))
}

fn valid_clsid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 38
        && bytes[0] == b'{'
        && bytes[37] == b'}'
        && [9, 14, 19, 24].iter().all(|&index| bytes[index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 0 | 9 | 14 | 19 | 24 | 37) || byte.is_ascii_hexdigit()
        })
}

fn executable_from_command(command: &str) -> Option<String> {
    let command = command.trim();
    let path = if let Some(quoted) = command.strip_prefix('"') {
        let end = quoted.find('"')?;
        &quoted[..end]
    } else {
        let lowercase = command.to_ascii_lowercase();
        let end = lowercase.find(".exe")? + ".exe".len();
        &command[..end]
    };
    let path = path.trim().replace('/', r"\");
    let bytes = path.as_bytes();
    let absolute_drive_path =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let safe_for_protocol = !path
        .bytes()
        .any(|byte| matches!(byte, b'|' | b'\r' | b'\n'));
    (absolute_drive_path && safe_for_protocol).then_some(path)
}

fn shell_ide_dir(executable: &str) -> Option<String> {
    const EXECUTABLE_SUFFIX: &str = r"\Common7\IDE\TcXaeShell.exe";
    if executable.len() <= EXECUTABLE_SUFFIX.len()
        || !executable
            .to_ascii_lowercase()
            .ends_with(&EXECUTABLE_SUFFIX.to_ascii_lowercase())
    {
        return None;
    }
    let ide_len = executable.len() - r"\TcXaeShell.exe".len();
    Some(executable[..ide_len].to_owned())
}

pub(crate) fn run() -> ExitCode {
    #[cfg(windows)]
    let result = discover(&WindowsRegistry, |path| {
        std::path::Path::new(path).is_file()
    });
    #[cfg(not(windows))]
    let result = discover(&EmptyRegistry, |_| false);

    match result {
        Ok(discovery) => {
            for warning in discovery.warnings {
                eprintln!("tctidy: XAE Shell discovery: {warning}");
            }
            for instance in discovery.instances {
                println!("{}", instance.installer_record());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("tctidy: XAE Shell discovery failed: {error}");
            ExitCode::from(2)
        }
    }
}

pub(crate) fn clear_cache(version: &str) -> ExitCode {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        eprintln!("tctidy: XAE Shell cache cleanup failed: LOCALAPPDATA is not set");
        return ExitCode::from(2);
    };
    match clear_component_cache(Path::new(&local_app_data), version) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tctidy: XAE Shell cache cleanup failed: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(not(windows))]
struct EmptyRegistry;

#[cfg(not(windows))]
impl RegistryReader for EmptyRegistry {
    fn read_value(
        &self,
        _view: RegistryView,
        _key: &str,
        _value: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[cfg(windows)]
struct WindowsRegistry;

#[cfg(windows)]
impl RegistryReader for WindowsRegistry {
    fn read_value(
        &self,
        view: RegistryView,
        key: &str,
        value: &str,
    ) -> Result<Option<String>, String> {
        use std::io::ErrorKind;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
        use winreg::RegKey;

        let view_flag = match view {
            RegistryView::Registry32 => KEY_WOW64_32KEY,
            RegistryView::Registry64 => KEY_WOW64_64KEY,
        };
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let registry_key = match hklm.open_subkey_with_flags(key, KEY_READ | view_flag) {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        match registry_key.get_value::<String, _>(value) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[derive(Default)]
    struct FixtureRegistry {
        values: BTreeMap<(RegistryView, String, String), Result<Option<String>, String>>,
    }

    impl FixtureRegistry {
        fn value(mut self, view: RegistryView, key: &str, name: &str, value: &str) -> Self {
            self.values.insert(
                (view, key.to_owned(), name.to_owned()),
                Ok(Some(value.to_owned())),
            );
            self
        }

        fn error(mut self, view: RegistryView, key: &str, message: &str) -> Self {
            self.values.insert(
                (view, key.to_owned(), String::new()),
                Err(message.to_owned()),
            );
            self
        }
    }

    impl RegistryReader for FixtureRegistry {
        fn read_value(
            &self,
            view: RegistryView,
            key: &str,
            value: &str,
        ) -> Result<Option<String>, String> {
            self.values
                .get(&(view, key.to_owned(), value.to_owned()))
                .cloned()
                .unwrap_or(Ok(None))
        }
    }

    fn prog_id_key(prog_id: &str) -> String {
        format!(r"SOFTWARE\Classes\{prog_id}\CLSID")
    }

    fn server_key(clsid: &str) -> String {
        format!(r"SOFTWARE\Classes\CLSID\{clsid}\LocalServer32")
    }

    fn shell(
        registry: FixtureRegistry,
        view: RegistryView,
        prog_id: &str,
        clsid: &str,
        executable: &str,
    ) -> FixtureRegistry {
        registry
            .value(view, &prog_id_key(prog_id), "", clsid)
            .value(view, &server_key(clsid), "ServerExecutable", executable)
    }

    fn files(paths: &[&str]) -> impl Fn(&str) -> bool {
        let paths: BTreeSet<_> = paths.iter().map(|path| path.to_ascii_lowercase()).collect();
        move |path| paths.contains(&path.to_ascii_lowercase())
    }

    #[test]
    fn discovers_both_generations_from_their_registry_views_and_custom_paths() {
        let exe15 = r"D:\TwinCAT-legacy\Common7\IDE\TcXaeShell.exe";
        let exe17 = r"E:\Engineering\XAE64\Common7\IDE\TcXaeShell.exe";
        let registry = shell(
            shell(
                FixtureRegistry::default(),
                RegistryView::Registry32,
                PROG_ID_15,
                "{15151515-1515-1515-1515-151515151515}",
                exe15,
            ),
            RegistryView::Registry64,
            PROG_ID_17,
            "{17171717-1717-1717-1717-171717171717}",
            exe17,
        );

        let found = discover(&registry, files(&[exe15, exe17])).unwrap();

        assert!(found.warnings.is_empty());
        assert_eq!(found.instances.len(), 2);
        assert_eq!(found.instances[0].generation, ShellGeneration::V17);
        assert_eq!(found.instances[0].executable, exe17);
        assert_eq!(
            found.instances[0].extension_dir,
            r"E:\Engineering\XAE64\Common7\IDE\Extensions\TcTidy"
        );
        assert_eq!(
            found.instances[0].merge_marker,
            r"E:\Engineering\XAE64\Common7\IDE\extension.configurationchanged"
        );
        assert_eq!(
            found.instances[0].cache_relative,
            r"Beckhoff\TcXaeShell\17.0_IsoShell\ComponentModelCache"
        );
        assert_eq!(found.instances[1].generation, ShellGeneration::V15);
        assert_eq!(found.instances[1].executable, exe15);
        assert_eq!(
            found.instances[1].cache_relative,
            r"Beckhoff\TcXaeShell\15.0_IsoShell\ComponentModelCache"
        );
    }

    #[test]
    fn falls_back_to_the_quoted_local_server_command() {
        let clsid = "{15151515-1515-1515-1515-151515151515}";
        let exe = r"C:\Program Files (x86)\Beckhoff\TcXaeShell\Common7\IDE\TcXaeShell.exe";
        let registry = FixtureRegistry::default()
            .value(
                RegistryView::Registry32,
                &prog_id_key(PROG_ID_15),
                "",
                clsid,
            )
            .value(
                RegistryView::Registry32,
                &server_key(clsid),
                "",
                &format!(r#""{exe}" /Embedding"#),
            );

        let found = discover(&registry, files(&[exe])).unwrap();

        assert_eq!(found.instances.len(), 1);
        assert_eq!(found.instances[0].executable, exe);
    }

    #[test]
    fn ignores_a_registration_from_the_wrong_bitness_view() {
        let exe = r"C:\XAE\Common7\IDE\TcXaeShell.exe";
        let registry = shell(
            FixtureRegistry::default(),
            RegistryView::Registry64,
            PROG_ID_15,
            "{15151515-1515-1515-1515-151515151515}",
            exe,
        );

        let found = discover(&registry, files(&[exe])).unwrap();

        assert!(found.instances.is_empty());
        assert!(found.warnings.is_empty());
    }

    #[test]
    fn rejects_malformed_incomplete_and_missing_registrations_without_guessing() {
        let bad_clsid = shell(
            FixtureRegistry::default(),
            RegistryView::Registry32,
            PROG_ID_15,
            "not-a-clsid",
            r"C:\Wrong\Common7\IDE\TcXaeShell.exe",
        );
        let registry = shell(
            bad_clsid,
            RegistryView::Registry64,
            PROG_ID_17,
            "{17171717-1717-1717-1717-171717171717}",
            r"C:\Missing\Common7\IDE\TcXaeShell.exe",
        );

        let found = discover(&registry, files(&[])).unwrap();

        assert!(found.instances.is_empty());
        assert_eq!(found.warnings.len(), 2);
        assert!(found
            .warnings
            .iter()
            .any(|warning| warning.contains("CLSID")));
        assert!(found
            .warnings
            .iter()
            .any(|warning| warning.contains("does not exist")));
    }

    #[test]
    fn reports_a_progid_without_a_local_server_as_incomplete() {
        let registry = FixtureRegistry::default().value(
            RegistryView::Registry32,
            &prog_id_key(PROG_ID_15),
            "",
            "{15151515-1515-1515-1515-151515151515}",
        );

        let found = discover(&registry, |_| true).unwrap();

        assert!(found.instances.is_empty());
        assert_eq!(found.warnings.len(), 1);
        assert!(found.warnings[0].contains("no LocalServer32"));
    }

    #[test]
    fn duplicate_executables_prefer_the_newer_generation_deterministically() {
        let executable = r"C:\TwinCAT\Common7\IDE\TcXaeShell.exe";
        let registry = shell(
            shell(
                FixtureRegistry::default(),
                RegistryView::Registry32,
                PROG_ID_15,
                "{15151515-1515-1515-1515-151515151515}",
                executable,
            ),
            RegistryView::Registry64,
            PROG_ID_17,
            "{17171717-1717-1717-1717-171717171717}",
            &executable.to_ascii_lowercase(),
        );

        let found = discover(&registry, |_| true).unwrap();

        assert_eq!(found.instances.len(), 1);
        assert_eq!(found.instances[0].generation, ShellGeneration::V17);
        assert_eq!(found.warnings.len(), 1);
        assert!(found.warnings[0].contains("duplicates"));
    }

    #[test]
    fn rejects_an_executable_outside_the_expected_shell_layout() {
        let registry = shell(
            FixtureRegistry::default(),
            RegistryView::Registry64,
            PROG_ID_17,
            "{17171717-1717-1717-1717-171717171717}",
            r"C:\Windows\notepad.exe",
        );

        let found = discover(&registry, |_| true).unwrap();

        assert!(found.instances.is_empty());
        assert!(found.warnings[0].contains("Common7\\IDE\\TcXaeShell.exe"));
    }

    #[test]
    fn a_registry_read_error_is_not_misreported_as_no_installation() {
        let registry = FixtureRegistry::default().error(
            RegistryView::Registry32,
            &prog_id_key(PROG_ID_15),
            "access denied",
        );

        let error = discover(&registry, |_| false).unwrap_err();

        assert!(error.contains("access denied"));
        assert!(error.contains(PROG_ID_15));
    }

    #[test]
    fn installer_record_has_a_stable_pipe_delimited_contract() {
        let instance = ShellInstance {
            generation: ShellGeneration::V17,
            executable: r"C:\XAE\Common7\IDE\TcXaeShell.exe".to_owned(),
            extension_dir: r"C:\XAE\Common7\IDE\Extensions\TcTidy".to_owned(),
            merge_marker: r"C:\XAE\Common7\IDE\extension.configurationchanged".to_owned(),
            cache_relative: r"Beckhoff\TcXaeShell\17.0_IsoShell\ComponentModelCache".to_owned(),
        };

        assert_eq!(
            instance.installer_record(),
            r"17.0|x64|C:\XAE\Common7\IDE\TcXaeShell.exe|C:\XAE\Common7\IDE\Extensions\TcTidy|C:\XAE\Common7\IDE\extension.configurationchanged|Beckhoff\TcXaeShell\17.0_IsoShell\ComponentModelCache"
        );
    }

    #[test]
    fn cache_cleanup_uses_the_supplied_profile_and_selected_generation() {
        let root = std::env::temp_dir().join(format!("tctidy-xae-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let original_user = root.join("original-user");
        let elevated_user = root.join("elevated-user");
        let original_15 = component_cache_path(&original_user, "15.0").unwrap();
        let original_17 = component_cache_path(&original_user, "17.0").unwrap();
        let elevated_15 = component_cache_path(&elevated_user, "15.0").unwrap();
        for cache in [&original_15, &original_17, &elevated_15] {
            std::fs::create_dir_all(cache).unwrap();
            std::fs::write(cache.join("fixture.bin"), b"cache").unwrap();
        }

        clear_component_cache(&original_user, "15.0").unwrap();

        assert!(!original_15.exists());
        assert!(original_17.exists());
        assert!(elevated_15.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_cleanup_rejects_unrecognized_or_traversing_generations() {
        let local_app_data = std::path::Path::new(r"C:\Users\fixture\AppData\Local");

        assert!(component_cache_path(local_app_data, "16.0").is_err());
        assert!(component_cache_path(local_app_data, r"..\..\Victim").is_err());
        assert!(component_cache_path(std::path::Path::new("relative"), "15.0").is_err());
    }
}
