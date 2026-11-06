//! Atomic file writing: temp file in the same directory, then `rename`.
//!
//! This is the non-destruction guarantee at the IO boundary: a crash mid-write
//! can never leave a half-written source file, because the target is only ever
//! replaced by an atomic rename of a fully-written temp file.
//!
//! # Symbolic links
//!
//! Renaming onto a symbolic link would replace the link itself with a regular
//! file and leave the file it points to stale. So when the path is a link, the
//! chain of links is followed to the file it ends at, the temp file is created
//! in *that* file's directory (the rename must stay within one file system),
//! and that file is replaced; every link in the chain is left as it was. A link
//! that cannot be followed to an existing file (a dangling link, a loop) is
//! refused before anything is created, so neither the link nor any file changes.
//!
//! # Durability
//!
//! The steps run in this order: create the temp file, write it, sync it
//! (`File::sync_all`: `fsync`, `F_FULLFSYNC` on macOS, `FlushFileBuffers` on
//! Windows), rename it over the file, then sync the directory that holds the
//! file. Syncing the file before the rename means the rename can never expose
//! content that is not on disk yet. The rename itself is a change to the
//! directory, so it only survives a power loss once the directory is synced.
//!
//! - **Unix**: the directory is opened and synced after the rename. If it
//!   cannot be opened or synced, the new content is already in place (the
//!   rename cannot be taken back safely), so the write returns
//!   [`WriteError::NotDurable`] with the directory's path. Callers must report
//!   it as written but not durable, never as a plain success.
//! - **Windows**: the directory step does nothing. Windows documents no
//!   directory equivalent of `fsync`: `File::open` refuses a directory,
//!   `FlushFileBuffers` is specified for files and volumes only (NTFS accepts
//!   it on a directory opened for writing with `FILE_FLAG_BACKUP_SEMANTICS`,
//!   but nothing promises that this persists the rename, and that open needs
//!   write access to the directory itself, which shares and restrictive ACLs
//!   may deny), and the documented way, `MoveFileExW` with
//!   `MOVEFILE_WRITE_THROUGH`, is not what std's `rename` calls and would need
//!   `unsafe`, which this crate forbids. NTFS records the rename in its
//!   journal, so after a power loss the file holds either its complete old
//!   content or its complete new content, never a mix; but the most recent
//!   rename can be lost, bringing back the old content. On Windows a
//!   successful write is therefore atomic, but its durability is not
//!   guaranteed.
//!
//! # Metadata
//!
//! The replacement is a new file, so what the old one carried is applied to
//! the temp file after its content is written and before it is synced and
//! renamed. If that fails, the write fails and the original stays as it was.
//! When there is no file yet, the new one gets the platform's defaults.
//!
//! - **Unix**: kept are the owner and group, every extended attribute (on
//!   Linux, Android and macOS; on Linux, POSIX ACLs and SELinux labels are
//!   extended attributes), and the permission bits with setuid, setgid and
//!   sticky, applied in that order: changing the owner clears the set-id bits,
//!   and a read-only mode would refuse extended attributes. Only root can give
//!   a file to another user, so a file owned by someone else is refused rather
//!   than silently made yours; so is a file whose group you are not in, or an
//!   attribute the system will not let you set. Until its mode is applied the
//!   temp file is private (0600), whatever the umask. Not kept: macOS ACLs and
//!   file flags (`chflags`), Linux inode flags (`chattr`; an immutable or
//!   append-only file cannot be replaced at all, so it is refused and left as
//!   is), and extended attributes on other Unix systems.
//! - **Windows**: kept are the hidden, system and encrypted attributes, given
//!   when the temp file is created, and the creation time. Not kept: a
//!   read-only file cannot be replaced (the rename fails and the file is left
//!   as is); the archive attribute is set, as on any changed file; compression
//!   and content indexing follow the directory; the security descriptor (owner
//!   and ACL) is the one a new file inherits from the directory, so
//!   permissions set on the file itself are lost; alternate data streams (such
//!   as the `Zone.Identifier` mark of the web) are dropped. std offers no API
//!   for these, and this crate forbids `unsafe`.
//! - **Everywhere**: the modification and change times are those of the
//!   write, as for any edit, and so is the creation time where a Unix file
//!   system records one. A file with several hard links is detached from the
//!   others: only the name written to sees the new content.

mod metadata;

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Why [`write_atomic`] failed.
#[derive(Debug)]
pub enum WriteError {
    /// Nothing was replaced: the file at the path, if there was one, is as it was.
    NotWritten(io::Error),
    /// The new content replaced the file, but the directory holding it could
    /// not be synced, so the replacement may not survive a power loss.
    NotDurable { dir: PathBuf, source: io::Error },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::NotWritten(e) => e.fmt(f),
            WriteError::NotDurable { dir, source } => {
                write!(f, "cannot sync directory {}: {source}", dir.display())
            }
        }
    }
}

impl From<io::Error> for WriteError {
    fn from(e: io::Error) -> Self {
        WriteError::NotWritten(e)
    }
}

/// The steps of [`write_atomic`], in the order they run. Tests inject a
/// failure before any of them to check the order and what each failure leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    CreateTemp,
    WriteTemp,
    CopyMetadata,
    SyncTemp,
    Rename,
    SyncDir,
}

/// Write `bytes` to `path` atomically. When `path` is a symbolic link, the file
/// it resolves to is replaced and the link is kept.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    write_atomic_with(path, bytes, &mut |_| Ok(()))
}

/// [`write_atomic`], calling `before` ahead of each step; an error it returns
/// is handled as that step's own failure.
fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    before: &mut dyn FnMut(Step) -> io::Result<()>,
) -> Result<(), WriteError> {
    let target = file_to_replace(path)?;
    // Read before anything is created: the temp file is made to match it.
    let original = match fs::metadata(&target) {
        Ok(meta) => Some(meta),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => {
            let what = format!("cannot read the metadata of {}: {e}", target.display());
            return Err(io::Error::new(e.kind(), what).into());
        }
    };
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".tctidy-tmp-{}-{}", std::process::id(), n));

    let original = original.as_ref().map(|meta| (meta, target.as_path()));
    if let Err(e) = write_temp(&tmp, bytes, original, before) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    if let Err(e) = before(Step::Rename).and_then(|()| fs::rename(&tmp, &target)) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    // From here on the new content is in place; a failure only means that the
    // rename may not survive a power loss, and is reported as such.
    before(Step::SyncDir)
        .and_then(|()| sync_dir(dir))
        .map_err(|source| WriteError::NotDurable {
            dir: std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf()),
            source,
        })
}

/// Flush `dir`, so that a rename inside it survives a power loss.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

/// No directory sync off Unix: the module docs explain why for Windows.
#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// Create the temp file at `tmp`, write `bytes` to it, give it the metadata of
/// the `original` file it replaces (with that file's path), and flush it.
fn write_temp(
    tmp: &Path,
    bytes: &[u8],
    original: Option<(&fs::Metadata, &Path)>,
    before: &mut dyn FnMut(Step) -> io::Result<()>,
) -> io::Result<()> {
    before(Step::CreateTemp)?;
    let mut file = metadata::temp_options(original.map(|(meta, _)| meta)).open(tmp)?;
    before(Step::WriteTemp)?;
    file.write_all(bytes)?;
    // After the content, since writing to a file clears its setuid and setgid
    // bits; before the sync, so the metadata is on disk with the content.
    before(Step::CopyMetadata)?;
    if let Some((meta, path)) = original {
        metadata::copy(meta, path, &file)?;
    }
    before(Step::SyncTemp)?;
    file.sync_all()
}

/// The directory entry a write to `path` replaces: `path` itself, or, when it
/// is a symbolic link, the file its chain of links ends at.
fn file_to_replace(path: &Path) -> io::Result<PathBuf> {
    let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    if !is_link {
        return Ok(path.to_path_buf());
    }
    fs::canonicalize(path).map_err(|e| {
        let why = if e.kind() == io::ErrorKind::NotFound {
            "the file it points to does not exist".to_owned()
        } else {
            e.to_string()
        };
        io::Error::new(
            e.kind(),
            format!("cannot follow symbolic link {}: {why}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    /// A fresh, empty scratch directory unique to this test invocation, so tests
    /// running in parallel never see each other's temp files.
    fn scratch() -> std::path::PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tctidy-iotest-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Whether any leftover `.tctidy-tmp-*` file remains in `dir`.
    fn has_temp_leftover(dir: &Path) -> bool {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with(".tctidy-tmp-"))
    }

    #[test]
    fn write_then_read_back() {
        let mut path = std::env::temp_dir();
        path.push(format!("tctidy-atomic-test-{}.txt", std::process::id()));
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        // overwrite atomically
        write_atomic(&path, b"world!").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"world!");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn successful_write_leaves_no_temp_file() {
        let dir = scratch();
        let target = dir.join("a.txt");
        write_atomic(&target, b"hello").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");
        assert!(
            !has_temp_leftover(&dir),
            "a successful atomic write must not leave a temp file behind"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_write_preserves_target_and_cleans_up() {
        // Force the rename to fail by making the target an existing, non-empty
        // directory: a file can never be renamed onto it. This is the crash/IO
        // failure path, the original must be untouched and no temp may leak.
        let dir = scratch();
        let target = dir.join("target");
        fs::create_dir(&target).unwrap();
        let sentinel = target.join("keep.txt");
        fs::write(&sentinel, b"original").unwrap();

        let result = write_atomic(&target, b"new content");
        assert!(
            result.is_err(),
            "writing onto an existing directory must fail, not clobber it"
        );

        // The "original" (the directory and its contents) is intact...
        assert!(target.is_dir());
        assert_eq!(fs::read(&sentinel).unwrap(), b"original");
        // ...and no half-written temp file is left behind.
        assert!(
            !has_temp_leftover(&dir),
            "a failed atomic write must clean up its temp file"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_writes_to_distinct_targets_do_not_collide() {
        // Temp names embed the pid and a process-global counter, so parallel
        // writes in the same directory never race on the same temp path.
        let dir = scratch();
        let n = 16;
        std::thread::scope(|s| {
            for i in 0..n {
                let dir = &dir;
                s.spawn(move || {
                    let target = dir.join(format!("f{i}.txt"));
                    let body = format!("content-{i}");
                    write_atomic(&target, body.as_bytes()).unwrap();
                });
            }
        });
        for i in 0..n {
            let target = dir.join(format!("f{i}.txt"));
            assert_eq!(
                fs::read(&target).unwrap(),
                format!("content-{i}").into_bytes()
            );
        }
        assert!(
            !has_temp_leftover(&dir),
            "concurrent writes must not leak temp files"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A step hook that records every step reached and fails `step`.
    fn fail_at(step: Step, seen: &mut Vec<Step>) -> impl FnMut(Step) -> io::Result<()> + '_ {
        move |reached| {
            seen.push(reached);
            if reached == step {
                Err(io::Error::other("injected failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn the_file_is_synced_before_the_rename_and_the_directory_after() {
        let dir = scratch();
        let target = dir.join("a.txt");
        fs::write(&target, b"old").unwrap();
        let mut seen = Vec::new();

        write_atomic_with(&target, b"new", &mut |step| {
            seen.push(step);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            seen,
            [
                Step::CreateTemp,
                Step::WriteTemp,
                Step::CopyMetadata,
                Step::SyncTemp,
                Step::Rename,
                Step::SyncDir
            ]
        );
        assert_eq!(fs::read(&target).unwrap(), b"new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_before_the_rename_leaves_the_original_and_no_temp() {
        for step in [
            Step::CreateTemp,
            Step::WriteTemp,
            Step::CopyMetadata,
            Step::SyncTemp,
            Step::Rename,
        ] {
            let dir = scratch();
            let target = dir.join("a.txt");
            fs::write(&target, b"old").unwrap();
            let mut seen = Vec::new();

            let result = write_atomic_with(&target, b"new", &mut fail_at(step, &mut seen));

            assert!(
                matches!(result, Err(WriteError::NotWritten(_))),
                "{step:?}: {result:?}"
            );
            assert_eq!(seen.last(), Some(&step), "nothing runs after {step:?}");
            assert_eq!(fs::read(&target).unwrap(), b"old", "{step:?}");
            assert!(!has_temp_leftover(&dir), "{step:?}");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_directory_sync_failure_is_reported_with_the_new_content_in_place() {
        let dir = scratch();
        let target = dir.join("a.txt");
        fs::write(&target, b"old").unwrap();
        let mut seen = Vec::new();

        let result = write_atomic_with(&target, b"new", &mut fail_at(Step::SyncDir, &mut seen));

        let error = result.expect_err("a failed directory sync is not a success");
        let WriteError::NotDurable { dir: synced, .. } = &error else {
            panic!("expected NotDurable, got {error:?}");
        };
        assert_eq!(synced, &dir);
        assert_eq!(
            error.to_string(),
            format!("cannot sync directory {}: injected failure", dir.display())
        );
        // The rename happened: the new content is in place, and no temp is left.
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!has_temp_leftover(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_makes_the_write_not_durable() {
        use std::os::unix::fs::PermissionsExt;

        // Write and search permission let the temp file be created and renamed,
        // but without read permission the directory cannot be opened to sync it.
        let dir = scratch();
        let target = dir.join("a.txt");
        fs::write(&target, b"old").unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o300)).unwrap();
        if fs::File::open(&dir).is_ok() {
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
            skip(
                "an_unreadable_directory_makes_the_write_not_durable",
                "running with privileges that ignore directory permissions",
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }

        let result = write_atomic(&target, b"new");
        let content = fs::read(&target).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        match result {
            Err(WriteError::NotDurable { dir: synced, .. }) => assert_eq!(synced, dir),
            other => panic!("expected NotDurable, got {other:?}"),
        }
        assert_eq!(content, b"new");
        assert!(!has_temp_leftover(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Say that a test could not run here. libtest has no skip status, so this
    /// writes to the real stderr (output capture only covers `eprintln!`),
    /// where `cargo test` shows it even for a test that returns early.
    fn skip(test: &str, why: &str) {
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {why}");
    }

    /// Create a symbolic link at `link` whose literal target is `target`, or
    /// return `None` when the platform refuses it (Windows without the
    /// symbolic link privilege).
    fn symlink(target: &Path, link: &Path) -> Option<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).unwrap();
            Some(())
        }
        #[cfg(windows)]
        {
            const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
            match std::os::windows::fs::symlink_file(target, link) {
                Ok(()) => Some(()),
                Err(e) if e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) => None,
                Err(e) => panic!("cannot create symlink {}: {e}", link.display()),
            }
        }
    }

    /// Assert that `link` is still a symbolic link to the literal `target`.
    fn assert_link(link: &Path, target: &Path) {
        let meta = fs::symlink_metadata(link).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "{} must still be a symbolic link",
            link.display()
        );
        assert_eq!(fs::read_link(link).unwrap(), target);
    }

    #[test]
    fn symlink_is_kept_and_its_target_updated() {
        let dir = scratch();
        let target = dir.join("target.st");
        let link = dir.join("link.st");
        fs::write(&target, b"old").unwrap();
        if symlink(Path::new("target.st"), &link).is_none() {
            skip(
                "symlink_is_kept_and_its_target_updated",
                "no symlink privilege",
            );
            return;
        }

        write_atomic(&link, b"new").unwrap();

        assert_link(&link, Path::new("target.st"));
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!has_temp_leftover(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn symlink_to_another_directory_updates_the_target_there() {
        let root = scratch();
        let (links, files) = (root.join("links"), root.join("files"));
        fs::create_dir(&links).unwrap();
        fs::create_dir(&files).unwrap();
        let relative_target = files.join("relative.st");
        let absolute_target = files.join("absolute.st");
        fs::write(&relative_target, b"old relative").unwrap();
        fs::write(&absolute_target, b"old absolute").unwrap();
        let relative = Path::new("..").join("files").join("relative.st");
        let (relative_link, absolute_link) = (links.join("relative.st"), links.join("absolute.st"));
        if symlink(&relative, &relative_link).is_none() {
            skip(
                "symlink_to_another_directory_updates_the_target_there",
                "no symlink privilege",
            );
            return;
        }
        symlink(&absolute_target, &absolute_link).unwrap();

        write_atomic(&relative_link, b"new relative").unwrap();
        write_atomic(&absolute_link, b"new absolute").unwrap();

        assert_link(&relative_link, &relative);
        assert_link(&absolute_link, &absolute_target);
        assert_eq!(fs::read(&relative_target).unwrap(), b"new relative");
        assert_eq!(fs::read(&absolute_target).unwrap(), b"new absolute");
        // The temp file lives beside the file it replaces, never beside the link.
        assert!(!has_temp_leftover(&links));
        assert!(!has_temp_leftover(&files));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_directory_synced_is_the_one_holding_the_replaced_file() {
        let root = scratch();
        let (links, files) = (root.join("links"), root.join("files"));
        fs::create_dir(&links).unwrap();
        fs::create_dir(&files).unwrap();
        fs::write(files.join("target.st"), b"old").unwrap();
        let link = links.join("link.st");
        if symlink(&Path::new("..").join("files").join("target.st"), &link).is_none() {
            skip(
                "the_directory_synced_is_the_one_holding_the_replaced_file",
                "no symlink privilege",
            );
            return;
        }
        let mut seen = Vec::new();

        let result = write_atomic_with(&link, b"new", &mut fail_at(Step::SyncDir, &mut seen));

        match result {
            Err(WriteError::NotDurable { dir, .. }) => {
                assert_eq!(dir, fs::canonicalize(&files).unwrap());
            }
            other => panic!("expected NotDurable, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn chained_symlinks_are_followed_to_the_file() {
        let dir = scratch();
        let target = dir.join("target.st");
        fs::write(&target, b"old").unwrap();
        let (first, second) = (dir.join("first.st"), dir.join("second.st"));
        if symlink(Path::new("second.st"), &first).is_none() {
            skip(
                "chained_symlinks_are_followed_to_the_file",
                "no symlink privilege",
            );
            return;
        }
        symlink(Path::new("target.st"), &second).unwrap();

        write_atomic(&first, b"new").unwrap();

        assert_link(&first, Path::new("second.st"));
        assert_link(&second, Path::new("target.st"));
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!has_temp_leftover(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dangling_symlink_is_refused_without_change() {
        let dir = scratch();
        let link = dir.join("link.st");
        if symlink(Path::new("missing.st"), &link).is_none() {
            skip(
                "dangling_symlink_is_refused_without_change",
                "no symlink privilege",
            );
            return;
        }

        let result = write_atomic(&link, b"new");

        assert!(result.is_err(), "a dangling link must be refused");
        assert_link(&link, Path::new("missing.st"));
        assert!(!dir.join("missing.st").exists(), "nothing may be created");
        assert!(!has_temp_leftover(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn symlink_loop_is_refused_without_change() {
        let dir = scratch();
        let (a, b) = (dir.join("a.st"), dir.join("b.st"));
        if symlink(Path::new("b.st"), &a).is_none() {
            skip(
                "symlink_loop_is_refused_without_change",
                "no symlink privilege",
            );
            return;
        }
        symlink(Path::new("a.st"), &b).unwrap();

        let result = write_atomic(&a, b"new");

        assert!(result.is_err(), "a link loop must be refused");
        assert_link(&a, Path::new("b.st"));
        assert_link(&b, Path::new("a.st"));
        assert!(!has_temp_leftover(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_into_a_read_only_directory_is_refused_without_change() {
        use std::os::unix::fs::PermissionsExt;

        // The target's directory refuses new entries, so its file cannot be
        // replaced atomically: the write must fail and leave both the link (in
        // a writable directory) and the target as they were.
        let root = scratch();
        let (links, files) = (root.join("links"), root.join("files"));
        fs::create_dir(&links).unwrap();
        fs::create_dir(&files).unwrap();
        let target = files.join("target.st");
        fs::write(&target, b"old").unwrap();
        let link = links.join("link.st");
        let literal = Path::new("..").join("files").join("target.st");
        symlink(&literal, &link).unwrap();
        fs::set_permissions(&files, fs::Permissions::from_mode(0o555)).unwrap();
        if fs::write(files.join("probe"), b"").is_ok() {
            skip(
                "symlink_into_a_read_only_directory_is_refused_without_change",
                "running with privileges that ignore directory permissions",
            );
            fs::set_permissions(&files, fs::Permissions::from_mode(0o755)).unwrap();
            let _ = fs::remove_dir_all(&root);
            return;
        }

        let result = write_atomic(&link, b"new");
        fs::set_permissions(&files, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err(), "the target cannot be replaced, so refuse");
        assert_link(&link, &literal);
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(!has_temp_leftover(&links));
        assert!(!has_temp_leftover(&files));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_permission_mode_is_kept() {
        // 0751 is the mode of the issue; 4755 carries the setuid bit.
        for kept in [0o751, 0o600, 0o4755] {
            let dir = scratch();
            let target = dir.join("a.st");
            fs::write(&target, b"old").unwrap();
            set_mode(&target, kept);

            write_atomic(&target, b"new").unwrap();

            assert_eq!(fs::read(&target).unwrap(), b"new");
            assert_eq!(mode(&target), kept, "{kept:o}");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_temp_file_is_private_until_the_mode_is_applied() {
        // The content must never be readable by others for the length of the
        // write, even when the umask would make a new file world-readable.
        let dir = scratch();
        let target = dir.join("a.st");
        fs::write(&target, b"old").unwrap();
        set_mode(&target, 0o600);
        let mut temp_mode = None;

        write_atomic_with(&target, b"secret", &mut |step| {
            if step == Step::CopyMetadata {
                let temp = fs::read_dir(&dir)
                    .unwrap()
                    .filter_map(Result::ok)
                    .find(|e| e.file_name().to_string_lossy().starts_with(".tctidy-tmp-"))
                    .expect("the temp file exists before its metadata is set");
                temp_mode = Some(mode(&temp.path()));
            }
            Ok(())
        })
        .unwrap();

        let temp_mode = temp_mode.expect("the metadata step ran");
        assert_eq!(temp_mode & 0o077, 0, "temp file mode {temp_mode:o}");
        assert_eq!(mode(&target), 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn unix_a_new_file_gets_the_default_mode() {
        // With no file to replace there is no mode to keep: the new file gets
        // what any new file gets here (0666 less the umask), as before.
        let dir = scratch();
        let reference = dir.join("reference");
        fs::write(&reference, b"").unwrap();
        let created = dir.join("created");

        write_atomic(&created, b"new").unwrap();

        assert_eq!(mode(&created), mode(&reference));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn unix_metadata_kept_is_the_one_of_the_file_a_link_points_to() {
        let dir = scratch();
        let target = dir.join("target.st");
        fs::write(&target, b"old").unwrap();
        set_mode(&target, 0o751);
        let link = dir.join("link.st");
        symlink(Path::new("target.st"), &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert_link(&link, Path::new("target.st"));
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(mode(&target), 0o751);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn unix_owner_group_and_set_id_bits_are_kept() {
        use std::os::unix::fs::{chown, MetadataExt};

        let dir = scratch();
        let target = dir.join("a.st");
        fs::write(&target, b"old").unwrap();
        if fs::metadata(&target).unwrap().uid() != 0 {
            skip(
                "unix_owner_group_and_set_id_bits_are_kept",
                "giving a file another owner needs root",
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        // nobody:nogroup on most systems; any id other than root's will do.
        chown(&target, Some(65534), Some(65534)).unwrap();
        set_mode(&target, 0o6755);

        write_atomic(&target, b"new").unwrap();

        let meta = fs::metadata(&target).unwrap();
        assert_eq!((meta.uid(), meta.gid()), (65534, 65534));
        // Changing the owner clears the set-id bits: the mode must come after.
        assert_eq!(mode(&target), 0o6755);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The value of the extended attribute `name` of `path`.
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    fn xattr(path: &Path, name: &str) -> rustix::io::Result<Vec<u8>> {
        let mut buf = vec![0; 4096];
        let len = rustix::fs::getxattr(path, name, &mut buf[..])?;
        buf.truncate(len);
        Ok(buf)
    }

    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    #[test]
    fn extended_attributes_are_kept() {
        use rustix::fs::{setxattr, XattrFlags};

        let dir = scratch();
        let target = dir.join("a.st");
        fs::write(&target, b"old").unwrap();
        let name = "user.tctidy_test";
        if let Err(e) = setxattr(&target, name, b"kept", XattrFlags::empty()) {
            skip(
                "extended_attributes_are_kept",
                &format!("this file system refuses user extended attributes: {e}"),
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }

        write_atomic(&target, b"new").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(xattr(&target, name).unwrap(), b"kept");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn posix_acls_are_kept() {
        use rustix::fs::{setxattr, XattrFlags};
        use std::os::unix::fs::MetadataExt;

        let dir = scratch();
        let target = dir.join("a.st");
        fs::write(&target, b"old").unwrap();
        // The ACL `user::rw-, user:<uid>:r--, group::r--, mask::r--, other::---`
        // as Linux stores it: version 2, then (tag, permissions, id) entries.
        let uid = fs::metadata(&target).unwrap().uid();
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, perm, id) in [
            (0x01u16, 6u16, u32::MAX),
            (0x02, 4, uid),
            (0x04, 4, u32::MAX),
            (0x10, 4, u32::MAX),
            (0x20, 0, u32::MAX),
        ] {
            acl.extend(tag.to_le_bytes());
            acl.extend(perm.to_le_bytes());
            acl.extend(id.to_le_bytes());
        }
        let name = "system.posix_acl_access";
        if let Err(e) = setxattr(&target, name, &acl, XattrFlags::empty()) {
            skip(
                "posix_acls_are_kept",
                &format!("this file system refuses POSIX ACLs: {e}"),
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        let before = xattr(&target, name).unwrap();

        write_atomic(&target, b"new").unwrap();

        assert_eq!(xattr(&target, name).unwrap(), before);
        assert_eq!(mode(&target), 0o640);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    #[cfg(windows)]
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

    #[cfg(windows)]
    #[test]
    fn windows_hidden_and_system_attributes_are_kept() {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

        let dir = scratch();
        let target = dir.join("a.st");
        let kept = FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .attributes(kept)
            .open(&target)
            .unwrap()
            .write_all(b"old")
            .unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().file_attributes() & kept,
            kept
        );

        write_atomic(&target, b"new").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&target).unwrap().file_attributes() & kept,
            kept
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_creation_time_is_kept() {
        use std::os::windows::fs::FileTimesExt;

        let dir = scratch();
        let target = dir.join("a.st");
        fs::write(&target, b"old").unwrap();
        let created = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .unwrap()
            .set_times(fs::FileTimes::new().set_created(created))
            .unwrap();

        write_atomic(&target, b"new").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::metadata(&target).unwrap().created().unwrap(), created);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    // Clearing the read-only attribute is what the test means on Windows.
    #[allow(clippy::permissions_set_readonly_false)]
    fn windows_read_only_file_is_refused_and_left_as_is() {
        let dir = scratch();
        let target = dir.join("a.st");
        fs::write(&target, b"old").unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions.clone()).unwrap();

        let result = write_atomic(&target, b"new");

        assert!(
            matches!(result, Err(WriteError::NotWritten(_))),
            "{result:?}"
        );
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(fs::metadata(&target).unwrap().permissions().readonly());
        assert!(!has_temp_leftover(&dir));
        permissions.set_readonly(false);
        fs::set_permissions(&target, permissions).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}
