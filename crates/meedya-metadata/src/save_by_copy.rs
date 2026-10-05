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
//    when the save was then refused, until the copy was deleted.)
//    Permission bits are not the whole of it, though: a folder can pass an
//    access control list on to every new file made in it, and the copy got
//    it however private its bits were (Codex's review of revision 11,
//    finding 1: a folder whose list passed on "everyone may read" made the
//    copy of a private recording - and then the saved file - readable by
//    every account on the machine). So, before a byte is written into it,
//    the copy has none (`access_rules`): on Linux whatever its folder gave
//    it is taken off; on macOS, where this program can only read such
//    lists, a folder that passes any entry on is refused before a copy is
//    made, and a copy that got one anyway is refused; on any other Unix
//    system, which this program cannot read lists on, every save is
//    refused. Windows has no such permission bits: there the copy gets
//    whatever access rules its folder gives every new file in it, which may
//    let others read it where the original's own rules did not, for as
//    long as it exists — and, since a file's own rules are not copied, the
//    saved file keeps the folder's rules afterwards too (see the losses
//    below). A file with the set-user-ID or set-group-ID permission is
//    refused here too (a media file should never carry them).
// 3. **lofty saves into that same handle**, and the checks read the copy
//    back through it (see `tag_io::save_mp4_checked`).
// 4. **Before the rename**, the copy is given the original's group (its
//    owner stays whoever saved it), then the original's own access control
//    list (on Linux; on macOS an original that has one is refused, before
//    the copy is made and again here), then the original's permissions -
//    in that order, and only now, once it has passed every check and is
//    about to take the original's place, so it never lets in more than the
//    original at any moment. The group: when the copy cannot be given it,
//    the save is refused if the original lets its group in at all
//    (Codex's review of revision 11, finding 1: a file kept 0640 in a group
//    of its own came back in its folder's group, readable by everyone in
//    that one). The copy is then flushed to the disk (`sync_all`), and
//    both names are checked to still name the files the two handles hold:
//    device and inode number on Unix, volume serial number and file index
//    on Windows. A name that now names something else — the copy swapped
//    for another file, or the original replaced by someone else's file
//    while the save was being checked — refuses the save.
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
//   - the owner, when the program saving is not the file's owner (the new
//     file belongs to whoever saved it). The group is kept (step 4) - or
//     the save refused - since revision 12; until then it was lost too;
//   - access control lists, on Windows (the saved file has its folder's
//     rules) - on Linux the file's own list is carried over, and on macOS
//     a file with one is refused (`access_rules`) since revision 12;
//   - extended attributes — on Linux, and on macOS too (Finder tags and
//     comments among them): the copy is written through its own handle,
//     and the standard library has no way to copy those onto a handle
//     (`std::fs::copy`, which did copy them on macOS, is the step L3
//     removed);
//   - the creation ("birth") time, where the system keeps one: the new
//     file's is the time of the save;
//   - on Windows, any named alternate data stream: NTFS can keep further
//     named streams of data beside a file's main contents (`song.m4a:notes`
//     — some programs keep notes, or where a download came from, there).
//     Only the main stream is copied (`io::copy` reads that alone), so after
//     the rename they are gone — and nothing checks for them first, so the
//     save goes ahead without a word (Codex's catch-up review of revisions
//     8-10, finding 6; worked out from the calls used, not tried on
//     Windows);
//   - and it needs the folder to be writable, not just the file.
//   The permission bits (read, write, run) ARE copied; the set-user-ID and
//   set-group-ID ones refuse the save instead.
// - Promise the copy is always deleted. On every refusal and error it is
//   deleted, explicitly, before the error is returned (`TempCopy::discard`)
//   — by its name, and only while that name still names it: a file put at
//   its name meanwhile is someone else's and is left alone. It counts as
//   deleted only when nothing names it any more: on Unix its open handle
//   says so (the file's count of names is 0); elsewhere, only when this
//   program deleted it by its own name. When that cannot be confirmed, the
//   error says so: deleting it failed (its folder made read-only since the
//   copy was made, say) - and the error names it, so whoever called can
//   delete it; or the copy was moved, or given another name, while the save
//   was being checked - it is left wherever it went, which this program
//   does not know, and the error says that. (Until Codex's catch-up review
//   of revisions 8-10, finding 7, a failed deletion was ignored without a
//   word, and this comment said the copy was deleted on every refusal and
//   error; until Codex's review of revision 11, finding 3, a copy moved away
//   counted as deleted, and the refusal said nothing about it.) The check
//   of the name and the deletion are two steps on a name, so a file swapped
//   in at that very instant would be deleted instead. A program stopped by
//   force, or one that crashes mid-save, cannot delete it at all: it stays,
//   hidden, beside the file. Wherever it stays beside the file, the copy is
//   named `.meedya-tag-save-<process id>-<number>.tmp` and can be deleted
//   by hand, and the original is untouched. (Dropping a copy that was neither
//   put in place nor discarded — a save interrupted by a crash that still
//   unwinds — tries once more to delete it, silently, as a last resort.)
// - Be cheap for a large file: every save copies the whole file, and lofty
//   reads the whole file into memory to save it.
// - Save on a disk that does not keep a file's number steady. On macOS a
//   FAT or exFAT disk (a USB stick, a memory card) gives an empty file a
//   stand-in number and changes it once data is written into it (measured:
//   18446744073709551609, then 4), so step 4 sees the copy - numbered when
//   it was made, empty - as replaced, and refuses every M4A save there; and
//   since its name no longer seems to name it, the copy is not deleted but
//   left beside the file, and the error says so. (Found in revision 12;
//   since revision 10 this happened on such disks with no word about the
//   copy. Not fixed yet.)

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::access_rules;
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

    /// Whether the file's name still names the file its handle holds.
    fn still_named(&self) -> bool {
        identity_of_name(&self.path).ok() == Some(self.identity)
    }

    /// Refuses, before any copy is made, what a save by copy could not keep
    /// (step 2 at the top of this file): the set-user-ID or set-group-ID
    /// permission, and access rules beyond the permission bits that
    /// `access_rules` cannot keep private or carry over (in `folder`, which
    /// the copy would be made in, or on the file itself).
    fn refuse_what_a_copy_cannot_keep(&self, folder: &Path) -> Result<(), MetadataError> {
        refuse_program_permissions(&self.file.metadata()?)?;
        access_rules::check_before_copying(&self.file, &self.path, &|| self.still_named(), folder)
            .map_err(nothing_was_written)
    }
}

/// `why` - a plain sentence from `access_rules` - as the refusal it is.
fn nothing_was_written(why: String) -> MetadataError {
    MetadataError::WriteError(format!("{why}. Nothing was written."))
}

/// Refuses a file with the set-user-ID or set-group-ID permission (Codex's
/// review of revision 11, finding 1). They are permissions for programs -
/// "run as this file's owner, or group" - which a media file should never
/// carry; a copy given them would hand them to a new file owned by whoever
/// saved it, and a copy without them would drop them without a word. So
/// neither is done: the save is refused.
#[cfg(unix)]
fn refuse_program_permissions(metadata: &std::fs::Metadata) -> Result<(), MetadataError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o6000 == 0 {
        return Ok(());
    }
    Err(MetadataError::WriteError(
        "this file has the set-user-ID or set-group-ID permission - a permission for programs, \
         which a media file should never carry - and a save by copy could neither keep it safely \
         nor drop it without a word, so the save is refused. Nothing was written."
            .to_string(),
    ))
}

/// No such permissions outside Unix.
#[cfg(not(unix))]
fn refuse_program_permissions(_metadata: &std::fs::Metadata) -> Result<(), MetadataError> {
    Ok(())
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
    /// Whether the copy is finished with — put in the original's place, or
    /// discarded (deleted, or reported as not deletable) — so that dropping
    /// it does nothing more.
    done: bool,
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
        original.refuse_what_a_copy_cannot_keep(&folder)?;
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
            // From here on the copy exists, so every failure deletes it
            // again — and says so, naming it, when it cannot.
            let identity = match identity_of_handle(&file) {
                Ok(identity) => identity,
                Err(e) => {
                    // Which file it is cannot be read, so it cannot be
                    // checked before deleting; it was made by the call
                    // above an instant ago, so it is deleted by name.
                    drop(file);
                    let removed = std::fs::remove_file(&candidate).map_err(NotDeleted::Failed);
                    return Err(with_copy_not_deleted(e.into(), &candidate, removed));
                }
            };
            let mut copy = TempCopy {
                path: candidate,
                file,
                identity,
                done: false,
            };
            // Private from the moment it exists: owner-only permission bits
            // (above) and, before a byte is written into it, none of the
            // access rules its folder may have given it (`access_rules`;
            // Codex's review of revision 11, finding 1).
            let made_private =
                access_rules::make_private(&copy.file, &copy.path, &|| copy.still_named());
            if let Err(why) = made_private {
                return Err(copy.discard(nothing_was_written(why)));
            }
            return match copy.fill_from(original) {
                Ok(()) => Ok(copy),
                Err(e) => Err(copy.discard(e.into())),
            };
        }
        Err(MetadataError::WriteError(format!(
            "could not find a free name for the temporary copy an M4A save is checked on (the \
             {NAMES_TRIED} names tried in {} were all taken). Nothing was written.",
            folder.display()
        )))
    }

    /// Writes `original`'s whole contents into the copy, through the handle
    /// `create_new` returned: the name is never opened a second time.
    fn fill_from(&mut self, original: &mut Original) -> io::Result<()> {
        original.file.seek(SeekFrom::Start(0))?;
        io::copy(&mut original.file, &mut self.file)?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(())
    }

    /// The copy's open file, for saving into and reading back.
    pub(crate) fn file(&mut self) -> &mut File {
        &mut self.file
    }

    /// Whether the copy's name still names the file its handle holds.
    fn still_named(&self) -> bool {
        identity_of_name(&self.path).ok() == Some(self.identity)
    }

    /// Gives the copy the original's group, keeping its owner (step 4 at
    /// the top of this file) - BEFORE the original's permission bits, so
    /// that the group they let in is always the original's. When the copy
    /// cannot be given it (the program saving is not in that group, say),
    /// the save is refused if the original lets its group in at all;
    /// otherwise the copy keeps the group it was made with, which its bits
    /// then let in to nothing. (Until Codex's review of revision 11, finding
    /// 1, the group was not copied: a file kept 0640 in a group of its own
    /// came back in its folder's group - readable by everyone in that one.)
    #[cfg(unix)]
    fn give_group_of(&self, original: &std::fs::Metadata) -> Result<(), MetadataError> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let (wanted, now) = (original.gid(), self.file.metadata()?.gid());
        if wanted == now {
            return Ok(());
        }
        match std::os::unix::fs::fchown(&self.file, None, Some(wanted)) {
            Ok(()) => Ok(()),
            Err(_) if original.permissions().mode() & 0o070 == 0 => Ok(()),
            Err(e) => Err(MetadataError::WriteError(format!(
                "the saved copy could not be given this file's group (group {wanted}: {e}), and the \
                 file lets its group read or write it - the copy would have let its own folder's \
                 group (group {now}) in instead. Nothing was written."
            ))),
        }
    }

    /// No groups outside Unix.
    #[cfg(not(unix))]
    fn give_group_of(&self, _original: &std::fs::Metadata) -> Result<(), MetadataError> {
        Ok(())
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
    /// rename. On any failure the original is left as it was, and the copy
    /// is deleted — or, when it cannot be, named in the error
    /// ([`TempCopy::discard`]).
    pub(crate) fn replace(mut self, original: Original) -> Result<(), MetadataError> {
        match self.put_in_place(original) {
            Ok(()) => {
                self.done = true;
                Ok(())
            }
            Err(error) => Err(self.discard(error)),
        }
    }

    /// The steps of [`TempCopy::replace`], stopping at the first that fails.
    fn put_in_place(&mut self, original: Original) -> Result<(), MetadataError> {
        // The original's group, then its own access rules, then its
        // permission bits - in that order, so the copy never lets in more
        // than the original does at any moment (see `give_group_of`).
        let metadata = original.file.metadata()?;
        refuse_program_permissions(&metadata)?;
        self.give_group_of(&metadata)?;
        access_rules::carry_over(
            &original.file,
            &original.path,
            &|| original.still_named(),
            &self.file,
        )
        .map_err(nothing_was_written)?;
        self.file.set_permissions(metadata.permissions())?;
        #[cfg(test)]
        self.let_tests_look();
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
                    "was replaced by another file (or, on a disk that does not keep a file's \
                     number steady - a FAT or exFAT drive on macOS - its number changed as it was \
                     written, which refuses every M4A save on such a disk for now)"
                        .to_string(),
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
        Ok(())
    }

    /// Deletes this copy, because the save it was made for will not go
    /// ahead, and returns `error` — the reason — for the caller to pass on.
    /// When the copy cannot be confirmed deleted, the error says so as well
    /// ([`with_copy_not_deleted`]): when deleting it failed (its folder no
    /// longer writable, say), naming it so it can be deleted by hand; when
    /// it was moved or given another name while the save was being checked,
    /// saying so - it was left wherever it went, and anything now at its
    /// name was left alone (see the top of this file).
    pub(crate) fn discard(mut self, error: MetadataError) -> MetadataError {
        self.done = true;
        let removed = self.remove();
        with_copy_not_deleted(error, &self.path, removed)
    }

    /// Deletes the copy by its name - only if the name still names it: a
    /// file now at that name is someone else's and is left alone - and then
    /// confirms that no name for the copy is left anywhere.
    ///
    /// Until Codex's review of revision 11, finding 3, a name that was gone,
    /// or that named another file, counted as the copy being deleted. But a
    /// copy moved away while the save was being checked is not deleted: it
    /// still holds the whole file's contents, under a name this program does
    /// not know - and nothing said so.
    ///
    /// What it cannot do: rule out the instant between the check of the
    /// name and the deletion. A deletion works on a name, not on a handle,
    /// so a file swapped in at that instant would be deleted instead.
    fn remove(&self) -> Result<(), NotDeleted> {
        let deleted_by_its_name = match identity_of_name(&self.path) {
            Ok(identity) if identity == self.identity => {
                std::fs::remove_file(&self.path).map_err(NotDeleted::Failed)?;
                true
            }
            Ok(_) => false,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(NotDeleted::Failed(e)),
        };
        if self.no_name_left(deleted_by_its_name)? {
            Ok(())
        } else {
            Err(NotDeleted::Moved)
        }
    }

    /// Whether no name for the copy is left anywhere. On Unix its open
    /// handle says so - the file's count of names ("links") is 0 - whoever
    /// took the last name away (so a copy deleted by someone else counts as
    /// deleted, and one given a second name does not).
    #[cfg(unix)]
    fn no_name_left(&self, _deleted_by_its_name: bool) -> Result<bool, NotDeleted> {
        use std::os::unix::fs::MetadataExt;
        let metadata = self.file.metadata().map_err(NotDeleted::Failed)?;
        Ok(metadata.nlink() == 0)
    }

    /// Elsewhere only the name can say: the copy counts as deleted only
    /// when this program deleted it by that name. (On Windows a deleted
    /// file another handle holds open may still count its name until the
    /// handle closes, so the count of names is not relied on there; a copy
    /// someone else deleted is reported as not confirmed deleted - the
    /// careful side.)
    #[cfg(not(unix))]
    fn no_name_left(&self, deleted_by_its_name: bool) -> Result<bool, NotDeleted> {
        Ok(deleted_by_its_name)
    }
}

/// Why a temporary copy could not be confirmed deleted.
enum NotDeleted {
    /// Deleting it failed.
    Failed(io::Error),
    /// It was moved, or given another name, while the save was being
    /// checked, so it was left wherever it went.
    Moved,
}

/// `error`, as it is when `removed` says the temporary copy made as `path`
/// was deleted; otherwise a [`MetadataError::WriteError`] whose message is
/// `error`'s, followed by a sentence saying what became of the copy.
fn with_copy_not_deleted(
    error: MetadataError,
    path: &Path,
    removed: Result<(), NotDeleted>,
) -> MetadataError {
    let Err(not_deleted) = removed else {
        return error;
    };
    let first = match error {
        MetadataError::WriteError(message) => message,
        other => other.to_string(),
    };
    let what = match not_deleted {
        NotDeleted::Failed(why) => format!(
            "could not be deleted afterwards ({why}): it is {}, beside the file, and can be \
             deleted by hand",
            path.display()
        ),
        NotDeleted::Moved => format!(
            "could no longer be shown to be the file this save made, so it was not deleted: it was \
             moved, or given another name, while the save was being checked - or, on a disk that \
             does not keep a file's number steady (a FAT or exFAT drive on macOS), its number \
             changed as it was written. It may still hold the whole file's contents: it was made \
             as {}, beside the file (look there first); anything now at that name was left alone",
            path.display()
        ),
    };
    MetadataError::WriteError(format!(
        "{first} The temporary copy the save was made on {what}. The file itself was not changed."
    ))
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

/// Tests only: whether this environment lets a program write where
/// permissions say it may not — as the superuser does — shown by trying
/// it, in `dir`, on a file and in a folder made read-only for the purpose
/// (both removed again). A test that needs permissions to hold says so and
/// stops when they do not, rather than passing without testing anything.
#[cfg(all(test, unix))]
pub(crate) fn permissions_are_ignored_here(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let read_only = |path: &Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("mode");
    };
    let file = dir.join("read-only-file-probe");
    std::fs::write(&file, b"probe").expect("probe file");
    read_only(&file, 0o444);
    let file_ignored = OpenOptions::new().write(true).open(&file).is_ok();
    read_only(&file, 0o644);
    std::fs::remove_file(&file).expect("remove the probe file");
    let folder = dir.join("read-only-folder-probe");
    std::fs::create_dir(&folder).expect("probe folder");
    read_only(&folder, 0o555);
    let folder_ignored = File::create(folder.join("probe")).is_ok();
    read_only(&folder, 0o755);
    std::fs::remove_dir_all(&folder).expect("remove the probe folder");
    file_ignored || folder_ignored
}

/// Tests only: runs the test at `test_path` (`module_path!()` and the
/// test's name) again, in a child process whose file-creation mask is set
/// to 022 - the usual one, which takes away only the group's and others'
/// write permission - and returns `false`, after checking the child passed;
/// in that child it returns `true`, and the test goes on to do its checks.
///
/// Why (Codex's review of revision 11, finding 5): a test of the
/// permissions a new file gets proves nothing under a mask that takes more
/// away. Under 077 a copy made without its own owner-only permissions came
/// out owner-only all the same, so the test passed with the fix removed.
/// The child also proves its mask really is 022 (a new file comes out
/// 0644), and the parent that the child really ran the one test - so
/// neither can pass by doing nothing.
#[cfg(all(test, unix))]
pub(crate) fn rerun_with_the_usual_mask(test_path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "MEEDYA_TEST_USUAL_MASK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let dir = tempfile::tempdir().expect("tempdir");
        let probe = dir.path().join("mask-probe");
        File::create(&probe).expect("probe file");
        let mode = std::fs::metadata(&probe)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o644,
            "the child's file-creation mask is not 022"
        );
        return true;
    }
    // libtest names a test by its path inside the crate.
    let name = test_path
        .split_once("::")
        .map_or(test_path, |(_, inside)| inside);
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("umask 022 && exec \"$0\" --exact \"$1\" --nocapture --test-threads=1")
        .arg(std::env::current_exe().expect("this test program"))
        .arg(name)
        .env(CHILD, "1")
        .output()
        .expect("run the test again");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "{name}, run again with a mask of 022, failed:\n{said}"
    );
    assert!(
        said.contains("test result: ok. 1 passed"),
        "{name} did not run in the child:\n{said}"
    );
    false
}

// Tests only: helpers for the tests of who may read a copy (Codex's reviews of
// revisions 8-10, finding 1, and of revision 11, findings 1 and 5).

/// The permission bits of `path`, the program ones included.
#[cfg(all(test, unix))]
pub(crate) fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).expect("stat").permissions().mode() & 0o7777
}

/// The group `path` belongs to.
#[cfg(all(test, unix))]
pub(crate) fn gid_of(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).expect("stat").gid()
}

/// The groups this program's user is in (`id -G`).
#[cfg(all(test, unix))]
pub(crate) fn my_groups() -> Vec<u32> {
    let out = std::process::Command::new("id")
        .arg("-G")
        .output()
        .expect("run id -G");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(|gid| gid.parse().expect("a group number"))
        .collect()
}

/// Makes `folder` pass a reading entry on to every new file made in it:
/// "everyone may read" on macOS, "user 65534 may read" (a default list)
/// on Linux. `Err` says why it could not be done, for a test to print.
#[cfg(all(test, unix))]
pub(crate) fn pass_reading_on(folder: &Path) -> Result<(), String> {
    access_rules::add_entry(folder, "everyone allow read,file_inherit", "d:u:65534:r")
}

impl Drop for TempCopy {
    /// The last resort, for a copy neither put in place nor discarded — a
    /// save interrupted by a crash that still unwinds, say: deleted if it
    /// can be (only the copy itself: a file now at its name is left alone),
    /// silently, since there is no error left to say so in. Every ordinary
    /// way out goes through [`TempCopy::discard`] instead.
    fn drop(&mut self) {
        if !self.done {
            let _ = self.remove();
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
    fn a_copy_that_cannot_be_deleted_is_named_in_the_error() {
        // Codex's catch-up review of revisions 8-10, finding 7: the folder
        // made read-only after the copy was made, so the rename fails and
        // so does deleting the copy. That second failure used to be
        // ignored, leaving a hidden copy of the whole recording with
        // nothing to say so; now the error names it.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        if permissions_are_ignored_here(dir.path()) {
            eprintln!(
                "skipped: this environment ignores file permissions (running as the \
                 superuser?), so a folder cannot be made read-only to test this"
            );
            return;
        }
        let mode = |mode| std::fs::Permissions::from_mode(mode);
        let (path, mut original) = original_in(dir.path(), b"original");
        // Replacing: the rename fails, then deleting the copy does.
        let copy = TempCopy::of(&mut original).expect("copy");
        let copy_path = copy.path().to_path_buf();
        std::fs::set_permissions(dir.path(), mode(0o555)).expect("read-only folder");
        let replaced = copy.replace(original);
        std::fs::set_permissions(dir.path(), mode(0o755)).expect("writable again");
        let message = match replaced {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected the copy to be named, got {other:?}"),
        };
        assert!(message.contains("Permission denied"), "{message}");
        assert!(
            message.contains(&format!(
                "could not be deleted afterwards (Permission denied (os error 13)): it is {}",
                copy_path.display()
            )),
            "{message}"
        );
        assert!(copy_path.exists(), "the copy is where the error says");
        assert_eq!(std::fs::read(&path).expect("read"), b"original");
        // Discarding, as a refused save does: the same.
        let mut original = Original::open(&path).expect("open");
        let copy = TempCopy::of(&mut original).expect("copy");
        let other_path = copy.path().to_path_buf();
        std::fs::set_permissions(dir.path(), mode(0o555)).expect("read-only folder");
        let error = copy.discard(MetadataError::WriteError("refused.".to_string()));
        std::fs::set_permissions(dir.path(), mode(0o755)).expect("writable again");
        let message = error.to_string();
        assert!(
            message.contains("refused. The temporary copy the save was made on could not be"),
            "{message}"
        );
        assert!(
            message.contains(&other_path.display().to_string()),
            "{message}"
        );
        assert!(other_path.exists());
        // And when deleting works, the error is passed on as it was.
        let copy = TempCopy::of(&mut original).expect("copy");
        let third = copy.path().to_path_buf();
        match copy.discard(MetadataError::ReadError("as it was".to_string())) {
            MetadataError::ReadError(message) => assert_eq!(message, "as it was"),
            other => panic!("changed: {other:?}"),
        }
        assert!(!third.exists(), "deleted");
    }

    // The next two need permissions to hold. They used to look at the
    // refusal only IF there was one, so a guard that stopped refusing
    // passed them just the same (Codex's catch-up review of revisions
    // 8-10, finding 8 - shown with both guards broken: the read-only file
    // opened for writing, the copy made in another folder; both passed).
    // Now a missing refusal fails them, unless the environment is shown -
    // by trying - to ignore permissions, as the superuser does; then they
    // say so and stop.

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
        match result {
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("so the file's folder must be writable"),
                    "{message}"
                );
                assert!(message.contains("Nothing was written"), "{message}");
            }
            Ok(_) if permissions_are_ignored_here(dir.path()) => eprintln!(
                "skipped: this environment ignores file permissions (running as the \
                 superuser?), so a folder cannot be made read-only to test this"
            ),
            Ok(copy) => panic!(
                "a copy was made ({}) although the file's folder was read-only",
                copy.path().display()
            ),
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
        match Original::open(&path) {
            Err(error) => {
                let message = error.to_string();
                assert!(message.contains("read-only"), "{message}");
                assert!(message.contains("Nothing was written"), "{message}");
            }
            Ok(_) if permissions_are_ignored_here(dir.path()) => eprintln!(
                "skipped: this environment ignores file permissions (running as the \
                 superuser?), so a file cannot be made read-only to test this"
            ),
            Ok(_) => panic!("a read-only file was opened to be saved"),
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
        //
        // Codex's review of revision 11, finding 5: this test could pass
        // with that fix removed - under a creation mask of 077 a copy made
        // without its own owner-only permissions is owner-only anyway - and
        // it looked at the permission bits only, not at an access control
        // list. So it now runs in a child process with the usual mask (022)
        // set explicitly, and checks the list too - including in a folder
        // that passes a reading entry on to every new file (finding 1).
        if !rerun_with_the_usual_mask(concat!(
            module_path!(),
            "::the_copy_is_private_from_the_moment_it_exists"
        )) {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        for original_mode in [0o600, 0o644, 0o666] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("f.m4a");
            std::fs::write(&path, b"a private recording").expect("write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(original_mode))
                .expect("mode");
            let mut original = Original::open(&path).expect("open");
            let copy = TempCopy::of(&mut original).expect("copy");
            assert_eq!(
                mode_of(copy.path()),
                0o600,
                "the copy of a {original_mode:o} file"
            );
            assert_eq!(access_rules::entries_on(copy.path()), 0, "no access list");
            copy.replace(original).expect("replace");
            assert_eq!(mode_of(&path), original_mode, "after replacing");
        }
        // A folder that passes a reading entry on to every new file. The
        // copy is made there directly, past the check of the folder before
        // copying, so that the copy's own check is what is tested: on macOS
        // a copy that got the entry is refused, before a byte is written
        // into it; on Linux the entry is taken off first.
        let dir = tempfile::tempdir().expect("tempdir");
        let (_path, mut original) = original_in(dir.path(), b"a private recording");
        let passing = dir.path().join("passing");
        std::fs::create_dir(&passing).expect("folder");
        if let Err(why) = pass_reading_on(&passing) {
            eprintln!("skipped the folder that passes rules on: {why}");
            return;
        }
        let made = TempCopy::of_named(&mut original, &passing, [passing.join("copy.tmp")]);
        if cfg!(target_os = "macos") {
            match made {
                Err(error) => assert!(
                    error
                        .to_string()
                        .contains("was given access rules by its folder"),
                    "{error}"
                ),
                Ok(copy) => panic!(
                    "a copy with its folder's access list: {}",
                    access_rules::rules_of(copy.path())
                ),
            }
            assert_eq!(names_in(&passing), Vec::<String>::new(), "nothing left");
        } else {
            let copy = made.expect("a copy, its folder's list taken off");
            assert_eq!(mode_of(copy.path()), 0o600);
            assert_eq!(
                access_rules::entries_on(copy.path()),
                0,
                "{}",
                access_rules::rules_of(copy.path())
            );
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

    #[cfg(unix)]
    #[test]
    fn a_copy_is_counted_as_deleted_only_when_no_name_for_it_is_left() {
        // Codex's review of revision 11, finding 3. A copy moved away, or
        // given a second name, while the save is checked still exists, so a
        // refusal must say so; one deleted by someone else is gone, and
        // needs no word. The open handle tells them apart: the file's count
        // of names.
        let dir = tempfile::tempdir().expect("tempdir");
        let (_path, mut original) = original_in(dir.path(), b"original");
        let refusal =
            |copy: TempCopy| match copy.discard(MetadataError::WriteError("refused.".into())) {
                MetadataError::WriteError(message) => message,
                other => panic!("changed: {other:?}"),
            };
        // Moved away.
        let copy = TempCopy::of(&mut original).expect("copy");
        let made_as = copy.path().to_path_buf();
        let moved = dir.path().join("moved");
        std::fs::rename(&made_as, &moved).expect("move");
        let message = refusal(copy);
        assert!(
            message.starts_with("refused. The temporary copy"),
            "{message}"
        );
        assert!(
            message.contains("was moved, or given another name"),
            "{message}"
        );
        assert!(
            message.contains(&made_as.display().to_string()),
            "{message}"
        );
        assert_eq!(
            std::fs::read(&moved).expect("read"),
            b"original",
            "left where it went"
        );
        // Given a second name: its own name is deleted, the other is not.
        let copy = TempCopy::of(&mut original).expect("copy");
        let made_as = copy.path().to_path_buf();
        let second = dir.path().join("second-name");
        std::fs::hard_link(&made_as, &second).expect("second name");
        let message = refusal(copy);
        assert!(
            message.contains("was moved, or given another name"),
            "{message}"
        );
        assert!(!made_as.exists(), "its own name is deleted");
        assert!(second.exists(), "the other name is left alone");
        // Deleted by someone else: gone, so the error is passed on as it was.
        let copy = TempCopy::of(&mut original).expect("copy");
        std::fs::remove_file(copy.path()).expect("someone deletes it");
        assert_eq!(refusal(copy), "refused.");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_passes_access_rules_on_never_gets_a_copy_others_may_read() {
        // Codex's review of revision 11, finding 1 (reproduced before the
        // fix, on macOS: a private recording - 0600, no list of its own - in
        // a folder whose list passes "everyone may read" on to new files was
        // saved, and the copy, then the saved file, carried that entry).
        // macOS: refused before any copy is made. Linux: the copy's entry is
        // taken off, and the saved file has the original's list - none.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"a private recording");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("mode");
        if let Err(why) = pass_reading_on(dir.path()) {
            eprintln!("skipped: {why}");
            return;
        }
        let made = TempCopy::of(&mut original);
        if cfg!(target_os = "macos") {
            let message = match made {
                Err(error) => error.to_string(),
                Ok(copy) => panic!("a copy was made: {}", copy.path().display()),
            };
            assert!(
                message.contains("passes access rules on to every new file made in it"),
                "{message}"
            );
            assert!(message.contains("Nothing was written"), "{message}");
            assert_eq!(names_in(dir.path()), ["f.m4a"], "no copy was ever made");
        } else {
            let copy = made.expect("a copy");
            assert_eq!(access_rules::entries_on(copy.path()), 0);
            copy.replace(original).expect("replace");
        }
        assert_eq!(std::fs::read(&path).expect("read"), b"a private recording");
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(
            access_rules::entries_on(&path),
            0,
            "the file has its own list - none - not its folder's: {}",
            access_rules::rules_of(&path)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_s_own_access_rules_are_kept_or_the_save_is_refused() {
        // Codex's review of revision 11, finding 1: a file's own list - here
        // one entry, on macOS "user nobody may NOT read" (losing it would let
        // that account in), on Linux "user 65534 may read" - was not carried
        // over. Linux: the saved file has exactly the original's list.
        // macOS: refused, since lists are not written there; untouched.
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        if let Err(why) = access_rules::add_entry(&path, "user:nobody deny read", "u:65534:r") {
            eprintln!("skipped: {why}");
            return;
        }
        let (list, mode) = (access_rules::rules_of(&path), mode_of(&path));
        assert_eq!(access_rules::entries_on(&path), 1, "{list}");
        let made = TempCopy::of(&mut original);
        if cfg!(target_os = "macos") {
            let message = match made {
                Err(error) => error.to_string(),
                Ok(copy) => panic!("a copy was made: {}", copy.path().display()),
            };
            assert!(message.contains("has access rules of its own"), "{message}");
            assert_eq!(names_in(dir.path()), ["f.m4a"]);
            assert_eq!(std::fs::read(&path).expect("read"), b"original");
        } else {
            made.expect("a copy").replace(original).expect("replace");
        }
        assert_eq!(access_rules::rules_of(&path), list, "the same list");
        assert_eq!(mode_of(&path), mode);
    }

    #[cfg(unix)]
    #[test]
    fn a_group_the_copy_cannot_be_given_refuses_the_save_when_the_file_lets_its_group_in() {
        // Codex's review of revision 11, finding 1. The copy belongs to the
        // group its folder gives it; giving it the file's group can fail
        // (the program's user is not in that group). Then a file kept 0640
        // would have let the folder's group in instead: refused. A file
        // that lets its group in to nothing (0600) is saved, keeping the
        // copy's group, which it lets in to nothing.
        //
        // Set up on macOS only, where a new file takes its folder's group:
        // a folder in /private/tmp, which belongs to a group this user is
        // not in, gives the file that group; the folder is then moved to
        // one of the user's own groups, which the copy gets. (On Linux a
        // new file takes the program's own group, so such a file cannot be
        // made without the superuser.)
        use std::os::unix::fs::PermissionsExt;
        if !cfg!(target_os = "macos") {
            eprintln!(
                "skipped: a file in a group its owner is not in can be made without the superuser \
                 only where new files take their folder's group (macOS)"
            );
            return;
        }
        let shared = Path::new("/private/tmp");
        let foreign = gid_of(shared);
        let mine = my_groups();
        if mine.contains(&foreign) {
            eprintln!("skipped: this user is in /private/tmp's group, so it cannot be refused");
            return;
        }
        for (mode, refused) in [(0o640, true), (0o600, false)] {
            let dir = tempfile::Builder::new()
                .prefix("meedya-group-test")
                .tempdir_in(shared)
                .expect("tempdir");
            let path = dir.path().join("f.m4a");
            std::fs::write(&path, b"original").expect("write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("mode");
            assert_eq!(gid_of(&path), foreign, "the file takes the folder's group");
            std::os::unix::fs::chown(dir.path(), None, Some(mine[0])).expect("own group");
            let mut original = Original::open(&path).expect("open");
            let copy = TempCopy::of(&mut original).expect("copy");
            assert_eq!(
                gid_of(copy.path()),
                mine[0],
                "the copy takes the user's group"
            );
            let result = copy.replace(original);
            if refused {
                let message = result.expect_err("refused").to_string();
                assert!(
                    message.contains("could not be given this file's group"),
                    "{message}"
                );
                assert_eq!(gid_of(&path), foreign);
                assert_eq!(std::fs::read(&path).expect("read"), b"original");
                assert_eq!(names_in(dir.path()), ["f.m4a"], "the copy is deleted");
            } else {
                result.expect("saved");
                assert_eq!(
                    gid_of(&path),
                    mine[0],
                    "the copy's group, let in to nothing"
                );
                assert_eq!(mode_of(&path), 0o600);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_file_with_a_permission_for_programs_is_refused() {
        // Codex's review of revision 11, finding 1: the set-user-ID and
        // set-group-ID permissions (a program runs as the file's owner, or
        // group) were copied onto a new file owned by whoever saved it. A
        // media file should never carry them: refused - before a copy is
        // made, and at the rename if one was added while the save was
        // checked.
        use std::os::unix::fs::PermissionsExt;
        for bit in [0o4000, 0o2000] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("f.m4a");
            std::fs::write(&path, b"original").expect("write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644 | bit))
                .expect("mode");
            if mode_of(&path) & bit == 0 {
                eprintln!("skipped {bit:o}: this system took the permission off again");
                continue;
            }
            let mut original = Original::open(&path).expect("open");
            let message = match TempCopy::of(&mut original) {
                Err(error) => error.to_string(),
                Ok(copy) => panic!("a copy was made: {}", copy.path().display()),
            };
            assert!(
                message.contains("set-user-ID or set-group-ID permission"),
                "{message}"
            );
            assert_eq!(names_in(dir.path()), ["f.m4a"]);
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, mut original) = original_in(dir.path(), b"original");
        let copy = TempCopy::of(&mut original).expect("copy");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o4644)).expect("mode");
        let message = copy.replace(original).expect_err("refused").to_string();
        assert!(message.contains("set-user-ID"), "{message}");
        assert_eq!(std::fs::read(&path).expect("read"), b"original");
        assert_eq!(names_in(dir.path()), ["f.m4a"], "the copy is deleted");
    }
}
