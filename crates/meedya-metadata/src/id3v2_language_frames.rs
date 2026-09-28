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
// into the one frame lofty then saves. Nothing is deleted.
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
//   digits), an ID3v2.4 frame named `TLA` and a zero. Refusing leaves the
//   file exactly as it was; saving would have lost languages. (A file
//   split the way this crate split them before revision 6, and
//   `TagFile::save` still does — lofty's format-neutral save — has none of
//   these, so the case that actually happens is always read: checked on
//   real MP3, WAV and AIFF files split by `TagFile::save`.)
// - **The merge takes work in step with the number of values**: a set of
//   the values already kept sits beside the ordered list (`first_of_each`),
//   so a crafted frame holding 100,000 values costs 100,000 steps, not the
//   five billion comparisons searching the list for each value took until
//   Codex's review of revisions 5–7.
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
/// file, and every `ID3 ` chunk of a WAV or AIFF file. Other file types
/// give [`TlanFrames::AtMostOne`]: lofty does not write an ID3v2 tag into
/// them.
///
/// Fails with [`MetadataError::WriteError`] when the file seems to hold two
/// or more language frames but one of them cannot be read (see the top of this
/// file for which cases); the message says what, in plain words. A file
/// that cannot be read at all gives [`MetadataError::IoError`].
pub(crate) fn read_tlan_frames(
    path: &Path,
    file_type: FileType,
) -> Result<TlanFrames, MetadataError> {
    let tags = match file_type {
        FileType::Mpeg | FileType::Aac => tags_at_start(&mut open(path)?)?,
        FileType::Wav => tags_in_chunks(&mut open(path)?, false)?,
        FileType::Aiff => tags_in_chunks(&mut open(path)?, true)?,
        _ => return Ok(TlanFrames::AtMostOne),
    };
    tlan_frames_in_tags(&tags)
}

fn open(path: &Path) -> Result<BufReader<File>, MetadataError> {
    Ok(BufReader::new(File::open(path)?))
}

// ============================================================
// Finding the tags
// ============================================================

/// The ID3v2 tags at the start of an MP3 or AAC file, as lofty finds them:
/// zero bytes at the very start are skipped, then every tag that follows
/// directly on the one before. Each is returned from its `ID3` header to
/// the end of its body (a footer, if any, is left out).
fn tags_at_start(reader: &mut (impl Read + Seek)) -> Result<Vec<Vec<u8>>, MetadataError> {
    let mut byte = [0u8];
    loop {
        if reader.read(&mut byte)? == 0 {
            return Ok(Vec::new());
        }
        if byte[0] != 0 {
            break;
        }
    }
    reader.seek(SeekFrom::Current(-1))?;

    let mut tags = Vec::new();
    loop {
        let mut header = [0u8; 10];
        if !read_fully(reader, &mut header)? || !is_id3v2_header(&header) {
            break;
        }
        tags.push(read_tag_body(reader, header)?);
        if has_footer(&header) {
            reader.seek(SeekFrom::Current(10))?;
        }
    }
    Ok(tags)
}

/// Every ID3v2 tag in the `ID3 ` (or `id3 `) chunks of a WAV (`big_endian`
/// false: chunk sizes are little-endian) or AIFF (`big_endian` true) file,
/// in file order. Chunks are walked from byte 12, just after the
/// `RIFF`…`WAVE` or `FORM`…`AIFF` header; a chunk of odd size is followed
/// by one padding byte.
fn tags_in_chunks(
    reader: &mut (impl Read + Seek),
    big_endian: bool,
) -> Result<Vec<Vec<u8>>, MetadataError> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    let mut pos: u64 = 12;
    let mut tags = Vec::new();
    while pos + 8 <= file_len {
        reader.seek(SeekFrom::Start(pos))?;
        let mut chunk = [0u8; 8];
        reader.read_exact(&mut chunk)?;
        let size_bytes = [chunk[4], chunk[5], chunk[6], chunk[7]];
        let size = u64::from(if big_endian {
            u32::from_be_bytes(size_bytes)
        } else {
            u32::from_le_bytes(size_bytes)
        });
        if matches!(&chunk[..4], b"ID3 " | b"id3 ") {
            // The chunk holds one ID3v2 tag from its first byte; the tag
            // is read from the chunk's own bytes only, as lofty reads it.
            let mut content = Vec::new();
            reader.by_ref().take(size).read_to_end(&mut content)?;
            if let Ok(header) = <[u8; 10]>::try_from(content.get(..10).unwrap_or_default()) {
                if is_id3v2_header(&header) {
                    let tag_len =
                        10 + synchsafe([header[6], header[7], header[8], header[9]]) as usize;
                    content.truncate(tag_len);
                    tags.push(content);
                }
            }
        }
        pos += 8 + size + (size & 1);
    }
    Ok(tags)
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

/// The tag whose 10-byte `header` has just been read: the header followed
/// by the body its size names — or as much of the body as the file holds;
/// a frame then running past the end is found when the frames are walked.
fn read_tag_body(reader: &mut impl Read, header: [u8; 10]) -> Result<Vec<u8>, MetadataError> {
    let size = synchsafe([header[6], header[7], header[8], header[9]]);
    let mut tag = header.to_vec();
    reader.take(u64::from(size)).read_to_end(&mut tag)?;
    Ok(tag)
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

/// How many times `needle` occurs in `haystack`.
fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// How many times the name of a language frame — in any form lofty reads
/// as one in a tag of this version (see [`language_frame_names`]) — occurs
/// in one ID3v2 `tag`'s body: the cheap look at the top of this file.
fn language_names_in(tag: &[u8]) -> usize {
    language_frame_names(tag[3])
        .iter()
        .map(|name| occurrences(&tag[10..], name))
        .sum()
}

/// Whether one ID3v2 `tag` is an ID3v2.4 tag whose body holds the bytes of
/// [`OLD_NAME_IN_V2_4`] — a frame that may refuse the save on its own.
fn may_hold_old_name_in_v2_4(tag: &[u8]) -> bool {
    tag[3] >= 4 && occurrences(&tag[10..], OLD_NAME_IN_V2_4) > 0
}

/// The merged result for `tags` (each an ID3v2 tag from its header on);
/// see [`read_tlan_frames`].
fn tlan_frames_in_tags(tags: &[Vec<u8>]) -> Result<TlanFrames, MetadataError> {
    // The cheap look (top of this file): every form of the name, counted
    // in each tag's body. One old-named frame in an ID3v2.4 tag is enough
    // on its own to need a look, since it refuses the save by itself.
    let named_total: usize = tags.iter().map(|tag| language_names_in(tag)).sum();
    let old_name = tags.iter().any(|tag| may_hold_old_name_in_v2_4(tag));
    if named_total < 2 && !old_name {
        return Ok(TlanFrames::AtMostOne);
    }

    let mut frames = Vec::new();
    for tag in tags
        .iter()
        .filter(|tag| language_names_in(tag) > 0 || may_hold_old_name_in_v2_4(tag))
    {
        let found = tlan_frames_in_tag(tag).map_err(|problem| {
            MetadataError::WriteError(format!(
                "this file's ID3v2 tag may hold its languages in more than one language frame \
                 (named TLAN, or TLA in older tags), which lofty - the library this crate saves \
                 files with - does not read whole, so saving would lose languages. They are \
                 normally merged into one frame first, but {problem}, so they cannot be read \
                 here. Nothing was written. (For most such files, saving once with mutagen, \
                 which reads every language frame and saves them as one, lets this library \
                 write to them.)"
            ))
        })?;
        frames.extend(found);
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

/// The values of every language frame in one ID3v2 `tag` (from its header
/// on), one list per frame, in order — or, as plain words for the refusal
/// message, why they cannot be read.
fn tlan_frames_in_tag(tag: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let major = tag[3];
    let tag_flags = tag[5];
    let body = &tag[10..];
    if tag_flags & 0x80 != 0 {
        return Err(
            "the file's ID3v2 tag is \"unsynchronised\" (an old encoding this library \
                    does not undo)"
                .to_string(),
        );
    }
    if major >= 3 && tag_flags & 0x40 != 0 {
        return Err(
            "the file's ID3v2 tag has an \"extended header\", which this library does not read"
                .to_string(),
        );
    }

    // ID3v2.2: three-letter name, three-byte size, no flags. ID3v2.3 and
    // 2.4: four-byte name, four-byte size, two flag bytes.
    let (name_len, header_len) = if major == 2 { (3, 6) } else { (4, 10) };

    let mut frames = Vec::new();
    let mut pos = 0usize;
    while pos + header_len <= body.len() {
        let header = &body[pos..pos + header_len];
        if header[0] == 0 {
            // Padding: no more frames (as lofty reads it).
            break;
        }
        let name = &header[..name_len];
        let is_language = match frame_name(major, header) {
            FrameName::Language => true,
            FrameName::Other => false,
            FrameName::Invalid => {
                return Err(format!(
                    "a frame {pos} bytes into the tag has no valid name, so the frames after it \
                     cannot be found"
                ))
            }
            FrameName::OldLanguageNameInV2_4 => {
                return Err(format!(
                    "a frame {pos} bytes into the tag is named TLA followed by a zero byte - \
                     the old ID3v2.2 name for the language frame - inside an ID3v2.4 tag, which \
                     other programs (mutagen among them) read as a language but lofty does not: \
                     saving would turn it into an ordinary text frame named TLA"
                ))
            }
        };
        let size = match major {
            2 => u32::from_be_bytes([0, header[3], header[4], header[5]]),
            3 => u32::from_be_bytes([header[4], header[5], header[6], header[7]]),
            _ => synchsafe([header[4], header[5], header[6], header[7]]),
        };
        let start = pos + header_len;
        let end = start
            .checked_add(size as usize)
            .filter(|end| *end <= body.len())
            .ok_or_else(|| {
                format!(
                    "the frame {} {pos} bytes into the tag runs past the end of the tag",
                    String::from_utf8_lossy(name)
                )
            })?;
        if is_language {
            let content = frame_content(&body[start..end], major, header)?;
            frames.push(frame_values(content, major)?);
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
/// characters taken off, then split at each null character.
fn frame_values(content: &[u8], major: u8) -> Result<Vec<String>, String> {
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
                                .to_string(),
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
            ))
        }
    };
    Ok(decoded
        .trim_end_matches('\0')
        .split('\0')
        .map(str::to_string)
        .collect())
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
    fn frames_in_two_tags_are_merged_in_file_order() {
        let first = tag(3, 0, &[text_frame(3, b"TLAN", 0, b"eng")], 0);
        let second = tag(4, 0, &[text_frame(4, b"TLAN", 3, b"fra")], 0);
        assert_eq!(
            tlan_frames_in_tags(&[first, second]).expect("read"),
            several(&["eng", "fra"])
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
        let cases: [(u8, &[u8], FrameName); 14] = [
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

    /// The work `first_of_each` does on the crafted tag's values.
    fn merge_work(count: usize) -> u64 {
        let frames = tlan_frames_in_tag(&crafted_tag(count)).expect("read");
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
        let TlanFrames::Several(values) =
            tlan_frames_in_tags(&[crafted_tag(100_000)]).expect("read")
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
        // Zero bytes first (lofty skips them), then two tags back to back
        // — the second's frames count too — then an MPEG frame header.
        let mut bytes = vec![0u8; 5];
        bytes.extend(split_tag(4));
        bytes.extend(tag(3, 0, &[text_frame(3, b"TLAN", 0, b"deu")], 0));
        bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        bytes.extend(vec![0u8; 100]);
        assert_eq!(read(bytes, FileType::Mpeg), several(&["eng", "fra", "deu"]));
    }

    #[test]
    fn a_footer_is_stepped_over_to_reach_the_next_tag() {
        // An ID3v2.4 tag with the footer flag (0x10), its 10-byte footer,
        // then a second tag.
        let mut first = tag(4, 0x10, &[text_frame(4, b"TLAN", 3, b"eng")], 0);
        let footer: Vec<u8> = [b"3DI".as_slice(), &first[3..10]].concat();
        first.extend(footer);
        let mut bytes = first;
        bytes.extend(tag(4, 0, &[text_frame(4, b"TLAN", 3, b"fra")], 0));
        assert_eq!(read(bytes, FileType::Aac), several(&["eng", "fra"]));
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
    fn other_file_types_and_untagged_files_need_nothing() {
        assert_eq!(read(split_tag(4), FileType::Flac), TlanFrames::AtMostOne);
        assert_eq!(
            read(vec![0xFF, 0xFB, 0x90, 0x00], FileType::Mpeg),
            TlanFrames::AtMostOne
        );
        assert_eq!(read(Vec::new(), FileType::Mpeg), TlanFrames::AtMostOne);
    }
}
