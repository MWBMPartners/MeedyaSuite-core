// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Checking an M4A save atom by atom before it replaces the file (#102).
// =====================================================================
//
// Why this exists. Every M4A write in `tag_io` goes through lofty's
// format-neutral `Tag` (the language atom apart), and that route changes
// atoms nobody asked it to change. Revision 8 guarded against it by LISTING
// the ways it loses data (several cover images, several values in a text
// atom, a colon inside a freeform name…). The stand-in review of revision 8
// then found more the list missed, on real files written by mutagen: a
// title-only save turned the flag atoms `pgap`, `hdvd` and `shwm` from a
// number into the text "0" (mutagen then reads `pgap` as TRUE, and an
// `hdvd` of 2 would read back as 1); it renamed freeform atoms to lofty's
// own spelling (`----:com.apple.iTunes:Mood` became `…:MOOD`, `…:isrc`
// became `…:ISRC`); it dropped a `covr` atom whose second value had an
// unusual data type, losing all the cover art; and, by lofty's own code, it
// drops a `gnre` atom beside a `©gen` one and a `plID` value that is not
// eight bytes long. A list of cases will always miss some.
//
// So the save is no longer trusted at all. `tag_io` makes it on a TEMPORARY
// COPY of the file, and this module reads the `ilst` atoms (where an M4A
// file keeps its tags) of both the original and the copy straight from their
// bytes, and compares them:
//
// - every atom the caller did NOT ask to change must be in the copy exactly
//   as it is in the original — same name (a freeform atom's `mean` and
//   `name` spelled exactly the same), same values in the same order, each
//   with the same data type, locale and bytes: the whole atom, byte for byte;
// - every atom the caller DID ask to change must hold exactly what was asked
//   for — including the language atom — and nothing else of that name may
//   be left beside it.
//
// Only then does the copy replace the original, in one step (a rename, so a
// crash can never leave half a file). Anything else refuses the save with a
// plain message naming each atom that would have changed, and the original
// is never touched. The comparison is the guard: nothing here has to know
// in advance HOW lofty would change an atom.
//
// What it compares, and what it does not:
//
// - It reads what lofty reads and writes: the `ilst` atoms inside
//   `moov` → `udta` → `meta`, in every `udta` and `meta` there (lofty reads
//   the first `meta` of every `udta` and writes into the first), in file
//   order. `free` and `skip` atoms inside `ilst` are padding, holding no
//   metadata, and lofty drops them on reading — they are left out.
// - It does NOT compare anything outside `ilst`: the audio, chapters, other
//   `udta` atoms, and the sample offsets lofty corrects when the tags grow
//   or shrink are left to lofty. (Chapters were checked on the stand-in
//   reviewer's real file and survive a save.)
// - Atoms are compared by name; the ORDER of differently named atoms is
//   not compared, because lofty's save moves an atom it rewrites to the end
//   and no reader depends on that order. The order of atoms of the SAME
//   name, and of the values inside one atom, is compared.
//
// It is deliberately small and bounded: atom sizes are checked against the
// atom they sit in before anything is read, at most `MAX_ILST_BYTES` of
// `ilst` is read into memory, and anything it cannot read (a damaged size,
// an atom running past its parent) refuses the save rather than guessing.
//
// How the copy is made and how it takes the original's place — and what
// that costs compared with writing into the file (a hard link keeps the old
// tags, the folder must be writable, and more) — is in `save_by_copy`. A
// symbolic link is followed first, so the file it points to is the one
// replaced.

use std::collections::{BTreeMap, HashSet};
#[cfg(test)]
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
#[cfg(test)]
use std::path::Path;

use lofty::mp4::{AtomData, AtomIdent, DataType};
use lofty::picture::MimeType;

use crate::error::MetadataError;

/// At most this many bytes of `ilst` atoms are read from one file (lofty
/// itself will not read an `ilst` larger than 16 MiB, its own default
/// limit, so a file that got this far never comes near it).
const MAX_ILST_BYTES: u64 = 64 * 1024 * 1024;

/// How an atom inside `ilst` is named: four bytes (`©nam`, `covr`), or, for
/// a freeform `----` atom, its `mean` and `name` exactly as the file spells
/// them. Two atoms with the same key are "the same atom" for the comparison.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RawKey {
    Fourcc([u8; 4]),
    Freeform { mean: Vec<u8>, name: Vec<u8> },
}

impl RawKey {
    /// The key lofty writes an atom named `ident` under.
    pub(crate) fn of(ident: &AtomIdent<'_>) -> Self {
        match ident {
            AtomIdent::Fourcc(fourcc) => RawKey::Fourcc(*fourcc),
            AtomIdent::Freeform { mean, name } => RawKey::Freeform {
                mean: mean.as_bytes().to_vec(),
                name: name.as_bytes().to_vec(),
            },
        }
    }

    /// The name as a person reads it: `©nam`, or `----:com.apple.iTunes:MOOD`.
    pub(crate) fn display(&self) -> String {
        match self {
            // A four-byte name is Latin-1 by convention: byte 0xA9 is `©`.
            RawKey::Fourcc(fourcc) => fourcc.iter().map(|byte| char::from(*byte)).collect(),
            RawKey::Freeform { mean, name } => format!(
                "----:{}:{}",
                String::from_utf8_lossy(mean),
                String::from_utf8_lossy(name)
            ),
        }
    }
}

/// One value of an atom: one `data` child, as the file holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawValue {
    /// The four-byte type indicator: a "type set" byte (0 for the usual
    /// set) and the 24-bit type — 1 for UTF-8 text, 13 for a JPEG image,
    /// 21 for a whole number, 0 for "implicit" data such as a track number.
    pub(crate) type_indicator: u32,
    /// The four-byte locale (usually 0).
    pub(crate) locale: u32,
    /// The value itself.
    pub(crate) value: Vec<u8>,
}

/// One atom inside `ilst`, read straight from the file's bytes.
#[derive(Clone, Debug)]
pub(crate) struct RawAtom {
    pub(crate) key: RawKey,
    /// The whole atom, header included, exactly as the file holds it — what
    /// an atom nobody asked to change is compared by.
    pub(crate) bytes: Vec<u8>,
    /// Its values (`data` children), in order.
    pub(crate) values: Vec<RawValue>,
    /// How many of its parts are not a value, nor (for a freeform atom) its
    /// one `mean` and one `name`: a part lofty would not keep. An atom whose
    /// parts cannot be read at all counts one here, so it never passes as
    /// holding what was asked for.
    pub(crate) other_parts: usize,
}

/// For each atom the caller asked to change, the atoms the saved file must
/// hold under that name — each a list of values, in order. An empty list
/// means none may be left.
pub(crate) type Expected = BTreeMap<RawKey, Vec<Vec<RawValue>>>;

// ============================================================
// Reading the atoms
// ============================================================

/// Every atom inside the `ilst` atoms of the MP4 file at `path` (see the top
/// of this file for which `ilst` atoms), in file order: [`read_ilst_atoms_from`]
/// on the file at `path`. Used by the tests; the save itself reads through
/// the handles it holds.
#[cfg(test)]
pub(crate) fn read_ilst_atoms(path: &Path) -> Result<Vec<RawAtom>, MetadataError> {
    read_ilst_atoms_from(&mut File::open(path)?)
}

/// Every atom inside the `ilst` atoms of the MP4 file `file` reads (see the
/// top of this file for which `ilst` atoms), in file order. A file with no
/// `moov` atom, or no tags, gives none. `file` may be at any position; it
/// is read from its start.
///
/// Fails with [`MetadataError::WriteError`] when the file's atoms cannot be
/// read safely — a size that does not fit, more than `MAX_ILST_BYTES` of
/// tags — and with [`MetadataError::IoError`] when the file cannot be read.
pub(crate) fn read_ilst_atoms_from(
    file: &mut (impl Read + Seek),
) -> Result<Vec<RawAtom>, MetadataError> {
    read_ilst_atoms_within(file, MAX_ILST_BYTES)
}

/// The message for a file whose tags cannot be read safely.
pub(crate) fn unreadable(why: String) -> MetadataError {
    MetadataError::WriteError(format!(
        "this M4A file's tags cannot be checked before saving, because {why}. Nothing \
         was written. (Issue #102: every M4A save is checked against the original before it \
         replaces it, and a file that cannot be read that way is not saved.)"
    ))
}

/// [`read_ilst_atoms_from`], reading at most `limit` bytes of `ilst`
/// atoms (a parameter only so a test can show the limit is kept, without
/// making a 64 MiB file).
fn read_ilst_atoms_within(
    file: &mut (impl Read + Seek),
    limit: u64,
) -> Result<Vec<RawAtom>, MetadataError> {
    let mut reader = BufReader::new(file);
    let file_len = reader.seek(SeekFrom::End(0))?;

    // lofty reads the first `moov` atom only, and so does this: the atoms
    // at the top of the file are walked up to it and no further.
    let Some(moov) = first_moov(&mut reader, file_len).map_err(unreadable)? else {
        return Ok(Vec::new());
    };

    let mut atoms = Vec::new();
    let mut ilst_bytes = 0u64;
    let moov_children =
        boxes_in_file(&mut reader, moov.body_start, moov.end).map_err(unreadable)?;
    for udta in moov_children.iter().filter(|b| &b.name == b"udta") {
        let udta_children =
            boxes_in_file(&mut reader, udta.body_start, udta.end).map_err(unreadable)?;
        for meta in udta_children.iter().filter(|b| &b.name == b"meta") {
            let body_start = meta.body_start + meta_version_len(&mut reader, meta)?;
            let meta_children =
                boxes_in_file(&mut reader, body_start, meta.end).map_err(unreadable)?;
            for ilst in meta_children.iter().filter(|b| &b.name == b"ilst") {
                let len = ilst.end - ilst.body_start;
                ilst_bytes += len;
                if ilst_bytes > limit {
                    return Err(unreadable(format!(
                        "its tags take more than {} MiB",
                        limit / (1024 * 1024)
                    )));
                }
                reader.seek(SeekFrom::Start(ilst.body_start))?;
                let mut body = Vec::new();
                reader.by_ref().take(len).read_to_end(&mut body)?;
                atoms_in_ilst(&body, &mut atoms).map_err(unreadable)?;
            }
        }
    }
    Ok(atoms)
}

/// The first `moov` atom at the top of the file, or `None` when there is
/// none. Only the atoms before it are read (their headers only).
pub(crate) fn first_moov(
    reader: &mut (impl Read + Seek),
    file_len: u64,
) -> Result<Option<BoxAt>, String> {
    let mut pos = 0;
    while pos + 8 <= file_len {
        let Some(found) = boxes_in_file_from(reader, pos, file_len, true)?.pop() else {
            return Ok(None);
        };
        if &found.name == b"moov" {
            return Ok(Some(found));
        }
        pos = found.end;
    }
    Ok(None)
}

/// Where one atom ("box") sits in the file: its name, where its contents
/// start (just after its header), and where it ends.
pub(crate) struct BoxAt {
    pub(crate) name: [u8; 4],
    pub(crate) body_start: u64,
    pub(crate) end: u64,
}

/// Every atom between `start` and `end` of the file, reading only their
/// headers. An atom's header is a four-byte size and a four-byte name; a
/// size of 1 means a larger size follows in eight more bytes, and a size of
/// 0 means "to the end of the atom around it". A size that does not fit
/// inside `start..end` is refused (as plain words), never guessed at.
pub(crate) fn boxes_in_file(
    reader: &mut (impl Read + Seek),
    start: u64,
    end: u64,
) -> Result<Vec<BoxAt>, String> {
    boxes_in_file_from(reader, start, end, false)
}

/// [`boxes_in_file`], stopping after the first atom when `just_one`.
fn boxes_in_file_from(
    reader: &mut (impl Read + Seek),
    start: u64,
    end: u64,
    just_one: bool,
) -> Result<Vec<BoxAt>, String> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos.saturating_add(8) <= end {
        reader
            .seek(SeekFrom::Start(pos))
            .map_err(|e| format!("it could not be read ({e})"))?;
        let mut header = [0u8; 8];
        reader
            .read_exact(&mut header)
            .map_err(|e| format!("it could not be read ({e})"))?;
        let name = [header[4], header[5], header[6], header[7]];
        let (header_len, size) =
            match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
                1 => {
                    let mut large = [0u8; 8];
                    reader
                        .read_exact(&mut large)
                        .map_err(|e| format!("it could not be read ({e})"))?;
                    (16, u64::from_be_bytes(large))
                }
                0 => (8, end - pos),
                size => (8, u64::from(size)),
            };
        let atom_end = pos
            .checked_add(size)
            .filter(|atom_end| size >= header_len && *atom_end <= end)
            .ok_or_else(|| {
                format!(
                    "the atom {} at byte {pos} has a size that does not fit",
                    String::from_utf8_lossy(&name)
                )
            })?;
        out.push(BoxAt {
            name,
            body_start: pos + header_len,
            end: atom_end,
        });
        if just_one {
            break;
        }
        pos = atom_end;
    }
    Ok(out)
}

/// How many bytes of version and flags `meta` starts with: 4 for the usual
/// ("full") form, 0 when it is written as a plain atom — decided as lofty
/// decides it, by whether the four bytes after the first four name a child
/// atom lofty knows (lofty 0.22.4, `mp4/read/mod.rs`, `meta_is_full`).
pub(crate) fn meta_version_len(
    reader: &mut (impl Read + Seek),
    meta: &BoxAt,
) -> Result<u64, MetadataError> {
    if meta.end - meta.body_start < 8 {
        return Ok(4);
    }
    reader.seek(SeekFrom::Start(meta.body_start))?;
    let mut first = [0u8; 8];
    reader.read_exact(&mut first)?;
    Ok(match &first[4..] {
        b"hdlr" | b"ilst" | b"mhdr" | b"ctry" | b"lang" => 0,
        _ => 4,
    })
}

/// One atom inside a buffer: its name, header length, and whole bytes.
struct BoxIn<'a> {
    name: [u8; 4],
    whole: &'a [u8],
    body: &'a [u8],
}

/// Every atom in `buffer`, back to back (the same size rules as
/// [`boxes_in_file`]).
fn boxes_in(buffer: &[u8]) -> Result<Vec<BoxIn<'_>>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= buffer.len() {
        let name = [
            buffer[pos + 4],
            buffer[pos + 5],
            buffer[pos + 6],
            buffer[pos + 7],
        ];
        let size32 = u32::from_be_bytes([
            buffer[pos],
            buffer[pos + 1],
            buffer[pos + 2],
            buffer[pos + 3],
        ]);
        let (header_len, size) = match size32 {
            1 => {
                let large = buffer
                    .get(pos + 8..pos + 16)
                    .ok_or_else(|| "an atom's size is cut short".to_string())?;
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(large);
                (16usize, u64::from_be_bytes(bytes))
            }
            0 => (8, (buffer.len() - pos) as u64),
            size => (8, u64::from(size)),
        };
        let end = usize::try_from(size)
            .ok()
            .and_then(|size| pos.checked_add(size))
            .filter(|end| size >= header_len as u64 && *end <= buffer.len())
            .ok_or_else(|| {
                format!(
                    "the atom {} inside the tags has a size that does not fit",
                    String::from_utf8_lossy(&name)
                )
            })?;
        out.push(BoxIn {
            name,
            whole: &buffer[pos..end],
            body: &buffer[pos + header_len..end],
        });
        pos = end;
    }
    Ok(out)
}

/// The atoms of one `ilst` atom's contents, added to `atoms` in order.
fn atoms_in_ilst(body: &[u8], atoms: &mut Vec<RawAtom>) -> Result<(), String> {
    for item in boxes_in(body)? {
        if matches!(&item.name, b"free" | b"skip") {
            continue; // padding (see the top of this file)
        }
        atoms.push(raw_atom(&item));
    }
    Ok(())
}

/// One `ilst` atom read into its key, values and other parts.
fn raw_atom(item: &BoxIn<'_>) -> RawAtom {
    let mut atom = RawAtom {
        key: RawKey::Fourcc(item.name),
        bytes: item.whole.to_vec(),
        values: Vec::new(),
        other_parts: 0,
    };
    let Ok(parts) = boxes_in(item.body) else {
        // Its parts cannot be read: compared by its bytes only, and never
        // taken as holding what was asked for.
        atom.other_parts = 1;
        return atom;
    };
    let (mut mean, mut name) = (None, None);
    for part in parts {
        match &part.name {
            // `data`: four bytes of type indicator, four of locale, then
            // the value (lofty skips a `data` atom shorter than that).
            b"data" if part.body.len() >= 8 => atom.values.push(RawValue {
                type_indicator: u32::from_be_bytes([
                    part.body[0],
                    part.body[1],
                    part.body[2],
                    part.body[3],
                ]),
                locale: u32::from_be_bytes([
                    part.body[4],
                    part.body[5],
                    part.body[6],
                    part.body[7],
                ]),
                value: part.body[8..].to_vec(),
            }),
            // A freeform atom's `mean` and `name`: four bytes of version
            // and flags, then the text. The first of each names the atom.
            b"mean" if &item.name == b"----" && mean.is_none() && part.body.len() >= 4 => {
                mean = Some(part.body[4..].to_vec());
            }
            b"name" if &item.name == b"----" && name.is_none() && part.body.len() >= 4 => {
                name = Some(part.body[4..].to_vec());
            }
            _ => atom.other_parts += 1,
        }
    }
    if let (Some(mean), Some(name)) = (mean, name) {
        atom.key = RawKey::Freeform { mean, name };
    } else if &item.name == b"----" {
        // A freeform atom without its names: nothing lofty could keep.
        atom.other_parts += 1;
    }
    atom
}

// ============================================================
// What lofty writes for a value
// ============================================================

/// The value lofty writes for `data`, byte for byte (lofty 0.22.4,
/// `mp4/ilst/write.rs`: every value is written with the usual type set, the
/// type below, and a locale of 0) — or `None` for a kind of value whose
/// written form this does not reproduce, so a save asked to store one is
/// refused rather than passed unchecked. (Text as UTF-16 and unsigned
/// whole numbers are such kinds: lofty writes UTF-16 text as UTF-8 bytes
/// under the UTF-16 type, and unsigned numbers in a size of its own
/// choosing. Nothing this crate writes produces either.)
pub(crate) fn raw_value(data: &AtomData) -> Option<RawValue> {
    let (code, value): (DataType, Vec<u8>) = match data {
        AtomData::UTF8(text) => (DataType::Utf8, text.as_bytes().to_vec()),
        // A flag: one byte, 0 or 1, as a signed whole number.
        AtomData::Bool(flag) => (DataType::BeSignedInteger, vec![u8::from(*flag)]),
        AtomData::SignedInteger(number) => {
            (DataType::BeSignedInteger, number.to_be_bytes().to_vec())
        }
        AtomData::Unknown { code, data } => (*code, data.clone()),
        AtomData::Picture(picture) => {
            let code = match picture.mime_type() {
                None => DataType::Reserved,
                Some(MimeType::Gif) => DataType::Gif,
                Some(MimeType::Jpeg) => DataType::Jpeg,
                Some(MimeType::Png) => DataType::Png,
                Some(MimeType::Bmp) => DataType::Bmp,
                Some(_) => return None,
            };
            (code, picture.data().to_vec())
        }
        _ => return None,
    };
    Some(RawValue {
        type_indicator: u32::from(code),
        locale: 0,
        value,
    })
}

// ============================================================
// Comparing
// ============================================================

/// Every way the saved file's atoms (`saved`) differ from what was allowed:
/// an atom nobody asked to change that is not exactly as in the `original`,
/// or an asked-for atom (a key of `expected`) that does not hold exactly
/// what was asked. Each is one plain sentence; none means the save may
/// replace the original.
pub(crate) fn differences(
    original: &[RawAtom],
    saved: &[RawAtom],
    expected: &Expected,
) -> Vec<String> {
    let group = |atoms: &[RawAtom]| {
        let mut groups: BTreeMap<RawKey, Vec<RawAtom>> = BTreeMap::new();
        for atom in atoms {
            groups
                .entry(atom.key.clone())
                .or_default()
                .push(atom.clone());
        }
        groups
    };
    let before = group(original);
    let after = group(saved);

    // Every name, in the order a person meets it: the original's atoms,
    // then any the save adds, then any it was asked for.
    let mut seen = HashSet::new();
    let keys: Vec<&RawKey> = original
        .iter()
        .map(|atom| &atom.key)
        .chain(saved.iter().map(|atom| &atom.key))
        .chain(expected.keys())
        .filter(|key| seen.insert(*key))
        .collect();

    let none = Vec::new();
    let mut out = Vec::new();
    for key in keys {
        let was = before.get(key).unwrap_or(&none);
        let now = after.get(key).unwrap_or(&none);
        match expected.get(key) {
            Some(wanted) => {
                let as_asked = now.len() == wanted.len()
                    && now
                        .iter()
                        .zip(wanted)
                        .all(|(atom, values)| atom.other_parts == 0 && &atom.values == values);
                if !as_asked {
                    out.push(format!(
                        "{} was asked to hold {}, but after saving would hold {}",
                        key.display(),
                        describe_wanted(wanted),
                        describe_atoms(now)
                    ));
                }
            }
            None => {
                let unchanged =
                    was.len() == now.len() && was.iter().zip(now).all(|(a, b)| a.bytes == b.bytes);
                if !unchanged {
                    out.push(format!(
                        "{} would change: now {}; after saving, {}",
                        key.display(),
                        describe_atoms(was),
                        describe_atoms(now)
                    ));
                }
            }
        }
    }
    out
}

/// The asked-for atoms, in words.
fn describe_wanted(wanted: &[Vec<RawValue>]) -> String {
    match wanted {
        [] => "nothing".to_string(),
        [values] => describe_values(values),
        _ => format!(
            "{} atoms of that name: {}",
            wanted.len(),
            wanted
                .iter()
                .map(|values| format!("[{}]", describe_values(values)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Some atoms of one name, in words.
fn describe_atoms(atoms: &[RawAtom]) -> String {
    let one = |atom: &RawAtom| {
        let mut text = describe_values(&atom.values);
        if atom.other_parts > 0 {
            text.push_str(&format!(" and {} other part(s)", atom.other_parts));
        }
        text
    };
    match atoms {
        [] => "nothing".to_string(),
        [atom] => one(atom),
        _ => format!(
            "{} atoms of that name: {}",
            atoms.len(),
            atoms
                .iter()
                .map(|atom| format!("[{}]", one(atom)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// An atom's values, in words (at most four shown).
fn describe_values(values: &[RawValue]) -> String {
    const SHOWN: usize = 4;
    let listed: Vec<String> = values.iter().take(SHOWN).map(describe_value).collect();
    let more = if values.len() > SHOWN {
        format!(" and {} more", values.len() - SHOWN)
    } else {
        String::new()
    };
    match values.len() {
        0 => "no value".to_string(),
        1 => format!("one value, {}", listed[0]),
        n => format!("{n} values: {}{more}", listed.join(", ")),
    }
}

/// One value, in words: what kind of value it is, and enough of it to tell
/// two apart — the text itself (cut at 40 characters), or a whole number
/// and how many bytes it takes. A signed and an unsigned whole number, or a
/// value with another locale, read differently, so a change of type or
/// locale alone is visible in the message too.
fn describe_value(value: &RawValue) -> String {
    let bytes = value.value.len();
    let size = if bytes == 1 {
        "1 byte".to_string()
    } else {
        format!("{bytes} bytes")
    };
    let mut text = match value.type_indicator {
        1 | 2 => {
            let text = String::from_utf8_lossy(&value.value);
            let cut: String = text.chars().take(40).collect();
            let ellipsis = if cut.len() < text.len() { "…" } else { "" };
            let kind = if value.type_indicator == 2 {
                "the UTF-16 text"
            } else {
                "the text"
            };
            format!("{kind} {cut:?}{ellipsis}")
        }
        21 | 22 if (1..=8).contains(&bytes) => {
            let number = value
                .value
                .iter()
                .fold(0u64, |total, byte| (total << 8) | u64::from(*byte));
            let kind = if value.type_indicator == 22 {
                "unsigned whole number"
            } else {
                "whole number"
            };
            format!("the {kind} {number} in {size}")
        }
        0 => format!("untyped data ({size})"),
        12 => format!("a GIF image ({size})"),
        13 => format!("a JPEG image ({size})"),
        14 => format!("a PNG image ({size})"),
        27 => format!("a BMP image ({size})"),
        other => format!("data of type {other} ({size})"),
    };
    if value.locale != 0 {
        text.push_str(&format!(" with locale {}", value.locale));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn data(type_code: u32, locale: u32, value: &[u8]) -> Vec<u8> {
        let mut content = type_code.to_be_bytes().to_vec();
        content.extend_from_slice(&locale.to_be_bytes());
        content.extend_from_slice(value);
        atom(b"data", &content)
    }

    fn freeform(mean: &[u8], name: &[u8], values: &[Vec<u8>]) -> Vec<u8> {
        let mut content = atom(b"mean", &[&[0, 0, 0, 0], mean].concat());
        content.extend(atom(b"name", &[&[0, 0, 0, 0], name].concat()));
        content.extend(values.concat());
        atom(b"----", &content)
    }

    fn read(ilst_body: &[u8]) -> Vec<RawAtom> {
        let mut atoms = Vec::new();
        atoms_in_ilst(ilst_body, &mut atoms).expect("readable");
        atoms
    }

    #[test]
    fn an_ilst_is_read_into_names_values_types_and_locales() {
        let body = [
            atom(b"\xa9nam", &data(1, 0, b"Title")),
            atom(b"free", &[1, 1, 1]),
            atom(b"pgap", &data(21, 0, &[0])),
            freeform(b"com.apple.iTunes", b"Mood", &[data(1, 7, b"calm")]),
            atom(
                b"covr",
                &[data(13, 0, b"jpeg"), data(1, 0, b"odd")].concat(),
            ),
        ]
        .concat();
        let atoms = read(&body);
        let keys: Vec<RawKey> = atoms.iter().map(|a| a.key.clone()).collect();
        assert_eq!(
            keys,
            [
                RawKey::Fourcc(*b"\xa9nam"),
                // `free` is padding and left out.
                RawKey::Fourcc(*b"pgap"),
                RawKey::Freeform {
                    mean: b"com.apple.iTunes".to_vec(),
                    name: b"Mood".to_vec()
                },
                RawKey::Fourcc(*b"covr"),
            ]
        );
        assert_eq!(
            atoms[2].values,
            [RawValue {
                type_indicator: 1,
                locale: 7,
                value: b"calm".to_vec()
            }]
        );
        assert_eq!(
            atoms[3]
                .values
                .iter()
                .map(|v| v.type_indicator)
                .collect::<Vec<_>>(),
            [13, 1]
        );
        assert!(atoms.iter().all(|a| a.other_parts == 0));
        assert_eq!(atoms[0].bytes, atom(b"\xa9nam", &data(1, 0, b"Title")));
    }

    #[test]
    fn a_size_that_does_not_fit_is_refused_not_guessed() {
        // An atom claiming 400 bytes inside a 20-byte `ilst` body.
        let mut body = atom(b"\xa9nam", &data(1, 0, b"x"));
        body[3] = 200;
        body[2] = 1;
        let mut atoms = Vec::new();
        let problem = atoms_in_ilst(&body, &mut atoms).expect_err("refused");
        assert!(problem.contains("does not fit"), "{problem}");
        // A size smaller than its own header, too.
        let mut body = atom(b"\xa9nam", &data(1, 0, b"x"));
        body[..4].copy_from_slice(&4u32.to_be_bytes());
        assert!(atoms_in_ilst(&body, &mut Vec::new()).is_err());
    }

    #[test]
    fn an_atom_whose_parts_cannot_be_read_never_passes_as_asked() {
        // `data` inside claims more bytes than the atom holds.
        let mut inner = data(1, 0, b"x");
        inner[3] = 99;
        let body = atom(b"\xa9nam", &inner);
        let atoms = read(&body);
        assert_eq!(atoms[0].other_parts, 1);
        let mut expected = Expected::new();
        expected.insert(
            RawKey::Fourcc(*b"\xa9nam"),
            vec![vec![RawValue {
                type_indicator: 1,
                locale: 0,
                value: b"x".to_vec(),
            }]],
        );
        assert_eq!(differences(&[], &atoms, &expected).len(), 1);
    }

    #[test]
    fn an_unchanged_atom_must_match_byte_for_byte() {
        let before = read(&atom(b"pgap", &data(21, 0, &[0])));
        // The same value as text; the same byte with another type; the
        // same value with another locale; a second value added.
        for after in [
            atom(b"pgap", &data(1, 0, b"0")),
            atom(b"pgap", &data(22, 0, &[0])),
            atom(b"pgap", &data(21, 1, &[0])),
            atom(b"pgap", &[data(21, 0, &[0]), data(21, 0, &[1])].concat()),
            Vec::new(),
        ] {
            let found = differences(&before, &read(&after), &Expected::new());
            assert_eq!(found.len(), 1, "{found:?}");
            assert!(found[0].starts_with("pgap would change: now one value, the whole number 0 in 1 byte; after saving, "), "{found:?}");
        }
        assert!(differences(&before, &before, &Expected::new()).is_empty());
    }

    #[test]
    fn a_freeform_name_is_compared_exactly() {
        let before = read(&freeform(
            b"com.apple.iTunes",
            b"Mood",
            &[data(1, 0, b"calm")],
        ));
        let after = read(&freeform(
            b"com.apple.iTunes",
            b"MOOD",
            &[data(1, 0, b"calm")],
        ));
        let found = differences(&before, &after, &Expected::new());
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(
            found[0],
            "----:com.apple.iTunes:Mood would change: now one value, the text \"calm\"; after \
             saving, nothing"
        );
        assert_eq!(
            found[1],
            "----:com.apple.iTunes:MOOD would change: now nothing; after saving, one value, the \
             text \"calm\""
        );
    }

    #[test]
    fn an_asked_for_atom_must_hold_exactly_what_was_asked_and_nothing_beside_it() {
        let isrc = |value: &[u8]| freeform(b"com.apple.iTunes", b"ISRC", &[data(1, 0, value)]);
        let key = RawKey::Freeform {
            mean: b"com.apple.iTunes".to_vec(),
            name: b"ISRC".to_vec(),
        };
        let mut expected = Expected::new();
        expected.insert(
            key,
            vec![vec![RawValue {
                type_indicator: 1,
                locale: 0,
                value: b"X".to_vec(),
            }]],
        );
        let original = read(&[isrc(b"OLD"), isrc(b"NEW")].concat());
        // Exactly as asked: allowed, whatever the original held.
        assert!(differences(&original, &read(&isrc(b"X")), &expected).is_empty());
        // Left beside the old ones (before or after them), not stored, or
        // gone altogether: refused. The two middle cases differ from what
        // was asked ONLY in how many atoms there are — checking the atoms
        // pair by pair alone would pass them.
        for saved in [
            [isrc(b"OLD"), isrc(b"NEW"), isrc(b"X")].concat(),
            [isrc(b"X"), isrc(b"OLD")].concat(),
            Vec::new(),
            isrc(b"OLD"),
        ] {
            let found = differences(&original, &read(&saved), &expected);
            assert_eq!(found.len(), 1, "{found:?}");
            assert!(
                found[0].starts_with(
                    "----:com.apple.iTunes:ISRC was asked to hold one value, the text \"X\", \
                     but after saving would hold"
                ),
                "{found:?}"
            );
        }
    }

    #[test]
    fn values_are_described_in_plain_words() {
        let value = |type_indicator, locale, value: &[u8]| RawValue {
            type_indicator,
            locale,
            value: value.to_vec(),
        };
        assert_eq!(describe_value(&value(1, 0, b"calm")), "the text \"calm\"");
        assert_eq!(
            describe_value(&value(21, 0, &[0, 0, 0, 1])),
            "the whole number 1 in 4 bytes"
        );
        assert_eq!(
            describe_value(&value(13, 0, &[0; 30])),
            "a JPEG image (30 bytes)"
        );
        assert_eq!(
            describe_value(&value(1, 5, b"x")),
            "the text \"x\" with locale 5"
        );
        assert_eq!(
            describe_value(&value(99, 0, &[1])),
            "data of type 99 (1 byte)"
        );
        assert_eq!(
            describe_value(&value(22, 0, &[1])),
            "the unsigned whole number 1 in 1 byte"
        );
    }

    #[test]
    fn lofty_s_written_form_is_reproduced_for_what_this_crate_writes() {
        assert_eq!(
            raw_value(&AtomData::UTF8("x".into())),
            Some(RawValue {
                type_indicator: 1,
                locale: 0,
                value: b"x".to_vec()
            })
        );
        assert_eq!(
            raw_value(&AtomData::Bool(true)).map(|v| (v.type_indicator, v.value)),
            Some((21, vec![1]))
        );
        assert_eq!(
            raw_value(&AtomData::SignedInteger(1)).map(|v| v.value),
            Some(vec![0, 0, 0, 1])
        );
        // Kinds whose written form is not reproduced: refused, never
        // passed unchecked.
        assert_eq!(raw_value(&AtomData::UTF16("x".into())), None);
        assert_eq!(raw_value(&AtomData::UnsignedInteger(1)), None);
    }
}
