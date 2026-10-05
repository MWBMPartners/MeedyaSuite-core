// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// A file's ID3v2 language (`TLAN`) frames, read straight from its bytes.
// ======================================================================
//
// Why this exists (the stand-in review of revision 6). ID3v2 lists
// several languages in ONE `TLAN` frame, the values separated by null
// characters, and that is what this crate writes. But some tools leave a
// file with one `TLAN` frame PER language instead: this crate itself did,
// before revision 6, and `meedya-tags-extended`'s `TagFile::save` still
// does (issue #100 — it is built on lofty 0.21). ID3v2 does not allow two
// frames with the same name, and lofty, reading such a file, keeps only
// the LAST of them (lofty 0.22.4, `id3/v2/read.rs`: a repeated frame
// replaces the one before it). So the next save through lofty — a title
// change, a ReplayGain write, adding lyrics — wrote the file back with that
// one language and silently deleted the others. (Before that save mutagen
// still read all three and ffprobe the first: the languages were in the
// file, only lofty could not see them. Measured on the reviewer's files.)
//
// lofty offers no way to read the repeated frames, so this module reads
// them itself, and `tag_io` merges them — every language, in file order —
// into the one frame lofty then saves, when they are all in ONE ID3v2 tag.
//
// Frames spread over MORE than one tag are refused instead. This comment
// used to end "Nothing is deleted", and for such a file that was not true:
// an MP3 file can hold several ID3v2 tags one after another, and a WAV or
// AIFF file several ID3 chunks, and lofty's save rewrites only one of them
// and leaves the others as they were. The merge then put every language
// into the rewritten tag while the other still held its own, so the file
// ended up with its languages in two places, in an order that changed from
// one save to the next — the stand-in review of revision 8 measured it on
// a WAV file with an `id3 ` chunk (`eng`) and an `ID3 ` chunk (`fra`):
// mutagen read `eng`, then `fra, eng` after one save. Such a file is now
// refused before anything is written (see `tlan_frames_in_tags`).
//
// And not only when BOTH tags hold languages (the stand-in review of
// revision 9, M2). Revision 9 refused only a file whose language frames
// were in two or more of its tags; a file with two tags and its language
// frames in just ONE of them still went through, and still lost: lofty
// reads and rewrites only one of the tags, so on an MP3 whose only
// language frame sat in its SECOND tag, a write of the language `deu`
// reported success while the file kept `eng` (the frame lofty never
// touched), and on one whose second tag held `eng` and `fra` in two
// frames, a title-only write left mutagen reading `fra` alone. So a file
// with two or more ID3v2 tags (MP3), or two or more ID3 chunks (WAV,
// AIFF), is refused whenever ANY of them holds a language frame. One tag
// followed by an ID3v1 tail or an APE tag at the end of the file, and a
// tag with padding, are still one tag.
//
// It is deliberately small, and deliberately cautious:
//
// - **Every name lofty reads as the language frame counts**: `TLAN`; in an
//   ID3v2.2 tag, that version's three-letter `TLA`; and in an ID3v2.3 tag
//   also `TLA` followed by a zero byte — ID3v2.2's name in an ID3v2.3
//   frame header, which some programs write and lofty reads as `TLAN`.
//   Frames of any of these names are merged together, in file order.
//   (Until Codex's review of revisions 5–7 only `TLAN` was looked for in
//   an ID3v2.3 tag, so a file holding two `TLA`-and-zero frames, `eng` and
//   `fra`, was let through, and a title-only save kept `fra` alone —
//   reproduced before the fix.) An ID3v2.4 tag is different: lofty does
//   not read a `TLA`-and-zero frame there as a language, but mutagen does,
//   and lofty's save would turn it into an ordinary text frame — so such a
//   frame refuses the save, even on its own (see `OLD_NAME_IN_V2_4`).
// - **A cheap look first.** A frame's name is never changed in the file —
//   compression, encryption and unsynchronisation all act on what follows
//   the frame header, and the name holds no `0xFF` byte for
//   unsynchronisation to touch — so a tag holding two language frames holds
//   the bytes of their names (`TLAN`, or `TLA` and a zero) at least twice
//   between them. When the file's ID3v2 tags hold them fewer than two times
//   in all (and no ID3v2.4 tag holds `TLA` and a zero at all), there is
//   nothing to merge and the frames are never parsed. Only a file that may
//   really have the fault pays for, or can be affected by, the parsing
//   below.
// - **Only the plain case is read**: frame headers (a name, a size — plain
//   in ID3v2.3, "synchsafe" in ID3v2.4, which means seven bits per byte —
//   and two flag bytes; ID3v2.2's three-letter names and three-byte sizes
//   too), the text-encoding byte, and the values separated by null
//   characters. Names and text are read exactly as lofty reads them, so a
//   merged value is the value lofty would have read.
// - **Anything else refuses the save** instead of guessing: a tag that is
//   unsynchronised or has an extended header, a language frame that is
//   compressed or encrypted, an unknown text encoding, text that does not
//   decode, a frame that runs past the end of its tag, a frame whose name
//   lofty would not accept (it must be three or four capital letters or
//   digits), an ID3v2.4 frame named `TLA` and a zero, a language frame in
//   a file with more than one ID3v2 tag (above). Refusing leaves the
//   file exactly as it was; saving would have lost languages. (A file
//   split the way this crate split them before revision 6, and
//   `TagFile::save` still does — lofty's format-neutral save — has none of
//   these, so the case that actually happens is always read: checked on
//   real MP3, WAV and AIFF files split by `TagFile::save`.)
// - **The merge takes work in step with the number of values**: a set of
//   the values already kept sits beside the ordered list (`first_of_each`),
//   so a crafted frame holding 100,000 values costs 100,000 steps, not the
//   five billion comparisons searching the list for each value took until
//   Codex's review of revisions 5–7. (Such a frame is now refused by the
//   memory budget below before it is merged - 100,000 values cost over six
//   megabytes of it - but the merge stays in step with whatever is let
//   through.)
// - **What it holds in memory is bounded** (Codex's catch-up review of
//   revisions 8-10, finding 10). The tags are found by their headers alone,
//   and a tag's or a chunk's size is cut to what the file - or its chunk -
//   really holds before anything is read; the cheap look reads a tag a
//   block at a time and keeps only its counts; and only the language
//   frames' contents are read into memory, within a budget of 1 MiB for a
//   whole file (`LANGUAGE_BYTES_BUDGET`) - past that the save is refused,
//   in plain words. Until then every tag, and every ID3 chunk WHOLE, was
//   read into memory and kept while the next was read: a WAV file whose ID3
//   chunk held a small tag in 400 MiB of padding, and an MP3 with four
//   100 MiB tags one after another, each took the program to 410 MiB of
//   memory (measured).
//   The budget is charged more than the text (Codex's review of revision
//   11, finding 2): each language frame costs a fixed 64 bytes on top of
//   its own bytes, and each value found in one another 64 - what keeping a
//   frame's list and a value's string really costs - and no more than 256
//   language frames are read in one tag, or 1,024 in a file. A frame with
//   nothing in it is passed over, as lofty passes over it, with nothing
//   kept for it. Until then only the text was charged: a tag of one million
//   empty `TLAN` frames was charged nothing, kept an empty list for each,
//   and took the program from 2 MB to 52 MB (measured), and one frame of
//   half a million one-letter values, inside the 1 MiB, took it to 26 MB.
//   So "at most 1 MiB held", which this comment used to say, was never
//   true. What is true: the budget bounds what is read and kept, and what
//   the program holds while reading stays within a few times the budget -
//   decoding a frame's text can briefly double it, and the lists grow in
//   steps. Measured on those two files, both now refused, with
//   `/usr/bin/time -l`: no more than 2 MB above the program's own use.
// - **The budget applies only when there is something to merge.** The
//   cheap look decides first; a file with at most one language frame (and
//   one tag, and no old `TLA` name in an ID3v2.4 tag) needs nothing here,
//   so its frame is never read - however large it is. lofty has already
//   read it, within lofty's own limit (next point).
// - **What it cannot bound: lofty's own reading, which comes first.**
//   `tag_io` reads the file through lofty and only then asks here, so by
//   the time a file is refused here, lofty has already read it. lofty reads
//   an ID3v2 tag frame by frame and throws padding away unkept, but holds
//   each frame whole, and refuses - with its own error - a frame larger
//   than its allocation limit, 16 MiB unless a program sets another (lofty
//   0.22.4, `GlobalOptions::allocation_limit`): a crafted file can still
//   make lofty hold up to that much per frame while reading it. And lofty's
//   SAVE of an MP3, WAV or AIFF file reads the whole file into memory to
//   rewrite it, however large it is (lofty 0.22.4, `id3/v2/write/mod.rs`
//   and `id3/v2/write/chunk_file.rs`). Nothing here changes either.
//
// What it CANNOT do: see a tag lofty would find somewhere unusual — an
// ID3v2 tag buried after junk bytes at the start of an MP3, or after an APE
// tag there. That costs nothing, though: lofty's writer only ever replaces
// the tag at the very start of an MP3 (or the first `ID3 ` chunk of a WAV
// or AIFF file), so a tag found anywhere else is left in the file as it
// was, languages and all.

use std::collections::HashSet;
use std::fs::File;
use std::hash::Hash;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use lofty::file::FileType;

use crate::error::MetadataError;

/// What a file's language frames hold, read from its bytes. A language
/// frame is a `TLAN` frame, or one of the older names lofty reads as
/// `TLAN` (see [`language_frame_names`]).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TlanFrames {
    /// The file has no ID3v2 tag, or at most one language frame in all of
    /// its ID3v2 tags — so lofty's own reading already holds every
    /// language, and nothing needs merging.
    AtMostOne,
    /// Two or more language frames: every language they list, in the order
    /// the file holds them (each frame's values in their own order). A
    /// value repeated exactly is kept once, and an empty value is left out:
    /// neither is a language that could be lost.
    Several(Vec<String>),
}

/// Reads every language frame of the ID3v2 tag(s) lofty reads from the file
/// at `path`, of type `file_type` — the tags at the start of an MP3 or AAC
/// file, and every `ID3 ` chunk of a WAV or AIFF file — within a memory
/// budget of 1 MiB for the whole file (see the top of this file for what
/// it counts, and when it applies). Other file types give
/// [`TlanFrames::AtMostOne`]: lofty does not write an ID3v2 tag into them.
///
/// Fails with [`MetadataError::WriteError`] when the file seems to hold two
/// or more language frames but one of them cannot be read (see the top of
/// this file for which cases - more than the budget, or more than 256
/// language frames in one tag or 1,024 in the file, among them), or when
/// the file has more than one ID3v2 tag (an MP3 file's tags one after
/// another, or several ID3 chunks of a WAV or AIFF file) and any of them
/// holds a language frame; the message says what, in plain words. A file
/// that cannot be read at all gives [`MetadataError::IoError`].
pub(crate) fn read_tlan_frames(
    path: &Path,
    file_type: FileType,
) -> Result<TlanFrames, MetadataError> {
    let places = match file_type {
        FileType::Mpeg | FileType::Aac => TagPlaces::AtStart,
        FileType::Wav => TagPlaces::InChunks { big_endian: false },
        FileType::Aiff => TagPlaces::InChunks { big_endian: true },
        _ => return Ok(TlanFrames::AtMostOne),
    };
    let mut reader = BufReader::new(File::open(path)?);
    tlan_frames_in_file(
        &mut reader,
        &places,
        &mut Budget::new(LANGUAGE_BYTES_BUDGET),
    )
}

// ============================================================
// How much is held in memory
// ============================================================

/// The memory budget for one file's language frames, over all of its tags
/// together: 1 MiB. A list of languages is a few bytes; a file asking for
/// more is refused with a plain message (see the top of this file for what
/// is charged to it, and what that does and does not bound).
const LANGUAGE_BYTES_BUDGET: u64 = 1024 * 1024;

/// What each language frame costs from the budget on top of its own bytes,
/// even an empty one: about what keeping it costs (an entry in the list of
/// frames, with room for that list to grow). Without it a frame with
/// nothing in it was free, and a tag of a million of them took the program
/// to 52 MB (Codex's review of revision 11, finding 2).
const FRAME_COST: u64 = 64;

/// What each value found in a language frame costs from the budget, on top
/// of the frame's bytes: about what keeping it costs (its string, and its
/// entry in the frame's list). Without it, one frame of half a million
/// one-letter values, inside the 1 MiB, took the program to 26 MB.
const VALUE_COST: u64 = 64;

/// The most language frames read in one tag, and in one file - empty ones
/// included. A file lists a handful of languages at most; one asking for
/// more is refused in plain words rather than read.
const MAX_FRAMES_PER_TAG: u64 = 256;
const MAX_FRAMES_PER_FILE: u64 = 1024;

/// How many bytes of a tag the cheap look reads at a time.
const SCAN_BLOCK: usize = 64 * 1024;

/// What one file's language frames have taken so far: bytes from the
/// budget (`taken`), and how many frames (`frames`) - both for the tests
/// too.
struct Budget {
    limit: u64,
    taken: u64,
    frames: u64,
}

impl Budget {
    fn new(limit: u64) -> Self {
        Budget {
            limit,
            taken: 0,
            frames: 0,
        }
    }

    /// Takes `len` more bytes from the budget, or says - in plain words,
    /// for the refusal - that the file asks for more than it allows.
    fn take(&mut self, len: u64) -> Result<(), Cannot> {
        if len > self.limit - self.taken {
            return Err(Cannot::Read(format!(
                "its language frames need more memory to read than any list of languages does - \
                 over {} bytes, counting each frame's text and a fixed {FRAME_COST} bytes for \
                 every frame and for every value in one - and this library does not read that \
                 much into memory",
                self.limit
            )));
        }
        self.taken += len;
        Ok(())
    }

    /// Takes one more language frame of `size` bytes - an empty one too -
    /// from the file's count and its budget ([`FRAME_COST`] and the bytes),
    /// or says why not.
    fn take_frame(&mut self, size: u64) -> Result<(), Cannot> {
        self.frames += 1;
        if self.frames > MAX_FRAMES_PER_FILE {
            return Err(Cannot::Read(format!(
                "its tags hold more than {MAX_FRAMES_PER_FILE} language frames in all - far more \
                 than any list of languages needs - and this library does not read that many"
            )));
        }
        self.take(FRAME_COST.saturating_add(size))
    }

    /// Takes `count` values found in one language frame from the budget
    /// ([`VALUE_COST`] each), or says why not.
    fn take_values(&mut self, count: u64) -> Result<(), Cannot> {
        self.take(count.saturating_mul(VALUE_COST))
    }
}

/// Why the frames of a tag could not be read: as plain words, for the
/// refusal message - or because the file could not be read at all.
enum Cannot {
    Read(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for Cannot {
    fn from(e: std::io::Error) -> Self {
        Cannot::Io(e)
    }
}

impl From<String> for Cannot {
    fn from(why: String) -> Self {
        Cannot::Read(why)
    }
}

// ============================================================
// Finding the tags
// ============================================================

/// Where in a file its ID3v2 tags are looked for.
enum TagPlaces {
    /// At the start of an MP3 or AAC file: zero bytes at the very start
    /// are skipped, then every tag that follows directly on the one before
    /// (as lofty finds them).
    AtStart,
    /// In the `ID3 ` (or `id3 `) chunks of a WAV file (`big_endian` false:
    /// chunk sizes are little-endian) or an AIFF file (`big_endian` true).
    InChunks { big_endian: bool },
    /// Tests only: exactly these tags.
    #[cfg(test)]
    Listed(Vec<TagAt>),
}

/// One ID3v2 tag in a file: its 10-byte header, and where its body starts
/// and ends - cut to what its container (the file, or its chunk) really
/// holds, never taken from a size alone. A footer, if any, is left out.
#[derive(Clone, Debug)]
struct TagAt {
    header: [u8; 10],
    body_start: u64,
    body_end: u64,
}

impl TagAt {
    /// The tag's ID3v2 version: 2, 3 or 4.
    fn major(&self) -> u8 {
        self.header[3]
    }
}

/// Calls `visit` on every ID3v2 tag at `places` in the file `reader` reads,
/// in file order. Only headers are read to find them - a tag's size and a
/// chunk's size are checked against the file before anything is read
/// past them - so a tag or chunk that claims gigabytes costs nothing here.
/// (Until Codex's catch-up review of revisions 8-10, finding 10, every
/// tag, and every ID3 chunk WHOLE, was read into memory first and kept
/// while the next was read: a WAV file whose ID3 chunk held a small tag in
/// 400 MiB of padding, or an MP3 with four 100 MiB tags one after another,
/// took the program to 410 MiB of memory - measured.)
fn each_tag<R: Read + Seek>(
    reader: &mut BufReader<R>,
    places: &TagPlaces,
    mut visit: impl FnMut(&mut BufReader<R>, &TagAt) -> Result<(), MetadataError>,
) -> Result<(), MetadataError> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    match places {
        TagPlaces::AtStart => {
            reader.seek(SeekFrom::Start(0))?;
            let mut pos = 0u64;
            let mut byte = [0u8];
            loop {
                if reader.read(&mut byte)? == 0 {
                    return Ok(());
                }
                if byte[0] != 0 {
                    break;
                }
                pos += 1;
            }
            loop {
                reader.seek(SeekFrom::Start(pos))?;
                let mut header = [0u8; 10];
                if !read_fully(reader, &mut header)? || !is_id3v2_header(&header) {
                    return Ok(());
                }
                let size = u64::from(synchsafe([header[6], header[7], header[8], header[9]]));
                let body_start = pos + 10;
                visit(
                    reader,
                    &TagAt {
                        header,
                        body_start,
                        body_end: (body_start + size).min(file_len),
                    },
                )?;
                pos = body_start + size + if has_footer(&header) { 10 } else { 0 };
            }
        }
        TagPlaces::InChunks { big_endian } => {
            // Chunks are walked from byte 12, just after the `RIFF`…`WAVE`
            // or `FORM`…`AIFF` header; a chunk of odd size is followed by
            // one padding byte.
            let mut pos: u64 = 12;
            while pos + 8 <= file_len {
                reader.seek(SeekFrom::Start(pos))?;
                let mut chunk = [0u8; 8];
                reader.read_exact(&mut chunk)?;
                let size_bytes = [chunk[4], chunk[5], chunk[6], chunk[7]];
                let size = u64::from(if *big_endian {
                    u32::from_be_bytes(size_bytes)
                } else {
                    u32::from_le_bytes(size_bytes)
                });
                let content_start = pos + 8;
                let content_end = (content_start + size).min(file_len);
                // The chunk holds one ID3v2 tag from its first byte; the tag
                // is the chunk's own bytes only, as lofty reads it.
                if matches!(&chunk[..4], b"ID3 " | b"id3 ") && content_end - content_start >= 10 {
                    let mut header = [0u8; 10];
                    reader.read_exact(&mut header)?;
                    if is_id3v2_header(&header) {
                        let size =
                            u64::from(synchsafe([header[6], header[7], header[8], header[9]]));
                        visit(
                            reader,
                            &TagAt {
                                header,
                                body_start: content_start + 10,
                                body_end: (content_start + 10 + size).min(content_end),
                            },
                        )?;
                    }
                }
                pos = content_start + size + (size & 1);
            }
            Ok(())
        }
        #[cfg(test)]
        TagPlaces::Listed(tags) => {
            for tag in tags {
                visit(reader, tag)?;
            }
            Ok(())
        }
    }
}

/// Reads as many bytes as `buffer` holds; `false` when the file ends first.
fn read_fully(reader: &mut impl Read, buffer: &mut [u8]) -> Result<bool, MetadataError> {
    match reader.read_exact(buffer) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// `header` starts an ID3v2.2, 2.3 or 2.4 tag (the versions lofty reads).
fn is_id3v2_header(header: &[u8; 10]) -> bool {
    &header[..3] == b"ID3" && matches!(header[3], 2..=4)
}

/// An ID3v2.3 or 2.4 tag whose header says a 10-byte footer follows it.
fn has_footer(header: &[u8; 10]) -> bool {
    header[3] >= 3 && header[5] & 0x10 != 0
}

/// A "synchsafe" number, as ID3v2 stores a tag's size (and, in ID3v2.4, a
/// frame's): four bytes of seven bits each, the top bit of every byte
/// ignored — as lofty ignores it.
fn synchsafe(bytes: [u8; 4]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |total, byte| (total << 7) | u32::from(byte & 0x7F))
}

// ============================================================
// Reading the frames
// ============================================================

/// Every form in which a language frame's name is written in the frame
/// headers of a tag of ID3v2 version `major` — the forms lofty reads as the
/// language frame (see [`frame_name`]):
///
/// - ID3v2.2: `TLA`, that version's three-letter name.
/// - ID3v2.3: `TLAN`, and also `TLA` followed by a zero byte — ID3v2.2's
///   name in an ID3v2.3 frame header, which some programs write and lofty
///   reads as `TLAN`. (Found by Codex's review of revisions 5–7: two such
///   frames were not counted at all, so a title-only save deleted every
///   language but the last.)
/// - ID3v2.4: `TLAN` only. (A `TLA`-and-zero frame there is not one lofty
///   reads as a language; see [`OLD_NAME_IN_V2_4`] for what happens to it.)
fn language_frame_names(major: u8) -> &'static [&'static [u8]] {
    match major {
        2 => &[b"TLA"],
        3 => &[b"TLAN", b"TLA\0"],
        _ => &[b"TLAN"],
    }
}

/// ID3v2.2's language frame name, `TLA` followed by a zero byte, as it is
/// written in an ID3v2.4 frame header. lofty does not read it as a
/// language there (it converts old names in ID3v2.2 and 2.3 tags only): it
/// keeps the frame as one of its own and saves its text back as a
/// user-defined `TXXX` frame named `TLA`. mutagen does read it as the
/// language (measured with lofty 0.22.4 and mutagen: an ID3v2.4 file holding
/// such a frame, `eng`, and a `TLAN` frame, `fra`, which mutagen read as
/// `eng` and `fra`, came out of a title-only save with mutagen reading only
/// `fra` — and `eng` in `TXXX:TLA`). Such a frame refuses the save: it is a
/// language to other programs, and the save would take it out of the
/// language field.
const OLD_NAME_IN_V2_4: &[u8] = b"TLA\0";

/// What a frame's name makes it, as lofty reads that name.
#[derive(Debug, PartialEq, Eq)]
enum FrameName {
    /// A language frame: lofty reads it as `TLAN`.
    Language,
    /// [`OLD_NAME_IN_V2_4`]: a language to other programs, which lofty's
    /// save would not keep as one. Refuses the save.
    OldLanguageNameInV2_4,
    /// Any other frame.
    Other,
    /// A name lofty would not accept at all. Refuses the save, because
    /// the frames after it cannot be trusted to be found.
    Invalid,
}

/// What the frame whose header begins `header`, in a tag of ID3v2 version
/// `major`, is — going by its name, read exactly as lofty reads it (lofty
/// 0.22.4, `id3/v2/frame/header/parse.rs`): three bytes in ID3v2.2; four in
/// ID3v2.3 and 2.4, except that an ID3v2.3 frame whose fourth byte is zero
/// holds an ID3v2.2 name in the first three; then (not in ID3v2.2) any zero
/// bytes at the end are dropped. A name must then be three or four capital
/// letters or digits. lofty converts a three-letter name to its four-letter
/// one in ID3v2.2 and 2.3 tags only, which is why `TLA` is the language
/// frame there and not in ID3v2.4.
fn frame_name(major: u8, header: &[u8]) -> FrameName {
    if major >= 4 && &header[..4] == OLD_NAME_IN_V2_4 {
        return FrameName::OldLanguageNameInV2_4;
    }
    let raw = match major {
        2 => &header[..3],
        3 if header[3] == 0 => &header[..3],
        _ => &header[..4],
    };
    let name = if major == 2 {
        raw
    } else {
        let kept = raw.iter().rposition(|b| *b != 0).map_or(0, |last| last + 1);
        &raw[..kept]
    };
    let valid = matches!(name.len(), 3 | 4)
        && name
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    match name {
        _ if !valid => FrameName::Invalid,
        b"TLAN" => FrameName::Language,
        b"TLA" if major <= 3 => FrameName::Language,
        _ => FrameName::Other,
    }
}

/// How many times `needle` occurs in `window` ending after its first
/// `carried` bytes: in [`look_in`], those bytes are the end of the block
/// before, and an occurrence ending inside them was counted with it.
fn occurrences_ending_after(window: &[u8], needle: &[u8], carried: usize) -> usize {
    // A block without the name's first byte - padding, nearly always - is
    // passed over at once: the standard library's `contains` searches for
    // one byte at full speed even in a build made without optimisations,
    // where comparing at every position is slow (the tests read tags of
    // tens of megabytes).
    if !window.contains(&needle[0]) {
        return 0;
    }
    window
        .windows(needle.len())
        .enumerate()
        .filter(|(start, bytes)| start + needle.len() > carried && *bytes == needle)
        .count()
}

/// What the cheap look (top of this file) finds in one tag's body.
#[derive(Default)]
struct Look {
    /// How many times the name of a language frame - in any form lofty
    /// reads as one in a tag of this version (see
    /// [`language_frame_names`]) - occurs in the body.
    names: usize,
    /// Whether this is an ID3v2.4 tag whose body holds the bytes of
    /// [`OLD_NAME_IN_V2_4`] - a frame that may refuse the save on its own.
    old_name: bool,
}

/// The cheap look at one `tag`'s body, read a block at a time - nothing of
/// it is kept but the counts. The last three bytes of each block are kept
/// with the next, so a name split between two blocks is still found, once.
fn look_in<R: Read + Seek>(reader: &mut BufReader<R>, tag: &TagAt) -> std::io::Result<Look> {
    const CARRY: usize = 3; // one less than the longest name looked for
    let names = language_frame_names(tag.major());
    let mut look = Look::default();
    reader.seek(SeekFrom::Start(tag.body_start))?;
    let mut block = vec![0u8; CARRY + SCAN_BLOCK];
    let (mut carried, mut left) = (0usize, tag.body_end - tag.body_start);
    while left > 0 {
        let n = usize::try_from(left).map_or(SCAN_BLOCK, |left| left.min(SCAN_BLOCK));
        reader.read_exact(&mut block[carried..carried + n])?;
        let window = &block[..carried + n];
        for name in names {
            look.names = look
                .names
                .saturating_add(occurrences_ending_after(window, name, carried));
        }
        if tag.major() >= 4 && occurrences_ending_after(window, OLD_NAME_IN_V2_4, carried) > 0 {
            look.old_name = true;
        }
        let keep = window.len().min(CARRY);
        let from = window.len() - keep;
        block.copy_within(from..from + keep, 0);
        carried = keep;
        left -= n as u64;
    }
    Ok(look)
}

/// The merged result for every ID3v2 tag at `places` in `reader`'s file;
/// see [`read_tlan_frames`]. Language frames are read into memory within
/// `budget`; nothing else of a tag is kept (see the top of this file).
fn tlan_frames_in_file<R: Read + Seek>(
    reader: &mut BufReader<R>,
    places: &TagPlaces,
    budget: &mut Budget,
) -> Result<TlanFrames, MetadataError> {
    // The cheap look (top of this file), at every tag: every form of the
    // name, counted in each tag's body. One old-named frame in an ID3v2.4
    // tag is enough on its own to need a look, since it refuses the save
    // by itself; so is one language name anywhere in a file with more than
    // one tag, since a single language frame there refuses the save too
    // (M2 at the top of this file).
    let (mut tag_count, mut named_total, mut old_name) = (0usize, 0usize, false);
    each_tag(reader, places, |reader, tag| {
        let look = look_in(reader, tag)?;
        tag_count += 1;
        named_total = named_total.saturating_add(look.names);
        old_name |= look.old_name;
        Ok(())
    })?;
    let several_tags = tag_count > 1;
    if named_total < 2 && !old_name && !(several_tags && named_total > 0) {
        return Ok(TlanFrames::AtMostOne);
    }

    // The tags again, now reading the language frames of each one the
    // look found a name in (looked at again, rather than remembered, so
    // that nothing grows with the number of tags).
    let mut frames = Vec::new();
    let mut tags_holding_frames = 0;
    each_tag(reader, places, |reader, tag| {
        let look = look_in(reader, tag)?;
        if look.names == 0 && !look.old_name {
            return Ok(());
        }
        let found = match tlan_frames_in_tag(reader, tag, budget) {
            Ok(found) => found,
            Err(Cannot::Io(e)) => return Err(e.into()),
            Err(Cannot::Read(problem)) => {
                return Err(MetadataError::WriteError(format!(
                    "this file's ID3v2 tag may hold its languages in more than one language frame \
                     (named TLAN, or TLA in older tags), which lofty - the library this crate saves \
                     files with - does not read whole, so saving would lose languages. They are \
                     normally merged into one frame first, but {problem}, so they cannot be read \
                     here. Nothing was written. (For most such files, saving once with mutagen, \
                     which reads every language frame and saves them as one, lets this library \
                     write to them.)"
                )))
            }
        };
        if !found.is_empty() {
            tags_holding_frames += 1;
        }
        frames.extend(found);
        Ok(())
    })?;
    if several_tags && tags_holding_frames > 0 {
        // See the top of this file: lofty reads and rewrites only one of
        // the tags, so a language in another is not seen, not changed and
        // not merged - wherever it is.
        return Err(MetadataError::WriteError(format!(
            "this file has {tag_count} separate ID3v2 tags - an MP3 file with tags one after \
             another, or a WAV or AIFF file with more than one ID3 chunk - and language frames \
             (TLAN) in {tags_holding_frames} of them. lofty, the library this crate saves files \
             with, reads and rewrites only one of those tags and leaves the others as they are, \
             so a save could change the language in one tag while another still holds the old \
             one, or leave the languages in more than one place, in an order that can change \
             from one save to the next. Nothing was written. (Such a file needs its tags merged \
             into one by another tool before this library can write to it.)"
        )));
    }
    if frames.len() < 2 {
        // The letters were somewhere else (inside a text value, say).
        return Ok(TlanFrames::AtMostOne);
    }

    let values: Vec<String> = first_of_each(
        frames
            .into_iter()
            .flatten()
            .filter(|value| !value.is_empty()),
    );
    Ok(TlanFrames::Several(values))
}

/// Each of `values` once — the first time it appears — in the order given.
///
/// A set of the values already kept sits beside the list, so each value is
/// checked in one step however many came before it. (Until Codex's review
/// of revisions 5–7 the list itself was searched for every value, which on
/// a crafted file — one frame holding 100,000 different values, then a
/// second frame — took some five billion comparisons, stalling both
/// reading the file and any save to it. Now the work grows in step with the
/// number of values: see `merging_takes_work_in_step_with_the_values`.)
///
/// Generic only so that test can count the comparisons; the file's
/// languages are `String`s.
fn first_of_each<T: Eq + Hash + Clone>(values: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut seen: HashSet<T> = HashSet::new();
    let mut kept = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            kept.push(value);
        }
    }
    kept
}

/// The values of every language frame in one ID3v2 `tag`, one list per
/// frame, in order — or why they cannot be read (as plain words, for the
/// refusal message). The frames are walked from header to header; only a
/// language frame's contents are read into memory, within `budget`, and
/// every other frame is stepped over unread.
fn tlan_frames_in_tag<R: Read + Seek>(
    reader: &mut BufReader<R>,
    tag: &TagAt,
    budget: &mut Budget,
) -> Result<Vec<Vec<String>>, Cannot> {
    let major = tag.major();
    let tag_flags = tag.header[5];
    if tag_flags & 0x80 != 0 {
        return Err(Cannot::Read(
            "the file's ID3v2 tag is \"unsynchronised\" (an old encoding this library \
                    does not undo)"
                .to_string(),
        ));
    }
    if major >= 3 && tag_flags & 0x40 != 0 {
        return Err(Cannot::Read(
            "the file's ID3v2 tag has an \"extended header\", which this library does not read"
                .to_string(),
        ));
    }

    // ID3v2.2: three-letter name, three-byte size, no flags. ID3v2.3 and
    // 2.4: four-byte name, four-byte size, two flag bytes.
    let (name_len, header_len) = if major == 2 { (3, 6) } else { (4, 10) };
    let body_len = tag.body_end - tag.body_start;

    let mut frames = Vec::new();
    let mut language_frames_in_tag = 0u64;
    let mut pos = 0u64;
    reader.seek(SeekFrom::Start(tag.body_start))?;
    while pos + header_len as u64 <= body_len {
        let mut bytes = [0u8; 10];
        reader.read_exact(&mut bytes[..header_len])?;
        let header = &bytes[..header_len];
        if header[0] == 0 {
            // Padding: no more frames (as lofty reads it).
            break;
        }
        let name = &header[..name_len];
        let is_language = match frame_name(major, header) {
            FrameName::Language => true,
            FrameName::Other => false,
            FrameName::Invalid => {
                return Err(Cannot::Read(format!(
                    "a frame {pos} bytes into the tag has no valid name, so the frames after it \
                     cannot be found"
                )))
            }
            FrameName::OldLanguageNameInV2_4 => {
                return Err(Cannot::Read(format!(
                    "a frame {pos} bytes into the tag is named TLA followed by a zero byte - \
                     the old ID3v2.2 name for the language frame - inside an ID3v2.4 tag, which \
                     other programs (mutagen among them) read as a language but lofty does not: \
                     saving would turn it into an ordinary text frame named TLA"
                )))
            }
        };
        let size = u64::from(match major {
            2 => u32::from_be_bytes([0, header[3], header[4], header[5]]),
            3 => u32::from_be_bytes([header[4], header[5], header[6], header[7]]),
            _ => synchsafe([header[4], header[5], header[6], header[7]]),
        });
        let end = pos + header_len as u64 + size;
        if end > body_len {
            return Err(Cannot::Read(format!(
                "the frame {} {pos} bytes into the tag runs past the end of the tag",
                String::from_utf8_lossy(name)
            )));
        }
        if is_language {
            // Counted and charged before anything is kept - an empty frame
            // too, so a tag of countless empty frames is refused, never
            // walked to its end (see the top of this file).
            language_frames_in_tag += 1;
            if language_frames_in_tag > MAX_FRAMES_PER_TAG {
                return Err(Cannot::Read(format!(
                    "one of its tags holds more than {MAX_FRAMES_PER_TAG} language frames - far \
                     more than any list of languages needs - and this library does not read that \
                     many"
                )));
            }
            budget.take_frame(size)?;
            if size == 0 {
                // An empty frame is passed over, as lofty passes over it
                // (lofty 0.22.4, `id3/v2/frame/read.rs`: "Encountered a
                // zero length frame, skipping"), and nothing is kept for
                // it: it is not a language frame to lofty, so it changes
                // nothing about what needs merging. (Until Codex's review
                // of revision 11, finding 2, each one kept an empty list.)
                pos = end;
                continue;
            }
            let mut content = vec![0u8; usize::try_from(size).unwrap_or(usize::MAX)];
            reader.read_exact(&mut content)?;
            let content = frame_content(&content, major, header)?;
            frames.push(frame_values(content, major, budget)?);
        } else {
            // Stepped over, unread: within the reader's buffer when it can
            // be, so walking many small frames reads the tag only once.
            reader.seek_relative(i64::try_from(size).unwrap_or(i64::MAX))?;
        }
        pos = end;
    }
    Ok(frames)
}

/// A language frame's content with the extra bytes some frame flags add in
/// front of it taken off, or why it cannot be read.
fn frame_content<'a>(content: &'a [u8], major: u8, header: &[u8]) -> Result<&'a [u8], String> {
    if major == 2 {
        return Ok(content);
    }
    let flags = header[9];
    // ID3v2.3: compression 0x80, encryption 0x40, grouping 0x20.
    // ID3v2.4: grouping 0x40, compression 0x08, encryption 0x04,
    //          unsynchronisation 0x02, data-length indicator 0x01.
    let (unreadable, grouping, length_indicator) = if major == 3 {
        (flags & 0xC0 != 0, flags & 0x20 != 0, false)
    } else {
        (flags & 0x0E != 0, flags & 0x40 != 0, flags & 0x01 != 0)
    };
    if unreadable {
        return Err(
            "one of those frames is compressed, encrypted or unsynchronised, which this \
             library does not undo"
                .to_string(),
        );
    }
    // A grouping byte comes first, then the four-byte data length.
    let skip = usize::from(grouping) + if length_indicator { 4 } else { 0 };
    content
        .get(skip..)
        .ok_or_else(|| "one of those frames is shorter than its own flags say".to_string())
}

/// The values of one language frame's `content` (its text-encoding byte
/// and text), decoded exactly as lofty decodes them: trailing null
/// characters taken off, then split at each null character. The values are
/// counted, and charged to `budget` ([`VALUE_COST`] each), before any is
/// kept.
fn frame_values(content: &[u8], major: u8, budget: &mut Budget) -> Result<Vec<String>, Cannot> {
    let Some((&encoding, text)) = content.split_first() else {
        return Ok(Vec::new());
    };
    let decoded = match encoding {
        // ISO-8859-1: every byte is the character with that number.
        0 => text.iter().map(|&b| char::from(b)).collect(),
        // UTF-16 starting with a byte-order mark.
        1 => {
            if text.is_empty() {
                String::new()
            } else {
                let big_endian = match text {
                    [0xFE, 0xFF, ..] => true,
                    [0xFF, 0xFE, ..] => false,
                    _ => {
                        return Err(
                            "one of those frames is UTF-16 text without a byte-order mark"
                                .to_string()
                                .into(),
                        )
                    }
                };
                utf16(&text[2..], big_endian)?
            }
        }
        // UTF-16 big-endian, and UTF-8: ID3v2.3 and 2.4 only.
        2 if major >= 3 => utf16(text, true)?,
        3 if major >= 3 => String::from_utf8(text.to_vec())
            .map_err(|_| "one of those frames is not valid UTF-8 text".to_string())?,
        other => {
            return Err(format!(
                "one of those frames uses text encoding {other}, which ID3v2.{major} does not \
                 define"
            )
            .into())
        }
    };
    let text = decoded.trim_end_matches('\0');
    // Counting the values keeps none of them (`split` only points into the
    // text), so a frame of countless tiny values is refused here, before
    // a string is made for each.
    budget.take_values(text.split('\0').count() as u64)?;
    Ok(text.split('\0').map(str::to_string).collect())
}

/// UTF-16 `bytes` in the given byte order. A byte-order mark in the middle
/// (each value of a list may start with its own) is dropped, as lofty
/// drops it.
fn utf16(bytes: &[u8], big_endian: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("one of those frames is UTF-16 text of an odd number of bytes".to_string());
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .filter(|pair| !matches!(pair, [0xFF, 0xFE] | [0xFE, 0xFF]))
        .map(|pair| {
            if big_endian {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from_le_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    String::from_utf16(&units)
        .map_err(|_| "one of those frames is not valid UTF-16 text".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// `tags` back to back in one in-memory file, and each one's place in
    /// it: its body ends where its bytes do (as a tag cut short in a file
    /// does, whatever its header says).
    fn listed(tags: &[Vec<u8>]) -> (Vec<u8>, Vec<TagAt>) {
        let mut bytes = Vec::new();
        let mut places = Vec::new();
        for tag in tags {
            let start = bytes.len() as u64;
            places.push(TagAt {
                header: tag[..10].try_into().expect("a header"),
                body_start: start + 10,
                body_end: start + tag.len() as u64,
            });
            bytes.extend_from_slice(tag);
        }
        (bytes, places)
    }

    /// The merged result for `tags`, read at exactly their own places.
    fn tlan_frames_in_tags(tags: &[Vec<u8>]) -> Result<TlanFrames, MetadataError> {
        tlan_frames_in_tags_within(tags, LANGUAGE_BYTES_BUDGET)
    }

    /// [`tlan_frames_in_tags`], with a budget of `limit` bytes.
    fn tlan_frames_in_tags_within(
        tags: &[Vec<u8>],
        limit: u64,
    ) -> Result<TlanFrames, MetadataError> {
        let (bytes, places) = listed(tags);
        tlan_frames_in_file(
            &mut BufReader::new(Cursor::new(bytes)),
            &TagPlaces::Listed(places),
            &mut Budget::new(limit),
        )
    }

    /// A budget with room for the review's crafted tag below (100,001
    /// values, about seven megabytes of charges), which the real budget
    /// refuses: the tests of the reading and the merge themselves use it.
    const ROOMY_BUDGET: u64 = 16 * 1024 * 1024;

    /// The language frames of the one `tag`, or why they cannot be read,
    /// with a budget of `limit` bytes.
    fn frames_of_within(tag: Vec<u8>, limit: u64) -> Result<Vec<Vec<String>>, String> {
        let (bytes, places) = listed(&[tag]);
        let mut budget = Budget::new(limit);
        match tlan_frames_in_tag(
            &mut BufReader::new(Cursor::new(bytes)),
            &places[0],
            &mut budget,
        ) {
            Ok(frames) => Ok(frames),
            Err(Cannot::Read(why)) => Err(why),
            Err(Cannot::Io(e)) => panic!("an in-memory file could not be read: {e}"),
        }
    }

    // Every test builds its tag byte by byte, so what is being read is
    // visible in the test itself and no binary file needs keeping in git.

    /// A four-byte synchsafe number (seven bits per byte).
    fn to_synchsafe(n: u32) -> [u8; 4] {
        [
            ((n >> 21) & 0x7F) as u8,
            ((n >> 14) & 0x7F) as u8,
            ((n >> 7) & 0x7F) as u8,
            (n & 0x7F) as u8,
        ]
    }

    /// One ID3v2.3 or 2.4 frame: name, size (plain in 2.3, synchsafe in
    /// 2.4), flag bytes `0` and `flags`, content.
    fn frame(major: u8, name: &[u8; 4], flags: u8, content: &[u8]) -> Vec<u8> {
        let size = u32::try_from(content.len()).expect("fits");
        let mut out = name.to_vec();
        if major == 4 {
            out.extend_from_slice(&to_synchsafe(size));
        } else {
            out.extend_from_slice(&size.to_be_bytes());
        }
        out.extend_from_slice(&[0, flags]);
        out.extend_from_slice(content);
        out
    }

    /// One text frame: the encoding byte, then `text` already encoded.
    fn text_frame(major: u8, name: &[u8; 4], encoding: u8, text: &[u8]) -> Vec<u8> {
        let mut content = vec![encoding];
        content.extend_from_slice(text);
        frame(major, name, 0, &content)
    }

    /// A whole tag: header (`major`, tag flags `flags`), the frames, then
    /// `padding` zero bytes.
    fn tag(major: u8, flags: u8, frames: &[Vec<u8>], padding: usize) -> Vec<u8> {
        let mut body: Vec<u8> = frames.concat();
        body.resize(body.len() + padding, 0);
        let mut out = b"ID3".to_vec();
        out.extend_from_slice(&[major, 0, flags]);
        out.extend_from_slice(&to_synchsafe(u32::try_from(body.len()).expect("fits")));
        out.extend_from_slice(&body);
        out
    }

    fn several(values: &[&str]) -> TlanFrames {
        TlanFrames::Several(values.iter().map(|v| v.to_string()).collect())
    }

    /// UTF-16 little-endian with a byte-order mark, as ID3's encoding 1.
    fn utf16le_bom(text: &str) -> Vec<u8> {
        let mut out = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out
    }

    #[test]
    fn three_utf8_frames_in_an_id3v2_4_tag_are_merged_in_order() {
        // The shape lofty leaves when it splits one frame: three TLAN
        // frames, UTF-8 (encoding 3), one value each, then padding.
        let tag = tag(
            4,
            0,
            &[
                text_frame(4, b"TSSE", 3, b"Lavf63"),
                text_frame(4, b"TLAN", 3, b"por"),
                text_frame(4, b"TLAN", 3, b"deu"),
                text_frame(4, b"TLAN", 3, b"zho"),
            ],
            64,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["por", "deu", "zho"])
        );
    }

    #[test]
    fn frame_sizes_are_plain_in_id3v2_3_and_synchsafe_in_id3v2_4() {
        // A 200-byte frame before the language frames: 200 is written
        // differently in the two forms (plain 0x000000C8, synchsafe
        // 0x00000148), so reading a size the wrong way loses the frames
        // after it — the test would then fail to find both languages.
        let long = vec![b'x'; 199];
        for major in [3u8, 4] {
            let tag = tag(
                major,
                0,
                &[
                    text_frame(major, b"TXXX", 0, &long),
                    text_frame(major, b"TLAN", 0, b"eng"),
                    text_frame(major, b"TLAN", 0, b"fra"),
                ],
                0,
            );
            assert_eq!(
                tlan_frames_in_tags(&[tag]).expect("read"),
                several(&["eng", "fra"]),
                "ID3v2.{major}"
            );
        }
    }

    #[test]
    fn every_text_encoding_is_decoded_as_lofty_decodes_it() {
        let mut utf16be = vec![];
        for unit in "deu".encode_utf16() {
            utf16be.extend_from_slice(&unit.to_be_bytes());
        }
        // Latin-1 with a trailing null; UTF-16 with a byte-order mark,
        // holding two values each with its own mark; UTF-16 big-endian;
        // UTF-8 holding two values.
        let mut two_utf16 = utf16le_bom("eng");
        two_utf16.extend_from_slice(&[0, 0]);
        two_utf16.extend_from_slice(&utf16le_bom("fra"));
        let tag = tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 0, b"cy\0"),
                text_frame(4, b"TLAN", 1, &two_utf16),
                text_frame(4, b"TLAN", 2, &utf16be),
                text_frame(4, b"TLAN", 3, "ja\0zh-Hant".as_bytes()),
            ],
            8,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["cy", "eng", "fra", "deu", "ja", "zh-Hant"])
        );
    }

    #[test]
    fn a_repeated_value_is_kept_once_and_an_empty_one_left_out() {
        let tag = tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 3, b"eng"),
                text_frame(4, b"TLAN", 3, b""),
                text_frame(4, b"TLAN", 3, b"eng\0fra"),
            ],
            0,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["eng", "fra"])
        );
    }

    #[test]
    fn one_frame_or_the_name_inside_text_needs_no_merging() {
        // One frame listing two languages: the normal form.
        let one = tag(4, 0, &[text_frame(4, b"TLAN", 3, b"eng\0fra")], 0);
        assert_eq!(
            tlan_frames_in_tags(&[one]).expect("read"),
            TlanFrames::AtMostOne
        );
        // No frame at all.
        let none = tag(4, 0, &[text_frame(4, b"TIT2", 3, b"Title")], 0);
        assert_eq!(
            tlan_frames_in_tags(&[none]).expect("read"),
            TlanFrames::AtMostOne
        );
        // "TLAN" twice in the bytes, but one of them is a title.
        let inside = tag(
            4,
            0,
            &[
                text_frame(4, b"TIT2", 3, b"TLAN"),
                text_frame(4, b"TLAN", 3, b"eng"),
            ],
            0,
        );
        assert_eq!(
            tlan_frames_in_tags(&[inside]).expect("read"),
            TlanFrames::AtMostOne
        );
        // No tags at all.
        assert_eq!(
            tlan_frames_in_tags(&[]).expect("read"),
            TlanFrames::AtMostOne
        );
    }

    #[test]
    fn language_frames_in_two_tags_refuse_the_save() {
        // They used to be merged into the first tag, while lofty's save left
        // the second as it was: the languages then sat in two places (the
        // stand-in review of revision 8). One frame in each tag is enough.
        let first = tag(3, 0, &[text_frame(3, b"TLAN", 0, b"eng")], 0);
        let second = tag(4, 0, &[text_frame(4, b"TLAN", 3, b"fra")], 0);
        let message = refusal(&[first.clone(), second]);
        assert!(message.contains("has 2 separate ID3v2 tags"), "{message}");
        assert!(
            message.contains("language frames (TLAN) in 2 of them"),
            "{message}"
        );
        assert!(message.contains("Nothing was written"), "{message}");
        // Language frames in only ONE of the tags refuse too (the stand-in
        // review of revision 9, M2 - this test used to say "a second tag
        // holding no language frame is no reason to refuse", and passed
        // both of these): lofty reads and rewrites one tag only, so a
        // language in the other is never seen.
        let no_language = tag(4, 0, &[text_frame(4, b"TIT2", 3, b"TLAN")], 0);
        let two = tag(
            3,
            0,
            &[
                text_frame(3, b"TLAN", 0, b"eng"),
                text_frame(3, b"TLAN", 0, b"fra"),
            ],
            0,
        );
        for tags in [
            vec![two.clone(), no_language.clone()],
            vec![no_language.clone(), two.clone()],
            vec![first.clone(), tag(4, 0, &[], 8)],
            vec![tag(4, 0, &[], 8), first.clone()],
        ] {
            let message = refusal(&tags);
            assert!(message.contains("in 1 of them"), "{message}");
        }
        // Two tags and no language frame in either: nothing to refuse
        // (the `TLAN` inside the title's text is not a frame).
        assert_eq!(
            tlan_frames_in_tags(&[no_language.clone(), tag(4, 0, &[], 8)]).expect("read"),
            TlanFrames::AtMostOne
        );
        // One tag, however its languages are held: merged or passed as
        // before.
        assert_eq!(
            tlan_frames_in_tags(&[two]).expect("read"),
            several(&["eng", "fra"])
        );
        assert_eq!(
            tlan_frames_in_tags(&[first]).expect("read"),
            TlanFrames::AtMostOne
        );
    }

    #[test]
    fn an_id3v2_2_tag_names_the_frame_tla() {
        // Three-letter names, three-byte plain sizes, no flags.
        let v22 = |name: &[u8; 3], text: &[u8]| {
            let mut out = name.to_vec();
            let size = u32::try_from(text.len() + 1).expect("fits").to_be_bytes();
            out.extend_from_slice(&size[1..]);
            out.push(0);
            out.extend_from_slice(text);
            out
        };
        let tag = tag(
            2,
            0,
            &[
                v22(b"TT2", b"Title"),
                v22(b"TLA", b"eng"),
                v22(b"TLA", b"deu"),
            ],
            4,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["eng", "deu"])
        );
    }

    // ------------------------------------------------------------------
    // ID3v2.2 names in ID3v2.3 tags (Codex's review of revisions 5–7):
    // lofty reads a frame named `TLA` followed by a zero byte, inside an
    // ID3v2.3 tag, as `TLAN`. Only `TLAN` used to be looked for, so two
    // such frames were let through and a save kept the last language only.
    // ------------------------------------------------------------------

    #[test]
    fn two_tla_frames_in_an_id3v2_3_tag_are_merged() {
        // The review's file: an ID3v2.3 tag, two frames named `TLA\0`,
        // Latin-1 `eng` and `fra`.
        let tag = tag(
            3,
            0,
            &[
                text_frame(3, b"TLA\0", 0, b"eng"),
                text_frame(3, b"TLA\0", 0, b"fra"),
            ],
            16,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["eng", "fra"])
        );
    }

    #[test]
    fn tla_and_tlan_frames_in_an_id3v2_3_tag_are_merged_in_file_order() {
        // Both names in one tag, in either order, with an unrelated
        // ID3v2.2-named frame (`TT2\0`, a title) before them: every one
        // is read, and the languages come out in file order.
        for (first, second, expected) in [
            (b"TLA\0", b"TLAN", ["deu", "eng", "fra"]),
            (b"TLAN", b"TLA\0", ["deu", "eng", "fra"]),
        ] {
            let tag = tag(
                3,
                0,
                &[
                    text_frame(3, b"TT2\0", 0, b"Title"),
                    text_frame(3, first, 0, b"deu"),
                    text_frame(3, second, 0, b"eng\0fra"),
                ],
                0,
            );
            assert_eq!(
                tlan_frames_in_tags(&[tag]).expect("read"),
                several(&expected),
                "{} then {}",
                String::from_utf8_lossy(first),
                String::from_utf8_lossy(second)
            );
        }
    }

    #[test]
    fn a_tla_frame_in_an_id3v2_4_tag_refuses_the_save_even_alone() {
        // mutagen reads it as a language, lofty does not, and lofty's save
        // would turn it into an ordinary text frame (`TXXX:TLA`): refused,
        // whether it is the only language-like frame or sits beside a
        // `TLAN` frame, before or after it.
        let old = text_frame(4, b"TLA\0", 0, b"eng");
        let tlan = text_frame(4, b"TLAN", 0, b"fra");
        for frames in [
            vec![old.clone()],
            vec![old.clone(), tlan.clone()],
            vec![tlan.clone(), old.clone()],
        ] {
            let message = refusal(&[tag(4, 0, &frames, 0)]);
            assert!(message.contains("inside an ID3v2.4 tag"), "{message}");
            assert!(message.contains("Nothing was written"), "{message}");
        }
        // The same bytes inside a text value are not a frame: no refusal.
        let inside = tag(4, 0, &[text_frame(4, b"TIT2", 0, b"TLA\0x"), tlan], 0);
        assert_eq!(
            tlan_frames_in_tags(&[inside]).expect("read"),
            TlanFrames::AtMostOne
        );
    }

    #[test]
    fn frame_names_are_read_as_lofty_reads_them() {
        use FrameName::{Invalid, Language, OldLanguageNameInV2_4, Other};
        // (version, the header's name bytes, what lofty makes of it).
        let cases: [(u8, &[u8], FrameName); 16] = [
            (2, b"TLA", Language),
            (2, b"TT2", Other),
            (2, b"TL\0", Invalid),
            (3, b"TLAN", Language),
            (3, b"TLA\0", Language),
            (3, b"TT2\0", Other),
            (3, b"TIT2", Other),
            (3, b"TL\0\0", Invalid),
            (3, b"tlan", Invalid),
            (4, b"TLAN", Language),
            (4, b"TLA\0", OldLanguageNameInV2_4),
            (4, b"TIT2", Other),
            (4, b"TL\0N", Invalid),
            (4, b"t!@#", Invalid),
            // Zero bytes at the END of a four-byte name are dropped first
            // (not in the middle, above): `TT2` and a zero is a valid
            // three-letter name to lofty, an ordinary frame; `TL` and two
            // zeros is too short.
            (4, b"TT2\0", Other),
            (4, b"TL\0\0", Invalid),
        ];
        for (major, name, expected) in cases {
            let mut header = name.to_vec();
            header.resize(10, 0);
            assert_eq!(
                frame_name(major, &header),
                expected,
                "ID3v2.{major} {:?}",
                String::from_utf8_lossy(name)
            );
        }
    }

    #[test]
    fn a_frame_whose_name_ends_in_a_zero_byte_is_stepped_over() {
        // An ID3v2.4 tag with a frame named `TT2` and a zero between two
        // language frames. lofty drops the zero and keeps the frame as an
        // ordinary one; read as a name with a zero in it, it would count as
        // no valid name, and the save would be refused for nothing (the
        // stand-in review of revision 8 found no test for this, M8b).
        let tag = tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 0, b"eng"),
                text_frame(4, b"TT2\0", 0, b"Title"),
                text_frame(4, b"TLAN", 0, b"fra"),
            ],
            0,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["eng", "fra"])
        );
    }

    // ------------------------------------------------------------------
    // The merge's work (Codex's review of revisions 5–7): searching the
    // list for each value made a crafted file cost billions of steps.
    // ------------------------------------------------------------------

    thread_local! {
        /// Comparisons and hash calculations made on `Counted` values.
        static WORK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }

    /// A text value that counts every comparison and hash calculation
    /// made with it — the merge's work, measured without a clock.
    #[derive(Clone, Debug)]
    struct Counted(String);

    impl PartialEq for Counted {
        fn eq(&self, other: &Self) -> bool {
            WORK.with(|work| work.set(work.get() + 1));
            self.0 == other.0
        }
    }

    impl Eq for Counted {}

    impl Hash for Counted {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            WORK.with(|work| work.set(work.get() + 1));
            self.0.hash(state);
        }
    }

    /// The review's crafted tag, with `count` values in the first frame:
    /// an ID3v2.4 tag whose first `TLAN` frame holds `x-000000`,
    /// `x-000001`, … separated by null characters, and whose second holds
    /// `eng`.
    fn crafted_tag(count: usize) -> Vec<u8> {
        let many: Vec<String> = (0..count).map(|n| format!("x-{n:06}")).collect();
        tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 3, many.join("\0").as_bytes()),
                text_frame(4, b"TLAN", 3, b"eng"),
            ],
            0,
        )
    }

    /// The work `first_of_each` does on the crafted tag's values (read
    /// with room for them: the real budget refuses the tag).
    fn merge_work(count: usize) -> u64 {
        let frames = frames_of_within(crafted_tag(count), ROOMY_BUDGET).expect("read");
        let values: Vec<Counted> = frames.into_iter().flatten().map(Counted).collect();
        assert_eq!(values.len(), count + 1);
        WORK.with(|work| work.set(0));
        let kept = first_of_each(values);
        assert_eq!(kept.len(), count + 1, "every value is distinct");
        WORK.with(std::cell::Cell::get)
    }

    #[test]
    fn merging_takes_work_in_step_with_the_values() {
        // The review's shape: 100,000 distinct values, then `eng`. Searching
        // the list for each value took 5,000,050,000 comparisons; a set
        // takes a few steps per value.
        let full = merge_work(100_000);
        assert!(
            full <= 5 * 100_001,
            "{full} steps for 100,001 values: more than five per value"
        );
        // Doubling the values roughly doubles the work (searching the list
        // would have quadrupled it).
        let half = merge_work(50_000);
        let ratio = full as f64 / half as f64;
        assert!(
            (1.5..3.0).contains(&ratio),
            "{half} steps for 50,001 values, {full} for 100,001: ratio {ratio}"
        );
    }

    #[test]
    fn the_crafted_tag_is_read_whole_and_in_order() {
        // Since Codex's review of revision 11 the real budget refuses it -
        // 100,001 values at 64 bytes each are over six megabytes - so it is
        // read here with room for them, to show the reading itself.
        let message = refusal(&[crafted_tag(100_000)]);
        assert!(
            message.contains("need more memory to read than any list of languages does"),
            "{message}"
        );
        let TlanFrames::Several(values) =
            tlan_frames_in_tags_within(&[crafted_tag(100_000)], ROOMY_BUDGET).expect("read")
        else {
            panic!("two frames");
        };
        assert_eq!(values.len(), 100_001);
        assert_eq!(values[0], "x-000000");
        assert_eq!(values[99_999], "x-099999");
        assert_eq!(values[100_000], "eng");
    }

    #[test]
    fn a_grouping_byte_and_a_data_length_are_stepped_over() {
        // ID3v2.4 flags: grouping 0x40 adds one byte, the data-length
        // indicator 0x01 adds four; both come before the encoding byte.
        let tag = tag(
            4,
            0,
            &[
                frame(
                    4,
                    b"TLAN",
                    0x40 | 0x01,
                    &[7, 0, 0, 0, 4, 3, b'e', b'n', b'g'],
                ),
                frame(4, b"TLAN", 0, &[3, b'f', b'r', b'a']),
            ],
            0,
        );
        assert_eq!(
            tlan_frames_in_tags(&[tag]).expect("read"),
            several(&["eng", "fra"])
        );
    }

    /// The refusal text for `tags`, which must be refused.
    fn refusal(tags: &[Vec<u8>]) -> String {
        match tlan_frames_in_tags(tags) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn what_cannot_be_read_is_refused_never_guessed() {
        let plain = text_frame(4, b"TLAN", 3, b"eng");
        let cases: [(&str, Vec<u8>, &str); 7] = [
            (
                "a compressed frame",
                tag(
                    4,
                    0,
                    &[plain.clone(), frame(4, b"TLAN", 0x08, &[3, b'x'])],
                    0,
                ),
                "compressed",
            ),
            (
                "an unsynchronised tag",
                tag(4, 0x80, &[plain.clone(), plain.clone()], 0),
                "unsynchronised",
            ),
            (
                "an extended header",
                tag(4, 0x40, &[plain.clone(), plain.clone()], 0),
                "extended header",
            ),
            (
                "an unknown encoding",
                tag(4, 0, &[plain.clone(), text_frame(4, b"TLAN", 7, b"fra")], 0),
                "text encoding 7",
            ),
            (
                "UTF-8 that does not decode",
                tag(
                    4,
                    0,
                    &[plain.clone(), text_frame(4, b"TLAN", 3, &[0xC3])],
                    0,
                ),
                "not valid UTF-8",
            ),
            (
                "a frame running past the tag",
                {
                    let mut t = tag(4, 0, &[plain.clone(), plain.clone()], 0);
                    t.truncate(t.len() - 2);
                    t
                },
                "runs past the end",
            ),
            (
                "a frame with no valid name between them",
                tag(
                    4,
                    0,
                    &[plain.clone(), frame(4, b"t!@#", 0, b"x"), plain.clone()],
                    0,
                ),
                "no valid name",
            ),
        ];
        for (what, tag, expected) in cases {
            let message = refusal(&[tag]);
            assert!(message.contains(expected), "{what}: {message}");
            assert!(message.contains("Nothing was written"), "{what}: {message}");
        }
    }

    // ------------------------------------------------------------------
    // Finding the tags in a file
    // ------------------------------------------------------------------

    fn read(bytes: Vec<u8>, file_type: FileType) -> TlanFrames {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f");
        std::fs::write(&path, bytes).expect("write");
        read_tlan_frames(&path, file_type).expect("read")
    }

    /// The refusal text for the file `bytes`, which must be refused.
    fn read_refusal(bytes: Vec<u8>, file_type: FileType) -> String {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f");
        std::fs::write(&path, bytes).expect("write");
        match read_tlan_frames(&path, file_type) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// What reading `split_tag`'s two frames takes from the budget: four
    /// bytes each (an encoding byte and `eng` / `fra`), and one value each.
    const TWO_SMALL_FRAMES: u64 = 2 * (FRAME_COST + 4) + 2 * VALUE_COST;

    fn split_tag(major: u8) -> Vec<u8> {
        tag(
            major,
            0,
            &[
                text_frame(major, b"TLAN", 0, b"eng"),
                text_frame(major, b"TLAN", 0, b"fra"),
            ],
            16,
        )
    }

    #[test]
    fn an_mp3_s_tags_are_found_after_leading_zero_bytes() {
        // Zero bytes first (lofty skips them), then the tags back to back,
        // then an MPEG frame header. One tag holding the languages: they
        // are read. A second tag after it - even one holding no language -
        // is found as well, so the save is refused (a language frame in a
        // file with two tags; the stand-in review of revision 9, M2).
        let mp3 = |tags: Vec<Vec<u8>>| {
            let mut bytes = vec![0u8; 5];
            for tag in tags {
                bytes.extend(tag);
            }
            bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            bytes.extend(vec![0u8; 100]);
            bytes
        };
        assert_eq!(
            read(mp3(vec![split_tag(4)]), FileType::Mpeg),
            several(&["eng", "fra"])
        );
        let title = tag(3, 0, &[text_frame(3, b"TIT2", 0, b"Title")], 0);
        let message = read_refusal(mp3(vec![split_tag(4), title]), FileType::Mpeg);
        assert!(message.contains("has 2 separate ID3v2 tags"), "{message}");
        assert!(message.contains("in 1 of them"), "{message}");
        let language = tag(3, 0, &[text_frame(3, b"TLAN", 0, b"deu")], 0);
        let message = read_refusal(mp3(vec![split_tag(4), language]), FileType::Mpeg);
        assert!(message.contains("in 2 of them"), "{message}");
    }

    #[test]
    fn a_footer_is_stepped_over_to_reach_the_next_tag() {
        // An ID3v2.4 tag with the footer flag (0x10), its 10-byte footer,
        // then a second tag. Both hold a language, so the save is refused —
        // which it could only be if the second tag was found past the footer
        // (missing it, one frame alone would need nothing).
        let mut first = tag(4, 0x10, &[text_frame(4, b"TLAN", 3, b"eng")], 0);
        let footer: Vec<u8> = [b"3DI".as_slice(), &first[3..10]].concat();
        first.extend(footer);
        let mut bytes = first;
        bytes.extend(tag(4, 0, &[text_frame(4, b"TLAN", 3, b"fra")], 0));
        let message = read_refusal(bytes, FileType::Aac);
        assert!(message.contains("has 2 separate ID3v2 tags"), "{message}");
    }

    /// A RIFF (`big_endian` false) or FORM file: the 12-byte header, an
    /// odd-sized chunk (so its padding byte must be stepped over), then
    /// an ID3 chunk holding `tag`.
    fn chunk_file(big_endian: bool, id3_name: &[u8; 4], tag: Vec<u8>) -> Vec<u8> {
        let size = |n: usize| {
            let n = u32::try_from(n).expect("fits");
            if big_endian {
                n.to_be_bytes()
            } else {
                n.to_le_bytes()
            }
        };
        let mut body: Vec<u8> = if big_endian {
            b"AIFF".to_vec()
        } else {
            b"WAVE".to_vec()
        };
        body.extend_from_slice(b"odd ");
        body.extend_from_slice(&size(3));
        body.extend_from_slice(&[1, 2, 3, 0]);
        body.extend_from_slice(id3_name);
        body.extend_from_slice(&size(tag.len()));
        body.extend_from_slice(&tag);
        let mut out: Vec<u8> = if big_endian {
            b"FORM".to_vec()
        } else {
            b"RIFF".to_vec()
        };
        out.extend_from_slice(&size(body.len()));
        out.extend(body);
        out
    }

    #[test]
    fn a_wav_or_aiff_file_s_id3_chunk_is_found() {
        assert_eq!(
            read(chunk_file(false, b"id3 ", split_tag(4)), FileType::Wav),
            several(&["eng", "fra"])
        );
        assert_eq!(
            read(chunk_file(true, b"ID3 ", split_tag(3)), FileType::Aiff),
            several(&["eng", "fra"])
        );
    }

    #[test]
    fn language_frames_in_two_id3_chunks_refuse_the_save() {
        // The stand-in review of revision 8's WAV: an `id3 ` chunk holding
        // `eng`, then an `ID3 ` chunk holding `fra`. Each chunk alone holds
        // one frame; together they are refused (and so for AIFF).
        for (big_endian, file_type) in [(false, FileType::Wav), (true, FileType::Aiff)] {
            let mut bytes = chunk_file(
                big_endian,
                b"id3 ",
                tag(4, 0, &[text_frame(4, b"TLAN", 0, b"eng")], 0),
            );
            let second = tag(4, 0, &[text_frame(4, b"TLAN", 0, b"fra")], 0);
            let size = u32::try_from(second.len()).expect("fits");
            bytes.extend_from_slice(b"ID3 ");
            bytes.extend_from_slice(&if big_endian {
                size.to_be_bytes()
            } else {
                size.to_le_bytes()
            });
            bytes.extend(second);
            // The file's own size, at bytes 4 to 8, now covers both chunks.
            let file_size = u32::try_from(bytes.len() - 8).expect("fits");
            bytes[4..8].copy_from_slice(&if big_endian {
                file_size.to_be_bytes()
            } else {
                file_size.to_le_bytes()
            });
            let message = read_refusal(bytes, file_type);
            assert!(
                message.contains("has 2 separate ID3v2 tags"),
                "{file_type:?}: {message}"
            );
        }
    }

    // ------------------------------------------------------------------
    // How much is read and held (Codex's catch-up review of revisions
    // 8-10, finding 10)
    // ------------------------------------------------------------------

    /// A file that is mostly zeros, made up as it is read, so a test can
    /// hold a "file" of hundreds of megabytes without making one: `parts`
    /// are the bytes that are not zero, each at its place. It counts every
    /// byte read, and the largest single read asked of it - a reader that
    /// pulled a whole chunk or tag into memory would ask for a large one.
    struct Sparse {
        len: u64,
        parts: Vec<(u64, Vec<u8>)>,
        pos: u64,
        read_total: u64,
        largest_read: usize,
    }

    impl Sparse {
        fn new(len: u64, parts: Vec<(u64, Vec<u8>)>) -> Self {
            Sparse {
                len,
                parts,
                pos: 0,
                read_total: 0,
                largest_read: 0,
            }
        }
    }

    impl Read for Sparse {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let left = self.len.saturating_sub(self.pos);
            let n = usize::try_from(left).map_or(buf.len(), |left| left.min(buf.len()));
            let out = &mut buf[..n];
            out.fill(0);
            for (at, bytes) in &self.parts {
                let start = self.pos.max(*at);
                let end = (self.pos + n as u64).min(at + bytes.len() as u64);
                if start < end {
                    out[(start - self.pos) as usize..(end - self.pos) as usize]
                        .copy_from_slice(&bytes[(start - at) as usize..(end - at) as usize]);
                }
            }
            self.pos += n as u64;
            self.read_total += n as u64;
            self.largest_read = self.largest_read.max(n);
            Ok(n)
        }
    }

    impl Seek for Sparse {
        fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
            let moved = match to {
                SeekFrom::Start(n) => Some(n),
                SeekFrom::End(by) => self.len.checked_add_signed(by),
                SeekFrom::Current(by) => self.pos.checked_add_signed(by),
            };
            self.pos = moved.ok_or_else(|| std::io::Error::other("before the start"))?;
            Ok(self.pos)
        }
    }

    /// The merged result for the file `sparse`, its tags found at
    /// `places`, with `budget`.
    fn read_sparse(
        sparse: &mut Sparse,
        places: TagPlaces,
        budget: &mut Budget,
    ) -> Result<TlanFrames, MetadataError> {
        tlan_frames_in_file(&mut BufReader::new(sparse), &places, budget)
    }

    /// The start of a WAV (`big_endian` false) or AIFF file whose first
    /// chunk is an `ID3 ` chunk of `chunk_size` bytes: the 20 bytes before
    /// the tag.
    fn chunk_header(big_endian: bool, chunk_size: u32) -> Vec<u8> {
        let size = |n: u32| {
            if big_endian {
                n.to_be_bytes()
            } else {
                n.to_le_bytes()
            }
        };
        let mut out: Vec<u8> = if big_endian {
            b"FORM".to_vec()
        } else {
            b"RIFF".to_vec()
        };
        out.extend_from_slice(&size(chunk_size.saturating_add(12)));
        out.extend_from_slice(if big_endian { b"AIFF" } else { b"WAVE" });
        out.extend_from_slice(b"ID3 ");
        out.extend_from_slice(&size(chunk_size));
        out
    }

    #[test]
    fn an_id3_chunk_of_padding_is_never_read_whole() {
        // Codex's catch-up review of revisions 8-10, finding 10: a WAV or
        // AIFF file whose ID3 chunk holds a small tag and then hundreds of
        // megabytes of padding. The whole chunk was read into memory (a
        // 400 MiB one took the program to 410 MiB - measured); now only
        // the tag is read, and only its language frames are held.
        let chunk = 256 * 1024 * 1024;
        for (big_endian, places) in [
            (false, TagPlaces::InChunks { big_endian: false }),
            (true, TagPlaces::InChunks { big_endian: true }),
        ] {
            let mut file = Sparse::new(
                20 + u64::from(chunk),
                vec![(0, chunk_header(big_endian, chunk)), (20, split_tag(4))],
            );
            let mut budget = Budget::new(LANGUAGE_BYTES_BUDGET);
            let found = read_sparse(&mut file, places, &mut budget).expect("read");
            assert_eq!(found, several(&["eng", "fra"]));
            assert!(
                file.read_total <= 256 * 1024,
                "{} bytes read of a 256 MiB chunk",
                file.read_total
            );
            assert!(file.largest_read <= SCAN_BLOCK, "{}", file.largest_read);
            assert_eq!(
                budget.taken, TWO_SMALL_FRAMES,
                "only the two frames' contents (and their fixed charges) are held"
            );
        }
    }

    #[test]
    fn a_tag_of_padding_is_looked_at_a_block_at_a_time() {
        // The tag itself says it is 32 MiB long - two language frames,
        // then padding to the end. It is read (to look for the names), but
        // a block at a time, never held whole.
        let mut tag = split_tag(4);
        let size = 32 * 1024 * 1024;
        tag[6..10].copy_from_slice(&to_synchsafe(size));
        let mut file = Sparse::new(
            20 + 10 + u64::from(size),
            vec![(0, chunk_header(false, 10 + size)), (20, tag)],
        );
        let mut budget = Budget::new(LANGUAGE_BYTES_BUDGET);
        let found = read_sparse(
            &mut file,
            TagPlaces::InChunks { big_endian: false },
            &mut budget,
        );
        assert_eq!(found.expect("read"), several(&["eng", "fra"]));
        assert!(file.largest_read <= SCAN_BLOCK, "{}", file.largest_read);
        assert_eq!(budget.taken, TWO_SMALL_FRAMES);
    }

    #[test]
    fn many_large_tags_one_after_another_are_never_held() {
        // An MP3 with six 8 MiB tags, one after another - each mostly
        // padding - all read into memory and kept together until finding
        // 10. Now each is looked at a block at a time. With a language
        // frame in the first, the file is refused (two or more tags, M2);
        // with none, nothing needs merging. Either way nothing of the tags
        // is held but that one frame.
        let size: u32 = 8 * 1024 * 1024;
        for language in [true, false] {
            let mut parts = Vec::new();
            let mut at = 0u64;
            for n in 0..6 {
                let frames = if language && n == 0 {
                    vec![text_frame(4, b"TLAN", 0, b"eng")]
                } else {
                    vec![text_frame(4, b"TIT2", 0, b"Title")]
                };
                let mut tag = tag(4, 0, &frames, 0);
                tag[6..10].copy_from_slice(&to_synchsafe(size));
                parts.push((at, tag));
                at += 10 + u64::from(size);
            }
            parts.push((at, vec![0xFF, 0xFB, 0x90, 0x00]));
            let mut file = Sparse::new(at + 4, parts);
            let mut budget = Budget::new(LANGUAGE_BYTES_BUDGET);
            let found = read_sparse(&mut file, TagPlaces::AtStart, &mut budget);
            if language {
                let message = found.expect_err("refused").to_string();
                assert!(message.contains("has 6 separate ID3v2 tags"), "{message}");
                assert_eq!(
                    budget.taken,
                    FRAME_COST + 4 + VALUE_COST,
                    "the one frame's contents, and its fixed charges"
                );
            } else {
                assert_eq!(found.expect("read"), TlanFrames::AtMostOne);
                assert_eq!(budget.taken, 0);
            }
            assert!(file.largest_read <= SCAN_BLOCK, "{}", file.largest_read);
        }
    }

    #[test]
    fn a_name_across_the_join_of_two_blocks_is_counted_once() {
        // The cheap look reads a tag a block at a time, keeping the last
        // three bytes of each block with the next: a name across the join
        // is found, and a three-letter one ending just before it is not
        // counted a second time.
        for (major, name) in [(4u8, &b"TLAN"[..]), (3, b"TLA\0"), (2, b"TLA")] {
            for at in SCAN_BLOCK - 4..=SCAN_BLOCK {
                let mut body = vec![0u8; 2 * SCAN_BLOCK];
                body[at..at + name.len()].copy_from_slice(name);
                let mut tag = b"ID3".to_vec();
                tag.extend_from_slice(&[major, 0, 0]);
                tag.extend_from_slice(&to_synchsafe(u32::try_from(body.len()).expect("fits")));
                tag.extend(body);
                let (bytes, places) = listed(&[tag]);
                let look =
                    look_in(&mut BufReader::new(Cursor::new(bytes)), &places[0]).expect("read");
                assert_eq!(look.names, 1, "ID3v2.{major}, at byte {at}");
            }
        }
    }

    #[test]
    fn language_frames_past_the_budget_are_refused_in_plain_words() {
        // The budget is for the whole file: two frames of 600 KiB each fit
        // one at a time but not together.
        let half = vec![b'x'; 600 * 1024];
        let big = tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 0, &half),
                text_frame(4, b"TLAN", 0, &half),
            ],
            0,
        );
        let message = refusal(&[big]);
        assert!(
            message.contains(
                "need more memory to read than any list of languages does - over 1048576 bytes"
            ),
            "{message}"
        );
        assert!(message.contains("Nothing was written"), "{message}");
        // Exactly at the budget is read; one byte past it is not (the two
        // frames below hold four bytes each - an encoding byte and `eng` /
        // `fra` - and one value each: `TWO_SMALL_FRAMES`).
        let (bytes, places) = listed(&[split_tag(4)]);
        let read_with = |limit| {
            tlan_frames_in_file(
                &mut BufReader::new(Cursor::new(bytes.clone())),
                &TagPlaces::Listed(places.clone()),
                &mut Budget::new(limit),
            )
        };
        assert_eq!(
            read_with(TWO_SMALL_FRAMES).expect("read"),
            several(&["eng", "fra"])
        );
        assert!(read_with(TWO_SMALL_FRAMES - 1).is_err());
    }

    #[test]
    fn other_file_types_and_untagged_files_need_nothing() {
        assert_eq!(read(split_tag(4), FileType::Flac), TlanFrames::AtMostOne);
        assert_eq!(
            read(vec![0xFF, 0xFB, 0x90, 0x00], FileType::Mpeg),
            TlanFrames::AtMostOne
        );
        assert_eq!(read(Vec::new(), FileType::Mpeg), TlanFrames::AtMostOne);
    }

    // What the budget is charged, and the caps on frames (Codex's review of
    // revision 11, finding 2). Each test fails if its charge or cap is
    // taken out: shown by planting each fault.

    /// An MP3 file at `path` whose one ID3v2.3 tag holds `count` empty
    /// `TLAN` frames - ten bytes of header each, nothing in them - written
    /// frame by frame, so the test itself never holds the file.
    fn write_empty_language_frames(path: &Path, count: u32) {
        use std::io::Write;
        let mut out = std::io::BufWriter::new(File::create(path).expect("create"));
        out.write_all(b"ID3\x03\x00\x00").expect("write");
        out.write_all(&to_synchsafe(count * 10)).expect("write");
        for _ in 0..count {
            out.write_all(b"TLAN\0\0\0\0\0\0").expect("write");
        }
        out.flush().expect("flush");
    }

    #[test]
    fn a_million_empty_language_frames_are_refused_not_kept() {
        // Each used to keep an empty list and be charged nothing: reading a
        // file of a million took the program from 2 MB to 52 MB, and the
        // file was let through. Now each is charged and none is kept, and
        // the tag is refused at the 257th - read here from a real file of
        // ten megabytes. (Its peak memory, this test run on its own under
        // `/usr/bin/time -l`, is recorded with revision 12.)
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty-frames.mp3");
        write_empty_language_frames(&path, 1_000_000);
        let mut budget = Budget::new(LANGUAGE_BYTES_BUDGET);
        let found = tlan_frames_in_file(
            &mut BufReader::new(File::open(&path).expect("open")),
            &TagPlaces::AtStart,
            &mut budget,
        );
        let message = found.expect_err("refused").to_string();
        assert!(
            message.contains("holds more than 256 language frames"),
            "{message}"
        );
        assert!(message.contains("Nothing was written"), "{message}");
        assert_eq!(budget.frames, MAX_FRAMES_PER_TAG, "no frame past the cap");
        assert_eq!(budget.taken, MAX_FRAMES_PER_TAG * FRAME_COST);
        // And through the reader every save uses.
        let message = match read_tlan_frames(&path, FileType::Mpeg) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(
            message.contains("holds more than 256 language frames"),
            "{message}"
        );
    }

    #[test]
    fn an_empty_language_frame_is_passed_over_and_nothing_kept_for_it() {
        // lofty passes over a frame with nothing in it - it is not a
        // language frame to lofty - so one real frame and two empty ones
        // need no merging, and two real ones with an empty one between
        // them merge as two. Each empty one is still charged.
        let empty = frame(4, b"TLAN", 0, &[]);
        let one_real = tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 0, b"eng"),
                empty.clone(),
                empty.clone(),
            ],
            0,
        );
        let (bytes, places) = listed(&[one_real]);
        let mut budget = Budget::new(LANGUAGE_BYTES_BUDGET);
        let found = tlan_frames_in_file(
            &mut BufReader::new(Cursor::new(bytes)),
            &TagPlaces::Listed(places),
            &mut budget,
        );
        assert_eq!(found.expect("read"), TlanFrames::AtMostOne);
        assert_eq!(budget.frames, 3, "the empty ones are counted");
        assert_eq!(
            budget.taken,
            3 * FRAME_COST + 4 + VALUE_COST,
            "every frame is charged; only the real one has text and a value"
        );
        let two_real = tag(
            4,
            0,
            &[
                text_frame(4, b"TLAN", 0, b"eng"),
                empty,
                text_frame(4, b"TLAN", 0, b"fra"),
            ],
            0,
        );
        assert_eq!(
            tlan_frames_in_tags(&[two_real]).expect("read"),
            several(&["eng", "fra"])
        );
    }

    #[test]
    fn countless_tiny_values_are_refused_before_they_are_kept() {
        // One frame of half a million one-letter values, inside the 1 MiB
        // of text, and a second frame: it used to be read whole - a string
        // for every value, 26 MB at the peak. Now the values are counted,
        // and charged, before one is kept.
        let mut text = Vec::with_capacity(1_000_000);
        for _ in 0..500_000 {
            text.extend_from_slice(b"a\0");
        }
        let big = tag(
            3,
            0,
            &[
                text_frame(3, b"TLAN", 0, &text),
                text_frame(3, b"TLAN", 0, b"eng"),
            ],
            0,
        );
        let message = refusal(&[big]);
        assert!(
            message.contains("need more memory to read than any list of languages does"),
            "{message}"
        );
    }

    #[test]
    fn more_language_frames_than_a_tag_or_a_file_may_hold_are_refused() {
        // 256 small language frames in one tag are read; 257 are not.
        let frames = |count: usize| -> Vec<Vec<u8>> {
            (0..count)
                .map(|_| text_frame(4, b"TLAN", 0, b"e"))
                .collect()
        };
        let at_cap = usize::try_from(MAX_FRAMES_PER_TAG).expect("fits");
        assert_eq!(
            tlan_frames_in_tags(&[tag(4, 0, &frames(at_cap), 0)]).expect("read"),
            several(&["e"])
        );
        let message = refusal(&[tag(4, 0, &frames(at_cap + 1), 0)]);
        assert!(
            message.contains("one of its tags holds more than 256 language frames"),
            "{message}"
        );
        // Five tags of 250 each: within each tag's cap, past the file's
        // (1,024) - refused at that point, before the file's other tags
        // are read.
        let tags: Vec<Vec<u8>> = (0..5).map(|_| tag(4, 0, &frames(250), 0)).collect();
        let (bytes, places) = listed(&tags);
        let mut budget = Budget::new(LANGUAGE_BYTES_BUDGET);
        let found = tlan_frames_in_file(
            &mut BufReader::new(Cursor::new(bytes)),
            &TagPlaces::Listed(places),
            &mut budget,
        );
        let message = found.expect_err("refused").to_string();
        assert!(
            message.contains("its tags hold more than 1024 language frames in all"),
            "{message}"
        );
        assert_eq!(budget.frames, MAX_FRAMES_PER_FILE + 1);
    }
}
