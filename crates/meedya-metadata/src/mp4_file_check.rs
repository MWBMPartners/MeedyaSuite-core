// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Checking an M4A save across the WHOLE file, not just its tags (#102).
// ====================================================================
//
// Why this exists (the stand-in review of revision 9, findings H1 and M1).
// Revision 9 compared the saved copy's tags — the `ilst` atoms, in
// `mp4_save_check` — with the original's, and left everything else to
// lofty, on the belief that lofty corrects every sample offset when the
// tags grow or shrink. The review showed, on real files made by ffmpeg,
// that a save can damage what lies OUTSIDE the tags, where that check could
// not see it:
//
// - A FRAGMENTED file — the way streaming tools write M4A: a `moov` with no
//   samples in it (it holds `mvex` instead), then pieces, each a `moof`
//   saying where its audio starts and an `mdat` holding it. A save that
//   changes the size of the tags moves every piece, and lofty did not
//   correct where each piece says its audio starts: after a title-only
//   write, and after a long comment, ffmpeg could no longer decode the
//   audio (hundreds of errors), while the tag check passed.
// - A `meta` box holding its handler (`hdlr`, which tells other programs
//   what kind of tags follow) but NO tag list (`ilst`) — what a tag
//   removal through lofty itself leaves. lofty's save then writes the new
//   tag list OVER the handler (lofty 0.22.4, `mp4/ilst/write.rs`: it takes
//   the box's first part for the tag list when it finds none), and after
//   that neither ffprobe nor Apple's own media framework (AVFoundation)
//   reads the file's tags at all. The tag check passed that too.
//
// So the principle of `mp4_save_check` — verify the result, never list the
// ways it can go wrong — now covers the whole saved file. The copy replaces
// the original only when this module proves ALL of these:
//
// (a) every `mdat` (the audio and other sample data) holds, byte for byte,
//     what it held before — compared by reading both files in blocks,
//     never by loading a whole file into memory;
// (b) every chunk offset in every `stco` or `co64` table (where each piece
//     of a track's samples starts in the file) equals its old value plus
//     exactly how far the `mdat` it points into moved — and one that points
//     into no `mdat` at all cannot be checked, so it refuses the save;
// (c) everything else inside `moov` is byte for byte as it was, EXCEPT:
//     the contents of `ilst` (compared by `mp4_save_check`), the size
//     fields of the atoms on the path `moov` → `udta` → `meta` → `ilst`
//     (they grow and shrink with the tags), and `free` / `skip` padding
//     inside `udta` and `meta` (lofty uses and makes padding there);
// (d) every atom at the top of the file other than `moov`, `mdat`, `free`
//     and `skip` is byte for byte as it was, and they are all in the same
//     order as before.
//
// One thing is allowed that none of those names: when the file has no
// `udta` (or its `udta` has no `meta`), lofty makes one to hold the new
// tags, with the standard iTunes handler inside — adding tags to a file
// with none needs that box, and nothing is lost. What lofty makes is
// checked to be exactly that, byte for byte (see `LOFTY_HDLR`).
//
// And two kinds of file are refused BEFORE anything is written, with a
// plain message, because it is known that the save would damage them: a
// fragmented file (any `moof`, `mfra` or `sidx` atom at the top, or `mvex`
// in `moov`), and a `meta` that has parts but no `ilst`.
//
// The comparison refuses a lost handler by itself, through (c), only later
// and less helpfully. It would NOT catch a fragmented file through (a) to
// (d): each piece says where its audio starts as a position in the whole
// file, inside its `moof`, and the damage is that lofty moved the pieces
// and left those positions as they were — so every `moof` is byte for byte
// the same, the audio is the same, and the file has no chunk offset table
// to check. (Measured: with the check before saving removed, a fragmented
// file's save passed (a) to (d) and broke the audio.) So the comparison
// also refuses any fragmented file outright, whatever the check before
// saving did.
//
// What it does NOT do: judge whether the ORIGINAL is a sound file. It
// proves that the save changed nothing it was not asked to change; a file
// that was already damaged is saved as damaged as it was.

use std::io::{BufReader, Read, Seek, SeekFrom};

use crate::error::MetadataError;
use crate::mp4_save_check::{boxes_in_file, first_moov, meta_version_len, unreadable, BoxAt};

/// The handler atom lofty writes into a `meta` box it makes itself (lofty
/// 0.22.4, `mp4/ilst/write.rs`, `create_meta`): 33 bytes — size, `hdlr`,
/// version and flags, a zero "pre-defined" field, the handler type `mdir`
/// and `appl` (the iTunes metadata handler), and nine zero bytes.
const LOFTY_HDLR: [u8; 33] = [
    0, 0, 0, 33, b'h', b'd', b'l', b'r', 0, 0, 0, 0, 0, 0, 0, 0, b'm', b'd', b'i', b'r', b'a',
    b'p', b'p', b'l', 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// How much of each file is read at a time when two stretches of bytes
/// are compared.
const BLOCK: usize = 64 * 1024;

// ============================================================
// Refusing before anything is written
// ============================================================

/// Refuses, with a plain [`MetadataError::WriteError`], an M4A file whose
/// save is known to damage it (see the top of this file): a fragmented
/// file, or a file whose `meta` box — the one lofty writes into, the first
/// in the first `udta` — has parts but no tag list. Anything else passes.
/// Fails with the "cannot be checked" refusal when the file's atoms cannot
/// be read safely.
pub(crate) fn refuse_what_a_save_would_damage(
    file: &mut (impl Read + Seek),
) -> Result<(), MetadataError> {
    let mut reader = BufReader::new(file);
    let file_len = reader.seek(SeekFrom::End(0))?;
    let top = boxes_in_file(&mut reader, 0, file_len).map_err(unreadable)?;
    let fragmented = |name: &[u8; 4]| {
        MetadataError::WriteError(format!(
            "this M4A file is fragmented (stored in pieces, as streaming tools write it - it has \
             a `{}` atom), and its tags cannot be saved safely: saving moves every piece, and the \
             library this crate saves M4A files with does not correct where each piece says its \
             audio starts, which breaks the audio. Nothing was written. (Issue #102.)",
            String::from_utf8_lossy(name)
        ))
    };
    if let Some(atom) = top
        .iter()
        .find(|atom| matches!(&atom.name, b"moof" | b"mfra" | b"sidx"))
    {
        return Err(fragmented(&atom.name));
    }
    let Some(moov) = first_moov(&mut reader, file_len).map_err(unreadable)? else {
        return Ok(());
    };
    let moov_children =
        boxes_in_file(&mut reader, moov.body_start, moov.end).map_err(unreadable)?;
    if moov_children.iter().any(|atom| &atom.name == b"mvex") {
        return Err(fragmented(b"mvex"));
    }
    let Some(udta) = moov_children.iter().find(|atom| &atom.name == b"udta") else {
        return Ok(());
    };
    let udta_children =
        boxes_in_file(&mut reader, udta.body_start, udta.end).map_err(unreadable)?;
    let Some(meta) = udta_children.iter().find(|atom| &atom.name == b"meta") else {
        return Ok(());
    };
    let body_start = meta.body_start + meta_version_len(&mut reader, meta).map_err(unreadable)?;
    let parts = boxes_in_file(&mut reader, body_start, meta.end).map_err(unreadable)?;
    let has_a_part = parts.iter().any(|part| !is_padding(&part.name));
    let has_ilst = parts.iter().any(|part| &part.name == b"ilst");
    if has_a_part && !has_ilst {
        return Err(MetadataError::WriteError(format!(
            "this M4A file's metadata box has no tag list, and saving would damage it: the \
             library this crate saves M4A files with would write the new tag list over the box's \
             first part ({}), after which other programs - ffprobe, and Apple's own - no longer \
             read the file's tags. Nothing was written. (A file left like this when all its tags \
             were removed can be given an empty tag list by another tool first: mutagen's \
             `clear()` and `save()` does. Issue #102.)",
            names(&parts)
        )));
    }
    Ok(())
}

// ============================================================
// Comparing the whole file
// ============================================================

/// Every way the `saved` M4A file differs from the `original` OUTSIDE its
/// tag list, beyond what a tag save may change — the checks (a) to (d) at
/// the top of this file. Each is one plain sentence; none means the save
/// changed nothing else. Both files are read from their start, in blocks.
///
/// Fails with the "cannot be checked" refusal when either file's atoms
/// cannot be read safely.
pub(crate) fn differences_outside_the_tags(
    original: &mut (impl Read + Seek),
    saved: &mut (impl Read + Seek),
) -> Result<Vec<String>, MetadataError> {
    let len_a = original.seek(SeekFrom::End(0))?;
    let len_b = saved.seek(SeekFrom::End(0))?;
    let mut walk = Walk {
        a: BufReader::with_capacity(BLOCK, original),
        b: BufReader::with_capacity(BLOCK, saved),
        len_a,
        len_b,
        mdats: Vec::new(),
        problems: Vec::new(),
    };
    walk.whole_file().map_err(|why| match why {
        Stop::Unreadable(why) => unreadable(why),
        Stop::Io(e) => MetadataError::IoError(e),
    })?;
    Ok(walk.problems)
}

/// Why the comparison stopped early.
enum Stop {
    /// A file's atoms cannot be read safely (said in plain words).
    Unreadable(String),
    /// A file could not be read at all.
    Io(std::io::Error),
}

impl From<std::io::Error> for Stop {
    fn from(e: std::io::Error) -> Self {
        Stop::Io(e)
    }
}

impl From<String> for Stop {
    fn from(why: String) -> Self {
        Stop::Unreadable(why)
    }
}

/// `free` and `skip`: padding, holding nothing.
fn is_padding(name: &[u8; 4]) -> bool {
    matches!(name, b"free" | b"skip")
}

/// An atom's name as a person reads it (`©` for byte 0xA9).
fn name_of(name: &[u8; 4]) -> String {
    name.iter().map(|byte| char::from(*byte)).collect()
}

/// A list of atoms' names, in brackets: `[hdlr, ilst, free]`.
fn names(atoms: &[BoxAt]) -> String {
    let listed: Vec<String> = atoms.iter().map(|atom| name_of(&atom.name)).collect();
    format!("[{}]", listed.join(", "))
}

/// Where an atom sits, as a person reads it: `moov → trak 2 → mdia`. An
/// atom is numbered when its parent holds more than one of that name.
fn path_to(parent: &str, atom: &BoxAt, siblings: &[BoxAt]) -> String {
    let same: Vec<&BoxAt> = siblings.iter().filter(|s| s.name == atom.name).collect();
    let mut name = name_of(&atom.name);
    if same.len() > 1 {
        let index = same.iter().position(|s| s.start == atom.start).unwrap_or(0);
        name.push_str(&format!(" {}", index + 1));
    }
    if parent.is_empty() {
        name
    } else {
        format!("{parent} → {name}")
    }
}

/// How an atom's own header is written: 8 bytes, or 16 with a 64-bit size.
fn header_len(atom: &BoxAt) -> u64 {
    atom.body_start - atom.start
}

/// Where one `mdat`'s contents sit in the original, and how far they moved
/// in the saved copy.
struct MovedData {
    start: u64,
    end: u64,
    moved_by: i128,
}

/// The two files being compared, and what has been found.
struct Walk<A: Read + Seek, B: Read + Seek> {
    a: BufReader<A>,
    b: BufReader<B>,
    /// How long each file is: no stretch past its end is ever read.
    len_a: u64,
    len_b: u64,
    mdats: Vec<MovedData>,
    problems: Vec<String>,
}

impl<A: Read + Seek, B: Read + Seek> Walk<A, B> {
    /// The atoms between `start` and `end` of each file — the parts of
    /// `whose` (a path such as `moov → udta`, or empty for the top of the
    /// file). Any bytes after the last of them (fewer than 8, too few for
    /// an atom) are compared as they are.
    fn parts(
        &mut self,
        whose: &str,
        a: (u64, u64),
        b: (u64, u64),
    ) -> Result<(Vec<BoxAt>, Vec<BoxAt>), Stop> {
        let in_a = boxes_in_file(&mut self.a, a.0, a.1)?;
        let in_b = boxes_in_file(&mut self.b, b.0, b.1)?;
        let tail_a = in_a.last().map_or(a.0, |atom| atom.end);
        let tail_b = in_b.last().map_or(b.0, |atom| atom.end);
        if !self.same_bytes((tail_a, a.1), (tail_b, b.1))? {
            let whose = if whose.is_empty() {
                "the file".to_string()
            } else {
                whose.to_string()
            };
            self.problems.push(format!(
                "the bytes after the last atom in {whose} would change"
            ));
        }
        Ok((in_a, in_b))
    }

    /// Whether the stretch `a` of the original and `b` of the copy hold the
    /// same bytes, read in blocks.
    ///
    /// A stretch that ends before it starts, or runs past the end of its
    /// file, refuses the save as "cannot be checked" BEFORE its ends are
    /// subtracted: the subtraction would otherwise stop the program (in a
    /// build that checks for overflow) or wrap round to an enormous length
    /// (in one that does not). Nothing well formed makes such a stretch;
    /// a `meta` too short for its version did (Codex's catch-up review of
    /// revisions 8-10, finding 2), and this keeps any other way of making
    /// one from doing the same.
    fn same_bytes(&mut self, a: (u64, u64), b: (u64, u64)) -> Result<bool, Stop> {
        if a.0 > a.1 || b.0 > b.1 || a.1 > self.len_a || b.1 > self.len_b {
            return Err(Stop::Unreadable(format!(
                "a part of the file to be compared does not lie inside it (bytes {} to {} of the \
                 original, {} to {} of the saved copy)",
                a.0, a.1, b.0, b.1
            )));
        }
        if a.1 - a.0 != b.1 - b.0 {
            return Ok(false);
        }
        self.a.seek(SeekFrom::Start(a.0))?;
        self.b.seek(SeekFrom::Start(b.0))?;
        let (mut block_a, mut block_b) = (vec![0u8; BLOCK], vec![0u8; BLOCK]);
        let mut left = a.1 - a.0;
        while left > 0 {
            let n = usize::try_from(left).map_or(BLOCK, |left| left.min(BLOCK));
            self.a.read_exact(&mut block_a[..n])?;
            self.b.read_exact(&mut block_b[..n])?;
            if block_a[..n] != block_b[..n] {
                return Ok(false);
            }
            left -= n as u64;
        }
        Ok(true)
    }

    /// Whether atom `x` of the original and `y` of the copy are the same,
    /// byte for byte, header included.
    fn same_atom(&mut self, x: &BoxAt, y: &BoxAt) -> Result<bool, Stop> {
        self.same_bytes((x.start, x.end), (y.start, y.end))
    }

    /// Checks (a), (b), (c) and (d): the top of the file.
    fn whole_file(&mut self) -> Result<(), Stop> {
        let len_a = self.a.seek(SeekFrom::End(0))?;
        let len_b = self.b.seek(SeekFrom::End(0))?;
        let (top_a, top_b) = self.parts("", (0, len_a), (0, len_b))?;
        let top_a: Vec<BoxAt> = top_a.into_iter().filter(|x| !is_padding(&x.name)).collect();
        let top_b: Vec<BoxAt> = top_b.into_iter().filter(|x| !is_padding(&x.name)).collect();
        if top_a
            .iter()
            .map(|x| x.name)
            .ne(top_b.iter().map(|y| y.name))
        {
            self.problems.push(format!(
                "the atoms at the top of the file would change: now {}; after saving, {}",
                names(&top_a),
                names(&top_b)
            ));
            return Ok(());
        }
        // A fragmented file: never passed (see the top of this file).
        if let Some(piece) = top_a
            .iter()
            .find(|atom| matches!(&atom.name, b"moof" | b"mfra" | b"sidx"))
        {
            self.problems.push(format!(
                "the file is fragmented (it has a `{}` atom), and where each piece says its \
                 audio starts is not checked, so the save cannot be shown to be safe",
                name_of(&piece.name)
            ));
            return Ok(());
        }
        // (a) The sample data first, which also tells (b) how far each
        // `mdat` moved.
        let mut mdat_number = 0;
        for (x, y) in top_a.iter().zip(&top_b) {
            if &x.name != b"mdat" {
                continue;
            }
            mdat_number += 1;
            if !self.same_bytes((x.body_start, x.end), (y.body_start, y.end))? {
                self.problems.push(format!(
                    "the audio and other sample data (mdat {mdat_number} at the top of the \
                     file) would change"
                ));
            }
            self.mdats.push(MovedData {
                start: x.body_start,
                end: x.end,
                moved_by: i128::from(y.body_start) - i128::from(x.body_start),
            });
        }
        // (c) and (d): lofty reads and writes the FIRST `moov` only; any
        // other is compared byte for byte, like every other atom here.
        let mut first_moov = true;
        for (x, y) in top_a.iter().zip(&top_b) {
            match &x.name {
                b"mdat" => {}
                b"moov" if first_moov => {
                    first_moov = false;
                    self.moov(x, y)?;
                }
                _ => {
                    if !self.same_atom(x, y)? {
                        self.problems.push(format!(
                            "the atom {} at the top of the file would change",
                            path_to("", x, &top_a)
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Check (c) for `moov`: its size may change, and a `udta` lofty made
    /// may have been added at its start; everything else is compared.
    fn moov(&mut self, x: &BoxAt, y: &BoxAt) -> Result<(), Stop> {
        if header_len(x) != header_len(y) {
            self.problems
                .push("the header of moov would change form".to_string());
        }
        let (in_a, mut in_b) = self.parts("moov", (x.body_start, x.end), (y.body_start, y.end))?;
        if in_a.iter().any(|atom| &atom.name == b"mvex") {
            self.problems.push(
                "the file is fragmented (its moov has an `mvex` atom), and where each piece says \
                 its audio starts is not checked, so the save cannot be shown to be safe"
                    .to_string(),
            );
            return Ok(());
        }
        if !in_a.iter().any(|atom| &atom.name == b"udta")
            && in_b.len() == in_a.len() + 1
            && &in_b[0].name == b"udta"
        {
            let made = in_b.remove(0);
            self.made_udta(&made)?;
        }
        self.pairs("moov", &in_a, &in_b, |walk, path, x, y| match &x.name {
            b"udta" => walk.udta(path, x, y),
            b"trak" => walk.down_to_offsets(path, x, y),
            _ => walk.same_or_note(path, x, y),
        })
    }

    /// Compares `in_a` and `in_b` pair by pair with `each`, once they are
    /// shown to hold the same names in the same order.
    fn pairs(
        &mut self,
        parent: &str,
        in_a: &[BoxAt],
        in_b: &[BoxAt],
        mut each: impl FnMut(&mut Self, &str, &BoxAt, &BoxAt) -> Result<(), Stop>,
    ) -> Result<(), Stop> {
        if in_a.iter().map(|x| x.name).ne(in_b.iter().map(|y| y.name)) {
            self.problems.push(format!(
                "the parts of {parent} would change: now {}; after saving, {}",
                names(in_a),
                names(in_b)
            ));
            return Ok(());
        }
        for (x, y) in in_a.iter().zip(in_b) {
            let path = path_to(parent, x, in_a);
            each(self, &path, x, y)?;
        }
        Ok(())
    }

    /// An atom nobody may change: byte for byte, or a problem is noted.
    fn same_or_note(&mut self, path: &str, x: &BoxAt, y: &BoxAt) -> Result<(), Stop> {
        if !self.same_atom(x, y)? {
            self.problems.push(format!("{path} would change"));
        }
        Ok(())
    }

    /// The atoms on the way from `moov` to the chunk offset tables —
    /// `trak`, `mdia`, `minf`, `stbl` — whose sizes never change (a table's
    /// entries change, not how many there are): header byte for byte, parts
    /// compared, and the tables checked by (b).
    fn down_to_offsets(&mut self, path: &str, x: &BoxAt, y: &BoxAt) -> Result<(), Stop> {
        if !self.same_bytes((x.start, x.body_start), (y.start, y.body_start))? {
            self.problems
                .push(format!("the size or name of {path} would change"));
            return Ok(());
        }
        let (in_a, in_b) = self.parts(path, (x.body_start, x.end), (y.body_start, y.end))?;
        self.pairs(path, &in_a, &in_b, |walk, path, x, y| match &x.name {
            b"mdia" | b"minf" | b"stbl" => walk.down_to_offsets(path, x, y),
            b"stco" => walk.offsets(path, x, y, 4),
            b"co64" => walk.offsets(path, x, y, 8),
            _ => walk.same_or_note(path, x, y),
        })
    }

    /// Check (b) for one chunk offset table (`width` bytes an entry: 4 in
    /// `stco`, 8 in `co64`): the header, version, flags and count byte for
    /// byte, and each entry moved exactly as far as the `mdat` it points
    /// into. Only the first wrong entry of a table is named.
    fn offsets(&mut self, path: &str, x: &BoxAt, y: &BoxAt, width: u64) -> Result<(), Stop> {
        let body_len = x.end - x.body_start;
        if !self.same_bytes((x.start, x.body_start), (y.start, y.body_start))?
            || y.end - y.body_start != body_len
        {
            self.problems
                .push(format!("the size or name of {path} would change"));
            return Ok(());
        }
        if body_len < 8 {
            return self.same_or_note(path, x, y);
        }
        if !self.same_bytes(
            (x.body_start, x.body_start + 8),
            (y.body_start, y.body_start + 8),
        )? {
            self.problems.push(format!(
                "the version, flags or number of entries of {path} would change"
            ));
            return Ok(());
        }
        self.a.seek(SeekFrom::Start(x.body_start + 4))?;
        let mut count = [0u8; 4];
        self.a.read_exact(&mut count)?;
        let count = u64::from(u32::from_be_bytes(count));
        let entries_end = count
            .checked_mul(width)
            .and_then(|len| len.checked_add(8))
            .filter(|len| *len <= body_len)
            .ok_or_else(|| {
                Stop::Unreadable(format!(
                    "{path} says it holds {count} entries, more than fit in it"
                ))
            })?;
        let width_bytes = usize::try_from(width).unwrap_or(8);
        let mut problem = None;
        self.a.seek(SeekFrom::Start(x.body_start + 8))?;
        self.b.seek(SeekFrom::Start(y.body_start + 8))?;
        for entry in 0..count {
            let (mut old, mut new) = ([0u8; 8], [0u8; 8]);
            self.a.read_exact(&mut old[8 - width_bytes..])?;
            self.b.read_exact(&mut new[8 - width_bytes..])?;
            let (old, new) = (u64::from_be_bytes(old), u64::from_be_bytes(new));
            // The `mdat` whose contents hold this offset, in the original.
            let at = self.mdats.partition_point(|data| data.end <= old);
            let Some(data) = self.mdats.get(at).filter(|data| data.start <= old) else {
                problem = Some(format!(
                    "entry {} of {path} points outside the sample data (to byte {old}), so \
                     whether it is still right after saving cannot be checked",
                    entry + 1
                ));
                break;
            };
            let should_be = i128::from(old) + data.moved_by;
            if i128::from(new) != should_be {
                problem = Some(format!(
                    "entry {} of {path} would point to the wrong place: it was {old}, the data \
                     it points into moves by {} bytes, so it should become {should_be}, but \
                     after saving it would be {new}",
                    entry + 1,
                    data.moved_by
                ));
                break;
            }
        }
        if let Some(problem) = problem {
            self.problems.push(problem);
            return Ok(());
        }
        let rest_a = (x.body_start + entries_end, x.end);
        let rest_b = (y.body_start + entries_end, y.end);
        if !self.same_bytes(rest_a, rest_b)? {
            self.problems.push(format!(
                "the bytes after the entries of {path} would change"
            ));
        }
        Ok(())
    }

    /// Check (c) for a `udta` in `moov`: its size may change, padding is
    /// not compared, a `meta` lofty made may have been added at its start,
    /// and every other part is compared.
    fn udta(&mut self, path: &str, x: &BoxAt, y: &BoxAt) -> Result<(), Stop> {
        if header_len(x) != header_len(y) {
            self.problems
                .push(format!("the header of {path} would change form"));
        }
        let (in_a, in_b) = self.parts(path, (x.body_start, x.end), (y.body_start, y.end))?;
        let in_a: Vec<BoxAt> = in_a.into_iter().filter(|x| !is_padding(&x.name)).collect();
        let mut in_b: Vec<BoxAt> = in_b.into_iter().filter(|y| !is_padding(&y.name)).collect();
        if !in_a.iter().any(|atom| &atom.name == b"meta")
            && in_b.len() == in_a.len() + 1
            && &in_b[0].name == b"meta"
        {
            let made = in_b.remove(0);
            self.made_meta(&format!("{path} → meta"), &made)?;
        }
        self.pairs(path, &in_a, &in_b, |walk, path, x, y| match &x.name {
            b"meta" => walk.meta(path, x, y),
            _ => walk.same_or_note(path, x, y),
        })
    }

    /// Check (c) for a `meta` in a `udta`: its size may change, its version
    /// and flags (when it has them) may not, and every part but its tag
    /// list (compared by `mp4_save_check`) and padding is compared.
    fn meta(&mut self, path: &str, x: &BoxAt, y: &BoxAt) -> Result<(), Stop> {
        if header_len(x) != header_len(y) {
            self.problems
                .push(format!("the header of {path} would change form"));
        }
        // A `meta` too short for its version and flags is refused here,
        // before anything below reads past its end (finding 2 of Codex's
        // catch-up review of revisions 8-10; see `meta_version_len`).
        let version_a = meta_version_len(&mut self.a, x)?;
        let version_b = meta_version_len(&mut self.b, y)?;
        if version_a != version_b
            || !self.same_bytes(
                (x.body_start, x.body_start + version_a),
                (y.body_start, y.body_start + version_b),
            )?
        {
            self.problems
                .push(format!("the version and flags of {path} would change"));
            return Ok(());
        }
        let (in_a, in_b) = self.parts(
            path,
            (x.body_start + version_a, x.end),
            (y.body_start + version_b, y.end),
        )?;
        // The tag list is left to `mp4_save_check`, which reads every byte
        // of it - a tag list with bytes after its last atom is refused there
        // (Codex's catch-up review of revisions 8-10, finding 3), so nothing
        // inside it goes unread by both comparisons.
        let kept = |atom: &BoxAt| !is_padding(&atom.name) && &atom.name != b"ilst";
        let in_a: Vec<BoxAt> = in_a.into_iter().filter(kept).collect();
        let in_b: Vec<BoxAt> = in_b.into_iter().filter(kept).collect();
        self.pairs(path, &in_a, &in_b, |walk, path, x, y| {
            walk.same_or_note(path, x, y)
        })
    }

    /// A `udta` lofty made (at the start of `moov`, when the file had
    /// none): exactly one `meta` it made, nothing else.
    fn made_udta(&mut self, y: &BoxAt) -> Result<(), Stop> {
        let in_b = boxes_in_file(&mut self.b, y.body_start, y.end)?;
        match in_b.as_slice() {
            [meta] if header_len(y) == 8 && &meta.name == b"meta" && meta.end == y.end => {
                self.made_meta("moov → udta → meta", meta)
            }
            _ => {
                self.problems.push(format!(
                    "moov would gain a udta holding {}, not the one box the library makes for \
                     new tags",
                    names(&in_b)
                ));
                Ok(())
            }
        }
    }

    /// A `meta` lofty made (when the file had none): version and flags 0,
    /// then exactly its standard handler and the tag list.
    fn made_meta(&mut self, path: &str, y: &BoxAt) -> Result<(), Stop> {
        if y.end - y.body_start < 4 {
            self.problems.push(format!(
                "{path} would be added, too short to be a metadata box"
            ));
            return Ok(());
        }
        let mut version = [0u8; 4];
        self.b.seek(SeekFrom::Start(y.body_start))?;
        self.b.read_exact(&mut version)?;
        let in_b = boxes_in_file(&mut self.b, y.body_start + 4, y.end)?;
        let standard = match in_b.as_slice() {
            [hdlr, ilst] if version == [0; 4] && &ilst.name == b"ilst" && ilst.end == y.end => {
                let mut bytes = Vec::new();
                self.b.seek(SeekFrom::Start(hdlr.start))?;
                (&mut self.b)
                    .take(hdlr.end - hdlr.start)
                    .read_to_end(&mut bytes)?;
                bytes == LOFTY_HDLR
            }
            _ => false,
        };
        if !standard {
            self.problems.push(format!(
                "{path} would be added holding {}, not the handler and tag list the library \
                 makes for new tags",
                names(&in_b)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// One atom: size, name, contents.
    fn atom(name: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(8 + content.len())
            .expect("fits")
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(name);
        out.extend_from_slice(content);
        out
    }

    /// A chunk offset table (`stco`) holding `offsets`.
    fn stco(offsets: &[u32]) -> Vec<u8> {
        let mut content = vec![0, 0, 0, 0];
        content.extend_from_slice(&u32::try_from(offsets.len()).expect("fits").to_be_bytes());
        for offset in offsets {
            content.extend_from_slice(&offset.to_be_bytes());
        }
        atom(b"stco", &content)
    }

    /// A full `meta` holding `parts`.
    fn meta(parts: &[Vec<u8>]) -> Vec<u8> {
        atom(b"meta", &[vec![0, 0, 0, 0], parts.concat()].concat())
    }

    fn ilst(title: &[u8]) -> Vec<u8> {
        let data = atom(b"data", &[&[0, 0, 0, 1, 0, 0, 0, 0], title].concat());
        atom(b"ilst", &atom(b"\xa9nam", &data))
    }

    /// A small M4A-shaped file: `ftyp`, then `moov` (one track whose one
    /// chunk offset points at the audio, a `udta` holding a `meta` with a
    /// handler, a tag list with the title `title`, padding, and a `chpl`),
    /// then `mdat`, all laid out so the offset is right. `moov_first` puts
    /// `moov` before `mdat`, so a longer title moves the audio.
    fn file(title: &[u8], padding: usize, moov_first: bool) -> Vec<u8> {
        let ftyp = atom(b"ftyp", b"M4A \0\0\0\0M4A ");
        let audio = b"AUDIO-SAMPLES-0123456789".to_vec();
        let build = |offset: u32| {
            let stbl = atom(
                b"stbl",
                &[atom(b"stsd", b"sample-description"), stco(&[offset])].concat(),
            );
            let minf = atom(b"minf", &stbl);
            let mdia = atom(b"mdia", &[atom(b"mdhd", b"media-header"), minf].concat());
            let trak = atom(b"trak", &[atom(b"tkhd", b"track-header"), mdia].concat());
            let hdlr = LOFTY_HDLR.to_vec();
            let meta = meta(&[hdlr, ilst(title), atom(b"free", &vec![1; padding])]);
            let udta = atom(b"udta", &[meta, atom(b"chpl", b"nero-chapters")].concat());
            atom(
                b"moov",
                &[atom(b"mvhd", b"movie-header"), trak, udta].concat(),
            )
        };
        let mdat = atom(b"mdat", &audio);
        if moov_first {
            let moov_len = build(0).len();
            let offset = u32::try_from(ftyp.len() + moov_len + 8).expect("fits");
            [ftyp, build(offset), mdat].concat()
        } else {
            let offset = u32::try_from(ftyp.len() + 8).expect("fits");
            [ftyp, mdat, build(offset)].concat()
        }
    }

    fn outside(original: &[u8], saved: &[u8]) -> Vec<String> {
        differences_outside_the_tags(
            &mut Cursor::new(original.to_vec()),
            &mut Cursor::new(saved.to_vec()),
        )
        .expect("readable")
    }

    /// `bytes` with the first occurrence of `find` replaced by `with`
    /// (same length, so nothing else moves).
    fn patched(bytes: &[u8], find: &[u8], with: &[u8]) -> Vec<u8> {
        assert_eq!(find.len(), with.len());
        let at = bytes
            .windows(find.len())
            .position(|window| window == find)
            .expect("found");
        let mut out = bytes.to_vec();
        out[at..at + with.len()].copy_from_slice(with);
        out
    }

    /// The first chunk offset in `bytes`.
    fn read_first_offset(bytes: &[u8]) -> [u8; 4] {
        let at = bytes
            .windows(4)
            .position(|window| window == b"stco")
            .expect("stco")
            + 12;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    }

    #[test]
    fn a_save_that_changes_only_the_tags_passes_in_either_layout() {
        for moov_first in [false, true] {
            let original = file(b"Old", 64, moov_first);
            // A longer title that no longer fits the padding: with `moov`
            // first, the audio moves, and so must the chunk offset.
            for saved in [
                file(b"New", 64, moov_first),
                file(&[b'x'; 200], 0, moov_first),
            ] {
                assert_eq!(outside(&original, &saved), Vec::<String>::new());
            }
        }
    }

    #[test]
    fn a_changed_byte_of_audio_is_found_check_a() {
        let original = file(b"Old", 64, false);
        let saved = patched(&original, b"SAMPLES", b"SAMPLEZ");
        let found = outside(&original, &saved);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("mdat 1"), "{found:?}");
    }

    #[test]
    fn an_offset_not_moved_with_the_audio_is_found_check_b() {
        let original = file(b"Old", 0, true);
        // A title that moves the audio, but the chunk offset left where it
        // was: what a save that does not correct offsets produces.
        let moved = file(&[b'x'; 200], 0, true);
        let right = u32::from_be_bytes(read_first_offset(&moved));
        let wrong = u32::from_be_bytes(read_first_offset(&original));
        assert_ne!(right, wrong);
        let saved = patched(&moved, &stco(&[right]), &stco(&[wrong]));
        let found = outside(&original, &saved);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains(
                "entry 1 of moov → trak → mdia → minf → stbl → stco would point to the wrong place"
            ),
            "{found:?}"
        );
    }

    #[test]
    fn an_offset_pointing_outside_the_audio_cannot_be_checked_check_b() {
        let good = file(b"Old", 64, false);
        let offset = u32::from_be_bytes(read_first_offset(&good));
        // Pointing into `ftyp` instead: nowhere a save could move right.
        let original = patched(&good, &stco(&[offset]), &stco(&[3]));
        let found = outside(&original, &original);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("points outside the sample data"),
            "{found:?}"
        );
    }

    #[test]
    fn anything_else_in_moov_that_changes_is_found_check_c() {
        let original = file(b"Old", 64, false);
        for (find, with, named) in [
            (
                &b"movie-header"[..],
                &b"movie-HEADER"[..],
                "moov → mvhd would change",
            ),
            (
                b"track-header",
                b"track-HEADER",
                "moov → trak → tkhd would change",
            ),
            (
                b"sample-description",
                b"sample-DESCRIPTION",
                "moov → trak → mdia → minf → stbl → stsd would change",
            ),
            (
                b"nero-chapters",
                b"nero-CHAPTERS",
                "moov → udta → chpl would change",
            ),
            (
                b"mdirappl",
                b"mdirAPPL",
                "moov → udta → meta → hdlr would change",
            ),
        ] {
            let saved = patched(&original, find, with);
            let found = outside(&original, &saved);
            assert_eq!(found, [named.to_string()], "{find:?}");
        }
        // Padding in `udta` and `meta`, the tag list, and the sizes on the
        // way to it may all change.
        assert!(outside(&original, &file(b"A different title", 3, false)).is_empty());
    }

    #[test]
    fn a_handler_written_over_is_found_check_c() {
        // lofty's save on a `meta` with a handler and no tag list: the tag
        // list takes the handler's place (the stand-in review of revision
        // 9, M1).
        let with = |parts: &[Vec<u8>]| {
            let udta = atom(b"udta", &meta(parts));
            [atom(b"ftyp", b"M4A \0\0\0\0"), atom(b"moov", &udta)].concat()
        };
        let original = with(&[LOFTY_HDLR.to_vec(), atom(b"free", &[1; 16])]);
        let saved = with(&[ilst(b"Changed"), atom(b"free", &[1; 16])]);
        let found = outside(&original, &saved);
        assert_eq!(
            found,
            ["the parts of moov → udta → meta would change: now [hdlr]; after saving, []"]
        );
    }

    #[test]
    fn a_top_level_atom_changed_or_moved_is_found_check_d() {
        let original = [file(b"Old", 64, false), atom(b"uuid", b"sixteen-byte-id!")].concat();
        let changed = patched(&original, b"sixteen", b"SIXTEEN");
        assert_eq!(
            outside(&original, &changed),
            ["the atom uuid at the top of the file would change"]
        );
        let ftyp_changed = patched(&original, b"M4A ", b"mp42");
        assert_eq!(
            outside(&original, &ftyp_changed),
            ["the atom ftyp at the top of the file would change"]
        );
        // Moved: `uuid` put before `ftyp`.
        let uuid = atom(b"uuid", b"sixteen-byte-id!");
        let reordered = [uuid.clone(), file(b"Old", 64, false)].concat();
        let found = outside(&original, &reordered);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("the atoms at the top of the file would change"),
            "{found:?}"
        );
        // Padding at the top of the file may come and go.
        let padded = [original.clone(), atom(b"free", &[0; 8])].concat();
        assert!(outside(&original, &padded).is_empty());
        assert!(outside(&padded, &original).is_empty());
    }

    #[test]
    fn the_comparison_refuses_a_fragmented_file_by_itself() {
        // Even with every piece byte for byte the same - which is exactly
        // the damage, since each piece's own positions were not moved.
        let ftyp = atom(b"ftyp", b"M4A \0\0\0\0");
        let pieces = [atom(b"moof", b"piece"), atom(b"mdat", b"audio")].concat();
        let top = [ftyp.clone(), atom(b"moov", b"")].concat();
        let file = [top.clone(), pieces.clone()].concat();
        let found = outside(&file, &file);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("fragmented (it has a `moof` atom)"),
            "{found:?}"
        );
        let in_moov = [ftyp, atom(b"moov", &atom(b"mvex", b"x"))].concat();
        let found = outside(&in_moov, &in_moov);
        assert!(found[0].contains("`mvex`"), "{found:?}");
    }

    #[test]
    fn a_udta_and_meta_lofty_makes_for_a_file_without_tags_pass() {
        let ftyp = atom(b"ftyp", b"M4A \0\0\0\0");
        let mvhd = atom(b"mvhd", b"movie-header");
        let original = [ftyp.clone(), atom(b"moov", &mvhd)].concat();
        let made_meta = meta(&[LOFTY_HDLR.to_vec(), ilst(b"New")]);
        let made = [
            ftyp.clone(),
            atom(b"moov", &[atom(b"udta", &made_meta), mvhd.clone()].concat()),
        ]
        .concat();
        assert!(outside(&original, &made).is_empty());
        // Anything else added is not passed: another handler, or a part
        // besides the handler and the tag list.
        let other_handler = patched(&made, b"mdirappl", b"mdtaappl");
        assert_eq!(outside(&original, &other_handler).len(), 1);
        let extra = [
            ftyp,
            atom(
                b"moov",
                &[
                    atom(b"udta", &[made_meta, atom(b"\xa9xyz", b"+51-000")].concat()),
                    mvhd,
                ]
                .concat(),
            ),
        ]
        .concat();
        assert_eq!(outside(&original, &extra).len(), 1);
    }

    #[test]
    fn a_meta_too_short_for_its_version_is_refused_never_a_panic() {
        // Codex's catch-up review of revisions 8-10, finding 2: a `meta`
        // holding 0 to 3 bytes was taken to have its four bytes of version
        // and flags all the same, so the comparison read past its end and
        // subtracted the start of what follows from its end, which comes
        // before it. These tests run as a debug build does, checking every
        // subtraction for overflow: before the fix, each case below stopped
        // with "attempt to subtract with overflow". Now each is refused as
        // a file that cannot be checked - as the first metadata box in the
        // `udta`, and as a second one after a normal box - by every reader
        // that meets it.
        let ftyp = atom(b"ftyp", b"M4A \0\0\0\0");
        for body in 0..4 {
            let short = atom(b"meta", &vec![0; body]);
            let normal = meta(&[LOFTY_HDLR.to_vec(), ilst(b"Title")]);
            let chpl = atom(b"chpl", b"nero-chapters");
            for (which, udta) in [
                ("first", [short.clone(), chpl.clone()].concat()),
                ("second", [normal, short, chpl].concat()),
            ] {
                let file = [ftyp.clone(), atom(b"moov", &atom(b"udta", &udta))].concat();
                let found = differences_outside_the_tags(
                    &mut Cursor::new(file.clone()),
                    &mut Cursor::new(file.clone()),
                );
                let message = found.expect_err("refused").to_string();
                assert!(
                    message.contains("fewer than the four bytes of version and flags"),
                    "{which} meta of {body} bytes: {message}"
                );
                assert!(message.contains("Nothing was written"), "{message}");
                let read =
                    crate::mp4_save_check::read_ilst_atoms_from(&mut Cursor::new(file.clone()));
                assert!(
                    read.is_err(),
                    "{which} meta of {body} bytes: the tag reader"
                );
                // The check before saving reads the first `meta` only.
                let before = refuse_what_a_save_would_damage(&mut Cursor::new(file));
                assert_eq!(before.is_err(), which == "first", "{which}, {body}");
            }
        }
    }

    #[test]
    fn a_stretch_that_ends_before_it_starts_is_refused_never_subtracted() {
        // Whatever made one, `same_bytes` refuses a stretch that is turned
        // round or runs past the end of its file before it subtracts its
        // ends (finding 2 again: the guard behind `meta_version_len`'s).
        let file = file(b"Old", 0, false);
        let len = file.len() as u64;
        let mut walk = Walk {
            a: BufReader::new(Cursor::new(file.clone())),
            b: BufReader::new(Cursor::new(file)),
            len_a: len,
            len_b: len,
            mdats: Vec::new(),
            problems: Vec::new(),
        };
        for (a, b) in [
            ((10, 6), (10, 14)),
            ((10, 14), (10, 6)),
            ((len - 2, len + 2), (0, 4)),
            ((0, 4), (len, len + 4)),
        ] {
            match walk.same_bytes(a, b) {
                Err(Stop::Unreadable(why)) => assert!(why.contains("does not lie inside it")),
                Err(Stop::Io(e)) => panic!("{a:?} {b:?}: read anyway ({e})"),
                Ok(same) => panic!("{a:?} {b:?}: compared ({same})"),
            }
        }
        assert!(walk.same_bytes((0, 8), (0, 8)).unwrap_or(false));
    }

    #[test]
    fn fragmented_files_and_a_meta_without_a_tag_list_are_refused_before_saving() {
        let refusal = |bytes: Vec<u8>| {
            refuse_what_a_save_would_damage(&mut Cursor::new(bytes))
                .expect_err("refused")
                .to_string()
        };
        let ftyp = atom(b"ftyp", b"M4A \0\0\0\0");
        for top in [b"moof", b"mfra", b"sidx"] {
            let bytes = [ftyp.clone(), atom(b"moov", &[]), atom(top, b"x")].concat();
            let message = refusal(bytes);
            assert!(message.contains("fragmented"), "{message}");
            assert!(
                message.contains(&format!("`{}`", name_of(top))),
                "{message}"
            );
        }
        let with_mvex = [ftyp.clone(), atom(b"moov", &atom(b"mvex", b"x"))].concat();
        assert!(refusal(with_mvex).contains("`mvex`"));

        let with_meta = |parts: &[Vec<u8>]| {
            [ftyp.clone(), atom(b"moov", &atom(b"udta", &meta(parts)))].concat()
        };
        let message = refusal(with_meta(&[LOFTY_HDLR.to_vec(), atom(b"free", &[1; 8])]));
        assert!(
            message.contains("metadata box has no tag list"),
            "{message}"
        );
        assert!(message.contains("[hdlr, free]"), "{message}");
        // Passed: a tag list beside the handler (even an empty one), a
        // `meta` holding only padding or nothing, and no `meta` at all.
        for parts in [
            vec![LOFTY_HDLR.to_vec(), atom(b"ilst", &[])],
            vec![atom(b"free", &[1; 8])],
            vec![],
        ] {
            assert!(refuse_what_a_save_would_damage(&mut Cursor::new(with_meta(&parts))).is_ok());
        }
        let no_meta = [ftyp, atom(b"moov", &atom(b"udta", &atom(b"chpl", b"x")))].concat();
        assert!(refuse_what_a_save_would_damage(&mut Cursor::new(no_meta)).is_ok());
    }
}
