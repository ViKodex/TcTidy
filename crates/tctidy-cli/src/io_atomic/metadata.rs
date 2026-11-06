//! Carry the metadata of the file being replaced over to the temp file that
//! replaces it, before the rename, so that a failure here leaves the original
//! untouched. The parent module's docs list what is kept on each platform.

use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

/// The options to create the temp file with, given the file it replaces.
pub(super) fn temp_options(original: Option<&Metadata>) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Private until the original's mode is applied, so the content is never
        // exposed more widely than by the file it replaces. A brand-new file
        // keeps the usual 0666 less the umask.
        if original.is_some() {
            options.mode(0o600);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        // Attributes can be given when the file is created, and std cannot set
        // them afterwards.
        if let Some(original) = original {
            options.attributes(original.file_attributes() & imp::KEPT_ATTRIBUTES);
        }
    }
    options
}

/// Apply the metadata of `original` (the file at `path`) to the temp file,
/// which has been written but not yet synced or renamed.
pub(super) fn copy(original: &Metadata, path: &Path, temp: &File) -> io::Result<()> {
    imp::copy(original, path, temp)
}

/// `e`, with what was being done in front of it.
fn context(e: io::Error, what: String) -> io::Error {
    io::Error::new(e.kind(), format!("{what}: {e}"))
}

#[cfg(unix)]
mod imp {
    use std::fs::{File, Metadata, Permissions};
    use std::io;
    use std::os::unix::fs::{fchown, MetadataExt, PermissionsExt};
    use std::path::Path;

    use super::context;

    pub(super) fn copy(original: &Metadata, path: &Path, temp: &File) -> io::Result<()> {
        // Owner and group first: changing them clears the setuid and setgid
        // bits and file capabilities, which the steps below set again. Only
        // root can give a file to another user; when that is refused, the write
        // is refused too, rather than silently making the file someone else's.
        let fresh = temp.metadata()?;
        let uid = (fresh.uid() != original.uid()).then_some(original.uid());
        let gid = (fresh.gid() != original.gid()).then_some(original.gid());
        if uid.is_some() || gid.is_some() {
            fchown(temp, uid, gid).map_err(|e| {
                let (uid, gid) = (original.uid(), original.gid());
                let path = path.display();
                context(
                    e,
                    format!("cannot keep owner {uid} and group {gid} of {path}"),
                )
            })?;
        }

        // Then the extended attributes (POSIX ACLs among them on Linux), while
        // the temp file is still writable: a read-only mode would refuse them.
        xattr::copy(path, temp)?;

        // The permission bits last, setuid, setgid and sticky included. Setting
        // the ACL above may have changed the group bits, so look again.
        let mode = original.mode() & 0o7777;
        if temp.metadata()?.mode() & 0o7777 != mode {
            temp.set_permissions(Permissions::from_mode(mode))
                .map_err(|e| {
                    context(
                        e,
                        format!("cannot keep mode {mode:o} of {}", path.display()),
                    )
                })?;
            // chmod drops a setgid bit for a group the caller is not in,
            // without an error: check that the mode really is the original's.
            let got = temp.metadata()?.mode() & 0o7777;
            if got != mode {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "cannot keep mode {mode:o} of {}: the system set {got:o}",
                        path.display()
                    ),
                ));
            }
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    mod xattr {
        use std::fs::File;
        use std::io;
        use std::path::Path;

        use rustix::fs::{fgetxattr, fsetxattr, getxattr, listxattr, XattrFlags};
        use rustix::io::Errno;

        use super::context;

        /// Give `temp` every extended attribute of the file at `path`.
        pub(super) fn copy(path: &Path, temp: &File) -> io::Result<()> {
            let names = match sized(|buf| listxattr(path, buf)) {
                Ok(names) => names,
                // A file system without extended attributes: nothing to keep.
                Err(e) if e == Errno::NOTSUP || e == Errno::OPNOTSUPP => return Ok(()),
                Err(e) => {
                    let what = format!("cannot list the extended attributes of {}", path.display());
                    return Err(context(e.into(), what));
                }
            };
            for name in names.split(|&b| b == 0).filter(|name| !name.is_empty()) {
                let what = || {
                    let name = String::from_utf8_lossy(name);
                    format!(
                        "cannot keep extended attribute {name} of {}",
                        path.display()
                    )
                };
                let value = sized(|buf| getxattr(path, name, buf))
                    .map_err(|e| context(e.into(), what()))?;
                if let Err(e) = fsetxattr(temp, name, &value, XattrFlags::empty()) {
                    // The system may already have given the new file this very
                    // value (an SELinux label inherited from the directory), and
                    // may refuse to set it again: that is not a loss.
                    let same = sized(|buf| fgetxattr(temp, name, buf)).is_ok_and(|v| v == value);
                    if !same {
                        return Err(context(e.into(), what()));
                    }
                }
            }
            Ok(())
        }

        /// Read a value of unknown size: ask for the size, then read it,
        /// again if it grew in between.
        fn sized(
            mut read: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
        ) -> rustix::io::Result<Vec<u8>> {
            let mut attempts = 0;
            loop {
                let mut buf = vec![0; read(&mut [])?];
                match read(&mut buf) {
                    Ok(len) => {
                        buf.truncate(len);
                        return Ok(buf);
                    }
                    Err(e) if e == Errno::RANGE && attempts < 8 => attempts += 1,
                    Err(e) => return Err(e),
                }
            }
        }
    }

    /// No extended attribute API is used on other Unix systems.
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    mod xattr {
        pub(super) fn copy(_path: &std::path::Path, _temp: &std::fs::File) -> std::io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::fs::{File, FileTimes, Metadata};
    use std::io;
    use std::os::windows::fs::FileTimesExt;
    use std::path::Path;

    use super::context;

    /// The attributes given to the temp file when it is created: hidden, system
    /// and encrypted (so an encrypted file is never replaced by a plain one).
    /// Read-only is left out (a read-only file cannot be replaced: the rename
    /// fails and the original stays), and archive is set by the system on any
    /// file that changed.
    pub(super) const KEPT_ATTRIBUTES: u32 =
        FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_ENCRYPTED;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    const FILE_ATTRIBUTE_ENCRYPTED: u32 = 0x4000;

    pub(super) fn copy(original: &Metadata, path: &Path, temp: &File) -> io::Result<()> {
        // A reformatted file is an edited file, not a new one: keep the date it
        // was created. Its modification time is the time of this write.
        let created = original.created()?;
        temp.set_times(FileTimes::new().set_created(created))
            .map_err(|e| {
                context(
                    e,
                    format!("cannot keep the creation time of {}", path.display()),
                )
            })
    }
}
