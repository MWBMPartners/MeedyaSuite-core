// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Saving an M4A file by copy, then rename (issue #102).
// =====================================================
//
// Why the save goes to a copy at all: see `mp4_save_check` and
// `mp4_file_check`. Every M4A save in `tag_io` is made on a temporary copy
// of the file and checked there, and the copy replaces the file only when
// the check finds nothing wrong. This module is the "copy" and the
// "replace": how the copy is made, and how it takes the file's place.
//
// The steps, and what each one guards against:
//
// 1. **The file is opened once, for reading and writing** (`Original`),
//    without being changed. A file this program may not write to is
//    refused here, with a plain message — the save would have failed
//    anyway (an in-place write needs the same permission), and without
//    this step a read-only file in a writable folder would have been
//    replaced by a writable copy. The copy is made from this handle, and
//    the checks read the original through it.
// 2. **The copy is a NEW file**, made with `create_new`, which fails when
//    anything at all already has that name — a file, or a symbolic link
//    (it does not follow one) — so nothing that was already there is ever
//    opened or written through. The copy's contents are then written
//    through the handle that very call returned; the name is never opened
//    again. (Until the stand-in review of revision 9, finding L3, the
//    contents were copied with `std::fs::copy`, which opens the name a
//    second time and follows a symbolic link: another user of a shared
//    folder could have put a link there between the two steps, and the
//    copy — the whole audio file — would have been written wherever the
//    link pointed.)
//    **And it is private from the moment it exists**: on Unix it is made
//    readable and writable by its owner only (permissions 0600 — the
//    program's file-creation mask can only take more away), before a
//    single byte is copied into it, and it keeps that until step 4. (Until
//    Codex's catch-up review of revisions 8-10, finding 1, it was made with
//    the usual permissions, normally 0644: the whole of a private
//    recording, kept 0600, could be read by every other account on the
//    machine for as long as the save was being checked — and still could
//    when the save was then refused, until the copy was deleted.) Windows
//    has no such permission bits: there the copy gets whatever access
//    rules its folder gives every new file in it, which may let others
//    read it where the original's own rules did not, for as long as it
//    exists — and, since a file's own rules are not copied, the saved file
//    keeps the folder's rules afterwards too (see the losses below).
// 3. **lofty saves into that same handle**, and the checks read the copy
//    back through it (see `tag_io::save_mp4_checked`).
// 4. **Before the rename**, the copy is given the original's permissions
//    (only now, once it has passed every check and is about to take the
//    original's place — never wider than the original's at any moment),
//    flushed to the disk (`sync_all`), and both names are checked to still
//    name the files the two handles hold: device and inode number on
//    Unix, volume serial number and file index on Windows. A name that
//    now names something else — the copy swapped for another file, or the
//    original replaced by someone else's file while the save was being
//    checked — refuses the save.
// 5. **The rename**, which puts the copy in the original's place in one
//    step: at every moment the name holds either the whole old file or
//    the whole new one.
//
// What it CANNOT do, stated plainly:
//
// - Close the gap between step 4 and step 5 completely. A rename works on
//   names, not on handles, so something swapped in the instant between the
//   check and the rename is not seen. The check narrows that gap to almost
//   nothing; no ordinary file operation removes it.
// - Survive every power cut. `sync_all` puts the copy's contents on the
//   disk before the rename, so the name should afterwards hold either the
//   old file or the complete new one, never half of one. The folder itself
//   is not flushed, so a power cut soon after a save can still undo the
//   rename (the file then has its old tags), and a disk's own write cache
//   is beyond anything a program can control.
// - Keep everything an in-place write keeps. The saved file is a NEW file,
//   so compared with writing into the old one it loses:
//   - other names for the same file: a hard link keeps the OLD tags;
//   - the owner and group, when the program saving is not the file's owner
//     (the new file belongs to whoever saved it);
//   - access control lists, and extended attributes — on Linux, and on
//     macOS too (Finder tags and comments among them): the copy is written
//     through its own handle, and the standard library has no way to copy
//     those onto a handle (`std::fs::copy`, which did copy them on macOS,
//     is the step L3 removed);
//   - the creation ("birth") time, where the system keeps one: the new
//     file's is the time of the save;
//   - and it needs the folder to be writable, not just the file.
//   The permission bits (read, write, run) ARE copied.
// - Avoid leaving a file behind if the program is killed mid-save. The copy
//   is deleted on every refusal and error, but a program stopped by force
//   cannot delete it: it stays, hidden, beside the file, named
//   `.meedya-tag-save-<process id>-<number>.tmp`, and can be deleted by
//   hand. The original is untouched in that case.
// - Be cheap for a large file: every save copies the whole file, and lofty
//   reads the whole file into memory to save it.

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::MetadataError;

// ============================================================
// Which file a name or a handle refers to
// ============================================================

/// Which file a handle holds, or a name names: the device and inode
/// number on Unix, the volume serial number and file index on Windows.
/// Two equal identities are the same file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    device: u64,
    file: u64,
}

/// The identity of the file `file` holds open.
#[cfg(unix)]
fn identity_of_handle(file: &File) -> io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(FileIdentity {
        device: metadata.dev(),
        file: metadata.ino(),
    })
}

/// The identity of what `path` names, WITHOUT following a symbolic link:
/// a link put where a file was is a different thing, not the file it
/// points to.
#[cfg(unix)]
fn identity_of_name(path: &Path) -> io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    Ok(FileIdentity {
        device: metadata.dev(),
        file: metadata.ino(),
    })
}

/// The identity of the file `file` holds open (Windows:
/// `GetFileInformationByHandle`, through `winapi-util`'s safe wrapper —
/// the standard library's own way to read these two numbers is not stable
/// yet).
#[cfg(windows)]
fn identity_of_handle(file: &File) -> io::Result<FileIdentity> {
    let information = winapi_util::file::information(file)?;
    Ok(FileIdentity {
        device: information.volume_serial_number(),
        file: information.file_index(),
    })
}

/// The identity of what `path` names, WITHOUT following a symbolic link
/// or other reparse point (`FILE_FLAG_OPEN_REPARSE_POINT`), opened without
/// asking to read or write it (access 0; `FILE_FLAG_BACKUP_SEMANTICS` lets
/// the call open a folder too, so a folder put in the file's place is
/// seen as different rather than as an error).
#[cfg(windows)]
fn identity_of_name(path: &Path) -> io::Result<FileIdentity> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    identity_of_handle(&file)
}

/// On any other system there is no identity to compare, so every file
/// counts as the same one and the check in step 4 finds nothing: the save
/// then relies on `create_new` alone (step 2).
#[cfg(not(any(unix, windows)))]
fn identity_of_handle(_file: &File) -> io::Result<FileIdentity> {
    Ok(FileIdentity { device: 0, file: 0 })
}

#[cfg(not(any(unix, windows)))]
fn identity_of_name(_path: &Path) -> io::Result<FileIdentity> {
    Ok(FileIdentity { device: 0, file: 0 })
}

// ============================================================
// The original
// ============================================================

/// The file being saved, opened once (step 1 at the top of this file).
pub(crate) struct Original {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
}

impl Original {
    /// Opens `real` — the file itself, links already followed — for
    /// reading and writing, without changing it. Fails with a plain
    /// [`MetadataError::WriteError`] when this program may not write to
    /// it, and with [`MetadataError::IoError`] when it cannot be opened for
    /// another reason.
    pub(crate) fn open(real: &Path) -> Result<Self, MetadataError> {
        let file = match OpenOptions::new().read(true).write(true).open(real) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                return Err(MetadataError::WriteError(format!(
                    "this file's tags cannot be saved, because the file is read-only or this \
                     program may not write to it ({}: {e}). Nothing was written.",
                    real.display()
                )))
            }
            Err(e) => return Err(e.into()),
        };
        let identity = identity_of_handle(&file)?;
        Ok(Original {
            path: real.to_path_buf(),
            file,
            identity,
        })
    }

    /// The open file, for reading (its position is anybody's: every reader
    /// seeks first).
    pub(crate) fn file(&mut self) -> &mut File {
        &mut self.file
    }
}

// ============================================================
// The temporary copy
// ============================================================

/// A temporary copy of an [`Original`], made in the same folder (so that
/// replacing the original is one rename on one disk), and deleted again
/// unless it replaces the original.
pub(crate) struct TempCopy {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
    kept: bool,
}

/// Numbers the temporary copies this process makes, so two saves at once
/// never pick the same name.
static COPIES_MADE: AtomicU64 = AtomicU64::new(0);

/// How many names a copy tries before giving up (each taken name is
/// skipped, never used).
const NAMES_TRIED: usize = 100;

/// The names a new copy in `folder` tries, in order (see the top of this
/// file for the pattern).
fn temporary_names(folder: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    (0..NAMES_TRIED).map(move |_| {
        folder.join(format!(
            ".meedya-tag-save-{}-{}.tmp",
            std::process::id(),
            COPIES_MADE.fetch_add(1, Ordering::Relaxed)
        ))
    })
}

impl TempCopy {
    /// A copy of `original`, beside it (step 2 at the top of this file).
    pub(crate) fn of(original: &mut Original) -> Result<Self, MetadataError> {
        let folder = original
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self::of_named(original, &folder, temporary_names(&folder))
    }

    /// [`TempCopy::of`], trying the given `names` (in `folder`) in turn.
    fn of_named(
        original: &mut Original,
        folder: &Path,
        names: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, MetadataError> {
        for candidate in names {
            // `create_new`: fails if the name is taken at all — a file or
            // a symbolic link, which is never followed — so nothing that
            // was already there is opened or written through.
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            // Private from the moment it exists, before anything is copied
            // into it (step 2 at the top of this file): owner only.
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = match options.open(&candidate) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(MetadataError::WriteError(format!(
                        "saving an M4A file makes a copy beside it, so the file's folder must be \
                         writable, and a new file could not be made in {} ({e}). Nothing was \
                         written.",
                        folder.display()
                    )))
                }
            };
            // From here on `copy` exists, so any failure deletes it again.
            let mut copy = TempCopy {
                path: candidate,
                file,
                identity: FileIdentity { device: 0, file: 0 },
                kept: false,
            };
            copy.identity = identity_of_handle(&copy.file)?;
            // The contents, through the handle `create_new` returned: the
            // name is never opened a second time.
            original.file.seek(SeekFrom::Start(0))?;
            io::copy(&mut original.file, &mut copy.file)?;
            copy.file.seek(SeekFrom::Start(0))?;
            return Ok(copy);
        }
        Err(MetadataError::WriteError(format!(
            "could not find a free name for the temporary copy an M4A save is checked on (the \
             {NAMES_TRIED} names tried in {} were all taken). Nothing was written.",
            folder.display()
        )))
    }

    /// The copy's open file, for saving into and reading back.
    pub(crate) fn file(&mut self) -> &mut File {
        &mut self.file
    }

    /// The copy's name (for tests).
    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Tests only: hands the copy's name to [`WHILE_THE_COPY_EXISTS`], when
    /// a test has set it. `tag_io` calls this at the two moments a save
    /// holds the whole recording in the copy before any check has run:
    /// just after the copy is made, and just after lofty has saved into it.
    #[cfg(test)]
    pub(crate) fn let_tests_look(&self) {
        WHILE_THE_COPY_EXISTS.with(|hook| {
            if let Some(look) = hook.borrow_mut().as_mut() {
                look(&self.path);
            }
        });
    }

    /// Puts this copy in `original`'s place (steps 4 and 5 at the top of
    /// this file): the original's permissions, a flush to the disk, the
    /// check that both names still name the two open files, then one
    /// rename. On any failure the copy is deleted and the original is left
    /// as it was.
    pub(crate) fn replace(mut self, original: Original) -> Result<(), MetadataError> {
        self.file
            .set_permissions(original.file.metadata()?.permissions())?;
        self.file.sync_all()?;
        let swapped = |what: &str, path: &Path, why: String| {
            MetadataError::WriteError(format!(
                "{what} ({}) {why} while the save was being checked, so the checked copy was \
                 not put in the file's place. Nothing was written.",
                path.display()
            ))
        };
        match identity_of_name(&self.path) {
            Ok(identity) if identity == self.identity => {}
            Ok(_) => {
                return Err(swapped(
                    "the temporary copy",
                    &self.path,
                    "was replaced by another file".to_string(),
                ))
            }
            Err(e) => {
                return Err(swapped(
                    "the temporary copy",
                    &self.path,
                    format!("could no longer be found ({e})"),
                ))
            }
        }
        match identity_of_name(&original.path) {
            Ok(identity) if identity == original.identity => {}
            Ok(_) => {
                return Err(swapped(
                    "the file",
                    &original.path,
                    "was replaced by another file".to_string(),
                ))
            }
            Err(e) => {
                return Err(swapped(
                    "the file",
                    &original.path,
                    format!("could no longer be found ({e})"),
                ))
            }
        }
        // The original's handle is closed before the rename: on Windows a
        // file another handle holds open may not be replaced.
        let Original { path, file, .. } = original;
        drop(file);
        std::fs::rename(&self.path, &path)?;
        self.kept = true;
        Ok(())
    }
}

/// Tests only: what a test hands [`WHILE_THE_COPY_EXISTS`] — called with
/// the copy's name.
#[cfg(test)]
pub(crate) type LookAtTheCopy = Box<dyn FnMut(&Path)>;

#[cfg(test)]
thread_local! {
    /// Tests only: called with the copy's name at the moments a save
    /// pauses with the copy in existence ([`TempCopy::let_tests_look`]), so
    /// a test can look at the copy — or change the folder around it — while
    /// it exists. Set per thread, so tests running side by side never see
    /// each other's.
    pub(crate) static WHILE_THE_COPY_EXISTS: std::cell::RefCell<Option<LookAtTheCopy>> =
        const { std::cell::RefCell::new(None) };
}

impl Drop for TempCopy {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        // Only the copy itself is deleted: if its name now names something
        // else (step 4 found it swapped), that is left alone. Nothing more
        // can be done if deleting fails; the copy is a hidden file beside
        // the original, never the original itself.
        if matches!(identity_of_name(&self.path), Ok(identity) if identity == self.identity) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("list")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    fn original_in(dir: &Path, contents: &[u8]) -> (PathBuf, Original) {
        let path = dir.join("f.m4a");
        std::fs::write(&path, contents).expect("write");
        let original = Original::open(&path).expect("open");
        (path, original)
    }

    #[test]
    fn a_temporary_copy_is_removed_unless_it_replaces_the_original() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        let copy = TempCopy::of(&mut original).expect("copy");
        let copy_path = copy.path().to_path_buf();
        assert_eq!(std::fs::read(&copy_path).expect("read"), b"original");
        drop(copy);
        assert!(!copy_path.exists(), "a copy not used is deleted");

        let mut copy = TempCopy::of(&mut original).expect("copy");
        copy.file().set_len(0).expect("truncate");
        copy.file().write_all(b"changed").expect("write");
        let copy_path = copy.path().to_path_buf();
        copy.replace(original).expect("replace");
        assert_eq!(std::fs::read(&path).expect("read"), b"changed");
        assert!(!copy_path.exists());
        assert_eq!(
            names_in(dir.path()),
            ["f.m4a"],
            "nothing is left beside the file"
        );
    }

    #[test]
    fn a_failed_rename_deletes_the_copy_and_leaves_the_original() {
        // The rename fails when the original's name has become a folder
        // holding a file (a rename cannot replace a folder that is not
        // empty). Found only because the copy is deleted when the rename
        // FAILS: a copy marked as kept before the rename (the stand-in
        // review of revision 9, planted fault A16) would stay behind.
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        let copy = TempCopy::of(&mut original).expect("copy");
        let copy_path = copy.path().to_path_buf();
        // Swap the original for a folder, keeping the check in step 4 happy
        // by handing `replace` an `Original` that names that folder.
        std::fs::remove_file(&path).expect("remove");
        std::fs::create_dir(&path).expect("folder");
        std::fs::write(path.join("inside"), b"x").expect("write");
        let inside = Original::open(&path.join("inside")).expect("open");
        let as_folder = Original {
            path: path.clone(),
            file: inside.file,
            identity: identity_of_name(&path).expect("identity"),
        };
        assert!(copy.replace(as_folder).is_err(), "the rename fails");
        assert!(!copy_path.exists(), "the copy is deleted");
        assert_eq!(
            std::fs::read(path.join("inside")).expect("read"),
            b"x",
            "what is at the name is left alone"
        );
        drop(original);
    }

    #[cfg(unix)]
    #[test]
    fn a_name_already_taken_is_never_opened_or_written_through() {
        // Every name the copy tries is already a symbolic link to a victim
        // file. `create_new` refuses each (it never follows a link), so the
        // save is refused and the victim is untouched. Opened with `create`
        // instead (the stand-in review of revision 9, planted fault A17),
        // the first link would be followed and the victim overwritten.
        let dir = tempfile::tempdir().expect("tempdir");
        let (_path, mut original) = original_in(dir.path(), b"original audio");
        let victim = dir.path().join("victim.txt");
        std::fs::write(&victim, b"victim").expect("write");
        let names: Vec<PathBuf> = (0..3)
            .map(|n| dir.path().join(format!("taken-{n}.tmp")))
            .collect();
        for name in &names {
            std::os::unix::fs::symlink(&victim, name).expect("link");
        }
        let refusal = match TempCopy::of_named(&mut original, dir.path(), names) {
            Err(MetadataError::WriteError(message)) => message,
            Err(other) => panic!("expected a refusal, got {other:?}"),
            Ok(_) => panic!("expected a refusal, got a copy"),
        };
        assert!(refusal.contains("could not find a free name"), "{refusal}");
        assert_eq!(std::fs::read(&victim).expect("read"), b"victim");
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_swapped_before_the_rename_is_refused() {
        // Someone replaces the temporary copy with their own file between
        // the save and the rename (L3). The name no longer names the copy's
        // handle, so the save is refused, the original is untouched, and
        // the other file is left where it is.
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        let copy = TempCopy::of(&mut original).expect("copy");
        let copy_path = copy.path().to_path_buf();
        let other = dir.path().join("other");
        std::fs::write(&other, b"someone else's").expect("write");
        std::fs::rename(&other, &copy_path).expect("swap");
        let refusal = match copy.replace(original) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(
            refusal.contains("was replaced by another file"),
            "{refusal}"
        );
        assert_eq!(std::fs::read(&path).expect("read"), b"original");
        assert_eq!(std::fs::read(&copy_path).expect("read"), b"someone else's");
    }

    #[cfg(unix)]
    #[test]
    fn an_original_replaced_before_the_rename_is_refused() {
        // The file itself is replaced by another while the save is being
        // checked: the copy (made from the old file) must not be put over
        // the new one.
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        let copy = TempCopy::of(&mut original).expect("copy");
        let copy_path = copy.path().to_path_buf();
        let newer = dir.path().join("newer");
        std::fs::write(&newer, b"a newer file").expect("write");
        std::fs::rename(&newer, &path).expect("replace");
        let refusal = match copy.replace(original) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(refusal.contains("the file ("), "{refusal}");
        assert_eq!(std::fs::read(&path).expect("read"), b"a newer file");
        assert!(!copy_path.exists(), "the copy is deleted");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_written_gets_a_plain_message() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("read-only folder");
        let result = TempCopy::of(&mut original);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
            .expect("writable again");
        // Running as the superuser, the folder is writable anyway.
        if let Err(error) = result {
            let message = error.to_string();
            assert!(
                message.contains("so the file's folder must be writable"),
                "{message}"
            );
            assert!(message.contains("Nothing was written"), "{message}");
        }
        assert_eq!(std::fs::read(&path).expect("read"), b"original");
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_file_is_refused_with_a_plain_message() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.m4a");
        std::fs::write(&path, b"original").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("read-only");
        // Running as the superuser, the file can be opened for writing.
        if let Err(error) = Original::open(&path) {
            let message = error.to_string();
            assert!(message.contains("read-only"), "{message}");
            assert!(message.contains("Nothing was written"), "{message}");
        }
        assert_eq!(names_in(dir.path()), ["f.m4a"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_copy_is_private_from_the_moment_it_exists() {
        // Codex's catch-up review of revisions 8-10, finding 1: the copy
        // was made with the usual permissions (0644 under the usual
        // creation mask), so a private file's whole contents were readable
        // by others while the save was checked. Now owner-only from the
        // start, whatever the original's - and the original's permissions
        // only once it replaces the original.
        use std::os::unix::fs::PermissionsExt;
        let mode =
            |path: &Path| std::fs::metadata(path).expect("stat").permissions().mode() & 0o777;
        for original_mode in [0o600, 0o644, 0o666] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("f.m4a");
            std::fs::write(&path, b"a private recording").expect("write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(original_mode))
                .expect("mode");
            let mut original = Original::open(&path).expect("open");
            let copy = TempCopy::of(&mut original).expect("copy");
            assert_eq!(
                mode(copy.path()),
                0o600,
                "the copy of a {original_mode:o} file"
            );
            copy.replace(original).expect("replace");
            assert_eq!(mode(&path), original_mode, "after replacing");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_copy_gets_the_original_s_permission_bits() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.m4a");
        std::fs::write(&path, b"original").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("mode");
        let mut original = Original::open(&path).expect("open");
        let copy = TempCopy::of(&mut original).expect("copy");
        copy.replace(original).expect("replace");
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o7777;
        assert_eq!(mode, 0o640);
    }
}
