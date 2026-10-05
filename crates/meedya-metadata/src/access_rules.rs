// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// A file's access rules beyond its permission bits, for a save by copy
// (issue #102; Codex's review of revision 11, finding 1).
// =====================================================================
//
// Why this exists. An M4A save is made on a temporary copy beside the file
// (`save_by_copy`), and revision 11 made that copy readable and writable by
// its owner only (0600) from the moment it exists. But a file's permission
// bits are not the whole of who may read it:
//
// - On macOS a folder can carry an access control list (ACL) whose entries
//   are passed on to every new file made in it, however private that file's
//   bits are - and macOS checks a list's entries before the bits. Reproduced
//   before this module: a private recording (0600, no list of its own) in a
//   folder whose list passes on "everyone may read" was saved, the save
//   said it succeeded, and the copy - while the save was checked, and then
//   for good, as the saved file - carried "everyone inherited allow read":
//   every account on the machine could read it.
// - On Linux a folder can carry a "default" list, which every new file gets
//   as its own. Made with mode 0600, a new file's list lets nobody else in
//   (the list's "mask", which limits every named entry, is cut to the group
//   bits, which are none), but giving the copy the original's bits just
//   before the rename widens the mask, and the entries the folder passed on
//   then apply.
// - And a file's own list - "bob may read", or "everyone may NOT read" - was
//   not carried over: the saved file had its folder's rules instead, wider
//   or narrower than the original's.
//
// What this module does, by system:
//
// - **Linux and Android**: the list is an extended attribute,
//   `system.posix_acl_access`, reached through the open file's handle (the
//   `rustix` wrapper). It is taken off the copy just after the copy is
//   made, before a byte is written into it, and the original's is put on
//   the copy just before the rename - each read back to prove it. A list of
//   the NFSv4 kind (`system.nfs4_acl`, on NFS shares) can be neither read
//   nor written here, so a file, or a copy, that has one is refused.
// - **macOS**: lists are reached only through the system's own `acl_*`
//   calls, which work on names, not on open files, and which this workspace
//   reaches only through a safe wrapper (`exacl`) - so here lists are only
//   READ, by name, each read checked before and after against the open file
//   the name should name. Nothing is ever written. The save is refused when
//   the folder passes any entry on to new files, when the original has a
//   list of its own (it could not be carried over), or when the copy turns
//   out to have one anyway. Ordinary folders - a home folder, Music,
//   Documents, Downloads, whose one entry ("everyone may not delete it") is
//   not passed on - are not affected, and neither is a disk that keeps no
//   such lists at all (a FAT-formatted drive, say).
// - **Windows**: nothing here. See `save_by_copy` for what a copy gets
//   there - its folder's rules - and what that means.
// - **Every other system** (iOS, iPadOS, the BSDs…): this program has no
//   way to read such a list there, so it cannot rule one out, and every M4A
//   save is refused, in plain words. (No current caller saves tags there:
//   the Swift bindings expose no saving.)
//
// The way chosen, and what was rejected (revision 12). The workspace keeps
// `unsafe` code out, and reaches system calls only through small, safe,
// permissively licensed wrappers (`winapi-util` was the first). `rustix`
// was already in the lockfile and reaches Linux's lists through the open
// handle. macOS hides its lists from the extended-attribute calls (reading
// `com.apple.system.Security` is refused: tried), so only `exacl` - new to
// the lockfile, MIT - reaches them, and only by name. Rejected: writing
// lists by name on macOS (a name swapped at the wrong moment would have the
// copy's list written onto someone else's file); going through `/dev/fd/N`
// to reach the open file (tried: writing a list that way reaches the open
// file, but reading one shows nothing, so a read-back would always say
// "none" - and none of it is documented); and calling the system's `acl_*`
// functions directly, which would bring `unsafe` code into this workspace.
//
// What it CANNOT do:
//
// - On macOS, rule out a swap at the very instant of a read by name: each
//   read is checked before and after, so such a swap would have to be made
//   and undone between the two checks.
// - On macOS, carry a file's list over, or make a copy private when its
//   folder passes entries on: both are refused instead.
// - See other access rules kept as extended attributes (a Linux security
//   module's label, such as SELinux's): a save by copy does not keep
//   extended attributes at all (see `save_by_copy`).

use std::fs::File;
use std::path::Path;

/// Says whether a name still names the open file it should: checked before
/// and after every read by name (macOS), so a list read is never one of a
/// file swapped in at that name.
pub(crate) type StillTheFile<'a> = &'a dyn Fn() -> bool;

/// Before the copy is made: refuses - as one plain sentence, without the
/// closing "Nothing was written" - what this program could not keep private
/// or carry over: on macOS a folder that passes entries on to new files, or
/// an original with a list of its own; on Linux an original with an NFSv4
/// list; on any system with no way to read lists, everything (see the top
/// of this file).
pub(crate) fn check_before_copying(
    original: &File,
    original_path: &Path,
    still_the_original: StillTheFile<'_>,
    folder: &Path,
) -> Result<(), String> {
    system::check_before_copying(original, original_path, still_the_original, folder)
}

/// Just after the copy is made, before a byte is written into it: takes off
/// every rule it got from its folder (Linux), or refuses if it has any
/// (macOS) - so that from here on its permission bits alone decide who may
/// read it.
pub(crate) fn make_private(
    copy: &File,
    copy_path: &Path,
    still_the_copy: StillTheFile<'_>,
) -> Result<(), String> {
    system::make_private(copy, copy_path, still_the_copy)
}

/// Just before the rename: gives the copy the original's own list and
/// reads it back (Linux), or refuses if the original now has one (macOS).
pub(crate) fn carry_over(
    original: &File,
    original_path: &Path,
    still_the_original: StillTheFile<'_>,
    copy: &File,
) -> Result<(), String> {
    system::carry_over(original, original_path, still_the_original, copy)
}

/// "1 entry", "2 entries".
#[cfg(any(target_os = "macos", test))]
fn entries(count: usize) -> String {
    if count == 1 {
        "1 entry".to_string()
    } else {
        format!("{count} entries")
    }
}

// ============================================================
// Linux and Android: the list is an extended attribute
// ============================================================

#[cfg(any(target_os = "linux", target_os = "android"))]
mod system {
    use super::{File, Path, StillTheFile};
    use rustix::fs::{fgetxattr, fremovexattr, fsetxattr, XattrFlags};
    use rustix::io::Errno;

    /// Where a file's own access control list is kept.
    pub(super) const POSIX_ACL: &str = "system.posix_acl_access";

    /// Where an NFSv4 list is shown, on an NFS share.
    const NFS4_ACL: &str = "system.nfs4_acl";

    /// The largest list read: far more than any real one (each entry is
    /// eight bytes, after a four-byte header).
    const LARGEST: usize = 64 * 1024;

    /// "There is none": the file has no such attribute, or its filesystem
    /// keeps none at all (EOPNOTSUPP, which is also ENOTSUP here).
    fn absent(e: Errno) -> bool {
        e == Errno::NODATA || e == Errno::OPNOTSUPP
    }

    /// The extended attribute `name` of the open `file`, or `None` when it
    /// has none.
    pub(super) fn attribute(file: &File, name: &str) -> Result<Option<Vec<u8>>, String> {
        let mut value = vec![0u8; LARGEST];
        match fgetxattr(file, name, &mut value[..]) {
            Ok(len) => {
                value.truncate(len);
                Ok(Some(value))
            }
            Err(e) if absent(e) => Ok(None),
            Err(e) => Err(format!(
                "this program could not read the access rules of a file it was saving ({name}: \
                 {e}), so it could not make sure the saved file is no more readable than the file"
            )),
        }
    }

    fn nfs4(what: &str) -> String {
        format!(
            "{what} has access rules of the NFSv4 kind (as on some network shares), which this \
             program can neither read nor carry over, so it cannot make sure the temporary copy \
             an M4A save is made on is private, or that the saved file keeps the file's own rules"
        )
    }

    pub(super) fn check_before_copying(
        original: &File,
        _original_path: &Path,
        _still_the_original: StillTheFile<'_>,
        _folder: &Path,
    ) -> Result<(), String> {
        match attribute(original, NFS4_ACL)? {
            Some(_) => Err(nfs4("this file")),
            None => Ok(()),
        }
    }

    pub(super) fn make_private(
        copy: &File,
        _copy_path: &Path,
        _still_the_copy: StillTheFile<'_>,
    ) -> Result<(), String> {
        match fremovexattr(copy, POSIX_ACL) {
            Ok(()) => {}
            Err(e) if absent(e) => {}
            Err(e) => {
                return Err(format!(
                    "the access rules the temporary copy an M4A save is made on got from its \
                     folder (a default access control list) could not be taken off it ({e}), so \
                     the copy could be readable by others"
                ))
            }
        }
        if attribute(copy, POSIX_ACL)?.is_some() {
            return Err(
                "the temporary copy an M4A save is made on still had access rules after they were \
                 taken off, so it could be readable by others"
                    .to_string(),
            );
        }
        match attribute(copy, NFS4_ACL)? {
            Some(_) => Err(nfs4("the temporary copy an M4A save is made on")),
            None => Ok(()),
        }
    }

    pub(super) fn carry_over(
        original: &File,
        _original_path: &Path,
        _still_the_original: StillTheFile<'_>,
        copy: &File,
    ) -> Result<(), String> {
        if attribute(original, NFS4_ACL)?.is_some() {
            return Err(nfs4("this file"));
        }
        let Some(list) = attribute(original, POSIX_ACL)? else {
            return Ok(());
        };
        fsetxattr(copy, POSIX_ACL, &list, XattrFlags::empty()).map_err(|e| {
            format!(
                "this file's own access rules (an access control list) could not be put on the \
                 saved copy ({e}), so the saved file would have lost them"
            )
        })?;
        if attribute(copy, POSIX_ACL)?.as_deref() != Some(&list[..]) {
            return Err(
                "this file's own access rules (an access control list) did not read back the same \
                 from the saved copy, so the saved file would not have kept them"
                    .to_string(),
            );
        }
        Ok(())
    }
}

// ============================================================
// macOS: lists are only read, by name
// ============================================================

#[cfg(target_os = "macos")]
mod system {
    use super::{entries, File, Path, StillTheFile};
    use exacl::{getfacl, AclEntry, AclOption, Flag};

    /// The entries of the list of whatever `path` names (a link itself, not
    /// what it points to), read by name - checked before and after against
    /// the open file `path` should name (`still`). A disk that keeps no such
    /// lists at all has none.
    fn entries_of(
        path: &Path,
        still: StillTheFile<'_>,
        what: &str,
    ) -> Result<Vec<AclEntry>, String> {
        let replaced = || {
            format!(
                "{what} ({}) was replaced by another file while its access rules were being read",
                path.display()
            )
        };
        if !still() {
            return Err(replaced());
        }
        let read = getfacl(path, AclOption::SYMLINK_ACL);
        if !still() {
            return Err(replaced());
        }
        read.or_else(|e| none_if_unsupported(e, what))
    }

    /// No list at all when the disk keeps none; any other failure refuses.
    fn none_if_unsupported(e: std::io::Error, what: &str) -> Result<Vec<AclEntry>, String> {
        if e.kind() == std::io::ErrorKind::Unsupported {
            Ok(Vec::new())
        } else {
            Err(format!(
                "the access rules of {what} could not be read ({e}), so this program could not \
                 make sure the saved file is no more readable than the file"
            ))
        }
    }

    pub(super) fn check_before_copying(
        _original: &File,
        original_path: &Path,
        still_the_original: StillTheFile<'_>,
        folder: &Path,
    ) -> Result<(), String> {
        let passed_on = getfacl(folder, None)
            .or_else(|e| none_if_unsupported(e, "this file's folder"))?
            .iter()
            .filter(|entry| entry.flags.contains(Flag::FILE_INHERIT))
            .count();
        if passed_on > 0 {
            return Err(format!(
                "this file's folder passes access rules on to every new file made in it (its \
                 access control list has {} marked to be inherited by files), which the \
                 temporary copy an M4A save is made on would get - so the copy, and then the \
                 saved file, could be readable by others however private the file is - and this \
                 program cannot take them off on macOS (to save here, remove those entries from \
                 the folder: `chmod -N` on the folder removes its whole list)",
                entries(passed_on)
            ));
        }
        refuse_own_list(original_path, still_the_original)
    }

    /// Refuses an original with a list of its own: it cannot be carried
    /// over on macOS.
    fn refuse_own_list(path: &Path, still: StillTheFile<'_>) -> Result<(), String> {
        let own = entries_of(path, still, "the file")?;
        if own.is_empty() {
            return Ok(());
        }
        Err(format!(
            "this file has access rules of its own (an access control list with {}), which a \
             save by copy cannot carry over on macOS - the saved file would lose them, so others \
             could gain or lose access to it (to save it, remove them: `chmod -N` on the file)",
            entries(own.len())
        ))
    }

    pub(super) fn make_private(
        _copy: &File,
        copy_path: &Path,
        still_the_copy: StillTheFile<'_>,
    ) -> Result<(), String> {
        let got = entries_of(
            copy_path,
            still_the_copy,
            "the temporary copy an M4A save is made on",
        )?;
        if got.is_empty() {
            return Ok(());
        }
        Err(format!(
            "the temporary copy an M4A save is made on was given access rules by its folder (an \
             access control list with {}), which this program cannot take off on macOS, so the \
             copy - and then the saved file - could be readable by others",
            entries(got.len())
        ))
    }

    pub(super) fn carry_over(
        _original: &File,
        original_path: &Path,
        still_the_original: StillTheFile<'_>,
        _copy: &File,
    ) -> Result<(), String> {
        refuse_own_list(original_path, still_the_original)
    }
}

// ============================================================
// Windows: nothing here (see `save_by_copy`)
// ============================================================

#[cfg(windows)]
mod system {
    use super::{File, Path, StillTheFile};

    pub(super) fn check_before_copying(
        _original: &File,
        _original_path: &Path,
        _still_the_original: StillTheFile<'_>,
        _folder: &Path,
    ) -> Result<(), String> {
        Ok(())
    }

    pub(super) fn make_private(
        _copy: &File,
        _copy_path: &Path,
        _still_the_copy: StillTheFile<'_>,
    ) -> Result<(), String> {
        Ok(())
    }

    pub(super) fn carry_over(
        _original: &File,
        _original_path: &Path,
        _still_the_original: StillTheFile<'_>,
        _copy: &File,
    ) -> Result<(), String> {
        Ok(())
    }
}

// ============================================================
// Any other system: no way to read a list, so nothing is saved
// ============================================================

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
)))]
mod system {
    use super::{File, Path, StillTheFile};

    const CANNOT: &str = "on this system this program has no way to read access control lists, \
                          so it cannot make sure the temporary copy an M4A save is made on is \
                          private, or that the saved file keeps the file's own access rules - \
                          so M4A files are not saved here";

    pub(super) fn check_before_copying(
        _original: &File,
        _original_path: &Path,
        _still_the_original: StillTheFile<'_>,
        _folder: &Path,
    ) -> Result<(), String> {
        Err(CANNOT.to_string())
    }

    pub(super) fn make_private(
        _copy: &File,
        _copy_path: &Path,
        _still_the_copy: StillTheFile<'_>,
    ) -> Result<(), String> {
        Err(CANNOT.to_string())
    }

    pub(super) fn carry_over(
        _original: &File,
        _original_path: &Path,
        _still_the_original: StillTheFile<'_>,
        _copy: &File,
    ) -> Result<(), String> {
        Err(CANNOT.to_string())
    }
}

// ============================================================
// Tests only: what a test needs to look at and set these lists
// ============================================================

/// Tests only: the number of entries in the access control list of the
/// file at `path` (0 when it has none, or its disk keeps none) - what a
/// test checks to see who besides the permission bits may read a file.
#[cfg(all(test, unix))]
pub(crate) fn entries_on(path: &Path) -> usize {
    #[cfg(target_os = "macos")]
    {
        exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)
            .map(|list| list.len())
            .unwrap_or_else(|e| {
                assert_eq!(e.kind(), std::io::ErrorKind::Unsupported, "{e}");
                0
            })
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // The list's own four entries (owner, group, mask, others) mirror
        // the permission bits; only a list with more says more than they do.
        let file = File::open(path).expect("open");
        system::attribute(&file, system::POSIX_ACL)
            .expect("read")
            .map_or(0, |list| {
                (list.len().saturating_sub(4) / 8).saturating_sub(4)
            })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        let _ = path;
        0
    }
}

/// Tests only: the whole access control list of the file at `path`, as
/// text - empty when it has none - so a test can compare two files' lists.
#[cfg(all(test, unix))]
pub(crate) fn rules_of(path: &Path) -> String {
    #[cfg(target_os = "macos")]
    {
        let list = exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL).unwrap_or_default();
        exacl::to_string(&list).expect("text")
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let file = File::open(path).expect("open");
        system::attribute(&file, system::POSIX_ACL)
            .expect("read")
            .map_or_else(String::new, |list| format!("{list:?}"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        let _ = path;
        String::new()
    }
}

/// Tests only: adds `entry` to the access control list of the file or
/// folder at `path`, through the system's own tool - `chmod +a` on macOS,
/// `setfacl -m` on Linux - in a test's own folder. Returns why not when the
/// tool is missing, or the disk keeps no such lists, so the test can say so
/// and stop.
#[cfg(all(test, unix))]
pub(crate) fn add_entry(path: &Path, macos: &str, linux: &str) -> Result<(), String> {
    let (tool, args): (&str, Vec<&std::ffi::OsStr>) = if cfg!(target_os = "macos") {
        (
            "/bin/chmod",
            vec!["+a".as_ref(), macos.as_ref(), path.as_os_str()],
        )
    } else {
        (
            "setfacl",
            vec!["-m".as_ref(), linux.as_ref(), path.as_os_str()],
        )
    };
    match std::process::Command::new(tool).args(&args).output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(format!(
            "{tool} could not add the entry: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Err(format!("{tool} could not be run: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_counted_in_plain_words() {
        assert_eq!(entries(1), "1 entry");
        assert_eq!(entries(3), "3 entries");
    }
}
