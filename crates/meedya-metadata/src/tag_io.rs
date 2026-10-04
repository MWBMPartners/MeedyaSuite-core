// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// File I/O tag reading and writing via lofty.
// =============================================
//
// Provides unified tag read/write for all supported audio formats:
// MP4/M4A, FLAC, OGG/Opus, MP3 (ID3v2), WavPack, APE, WAV, AIFF.
//
// File format is auto-detected from the file probe — consumers never
// specify which format to use. The CommonTag enum drives field name
// mapping to the correct tag type for the detected format.
//
// Includes convenience functions for writing ReplayGain and AcoustID
// results from meedya-fingerprint, closing the analysis→write loop.
//
// Every write in this file goes through ONE function, `edit_and_save`,
// which opens the file, lets the caller change its main tag, and saves
// it. It exists so that a file's language field — which can hold several
// languages — survives every write whole, whatever the write was for.
// Two lofty behaviours made that fail before (both found by the stand-in
// review of revision 5, and reproduced on real files):
//
// - ID3v2 (MP3, WAV, AIFF): lofty reads one `TLAN` frame holding
//   `por\0deu\0zho` as three separate language items, and saving a file
//   writes one `TLAN` frame per item. A reader keeps only the LAST of
//   several `TLAN` frames, so a title-only write turned three languages
//   into one (`zho`). `gather_languages_before_saving` puts them back into
//   one item just before every save.
// - MP4 (M4A): several languages belong in ONE
//   `----:com.apple.iTunes:LANGUAGE` atom holding several `data` atoms (the
//   form iTunes, mutagen and mp4ameta write). lofty's format-neutral `Tag`
//   keeps only the first `data` atom of an atom, so reading lost the rest,
//   and any write — even one that never touched the language — deleted
//   them from the file. MP4 files are therefore opened through lofty's own
//   MP4 type (`Mp4File` and its `Ilst`), and the language atom is read and
//   written there, whole; everything else still goes through the
//   format-neutral `Tag`, exactly as before.
//
// A third fault, found by the stand-in review of revision 6: a file can
// ALREADY hold one `TLAN` frame per language — this crate wrote files that
// way before revision 6, and `meedya-tags-extended`'s `TagFile::save`
// still does (#100). ID3v2 allows only one frame of each name, and lofty,
// reading such a file, keeps only the LAST `TLAN` frame, so the next save
// through this module wrote the file back with that one language and
// deleted the rest. (This file used to say those languages were "already
// lost" before any save here. They were not — mutagen still read all of
// them, and ffprobe the first — and it was this module's own save that
// deleted them.) `edit_and_save` and `read_tags` therefore read the file's
// `TLAN` frames straight from its bytes first (module
// `id3v2_language_frames`), and when there are several, put every
// language back into lofty's tag, in file order — so the save writes them
// all into one frame, and `read_tags` returns them all. Every older name
// lofty also reads as `TLAN` counts too (`TLA`, and in an ID3v2.3 tag
// `TLA` followed by a zero byte — found by Codex's review of revisions
// 5–7, when two such frames still lost a language). A file whose
// repeated frames cannot be read that way (compressed, encrypted, an
// unknown text encoding…) is refused rather than saved, because the save
// would delete languages.
//
// Which tag a language is compared against (the stand-in review of
// revision 6): `read_tags` reads a file's main tag, or failing that any
// tag it has — so for a WAV file holding only a RIFF INFO list, or an MP3
// holding only an APE tag, it reports THAT tag's language. But a write goes
// into the main tag, which such a file does not have yet (ID3v2 for both).
// "Is the caller's language unchanged?" used to be answered against the
// tag `read_tags` read, so writing the same language back was skipped as
// unchanged, the new main tag got none, and from then on `read_tags` read
// the main tag and reported no language at all. It is now answered against
// the tag the write goes into (see `write_tags`).
//
// Every M4A save is CHECKED before it replaces the file (issue #102; the
// stand-in review of revision 8). lofty's route for M4A tags changes atoms
// nobody asked it to change — a number flag rewritten as text, a freeform
// atom renamed to lofty's spelling, cover art with an unusual data type
// dropped, a second value cut off — and revision 8's list of such cases
// missed some. So `edit_and_save` now saves an M4A file to a temporary
// copy, and `mp4_save_check` compares the copy's atoms with the original's,
// read straight from both files' bytes: an atom the write did not ask to
// change must be byte for byte as it was, and an atom it did ask to change
// must hold exactly what was asked. Only then does the copy replace the
// original; otherwise the write is refused and the original is untouched.
// The real fix — a route that keeps those atoms — is still open in #102.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{FileType, TaggedFile};
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst, Mp4File};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, ItemValue, Tag, TagItem, TagType};

use crate::common_tags::CommonTag;
use crate::error::MetadataError;
use crate::id3v2_language_frames::{read_tlan_frames, TlanFrames};
use crate::json_path;
use crate::mp4_file_check;
use crate::mp4_save_check::{self, Expected, RawAtom, RawKey, RawValue};
use crate::save_by_copy::{Original, TempCopy};
use crate::tag_registry::{TagRegistry, TagScope};

/// A map of common tags to their values (supports multi-value fields).
pub type TagMap = HashMap<CommonTag, Vec<String>>;

/// The MP4 atom a language is stored in: the freeform
/// `----:com.apple.iTunes:LANGUAGE` atom, the one lofty's own
/// `ItemKey::Language` maps to for MP4 (lofty 0.22.4, `tag/item.rs`).
const MP4_LANGUAGE: AtomIdent<'static> = AtomIdent::Freeform {
    mean: Cow::Borrowed("com.apple.iTunes"),
    name: Cow::Borrowed("LANGUAGE"),
};

// ============================================================
// Reading
// ============================================================

/// Read all recognised tags from a media file.
///
/// Auto-detects the file format and reads whichever tag type is present
/// (ID3v2, Vorbis Comment, MP4 ilst, APE, etc.). Returns a `TagMap`
/// mapping `CommonTag` variants to their string values.
///
/// `CommonTag::Language` holds every language the file lists, in order,
/// one entry per language — except on APE, where the tag stores the list
/// as one value with null characters between the languages, and that one
/// value is returned as it is. On MP4 every `data` atom of the language
/// atom is returned, and on an ID3v2 file (MP3, WAV, AIFF, AAC) that holds
/// one `TLAN` frame per language, the languages of every frame are
/// returned, in file order (see the top of this file for why both needed
/// care). In the rare case that such repeated frames cannot be read (the
/// cases `id3v2_language_frames` lists, among them a language frame in a
/// file with more than one ID3v2 tag or ID3 chunk), this returns what lofty
/// reads and logs
/// a warning; a write to that file is refused, so it cannot delete or
/// scatter the others.
pub fn read_tags(path: &Path) -> Result<TagMap, MetadataError> {
    if !path.exists() {
        return Err(MetadataError::FileNotFound(path.display().to_string()));
    }

    let mut result = TagMap::new();
    match open_file(path)? {
        OpenedFile::Mp4(mut mp4) => {
            let mut ilst = mp4.remove_ilst().unwrap_or_default();
            let languages = take_mp4_languages(&mut ilst);
            // Everything except the language, read through the same
            // format-neutral view `Probe::read` would have given.
            collect_common_tags(&Tag::from(ilst), &mut result);
            let values = mp4_language_texts(&languages);
            if !values.is_empty() {
                result.insert(CommonTag::Language, values);
            }
        }
        OpenedFile::Other(mut tagged_file) => {
            // Several `TLAN` frames: every language they list (top of this
            // file). Reading never fails over them — see the doc comment.
            if let Err(e) = recover_languages_after_reading(&mut tagged_file, path) {
                log::warn!(
                    "{}: reporting the last TLAN frame's languages only: {e}",
                    path.display()
                );
            }
            // Try primary tag first, fall back to any available tag
            if let Some(tag) = read_source(&tagged_file) {
                collect_common_tags(tag, &mut result);
            }
        }
    }
    Ok(result)
}

/// The tag `read_tags` reads from: the file's main tag, or failing that
/// any tag it has.
fn read_source(tagged_file: &TaggedFile) -> Option<&Tag> {
    tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
}

/// Every language value `tag` holds, in order, as `read_tags` returns them.
fn languages_in(tag: &Tag) -> Vec<String> {
    tag.get_strings(&ItemKey::Language)
        .map(str::to_string)
        .collect()
}

/// Every `CommonTag` value in `tag`, added to `result`.
fn collect_common_tags(tag: &Tag, result: &mut TagMap) {
    // Extract standard accessor fields
    if let Some(v) = tag.title() {
        result
            .entry(CommonTag::Title)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.artist() {
        result
            .entry(CommonTag::Artist)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.album() {
        result
            .entry(CommonTag::Album)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.genre() {
        result
            .entry(CommonTag::Genre)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.comment() {
        result
            .entry(CommonTag::Comment)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.year() {
        result
            .entry(CommonTag::Year)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.track() {
        result
            .entry(CommonTag::TrackNumber)
            .or_default()
            .push(v.to_string());
    }
    if let Some(v) = tag.disk() {
        result
            .entry(CommonTag::DiscNumber)
            .or_default()
            .push(v.to_string());
    }

    // Extract by ItemKey for fields not covered by accessors
    let key_mappings: &[(ItemKey, CommonTag)] = &[
        (ItemKey::AlbumArtist, CommonTag::AlbumArtist),
        (ItemKey::Composer, CommonTag::Composer),
        (ItemKey::CopyrightMessage, CommonTag::Copyright),
        (ItemKey::Label, CommonTag::Label),
        (ItemKey::Isrc, CommonTag::Isrc),
        (ItemKey::Barcode, CommonTag::Upc),
        (ItemKey::EncoderSoftware, CommonTag::Encoder),
        (ItemKey::TrackTotal, CommonTag::TotalTracks),
        (ItemKey::DiscTotal, CommonTag::TotalDiscs),
        (ItemKey::Lyrics, CommonTag::Lyrics),
        (
            ItemKey::MusicBrainzRecordingId,
            CommonTag::MusicBrainzRecordingId,
        ),
        (
            ItemKey::MusicBrainzReleaseId,
            CommonTag::MusicBrainzReleaseId,
        ),
        (
            ItemKey::MusicBrainzReleaseGroupId,
            CommonTag::MusicBrainzReleaseGroupId,
        ),
        (ItemKey::MusicBrainzWorkId, CommonTag::MusicBrainzWorkId),
        (ItemKey::TrackSubtitle, CommonTag::Subtitle),
        (ItemKey::Language, CommonTag::Language),
        // NOTE (#65): lofty's ID3v2 key map lists "TEXT" => Writer BEFORE
        // "TEXT" => Lyricist (lofty 0.22.4, verified), so an MP3 TEXT frame
        // may surface via ItemKey::Writer instead of ItemKey::Lyricist and
        // be missed by this pairing — a known upstream quirk, accepted.
        // Deliberately NOT mapping Writer -> Lyricist here: Vorbis WRITER
        // is a distinct field and mapping it in would mis-read that tag.
        (ItemKey::Lyricist, CommonTag::Lyricist),
        (ItemKey::Conductor, CommonTag::Conductor),
        (ItemKey::Remixer, CommonTag::Remixer),
        (ItemKey::Arranger, CommonTag::Arranger),
        (ItemKey::Producer, CommonTag::Producer),
        (ItemKey::Engineer, CommonTag::Engineer),
        (ItemKey::MixEngineer, CommonTag::Mixer),
    ];

    for (key, common_tag) in key_mappings {
        for item in tag.get_items(key) {
            if let ItemValue::Text(text) = item.value() {
                result.entry(*common_tag).or_default().push(text.clone());
            }
        }
    }

    // Extract ReplayGain tags + other freeform-only fields (custom/freeform)
    let freeform_mappings: &[(&str, CommonTag)] = &[
        ("REPLAYGAIN_TRACK_GAIN", CommonTag::ReplayGainTrackGain),
        ("REPLAYGAIN_TRACK_PEAK", CommonTag::ReplayGainTrackPeak),
        ("REPLAYGAIN_ALBUM_GAIN", CommonTag::ReplayGainAlbumGain),
        ("REPLAYGAIN_ALBUM_PEAK", CommonTag::ReplayGainAlbumPeak),
        (
            "REPLAYGAIN_REFERENCE_LOUDNESS",
            CommonTag::ReplayGainReferenceLoudness,
        ),
        ("ISWC", CommonTag::Iswc),
        ("Acoustid Id", CommonTag::AcoustId),
    ];

    for (field_name, common_tag) in freeform_mappings {
        // Try as a custom text item (works for Vorbis, ID3v2 TXXX, MP4 freeform)
        let key = ItemKey::Unknown(field_name.to_string());
        for item in tag.get_items(&key) {
            if let ItemValue::Text(text) = item.value() {
                result.entry(*common_tag).or_default().push(text.clone());
            }
        }
    }
}

// ============================================================
// Writing
// ============================================================

/// Write a set of common tags to a media file.
///
/// Auto-detects the file format. Uses the file's existing primary tag type,
/// or creates an appropriate new one. Existing values for the given tags
/// are overwritten; other tags are preserved.
///
/// `CommonTag::Language` is not a plain pass-through (policy
/// MWBM-MEDIA-LANG, TRACK-070); see `write_language` below for what is
/// written for each format. Three rules apply to it here:
///
/// - **Several `Language` entries in one call are all written**, in the
///   order given — `[(Language, "eng"), (Language, "fra")]` writes both,
///   as does one entry `"eng\0fra"`. (Until the stand-in review of
///   revision 5 each entry replaced the one before, so only the last
///   survived.)
/// - **An unchanged value is left alone.** When the call's languages,
///   joined with null characters, are exactly what the tag this write goes
///   into already holds (its language values joined the same way), the
///   language field is not touched at all — not checked, not converted,
///   not rewritten — exactly as if no `Language` entry had been given.
///   This is what lets a caller read a file, change its title, and write
///   every field back: a file whose `LANGUAGE` another tool set to
///   `English` used to have that whole save refused. (The same rule
///   MeedyaManager adopted, COMPAT-030.) An empty value given for a file
///   that holds no language is likewise unchanged.
/// - **A value `read_tags` found in ANOTHER tag is copied, not refused.**
///   `read_tags` reads the file's main tag, or failing that any tag it has,
///   but every write goes into the main tag. So for a WAV file holding
///   only a RIFF INFO list, or an MP3 holding only an APE tag, `read_tags`
///   reports that other tag's language, and the main tag (ID3v2, for
///   both) does not hold it yet. When the caller writes back exactly what
///   `read_tags` returned, the value is written into the main tag if this
///   crate recognises it, and skipped — neither written nor refused — if
///   it does not: the caller did not choose it, it only carried it back.
///   (Until the stand-in review of revision 6 the comparison was made
///   against the tag `read_tags` read, so such a value counted as
///   unchanged, the new main tag got no language, and `read_tags` then
///   reported none.) The other tag keeps its language as it was.
/// - **A changed value this crate does not recognise refuses the WHOLE
///   call** with [`MetadataError::UnrecognisedLanguage`], before anything
///   is saved, so the file is left exactly as it was — none of the other
///   tags in `tags` are written either.
///
/// **On an M4A file every write is checked before it replaces the file**
/// (issue #102): it is saved to a temporary copy, and the copy replaces the
/// original only when every atom the call did not ask to change is byte for
/// byte as it was and every atom it asked to change holds exactly what was
/// asked. Otherwise the call fails with [`MetadataError::WriteError`],
/// naming each atom that would have changed and how, and the file is left
/// exactly as it was. Writing a field replaces its whole atom on purpose —
/// writing the artist on a file whose `©ART` holds two artists is allowed —
/// but lofty's route rewrites some atoms nobody asked for (1- and 2-byte
/// numbers such as `stik`, `rtng`, `tmpo` as 4 bytes, a 6-byte `disk` as 8,
/// the `pgap`/`hdvd`/`shwm` flags as text, freeform names in its own
/// spelling), so a write to a file holding any of those — as iTunes and
/// Apple Music files typically do — is refused until the real fix in #102.
/// The rest of the file is checked too — the audio, where each piece of it
/// starts, and every atom outside the tags must be as they were — and a
/// fragmented file, or one whose metadata box has no tag list, is refused
/// before anything is written.
///
/// **On an M4A file, what is stored is measured against what the CALLER
/// gave**, not against the library's own conversion of it (the stand-in
/// review of revision 9, M4), and a value the file would not store exactly
/// as given refuses the call before anything is written, whatever the file
/// already holds:
///
/// - a track or disc number, or a total of tracks or discs, must be a whole
///   number from 1 to 65535 written with digits only (`70000` and `abc`
///   used to be dropped without a word, and on a file holding track 3 of
///   12 the track number was lost);
/// - a year must be four digits, 1000 to 9999 (lofty puts the year in front
///   of an existing full date's month and day, so `2021` over `2019-03-01`
///   stores `2021-03-01`; anything shorter or longer would break that date,
///   and anything else was dropped — a full date can be written as
///   `ReleaseDate` instead);
/// - the compilation flag must be `1` or `0`;
/// - every other field is stored as the text given, and that is checked;
/// - a field an M4A file has no atom for — `Arranger`, `AcoustId` (the
///   `Acoustid Id` item) and `ReplayGainReferenceLoudness` (the
///   `REPLAYGAIN_REFERENCE_LOUDNESS` item) — is refused, naming it, instead
///   of being left out without a word. So [`write_acoustid_tags`] and
///   [`write_replaygain_tags`], which always write one of those, are
///   refused on every M4A file.
pub fn write_tags(path: &Path, tags: &[(CommonTag, String)]) -> Result<(), MetadataError> {
    if !path.exists() {
        return Err(MetadataError::FileNotFound(path.display().to_string()));
    }

    // Every `Language` entry in the call, in order: they are written
    // together, once (see the doc comment above).
    let language_entries: Vec<&str> = tags
        .iter()
        .filter(|(common_tag, _)| *common_tag == CommonTag::Language)
        .map(|(_, value)| value.as_str())
        .collect();

    edit_and_save(path, |tag, languages, asked| {
        // Every value is applied to the in-memory tag first; the file is
        // only saved once all of them have been accepted. A refused value
        // (a language; or, on an M4A file, a value it would not store as
        // given, or a field it has no atom for) therefore returns here with
        // the file untouched.
        for (common_tag, value) in tags {
            if *common_tag != CommonTag::Language {
                if tag.tag_type() == TagType::Mp4Ilst {
                    refuse_what_an_m4a_would_not_store(*common_tag, value)?;
                }
                // Recorded even when the value is the one already there:
                // writing it is still asking for it (see `edit_and_save`).
                asked.extend(keys_written(tag.tag_type(), *common_tag, value));
                write_common_tag_to_lofty(tag, *common_tag, value)?;
            }
        }
        if language_entries.is_empty() {
            return Ok(());
        }
        let given = language_entries.join("\0");
        if given == languages.current.join("\0") {
            // Unchanged: the tag being written already holds it.
            return Ok(());
        }
        match language_values_to_write(&language_entries, tag.tag_type()) {
            Ok(values) => languages.replacement = Some(values),
            // Not a value the caller chose: `read_tags` returned it from
            // another tag of this file, and the caller only carried it
            // back. There is nothing to refuse, and nothing that can be
            // written, so the language is left alone (see the doc comment).
            Err(MetadataError::UnrecognisedLanguage { .. })
                if given == languages.as_read.join("\0") => {}
            Err(refusal) => return Err(refusal),
        }
        Ok(())
    })
}

/// Write ReplayGain analysis results to a media file.
///
/// Writes track-level gain and peak, and the reference loudness; with
/// `album_result`, the album-level gain and peak too.
///
/// **On an M4A file this is refused** before anything is written: an M4A
/// file has no atom for the reference loudness, and a field that would be
/// left out is now refused rather than dropped without a word (see
/// [`write_tags`]). Write the gains and peaks with [`write_tags`] instead.
pub fn write_replaygain_tags(
    path: &Path,
    result: &meedya_fingerprint::ReplayGainResult,
    album_result: Option<&meedya_fingerprint::AlbumGainResult>,
) -> Result<(), MetadataError> {
    let mut tags = vec![
        (CommonTag::ReplayGainTrackGain, result.gain_string()),
        (CommonTag::ReplayGainTrackPeak, result.peak_string()),
        (
            CommonTag::ReplayGainReferenceLoudness,
            format!("{:.1} LUFS", result.reference_level),
        ),
    ];

    if let Some(album) = album_result {
        tags.push((CommonTag::ReplayGainAlbumGain, album.gain_string()));
        tags.push((CommonTag::ReplayGainAlbumPeak, album.peak_string()));
    }

    write_tags(path, &tags)
}

/// Write AcoustID fingerprint results to a media file.
///
/// Writes the AcoustID UUID and optionally the first MusicBrainz recording ID.
///
/// **On an M4A file this is refused** before anything is written: an M4A
/// file has no atom for the AcoustID item this writes (`Acoustid Id`), and a
/// field that would be left out is now refused rather than dropped without
/// a word (see [`write_tags`]). The MusicBrainz recording ID alone can be
/// written with [`write_tags`].
pub fn write_acoustid_tags(
    path: &Path,
    result: &meedya_fingerprint::AcoustIdResult,
) -> Result<(), MetadataError> {
    let mut tags = vec![(CommonTag::AcoustId, result.acoustid.clone())];

    if let Some(mb_id) = result.recording_ids.first() {
        tags.push((CommonTag::MusicBrainzRecordingId, mb_id.clone()));
    }

    write_tags(path, &tags)
}

/// Write tags driven by a TagRegistry and a JSON source document.
///
/// Iterates tag definitions in the given scope, extracts values from
/// the JSON source using each definition's `json_path`, and writes
/// the converted values to the file's freeform atoms.
///
/// Returns the number of tags successfully written.
///
/// **On an M4A file this refuses, rather than reporting a tag "written"
/// that is not** (issue #103, interim guard). Like every M4A write, the
/// save is also checked atom by atom on a temporary copy before it replaces
/// the file (see [`write_tags`]): a value that would be left beside an
/// older atom of the same name, or put back over by the file's own
/// languages, refuses the call too (the stand-in review of revision 8), and
/// nothing is written. Each atom is written under
/// the key `namespace:name` (`MeedyaMeta:ISRC`), but an MP4 freeform atom
/// needs the form `----:mean:name`, and lofty silently leaves out any key
/// not in that form when it saves an M4A file — so the call used to return
/// `Ok(1)` for a file that gained nothing (Codex's review of revisions 5–7,
/// and the stand-in review of revision 6; measured on a real M4A). Now, when
/// a key that would be written cannot be stored as a proper freeform atom,
/// the whole call fails with [`MetadataError::WriteError`], naming the key,
/// and nothing is saved. With the keys this function builds today, that is
/// every registry write to an M4A file that has a value to write; storing
/// them properly is the real fix, still open in #103. Other formats are
/// unchanged.
pub fn write_registry_tags(
    path: &Path,
    registry: &TagRegistry,
    json_source: &serde_json::Value,
    scope: TagScope,
) -> Result<usize, MetadataError> {
    if !path.exists() {
        return Err(MetadataError::FileNotFound(path.display().to_string()));
    }

    let defs = match scope {
        TagScope::Album => &registry.album_tags,
        TagScope::Track => &registry.track_tags,
    };

    // The file's languages are not touched here, but saving goes through
    // `edit_and_save` all the same, so they survive the save whole.
    edit_and_save(path, |tag, _languages, asked| {
        let mut count = 0;

        for def in defs {
            let Some(json_val) = json_path::extract_json_value(json_source, &def.json_path) else {
                continue;
            };
            let Some(string_val) = json_path::value_to_string(&json_val, &def.value_type) else {
                continue;
            };

            for atom in &def.atoms {
                // Write as a custom/freeform item with the full namespace
                let key_text = format!("{}:{}", atom.namespace, atom.name);
                if tag.tag_type() == TagType::Mp4Ilst && !is_mp4_freeform_key(&key_text) {
                    // #103 (interim guard): lofty would leave this key out
                    // of the saved file without a word. Refusing here
                    // returns before `edit_and_save` saves anything.
                    return Err(MetadataError::WriteError(format!(
                        "cannot write the registry tag {:?} to this M4A file: its key {key_text:?} \
                         is not in the form an MP4 freeform atom needs (----:mean:name), so \
                         saving would silently leave it out. Nothing was written. (Issue #103: \
                         until registry keys are stored as proper freeform atoms, such a write \
                         is refused rather than reported as written.)",
                        def.id
                    )));
                }
                let key = ItemKey::Unknown(key_text);
                asked.push(key.clone());
                // #65 — insert_unchecked: lofty's insert() rejects ItemKey::Unknown (re_map allow_unknown=false), silently dropping freeform atoms; insert_unchecked is lofty's documented API for Unknown keys.
                tag.insert_unchecked(TagItem::new(key, ItemValue::Text(string_val.clone())));
            }
            count += 1;
        }

        Ok(count)
    })
}

/// Whether lofty stores an item with the key `key_text` in an M4A file as
/// a proper MP4 freeform atom — `----:mean:name`, with the name kept whole —
/// when it saves the file (lofty 0.22.4, `mp4/atom_info.rs`: a key is
/// turned into an atom only when it starts with `----` and splits at its
/// colons into a mean and a name, or when it is exactly four characters,
/// which makes an ordinary four-letter atom instead; any other key is left
/// out of the saved file). The key must also come back exactly, so a name
/// holding a colon, which that split would cut short, does not count.
fn is_mp4_freeform_key(key_text: &str) -> bool {
    let key = ItemKey::Unknown(key_text.to_string());
    match AtomIdent::try_from(&key) {
        Ok(AtomIdent::Freeform { mean, name }) => format!("----:{mean}:{name}") == key_text,
        _ => false,
    }
}

// ------------------------------------------------------------
// Keeping a file's languages whole through a caller's own save
// ------------------------------------------------------------
//
// A caller that reads and saves a lofty `TaggedFile` itself (as
// `meedya-lyrics`' `embed_synced` does), instead of going through
// `write_tags`, takes the same two steps every save in this module takes,
// at the same two moments:
//
//   let mut tagged_file = lofty::read_from_path(path)?;
//   tag_io::recover_languages_after_reading(&mut tagged_file, path)?; // 1
//   // … the caller's own changes, languages included …
//   tag_io::gather_languages_before_saving(&mut tagged_file);        // 2
//   tagged_file.save_to_path(path, WriteOptions::default())?;
//
// They are two steps, not one, on purpose. Until Codex's review of
// revisions 5–7 a single public helper, `keep_languages_whole_before_saving`,
// did both just before the save — and step 1 reads the languages from the
// DISK, so it put the file's old languages back over whatever the caller
// had just done: a language replaced with `deu` came back as `eng` and
// `fra`, and a language deleted on purpose came back too. Step 1 belongs
// straight after reading, before any change; step 2 never reads the disk.
// (The combined helper was on an unreleased branch; no consuming app used
// it, so it was removed rather than kept alongside.)

/// **Step 1 of 2: call straight after reading `tagged_file` from `path`,
/// before changing anything in it.** When the file holds its languages in
/// several ID3v2 language frames — one `TLAN` frame per language, as this
/// crate wrote files before revision 6 and `meedya-tags-extended`'s
/// `TagFile::save` still does (#100), or older frames lofty also reads as
/// `TLAN` — lofty has read only the last of them. This reads the frames
/// straight from the file's bytes and puts every language they list, in file
/// order, into `tagged_file`'s ID3v2 tag, one item per language: exactly
/// what lofty would have read had they been in one frame. It does nothing
/// when the file has no ID3v2 tag, or at most one language frame.
///
/// **Why "straight after reading":** it puts back what the FILE holds,
/// replacing whatever languages `tagged_file` holds at the time. Called
/// after the caller has changed the languages, it would undo that change —
/// a language replaced with another, or deleted on purpose, would come
/// back. Called first, the caller's changes are made to the whole list and
/// are kept. Then, just before saving, call
/// [`gather_languages_before_saving`] (step 2), which does not read the
/// file.
///
/// Fails, without changing `tagged_file`, when the file seems to hold
/// several language frames that cannot be read (a compressed or encrypted
/// frame, an unknown text encoding, an unsynchronised tag…), language
/// frames in a file with more than one ID3v2 tag (an MP3 file's tags one
/// after another, or several ID3 chunks of a WAV or AIFF file: lofty reads
/// and rewrites only one of them, so a language in another is never seen,
/// changed or merged), or an old
/// `TLA` frame inside an ID3v2.4 tag (a language to other programs, which
/// lofty's save would turn into an ordinary text frame): saving would lose
/// languages, so do not save. The error ([`MetadataError::WriteError`])
/// says why, in plain words. It also fails when the file cannot be read
/// ([`MetadataError::IoError`]). `read_tags` and every write in this module
/// take this step themselves.
pub fn recover_languages_after_reading(
    tagged_file: &mut TaggedFile,
    path: &Path,
) -> Result<(), MetadataError> {
    if tagged_file.tag(TagType::Id3v2).is_none() {
        return Ok(());
    }
    if let TlanFrames::Several(values) = read_tlan_frames(path, tagged_file.file_type())? {
        if let Some(tag) = tagged_file.tag_mut(TagType::Id3v2) {
            tag.remove_key(&ItemKey::Language);
            for value in values {
                tag.push(TagItem::new(ItemKey::Language, ItemValue::Text(value)));
            }
        }
    }
    Ok(())
}

/// **Step 2 of 2: call just before saving `tagged_file`** (step 1,
/// [`recover_languages_after_reading`], comes straight after reading it).
/// Gathers every language item in `tagged_file`'s ID3v2 tag (and APE tag)
/// into ONE item, the values separated by null characters, so that saving
/// writes one `TLAN` frame (one APE `Language` item) holding every
/// language, instead of one frame per language. It works only on what
/// `tagged_file` holds — it never reads the file — so whatever the caller
/// changed, languages included, is what is saved.
///
/// Why it is needed: lofty reads one `TLAN` frame holding `por\0deu\0zho`
/// (ID3v2.4's own way of listing several values) as three separate
/// language items, but saving a file turns each item into its own frame
/// (lofty 0.22.4, `id3/v2/tag.rs`, `tag_frames`). A reader keeps only the
/// LAST of several `TLAN` frames, so any save — a title change, a
/// ReplayGain write, synchronised lyrics — silently cut three languages
/// down to the last one (found by the stand-in review of revision 5 on
/// MP3, WAV and AIFF). APE has the same one-item-per-key rule (lofty's
/// `ApeTag::insert` replaces an existing item), so an APE tag is gathered
/// the same way.
///
/// It does nothing when a tag holds one language item or none, or when
/// any of its language items is not text (there is nothing it could join
/// those with, so they are left exactly as they are). What it CANNOT do:
/// see the languages of a file that already holds several `TLAN` frames.
/// lofty reads only the last of those frames, so this function sees one
/// language, and a save would then delete the others from the file. (This
/// comment used to call those languages "already lost"; they were not —
/// they were still in the file, where mutagen read them all — until the
/// save. [`recover_languages_after_reading`], step 1, reads them from the
/// file straight after reading it; found by the stand-in review of
/// revision 6.) Nor does
/// it reach `meedya-tags-extended`'s `TagFile::save`: that crate is built
/// on an older lofty (0.21), whose `TaggedFile` is a different type this
/// function cannot take, so a file saved through it still has several
/// languages split into several frames (#100; measured on a real MP3).
/// Every save in `tag_io`, and `embed_synced`, reads such a file's frames
/// back whole.
pub fn gather_languages_before_saving(tagged_file: &mut TaggedFile) {
    for tag_type in [TagType::Id3v2, TagType::Ape] {
        let Some(tag) = tagged_file.tag_mut(tag_type) else {
            continue;
        };
        let items: Vec<&TagItem> = tag.get_items(&ItemKey::Language).collect();
        if items.len() < 2 {
            continue;
        }
        let Some(values) = items
            .iter()
            .map(|item| item.value().text())
            .collect::<Option<Vec<&str>>>()
        else {
            continue;
        };
        let joined = values.join("\0");
        tag.remove_key(&ItemKey::Language);
        tag.insert(TagItem::new(ItemKey::Language, ItemValue::Text(joined)));
    }
}

// ============================================================
// Opening and saving
// ============================================================

/// A file opened for reading or changing its tags.
enum OpenedFile {
    /// An MP4 file, through lofty's own MP4 type, so the language atom can
    /// be read and written whole (see the top of this file).
    Mp4(Mp4File),
    /// Any other format, through lofty's format-neutral `TaggedFile`.
    Other(TaggedFile),
}

/// Opens `path`, deciding its format exactly as `Probe::open(path)?.read()`
/// does (from the file name's extension), with lofty's default reading
/// options — so every file that is not MP4 is read exactly as before.
fn open_file(path: &Path) -> Result<OpenedFile, MetadataError> {
    let probe = Probe::open(path)?;
    if probe.file_type() == Some(FileType::Mp4) {
        let mut reader = probe.into_inner();
        Ok(OpenedFile::Mp4(Mp4File::read_from(
            &mut reader,
            ParseOptions::default(),
        )?))
    } else {
        Ok(OpenedFile::Other(probe.read()?))
    }
}

/// A file's language field while a write is being prepared.
struct LanguageField {
    /// Every language value the tag being written holds now — the tag the
    /// edit goes into, which is not always the tag `read_tags` reads (see
    /// `as_read`). What "unchanged" is measured against.
    current: Vec<String>,
    /// Every language value `read_tags` returns for the file: the main
    /// tag's, or when the file has no main tag yet, another tag's (a WAV
    /// file's RIFF INFO list, an MP3's APE tag). The same as `current`
    /// whenever the file has a main tag, and always on MP4.
    as_read: Vec<String>,
    /// The values to store instead, already in the form this tag stores
    /// (see `language_values_to_write`), or `None` to leave the field
    /// exactly as it is.
    replacement: Option<Vec<String>>,
}

/// Opens `path`, lets `edit` change the file's main tag (and, through the
/// [`LanguageField`], its languages), then saves the file. The ONE way
/// this module saves a file, so every write — whatever it is for — keeps
/// the file's languages whole (see the top of this file). Nothing is
/// saved when `edit` returns an error.
///
/// `edit` also records, in its third argument, the key of every item it
/// writes — even one it writes with the value already there, because
/// writing a value is asking for it. Only the MP4 save uses that list (see
/// below); every other format ignores it.
///
/// - **MP4**: the language atom is taken out of the file's `Ilst` first
///   and held on one side; the rest is split into lofty's format-neutral
///   `Tag` for `edit` (`split_tag`), merged back (`merge_tag` — together,
///   exactly the conversion lofty's own `TaggedFile` save performs), and
///   the language atom is put back as ONE atom holding every value: the
///   replacement if `edit` asked for one, otherwise every `data` atom the
///   file had. A file written before revision 6, with one atom per
///   language, is therefore rewritten in the usual one-atom form — every
///   value kept, in order, and checked to be.
///
///   That route changes atoms nobody asked it to change (issue #102; see
///   `mp4_save_check`), so the save is made on a temporary copy of the file
///   first, and the copy's atoms are compared with the original's, read
///   straight from both files' bytes: every atom the edit did not ask to
///   change must be byte for byte as it was, and every atom it did ask to
///   change must hold exactly what was asked (see `atoms_asked_for`). Only
///   then does the copy replace the original, in one rename. Otherwise the
///   write fails with [`MetadataError::WriteError`], naming each atom that
///   would have changed, and the original is not touched.
/// - **Every other format**: a file holding its languages in several
///   `TLAN` frames has them all put back first, straight after reading and
///   before `edit` runs ([`recover_languages_after_reading`]; the whole
///   write is refused, nothing saved, when they cannot be read), so both
///   what "unchanged" is measured against and what is saved hold every
///   language. Then `edit` gets the main tag (created if the file has
///   none), a replacement is stored with `put_languages`, and
///   [`gather_languages_before_saving`] runs just before the save — it
///   does not read the file, so the replacement is what is saved. These
///   formats are saved in place by lofty, as before.
fn edit_and_save<T>(
    path: &Path,
    edit: impl FnOnce(&mut Tag, &mut LanguageField, &mut Vec<ItemKey>) -> Result<T, MetadataError>,
) -> Result<T, MetadataError> {
    let mut asked = Vec::new();
    match open_file(path)? {
        OpenedFile::Mp4(mut mp4) => {
            // The real file, not a link to it: the checked copy replaces
            // whatever this names, so a symbolic link is followed first.
            let real = std::fs::canonicalize(path)?;
            // Opened once, for reading and writing, without being changed
            // (a file this program may not write to is refused here): the
            // copy is made from this handle, and the checks read it
            // (`save_by_copy`).
            let mut source = Original::open(&real)?;
            // A file the save is known to damage — a fragmented one, or a
            // `meta` with no tag list, whose handler lofty would write over
            // — is refused before anything is written (`mp4_file_check`).
            mp4_file_check::refuse_what_a_save_would_damage(source.file())?;
            // What the file holds now, read from its bytes before anything
            // changes: what the saved copy is compared with.
            let original = mp4_save_check::read_ilst_atoms_from(source.file())?;

            let mut ilst = mp4.remove_ilst().unwrap_or_default();
            let held = take_mp4_languages(&mut ilst);
            let current = mp4_language_texts(&held);
            let mut languages = LanguageField {
                as_read: current.clone(),
                current,
                replacement: None,
            };
            let (remainder, mut tag) = ilst.split_tag();
            let before = tag.clone();
            let out = edit(&mut tag, &mut languages, &mut asked)?;
            let expected = atoms_asked_for(&before, &tag, &asked, &languages, &original, &mut 0)?;

            let mut merged = remainder.merge_tag(tag);
            let data: Vec<AtomData> = match languages.replacement {
                Some(values) => values.into_iter().map(AtomData::UTF8).collect(),
                None => held.into_iter().flat_map(Atom::into_data).collect(),
            };
            // `replace_atom` also removes any language atom the format-
            // neutral tag might have produced, so the file ends up with
            // exactly one. `from_collection` gives `None` for no values:
            // then there is no language atom at all.
            match Atom::from_collection(MP4_LANGUAGE, data) {
                Some(atom) => merged.replace_atom(atom),
                None => merged.remove(&MP4_LANGUAGE).for_each(drop),
            }
            save_mp4_checked(source, &merged, &original, &expected)?;
            Ok(out)
        }
        OpenedFile::Other(mut tagged_file) => {
            // Before anything reads the languages: a file with several
            // `TLAN` frames gets every language back (or the write is
            // refused, the file untouched). See the top of this file.
            recover_languages_after_reading(&mut tagged_file, path)?;

            let as_read = read_source(&tagged_file)
                .map(languages_in)
                .unwrap_or_default();

            // #79 — an untagged file has no `primary_tag()`, and the old
            // fallback hardcoded `TagType::Id3v2` here regardless of
            // container. `insert_tag` silently no-ops when the container
            // doesn't support the tag type it's given (lofty
            // file/tagged_file.rs), so on e.g. a fresh untagged .m4a (the
            // standard MeedyaDL download product) the Id3v2 insert was
            // dropped on the floor and the `tag_mut(tag_type).unwrap()`
            // below panicked on the resulting `None`. `primary_tag_type()`
            // instead derives the correct tag type from the FILE type
            // (Flac/Opus/Vorbis/Speex -> VorbisComments, etc.), which is
            // always write-supported for its own format, so the insert
            // always lands. (MP4 no longer reaches here at all.)
            let tag_type = tagged_file
                .primary_tag()
                .map(Tag::tag_type)
                .unwrap_or_else(|| tagged_file.primary_tag_type());

            // The languages of the tag the edit goes INTO — none when the
            // file does not have that tag yet, whatever another tag holds
            // (see `write_tags`, and the top of this file).
            let current = tagged_file
                .tag(tag_type)
                .map(languages_in)
                .unwrap_or_default();

            // Ensure the tag exists before borrowing mutably
            if tagged_file.tag(tag_type).is_none() {
                tagged_file.insert_tag(Tag::new(tag_type));
            }

            // Unreachable now that tag_type always comes from an existing
            // tag or primary_tag_type() (both write-supported), but house
            // style forbids unwrap — surface a proper error instead of
            // assuming it can't happen.
            let tag = tagged_file.tag_mut(tag_type).ok_or_else(|| {
                MetadataError::UnsupportedFormat(format!(
                    "cannot create a {tag_type:?} tag in this container"
                ))
            })?;

            let mut languages = LanguageField {
                current,
                as_read,
                replacement: None,
            };
            let out = edit(tag, &mut languages, &mut asked)?;
            if let Some(values) = languages.replacement {
                put_languages(tag, values);
            }

            gather_languages_before_saving(&mut tagged_file);
            tagged_file.save_to_path(path, WriteOptions::default())?;
            Ok(out)
        }
    }
}

/// Every item's values in `tag`, gathered by key in one pass (one step each
/// in `steps`), in the order the tag holds them: what `atoms_asked_for`
/// looks keys up in, instead of searching the tag once per key.
fn values_by_key<'a>(tag: &'a Tag, steps: &mut u64) -> HashMap<&'a ItemKey, Vec<&'a ItemValue>> {
    let mut values: HashMap<&ItemKey, Vec<&ItemValue>> = HashMap::new();
    for item in tag.items() {
        *steps += 1;
        values.entry(item.key()).or_default().push(item.value());
    }
    values
}

/// What an M4A save must store for every atom the edit asked to change:
/// for each such atom's name, the atoms the saved file must hold under
/// that name, each a list of values, in order (`mp4_save_check`). Every
/// atom NOT named here must come out of the save byte for byte as
/// `original` holds it.
///
/// **Which atoms the edit asked to change**: those of every item key it
/// recorded in `asked` — what `write_tags` and `write_registry_tags` write,
/// even with the value already there; those of every item key whose values
/// differ between `before` (the tag as read) and `after` (the tag as
/// edited); `covr` when the pictures differ; and the language atom when
/// the edit gave languages to store. So `write_tags(Artist, "Carol")` on a
/// file whose `©ART` holds `Alice` and `Bob` asks for the whole `©ART` atom
/// to hold `Carol`: a deliberate replacement, allowed, not a value lost
/// (the stand-in review of revision 8, finding 5). A track or disc number
/// and its total share one atom (`trkn`, `disk`), so asking for either
/// asks for that atom, holding both.
///
/// **What each must hold**: exactly what lofty writes for the edited
/// items of those keys ALONE — worked out with lofty's own conversion
/// (`merge_tag`) on a tag holding nothing else — so a value left over
/// beside it (a second ISRC atom), or put back over it (a registry write
/// aimed at the language atom), shows up in the comparison.
///
/// **A key that has no MP4 atom at all** never gets here from `write_tags`:
/// it refuses such a field first, naming it
/// (`refuse_what_an_m4a_would_not_store` — `Arranger`, and the
/// `Acoustid Id` and `REPLAYGAIN_REFERENCE_LOUDNESS` items). This comment
/// used to list `Producer` and `Engineer` among them too; lofty 0.22.4 does
/// store both, as `----:com.apple.iTunes:PRODUCER` and `…:ENGINEER`.
/// (`write_registry_tags` refuses a key it cannot store itself, #103.)
///
/// Fails, so the save is refused before anything is written, when a value
/// asked for would not be stored at all (lofty drops a compilation flag of
/// "yes", say), or when the written form of a value cannot be reproduced
/// here (`mp4_save_check::raw_value`).
///
/// **The work grows in step with the number of tags** (the stand-in review
/// of revision 9, M3): each tag's values are gathered by key once and each
/// key looked up, rather than the whole tag searched for every key, which
/// took about two minutes on a file of 32,000 tags. `steps` counts the items
/// visited and the keys looked up, so a test can show that
/// (`working_out_what_was_asked_takes_steps_in_step_with_the_tags`).
fn atoms_asked_for(
    before: &Tag,
    after: &Tag,
    asked: &[ItemKey],
    languages: &LanguageField,
    original: &[RawAtom],
    steps: &mut u64,
) -> Result<Expected, MetadataError> {
    let refusal = |why: String| {
        MetadataError::WriteError(format!(
            "this M4A file could not be saved exactly as asked: {why}. Nothing was written. \
             (Issue #102: every M4A save is checked atom by atom before it replaces the file.)"
        ))
    };

    // Every key asked for, then every key the edit changed, once each.
    // Each tag's values are gathered by key ONCE, and each key is then
    // looked up (M3: this used to search the whole tag for every key, so a
    // file of 32,000 tags took some two minutes to save).
    let before_values = values_by_key(before, steps);
    let after_values = values_by_key(after, steps);
    let asked_keys: HashSet<&ItemKey> = asked.iter().collect();
    let mut seen: HashSet<&ItemKey> = HashSet::new();
    let mut keys: Vec<ItemKey> = Vec::new();
    for key in asked
        .iter()
        .chain(before.items().chain(after.items()).map(TagItem::key))
    {
        *steps += 1;
        if !seen.insert(key) {
            continue;
        }
        let changed = asked_keys.contains(key) || before_values.get(key) != after_values.get(key);
        if changed {
            keys.push(key.clone());
        }
    }
    let mut chosen: HashSet<ItemKey> = keys.iter().cloned().collect();
    for pair in [
        [ItemKey::TrackNumber, ItemKey::TrackTotal],
        [ItemKey::DiscNumber, ItemKey::DiscTotal],
    ] {
        if pair.iter().any(|key| chosen.contains(key)) {
            for key in pair {
                if chosen.insert(key.clone()) {
                    keys.push(key);
                }
            }
        }
    }
    let pictures_changed = !before.pictures().eq(after.pictures());

    // The atoms asked for, by name — and which of them were given a value.
    let mut idents: Vec<AtomIdent<'static>> = Vec::new();
    let mut named: HashSet<RawKey> = HashSet::new();
    let mut given: HashSet<RawKey> = HashSet::new();
    let mut requested = Tag::new(TagType::Mp4Ilst);
    for key in &keys {
        *steps += 1;
        // A key with no MP4 atom: nothing to compare (see above).
        if let Ok(ident) = AtomIdent::try_from(key.clone()) {
            if named.insert(RawKey::of(&ident)) {
                idents.push(ident);
            }
        }
    }
    for item in after.items().filter(|item| chosen.contains(item.key())) {
        *steps += 1;
        if let Ok(ident) = AtomIdent::try_from(item.key().clone()) {
            given.insert(RawKey::of(&ident));
        }
        requested.push_unchecked(item.clone());
    }
    if pictures_changed {
        let covr = AtomIdent::Fourcc(*b"covr");
        if named.insert(RawKey::of(&covr)) {
            idents.push(covr.clone());
        }
        for picture in after.pictures() {
            given.insert(RawKey::of(&covr));
            requested.push_picture(picture.clone());
        }
    }

    // lofty's own conversion of those items alone: what it writes for them,
    // gathered by name once.
    let (nothing, _) = Ilst::default().split_tag();
    let stored = nothing.merge_tag(requested);
    let mut stored_by_name: HashMap<RawKey, Vec<&Atom<'static>>> = HashMap::new();
    for atom in &stored {
        *steps += 1;
        stored_by_name
            .entry(RawKey::of(atom.ident()))
            .or_default()
            .push(atom);
    }

    let mut expected = Expected::new();
    for ident in idents {
        *steps += 1;
        let key = RawKey::of(&ident);
        let mut atoms = Vec::new();
        for atom in stored_by_name.get(&key).into_iter().flatten() {
            let values = atom
                .data()
                .map(mp4_save_check::raw_value)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    refusal(format!(
                        "the value given for {} is of a kind whose saved form cannot be checked",
                        key.display()
                    ))
                })?;
            atoms.push(values);
        }
        if atoms.is_empty() && given.contains(&key) {
            return Err(refusal(format!(
                "the value given for {} cannot be stored in an M4A file (lofty would leave it \
                 out)",
                key.display()
            )));
        }
        expected.insert(key, atoms);
    }

    let language = RawKey::of(&MP4_LANGUAGE);
    match &languages.replacement {
        Some(values) => {
            let atom: Vec<RawValue> = values
                .iter()
                .filter_map(|value| mp4_save_check::raw_value(&AtomData::UTF8(value.clone())))
                .collect();
            // `or_insert`: when an item key asked for the language atom too,
            // what it asked for stays expected, so the languages put back
            // over it fail the comparison — refused, not lost.
            expected.entry(language).or_insert_with(|| {
                if atom.is_empty() {
                    Vec::new()
                } else {
                    vec![atom]
                }
            });
        }
        None if !expected.contains_key(&language) => {
            // Languages not asked for. A file with ONE language atom keeps
            // it byte for byte, like any other atom. A file with several —
            // one per language, as revision 5 of this crate and lofty's own
            // format-neutral save write them — has them put into one atom
            // holding every value in order (see `edit_and_save`): the one
            // change made that nobody asked for, because it loses nothing.
            // It is checked value by value — type, locale and bytes — so a
            // value it would change still refuses the save.
            let held: Vec<&RawAtom> = original.iter().filter(|a| a.key == language).collect();
            if held.len() > 1 && held.iter().all(|atom| atom.other_parts == 0) {
                let values = held.iter().flat_map(|atom| atom.values.clone()).collect();
                expected.insert(language, vec![values]);
            }
        }
        None => {}
    }
    Ok(expected)
}

/// Saves `ilst` as the tags of the M4A file `source` (the real file, links
/// already followed, opened by [`Original::open`]): on a temporary copy
/// first — made, saved into and read back through its own handle
/// (`save_by_copy`) — which replaces the file only when every tag atom in
/// it is as `expected`, or else byte for byte as in `original` (see
/// `mp4_save_check`), AND nothing outside the tags changed beyond what a
/// tag save may change — the audio, the chunk offsets, everything else in
/// `moov` and at the top of the file (see `mp4_file_check`). Otherwise the
/// copy is deleted, the file is not touched, and the error names
/// everything that would have changed.
fn save_mp4_checked(
    mut source: Original,
    ilst: &Ilst,
    original: &[RawAtom],
    expected: &Expected,
) -> Result<(), MetadataError> {
    let mut copy = TempCopy::of(&mut source)?;
    ilst.save_to(copy.file(), WriteOptions::default())?;
    let saved = mp4_save_check::read_ilst_atoms_from(copy.file())?;
    let mut problems = mp4_save_check::differences(original, &saved, expected);
    problems.extend(mp4_file_check::differences_outside_the_tags(
        source.file(),
        copy.file(),
    )?);
    if !problems.is_empty() {
        const LISTED: usize = 8;
        let mut list = problems[..problems.len().min(LISTED)].join("; ");
        if problems.len() > LISTED {
            list.push_str(&format!("; and {} more", problems.len() - LISTED));
        }
        return Err(MetadataError::WriteError(format!(
            "this M4A file could not be saved exactly as asked: {list}. Nothing was written: the \
             save was made on a temporary copy, compared with the original (its tags atom by \
             atom, and the rest of the file byte for byte), and thrown away, so the file is \
             exactly as it was. (Issue #102: the route this library saves M4A files through \
             changes some things it was not asked to change; until it keeps them, such a save is \
             refused.)"
        )));
    }
    copy.replace(source)
}

/// The item keys `write_common_tag_to_lofty` writes for `common_tag` and
/// `value` into a tag of type `tag_type` — found by making that very write
/// into an empty tag and looking, so this can never drift from what the
/// write does. (An empty tag differs from the real one in one way: the year
/// goes into an existing recording date when there is one. On MP4, the only
/// format that uses these keys, that is the same key, `RecordingDate`,
/// either way.)
fn keys_written(tag_type: TagType, common_tag: CommonTag, value: &str) -> Vec<ItemKey> {
    let mut scratch = Tag::new(tag_type);
    // Only a language can be refused, and languages never come here.
    if write_common_tag_to_lofty(&mut scratch, common_tag, value).is_err() {
        return Vec::new();
    }
    scratch.items().map(|item| item.key().clone()).collect()
}

/// On an M4A file, refuses — before anything is written — a value the file
/// would not store exactly as the caller gave it, or a field it has no atom
/// for (see [`write_tags`] for the rules and why; the stand-in review of
/// revision 9, M4 and "keys with no M4A atom"). The answer depends on
/// `common_tag` and `value` only, never on what the file holds.
///
/// The field's written form is found by making the very write into an empty
/// tag and letting lofty convert it (`merge_tag`), as it will when saving —
/// so the check follows lofty, not a list kept by hand.
fn refuse_what_an_m4a_would_not_store(
    common_tag: CommonTag,
    value: &str,
) -> Result<(), MetadataError> {
    let refusal = |why: String| {
        MetadataError::WriteError(format!(
            "cannot write {common_tag:?} {value:?} to this M4A file: {why}. Nothing was written. \
             (Issue #102: an M4A write must store exactly what was given.)"
        ))
    };
    let digits = !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    match common_tag {
        CommonTag::TrackNumber
        | CommonTag::DiscNumber
        | CommonTag::TotalTracks
        | CommonTag::TotalDiscs => {
            let in_range = digits && matches!(value.parse::<u32>(), Ok(1..=65535));
            if !in_range {
                return Err(refusal(
                    "an M4A file stores it as a whole number from 1 to 65535, so it must be \
                     given as one, in digits only; the library this crate saves M4A files with \
                     would otherwise drop it, or change it, without a word"
                        .to_string(),
                ));
            }
        }
        CommonTag::Year if !(digits && value.len() == 4 && !value.starts_with('0')) => {
            return Err(refusal(
                "an M4A file keeps the year at the start of its date, so it must be four \
                 digits, as in 2019 (anything else would be dropped, or would break a full \
                 date already there); a full date can be written as the release date \
                 instead"
                    .to_string(),
            ));
        }
        CommonTag::Compilation if !matches!(value, "1" | "0") => {
            return Err(refusal(
                "an M4A file stores it as a number, 1 (yes) or 0 (no), so it must be given as 1 \
                 or 0"
                    .to_string(),
            ));
        }
        _ => {}
    }

    // What lofty writes for it, alone, in an empty tag.
    let mut scratch = Tag::new(TagType::Mp4Ilst);
    write_common_tag_to_lofty(&mut scratch, common_tag, value)?;
    let keys: Vec<ItemKey> = scratch.items().map(|item| item.key().clone()).collect();
    if let Some(key) = keys
        .iter()
        .find(|key| AtomIdent::try_from((*key).clone()).is_err())
    {
        return Err(refusal(format!(
            "the library this crate saves M4A files with has no M4A atom for it (it would be \
             the item {key:?}), so saving would leave it out without a word"
        )));
    }
    if keys.is_empty() {
        return Err(refusal(
            "the library this crate saves M4A files with would store nothing for it".to_string(),
        ));
    }
    let numbers = matches!(
        common_tag,
        CommonTag::TrackNumber
            | CommonTag::DiscNumber
            | CommonTag::TotalTracks
            | CommonTag::TotalDiscs
            | CommonTag::Compilation
    );
    if !numbers {
        // Stored as the text given, every atom of it, and nothing else.
        let (nothing, _) = Ilst::default().split_tag();
        let stored = nothing.merge_tag(scratch);
        let as_given = (&stored)
            .into_iter()
            .flat_map(Atom::data)
            .all(|data| matches!(data, AtomData::UTF8(text) if text == value));
        if !as_given || (&stored).into_iter().next().is_none() {
            return Err(refusal(
                "the library this crate saves M4A files with would not store it as the text \
                 given"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

/// Takes every language atom out of `ilst` (a file written before the
/// stand-in review of revision 5 may have one atom per language), in the
/// order the file holds them.
fn take_mp4_languages(ilst: &mut Ilst) -> Vec<Atom<'static>> {
    ilst.remove(&MP4_LANGUAGE).collect()
}

/// Every text value in `atoms`' `data` atoms, in order — what `read_tags`
/// returns for an MP4 file's language. A `data` atom that is not text is
/// skipped here (it is still kept in the file).
fn mp4_language_texts(atoms: &[Atom<'static>]) -> Vec<String> {
    atoms
        .iter()
        .flat_map(Atom::data)
        .filter_map(|data| match data {
            AtomData::UTF8(text) | AtomData::UTF16(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

// ============================================================
// Internal helpers
// ============================================================

/// Write a single CommonTag value to a lofty Tag, using the appropriate
/// ItemKey for the tag type.
///
/// Fails only for `CommonTag::Language`, when the value is not a language
/// this crate recognises (see `write_language`); every other tag is
/// written as given, exactly as before.
fn write_common_tag_to_lofty(
    tag: &mut Tag,
    common_tag: CommonTag,
    value: &str,
) -> Result<(), MetadataError> {
    match common_tag {
        // Standard accessor fields
        CommonTag::Title => tag.set_title(value.to_string()),
        CommonTag::Artist => tag.set_artist(value.to_string()),
        CommonTag::Album => tag.set_album(value.to_string()),
        CommonTag::Genre => tag.set_genre(value.to_string()),
        CommonTag::Comment => tag.set_comment(value.to_string()),
        CommonTag::Year => {
            if let Ok(y) = value.parse::<u32>() {
                tag.set_year(y);
            }
        }
        CommonTag::TrackNumber => {
            if let Ok(n) = value.parse::<u32>() {
                tag.set_track(n);
            }
        }
        CommonTag::DiscNumber => {
            if let Ok(n) = value.parse::<u32>() {
                tag.set_disk(n);
            }
        }

        // ItemKey-based fields
        CommonTag::AlbumArtist => {
            tag.insert(TagItem::new(
                ItemKey::AlbumArtist,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Composer => {
            tag.insert(TagItem::new(
                ItemKey::Composer,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Copyright => {
            tag.insert(TagItem::new(
                ItemKey::CopyrightMessage,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Label => {
            tag.insert(TagItem::new(ItemKey::Label, ItemValue::Text(value.into())));
        }
        CommonTag::Isrc => {
            tag.insert(TagItem::new(ItemKey::Isrc, ItemValue::Text(value.into())));
        }
        CommonTag::Upc => {
            tag.insert(TagItem::new(
                ItemKey::Barcode,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Encoder => {
            tag.insert(TagItem::new(
                ItemKey::EncoderSoftware,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::TotalTracks => {
            tag.insert(TagItem::new(
                ItemKey::TrackTotal,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::TotalDiscs => {
            tag.insert(TagItem::new(
                ItemKey::DiscTotal,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Lyrics => {
            tag.insert(TagItem::new(ItemKey::Lyrics, ItemValue::Text(value.into())));
        }
        CommonTag::ReleaseDate => {
            tag.insert(TagItem::new(
                ItemKey::RecordingDate,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Compilation => {
            tag.insert(TagItem::new(
                ItemKey::FlagCompilation,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Description => {
            tag.insert(TagItem::new(
                ItemKey::Description,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::MusicBrainzRecordingId => {
            tag.insert(TagItem::new(
                ItemKey::MusicBrainzRecordingId,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::MusicBrainzReleaseId => {
            tag.insert(TagItem::new(
                ItemKey::MusicBrainzReleaseId,
                ItemValue::Text(value.into()),
            ));
        }

        // Custom/freeform fields — use Unknown key with standard field names
        CommonTag::AcoustId => {
            // insert_unchecked — lofty Tag::insert() drops ItemKey::Unknown (pre-existing latent bug fixed with #65).
            tag.insert_unchecked(TagItem::new(
                ItemKey::Unknown("Acoustid Id".into()),
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::ReplayGainTrackGain => {
            tag.insert(TagItem::new(
                ItemKey::ReplayGainTrackGain,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::ReplayGainTrackPeak => {
            tag.insert(TagItem::new(
                ItemKey::ReplayGainTrackPeak,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::ReplayGainAlbumGain => {
            tag.insert(TagItem::new(
                ItemKey::ReplayGainAlbumGain,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::ReplayGainAlbumPeak => {
            tag.insert(TagItem::new(
                ItemKey::ReplayGainAlbumPeak,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::ReplayGainReferenceLoudness => {
            // insert_unchecked — Unknown key, see AcoustId (pre-existing, fixed with #65).
            tag.insert_unchecked(TagItem::new(
                ItemKey::Unknown("REPLAYGAIN_REFERENCE_LOUDNESS".into()),
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::CatalogNumber => {
            tag.insert(TagItem::new(
                ItemKey::CatalogNumber,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Barcode => {
            tag.insert(TagItem::new(
                ItemKey::Barcode,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::OriginalDate => {
            tag.insert(TagItem::new(
                ItemKey::OriginalReleaseDate,
                ItemValue::Text(value.into()),
            ));
        }

        // --- Work / release-group identifiers (#65) ---
        CommonTag::MusicBrainzReleaseGroupId => {
            tag.insert(TagItem::new(
                ItemKey::MusicBrainzReleaseGroupId,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::MusicBrainzWorkId => {
            tag.insert(TagItem::new(
                ItemKey::MusicBrainzWorkId,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Iswc => {
            // lofty has no dedicated ISWC ItemKey (verified lofty 0.22.4), so
            // this is a freeform Unknown key. It MUST use insert_unchecked:
            // Tag::insert() runs re_map(allow_unknown=false) which rejects
            // EVERY ItemKey::Unknown, silently dropping the value before it
            // even enters the Tag (lofty's own doc: insert_unchecked "is only
            // necessary if dealing with ItemKey::Unknown"). Serialises as
            // TXXX:ISWC (ID3v2) / ISWC (Vorbis). #65.
            tag.insert_unchecked(TagItem::new(
                ItemKey::Unknown("ISWC".into()),
                ItemValue::Text(value.into()),
            ));
        }

        // --- Core info (#65) ---
        CommonTag::Subtitle => {
            tag.insert(TagItem::new(
                ItemKey::TrackSubtitle,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Language => write_language(tag, value)?,

        // --- Contributor roles beyond Composer (#65) ---
        CommonTag::Lyricist => {
            tag.insert(TagItem::new(
                ItemKey::Lyricist,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Conductor => {
            tag.insert(TagItem::new(
                ItemKey::Conductor,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Remixer => {
            tag.insert(TagItem::new(
                ItemKey::Remixer,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Arranger => {
            // Role keys (Arranger/Producer/Engineer/Mixer) MUST use
            // insert_unchecked: lofty's ID3V2_MAP has no direct entry for them,
            // so Tag::insert() re_map fails and drops the item before it enters
            // the Tag. insert_unchecked pushes ItemKey::Arranger in; on ID3v2
            // save, `impl From<Tag> for Id3v2Tag` -> merge_tag's TIPL block
            // take_strings(ItemKey::Arranger) synthesises TIPL:arranger
            // (verified lofty 0.22.4 id3/v2/tag.rs). Vorbis writes ARRANGER.
            // MP4 ilst has NO arranger mapping, so M4A still drops it. #65
            // round-trip test: contributor_roles_id3v2_roundtrip.
            tag.insert_unchecked(TagItem::new(
                ItemKey::Arranger,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Producer => {
            // insert_unchecked — TIPL role, see the Arranger arm (#65).
            tag.insert_unchecked(TagItem::new(
                ItemKey::Producer,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Engineer => {
            // insert_unchecked — TIPL role, see the Arranger arm (#65).
            tag.insert_unchecked(TagItem::new(
                ItemKey::Engineer,
                ItemValue::Text(value.into()),
            ));
        }
        CommonTag::Mixer => {
            // insert_unchecked — TIPL role, see the Arranger arm (#65).
            tag.insert_unchecked(TagItem::new(
                ItemKey::MixEngineer,
                ItemValue::Text(value.into()),
            ));
        }
    }
    Ok(())
}

/// Writes a `CommonTag::Language` value (policy MWBM-MEDIA-LANG,
/// TRACK-070) into an in-memory `tag`, or refuses it. `write_tags` does
/// not come through here — it reads every `Language` entry of a call
/// together and stores them through `edit_and_save` (which is also what
/// makes the MP4 form below real) — but the conversion and the refusal
/// are the same ones, from `language_values_to_write` and
/// `put_languages`.
///
/// **Reading the caller's value.** It goes through the LANG-002 reader
/// that handles several values (`meedya_lang::from_legacy_three_letter_all`),
/// because a language field may hold more than one language, separated by
/// a null character (`"eng\0fra"` — ID3v2.4's own way of listing several
/// values). Every value is kept, in the order given; the first is the
/// primary language. (Until Codex's review r7 this used the single-value
/// reader, which returns the first value only, so `"eng\0fra"` was written
/// as `eng` alone and French was silently lost.)
///
/// **What is written, per value.**
/// - ID3v2 has no full-tag language field at all — TRACK-070's table shows
///   a dash — only the three-letter `TLAN` frame, so each value becomes its
///   ISO 639-2 **terminology** code (`pt-BR` → `por`), or `und` where the
///   language has none (`yue`, a private-use or grandfathered tag). The
///   codes go into ONE item, separated by null characters: one `TLAN` frame
///   holding `por\0deu`, which is ID3v2.4's own multi-value form (lofty
///   writes ID3v2.4 by default, so the nulls are kept). One item per value
///   does NOT work on a file: lofty makes one frame per item when it saves
///   a file, and a reader keeps only the LAST `TLAN` frame — see
///   [`gather_languages_before_saving`], which is what keeps this one item
///   whole through every later save too (a file read back holds one item
///   PER LANGUAGE again).
/// - Every other format has a free-text field that takes the canonical
///   BCP 47 tag (`eng` → `en`, `EN-gb` → `en-GB`).
///   - Vorbis comments hold several values as several `LANGUAGE` fields,
///     so one item is pushed per value (measured on real FLAC and Opus
///     files: every value reads back, in order).
///   - MP4 holds several values as ONE `----:com.apple.iTunes:LANGUAGE`
///     atom with one `data` atom per value — the form iTunes, mutagen and
///     mp4ameta write, and the one ffprobe reads its first value from. A
///     format-neutral `Tag` cannot express that (lofty turns each item into
///     an atom of its own), so in memory one item is pushed per value, and
///     `edit_and_save` writes the file's atom itself, through lofty's `Ilst`.
///     (Revision 5 saved the items as they were: one atom per language, and
///     ffprobe then showed the LAST language as the file's language.)
///   - APE is like ID3v2: an APE tag holds each key once (lofty's
///     `ApeTag::insert` replaces an existing item), and APEv2's own way of
///     listing several values is one item with the values separated by
///     null characters — which is what is written.
///
/// **Refusing.** A value the reader does not recognise — `zzz`, a language
/// *name* (`English`), a locale name (`en_GB`), or an empty value — is
/// refused with [`MetadataError::UnrecognisedLanguage`], and one such value
/// among several refuses the whole value: nothing is changed in `tag`.
/// Writing `und` in its place would quietly lose what the caller said, and
/// writing the text would put something that is not a language into a
/// language field (LANG-002: an unrecognised value's structured value is
/// `und`, with the original text kept alongside — a file's language field
/// has nowhere to keep the original, so the caller has to decide; COMPAT-040:
/// report doubt, never resolve it by guessing). Until Codex's review r7 an
/// unrecognised value was written as-is to free-text fields and as `und` to
/// ID3v2. Special values are recognised and written normally: `und`
/// (not known), `mul`, `zxx`, `mis`, the local-use range `qaa`–`qtz`,
/// private-use tags (`x-…`) and grandfathered tags. (`write_tags` refuses
/// only a CHANGED value: one identical to what the tag being written
/// already holds is left alone before it ever gets here, and one that
/// `read_tags` found in another tag of the file is skipped instead of
/// refused — see its doc comment.)
fn write_language(tag: &mut Tag, value: &str) -> Result<(), MetadataError> {
    let values = language_values_to_write(&[value], tag.tag_type())?;
    put_languages(tag, values);
    Ok(())
}

/// Stores `values` — already converted by `language_values_to_write` — as
/// `tag`'s languages, replacing any it had: one null-separated item on
/// ID3v2 and APE, one item per value everywhere else. See
/// [`write_language`] for why.
fn put_languages(tag: &mut Tag, values: Vec<String>) {
    tag.remove_key(&ItemKey::Language);
    if matches!(tag.tag_type(), TagType::Id3v2 | TagType::Ape) {
        tag.insert(TagItem::new(
            ItemKey::Language,
            ItemValue::Text(values.join("\0")),
        ));
    } else {
        for write_value in values {
            // `push`, not `insert`: `insert` replaces every earlier item
            // with the same key, which would keep only the last value.
            tag.push(TagItem::new(
                ItemKey::Language,
                ItemValue::Text(write_value),
            ));
        }
    }
}

/// The text to store for every language in `entries` — each entry one
/// value as a caller gave it, which may itself list several languages
/// separated by null characters — in order, for a tag of type `tag_type`;
/// or the refusal, naming the first value that is not recognised. Each
/// entry is read on its own, so the languages from `["eng\0fra", "deu"]`
/// are exactly those of `"eng\0fra\0deu"`. See [`write_language`] for the
/// rules.
fn language_values_to_write(
    entries: &[&str],
    tag_type: TagType,
) -> Result<Vec<String>, MetadataError> {
    let mut values = Vec::new();
    for (entry_index, entry) in entries.iter().enumerate() {
        let read = meedya_lang::from_legacy_three_letter_all(entry);
        let count = read.len();
        for (index, parsed) in read.into_iter().enumerate() {
            let Some(lang_tag) = parsed else {
                let mut problem = describe_unrecognised_language(entry, index, count);
                if entries.len() > 1 {
                    // Several `Language` entries in one call: say which.
                    problem = format!(
                        "in language entry {} of the {} given together, {problem}",
                        entry_index + 1,
                        entries.len()
                    );
                }
                return Err(MetadataError::UnrecognisedLanguage {
                    value: (*entry).to_string(),
                    problem,
                });
            };
            values.push(if tag_type == TagType::Id3v2 {
                meedya_lang::iso639_2_code(&lang_tag, meedya_lang::Iso639Form::Terminology)
            } else {
                lang_tag.tag
            });
        }
    }
    Ok(values)
}

/// Says, in plain words, which part of `value` was not recognised, for the
/// refusal message. `index` and `count` come from
/// `from_legacy_three_letter_all`; the value is split here exactly as that
/// reader splits it (trailing null characters, then LANG-001's four
/// whitespace characters, come off the whole value; it is split at each
/// null character; each part is trimmed of the same whitespace), so the
/// part named is the part the reader refused.
fn describe_unrecognised_language(value: &str, index: usize, count: usize) -> String {
    let lang_whitespace = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let part = value
        .trim_end_matches('\0')
        .trim_matches(lang_whitespace)
        .split('\0')
        .map(|p| p.trim_matches(lang_whitespace))
        .nth(index)
        .unwrap_or("");
    // Which value: the whole value when there is only one, otherwise its
    // position ("value 2 of 3") so a caller with a long list can find it.
    let subject = match (count > 1, part.is_empty()) {
        (false, true) => "it".to_string(),
        (false, false) => format!("{part:?}"),
        (true, true) => format!("value {} of {count}", index + 1),
        (true, false) => format!("value {} of {count}, {part:?},", index + 1),
    };
    let mut problem = if part.is_empty() {
        format!("{subject} is empty, and an empty value is not a language")
    } else {
        format!("{subject} is not a language tag this library recognises")
    };
    if part.contains('_') {
        // A hint only — the value is still refused, never converted: the
        // caller may have meant something else entirely.
        problem.push_str(
            " (it looks like an operating-system locale name; in a language tag the parts are \
             joined with hyphens, as in `en-GB`)",
        );
    }
    if count > 1 {
        problem.push_str(&format!(", so none of the {count} values was written"));
    }
    problem
}

#[cfg(test)]
mod tests {
    use super::*;
    // `TagType` used to be imported here only, kept out of the
    // module-level import so a non-test build didn't warn about it being
    // unused — now that the `Language` write (`write_language`)
    // reads `tag.tag_type()` in real (non-test) code too, the
    // module-level `use` above already brings it in via `use super::*;`,
    // so a second import here would be a redundant/unused-import warning.

    // ------------------------------------------------------------------
    // #79 — untagged-container fixtures
    //
    // insert_tag() silently no-ops for a container that doesn't support the
    // given tag type, and the pre-fix code hardcoded TagType::Id3v2 as the
    // fallback for a file with no primary_tag() (i.e. any freshly downloaded,
    // untagged file). On an untagged .m4a — the standard MeedyaDL download
    // product — that meant: no primary tag -> fallback Id3v2 -> insert_tag
    // no-ops because MP4 doesn't support Id3v2 -> tag_mut(Id3v2) is still
    // None -> `.unwrap()` panics. These tests build minimal, genuinely
    // untagged containers (no VORBIS_COMMENT / ilst block at all) byte-by-
    // byte rather than shipping binary fixtures in git, so the panic
    // reproduces honestly instead of being masked by an already-tagged file.
    // ------------------------------------------------------------------

    /// Build one MP4/ISO-BMFF atom: 4-byte big-endian size (content + header)
    /// + 4-byte fourcc + content.
    fn atom(fourcc: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + content.len());
        out.extend_from_slice(&((content.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(fourcc);
        out.extend_from_slice(content);
        out
    }

    /// Minimal, valid, UNTAGGED MP4/M4A container: `ftyp` + `moov > trak >
    /// mdia > (mdhd, hdlr="soun")`. No `udta`/`meta`/`ilst` (untagged) and no
    /// sample tables (so lofty reports zero-valued properties) — just enough
    /// for lofty's MP4 reader to recognise one audio track, which is all
    /// `TaggedFileExt::primary_tag_type()` (Mp4 -> Mp4Ilst) and the writer
    /// need. Verified against lofty 0.22.4's mp4::read/moov/properties
    /// parsing (find_audio_trak requires an "soun" hdlr + mdhd; minf/stbl are
    /// optional).
    fn minimal_untagged_m4a() -> Vec<u8> {
        let mut ftyp_content = Vec::new();
        ftyp_content.extend_from_slice(b"M4A "); // major brand
        ftyp_content.extend_from_slice(&0u32.to_be_bytes()); // minor version
        ftyp_content.extend_from_slice(b"M4A "); // compatible brand
        let ftyp = atom(b"ftyp", &ftyp_content);

        let mut mdhd_content = Vec::new();
        mdhd_content.push(0); // version
        mdhd_content.extend_from_slice(&[0, 0, 0]); // flags
        mdhd_content.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        mdhd_content.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        mdhd_content.extend_from_slice(&44_100u32.to_be_bytes()); // timescale
        mdhd_content.extend_from_slice(&0u32.to_be_bytes()); // duration
        let mdhd = atom(b"mdhd", &mdhd_content);

        let mut hdlr_content = Vec::new();
        hdlr_content.extend_from_slice(&0u32.to_be_bytes()); // version + flags
        hdlr_content.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        hdlr_content.extend_from_slice(b"soun"); // handler_type -> marks the audio track
        let hdlr = atom(b"hdlr", &hdlr_content);

        let mdia = atom(b"mdia", &[mdhd, hdlr].concat());
        let trak = atom(b"trak", &mdia);
        let moov = atom(b"moov", &trak);

        [ftyp, moov].concat()
    }

    /// One FLAC metadata block: 1-bit last-block flag + 7-bit type in the
    /// first byte, then a 24-bit big-endian content length, then content.
    fn flac_block(last: bool, block_type: u8, content: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + content.len());
        out.push((u8::from(last) << 7) | (block_type & 0x7F));
        out.extend_from_slice(&(content.len() as u32).to_be_bytes()[1..]);
        out.extend_from_slice(content);
        out
    }

    /// Minimal, valid, UNTAGGED FLAC stream: `"fLaC"` marker + a zeroed
    /// STREAMINFO block + a trailing (last-block) PADDING block. No
    /// VORBIS_COMMENT block, so lofty reports no primary tag and
    /// `primary_tag_type()` falls back to FLAC's native VorbisComments —
    /// exactly the fallback path under test.
    ///
    /// The trailing PADDING block isn't optional set-dressing: a FLAC file
    /// whose *only* metadata block is STREAMINFO (last=true, nothing after
    /// it) trips a real lofty write-path bug — flac/write.rs patches the
    /// previous last-block's header at an absolute file offset into a byte
    /// buffer that only holds the post-STREAMINFO tail, panicking with an
    /// out-of-bounds index (lofty's own code notes this padding logic is
    /// incomplete: `TODO ... lofty-rs/issues/445`). Ending on a PADDING
    /// block (as real encoders normally do, reserving room for tags) makes
    /// `end_padding_exists` true and skips that code path entirely — this
    /// fixture exercises meedya-metadata's #79 fallback fix, not an
    /// unrelated upstream corner case.
    fn minimal_untagged_flac() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"fLaC");
        out.extend_from_slice(&flac_block(false, 0, &[0u8; 34])); // STREAMINFO
        out.extend_from_slice(&flac_block(true, 1, &[0u8; 16])); // PADDING (last)
        out
    }

    /// Ogg's CRC-32: polynomial 0x04c11db7, zero init, **no** input/output
    /// reflection and no final XOR — deliberately not the common zlib
    /// CRC-32, so a stock crc32 crate would produce a page every parser
    /// rejects.
    fn ogg_crc(data: &[u8]) -> u32 {
        let mut crc: u32 = 0;
        for &byte in data {
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ 0x04c1_1db7
                } else {
                    crc << 1
                };
            }
        }
        crc
    }

    /// Build one Ogg page around `payload`, filling in the segment table and
    /// the CRC (which is computed over the whole page with the CRC field
    /// zeroed).
    fn ogg_page(header_type: u8, serial: u32, seq: u32, payload: &[u8]) -> Vec<u8> {
        let mut segments: Vec<u8> = Vec::new();
        let mut remaining = payload.len();
        while remaining >= 255 {
            segments.push(255);
            remaining -= 255;
        }
        segments.push(u8::try_from(remaining).expect("remaining < 255"));

        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // stream structure version
        page.push(header_type);
        page.extend_from_slice(&0i64.to_le_bytes()); // granule position
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&seq.to_le_bytes());
        page.extend_from_slice(&[0u8; 4]); // CRC placeholder
        page.push(u8::try_from(segments.len()).expect("segment count fits"));
        page.extend_from_slice(&segments);
        page.extend_from_slice(payload);

        let crc = ogg_crc(&page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        page
    }

    /// A minimal, valid, **untagged** Ogg Opus stream: an OpusHead
    /// identification page followed by an OpusTags comment page carrying a
    /// vendor string and zero user comments.
    ///
    /// "Untagged" for Opus means exactly this — the spec *requires* an
    /// OpusTags packet, so a stream with no comment header at all is
    /// malformed rather than untagged. Zero user comments is the real-world
    /// untagged state, and it is the state that exercises the #79 fix:
    /// `primary_tag()` finds nothing to prefer, so the fallback path runs.
    fn minimal_untagged_opus() -> Vec<u8> {
        const SERIAL: u32 = 0xDEAD_BEEF;

        let mut head = Vec::new();
        head.extend_from_slice(b"OpusHead");
        head.push(1); // version
        head.push(2); // channel count
        head.extend_from_slice(&312u16.to_le_bytes()); // pre-skip
        head.extend_from_slice(&48_000u32.to_le_bytes()); // input sample rate
        head.extend_from_slice(&0i16.to_le_bytes()); // output gain
        head.push(0); // channel mapping family

        let vendor = b"MeedyaSuite";
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        tags.extend_from_slice(&u32::try_from(vendor.len()).expect("fits").to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes()); // zero user comments

        let mut out = ogg_page(0x02, SERIAL, 0, &head); // BOS
        out.extend_from_slice(&ogg_page(0x00, SERIAL, 1, &tags));
        out
    }

    /// Ogg was one of the container families named in #79 as panicking.
    ///
    /// Note this shares the *code path* with the FLAC test above — both
    /// resolve to `TagType::VorbisComments` via `primary_tag_type()` — so
    /// what this adds is container-level coverage: it proves the fix works
    /// against Ogg page framing, not just FLAC's metadata-block layout.
    #[test]
    fn write_tags_on_untagged_opus_does_not_panic_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("untagged.opus");
        std::fs::write(&path, minimal_untagged_opus()).expect("write fixture");

        write_tags(
            &path,
            &[
                (CommonTag::Title, "Fixture Title".into()),
                (CommonTag::Artist, "Fixture Artist".into()),
            ],
        )
        .expect("write_tags on untagged opus");

        let read_back = read_tags(&path).expect("read_tags on written opus");
        assert_eq!(
            read_back.get(&CommonTag::Title).map(Vec::as_slice),
            Some(["Fixture Title".to_string()].as_slice())
        );
        assert_eq!(
            read_back.get(&CommonTag::Artist).map(Vec::as_slice),
            Some(["Fixture Artist".to_string()].as_slice())
        );
    }

    #[test]
    fn write_tags_on_untagged_m4a_does_not_panic_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("untagged.m4a");
        std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");

        // Pre-fix, this panicked inside write_tags (see the module comment).
        write_tags(
            &path,
            &[
                (CommonTag::Title, "Fixture Title".into()),
                (CommonTag::Artist, "Fixture Artist".into()),
            ],
        )
        .expect("write_tags on untagged m4a");

        let read_back = read_tags(&path).expect("read_tags on written m4a");
        assert_eq!(
            read_back.get(&CommonTag::Title).map(Vec::as_slice),
            Some(["Fixture Title".to_string()].as_slice())
        );
        assert_eq!(
            read_back.get(&CommonTag::Artist).map(Vec::as_slice),
            Some(["Fixture Artist".to_string()].as_slice())
        );
    }

    #[test]
    fn write_tags_on_untagged_flac_does_not_panic_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("untagged.flac");
        std::fs::write(&path, minimal_untagged_flac()).expect("write fixture");

        write_tags(
            &path,
            &[
                (CommonTag::Title, "Fixture Title".into()),
                (CommonTag::Album, "Fixture Album".into()),
            ],
        )
        .expect("write_tags on untagged flac");

        let read_back = read_tags(&path).expect("read_tags on written flac");
        assert_eq!(
            read_back.get(&CommonTag::Title).map(Vec::as_slice),
            Some(["Fixture Title".to_string()].as_slice())
        );
        assert_eq!(
            read_back.get(&CommonTag::Album).map(Vec::as_slice),
            Some(["Fixture Album".to_string()].as_slice())
        );
    }

    #[test]
    fn read_nonexistent_file_returns_error() {
        let result = read_tags(Path::new("/nonexistent/file.mp3"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            MetadataError::FileNotFound(_)
        ));
    }

    #[test]
    fn write_nonexistent_file_returns_error() {
        let result = write_tags(
            Path::new("/nonexistent/file.mp3"),
            &[(CommonTag::Title, "Test".into())],
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            MetadataError::FileNotFound(_)
        ));
    }

    #[test]
    fn replaygain_tag_values() {
        // Verify the convenience function produces correct tag tuples
        let rg = meedya_fingerprint::ReplayGainResult {
            integrated_loudness: -14.2,
            true_peak: 0.933,
            gain_db: -3.80,
            reference_level: -18.0,
        };
        // Check formatting matches ReplayGain spec
        assert_eq!(rg.gain_string(), "-3.80 dB");
        assert_eq!(rg.peak_string(), "0.933000");
    }

    #[test]
    fn acoustid_tag_values() {
        let result = meedya_fingerprint::AcoustIdResult {
            acoustid: "abc-def-123".into(),
            score: 0.95,
            recording_ids: vec!["mb-rec-001".into(), "mb-rec-002".into()],
            fingerprint: "AQAA".into(),
            duration_secs: 240,
        };
        // First MB recording ID should be used
        assert_eq!(result.recording_ids.first().unwrap(), "mb-rec-001");
    }

    #[test]
    fn write_common_tag_mapping() {
        // Verify that all CommonTag variants have a write implementation.
        // Derived from the tree via strum::EnumIter (#65) rather than a
        // hand-typed subset — a new variant added to the enum is exercised
        // here automatically, with no test edit required.
        use strum::IntoEnumIterator;

        let tag_type = TagType::Id3v2;
        let mut tag = Tag::new(tag_type);

        // Should not panic, and should be accepted, for any variant. "1"
        // parses fine for the numeric arms (Year/TrackNumber/DiscNumber).
        // `Language` is the one arm that can refuse (policy MWBM-MEDIA-LANG,
        // Codex review r7): "1" is not a language, so that arm is given a
        // real one here, and "1" is proved to be refused just below.
        for variant in CommonTag::iter() {
            let value = if variant == CommonTag::Language {
                "en"
            } else {
                "1"
            };
            write_common_tag_to_lofty(&mut tag, variant, value)
                .unwrap_or_else(|e| panic!("{variant:?} refused {value:?}: {e}"));
        }
        assert!(matches!(
            write_common_tag_to_lofty(&mut tag, CommonTag::Language, "1"),
            Err(MetadataError::UnrecognisedLanguage { .. })
        ));

        // Keep the original title assertion by writing Title last.
        write_common_tag_to_lofty(&mut tag, CommonTag::Title, "Test").expect("title");
        assert_eq!(tag.title().as_deref(), Some("Test"));
    }

    // ------------------------------------------------------------------
    // Behavioural save/reload round-trip (#65)
    //
    // ELI5: prove the new tags actually survive being written and read back —
    // not just that the write code runs without panicking.
    //
    // Why: `write_common_tag_mapping` above only checks no arm panics; it never
    // asserts a value PERSISTS. lofty's `Tag::insert()` SILENTLY DROPS (a)
    // every `ItemKey::Unknown` (its `re_map` runs with allow_unknown=false) and
    // (b) role keys with no `ID3V2_MAP` entry — Arranger/Producer/Engineer/
    // MixEngineer, which lofty instead synthesises into the ID3v2 `TIPL` frame
    // at save time inside `impl From<Tag> for Id3v2Tag`. So a write arm using
    // `insert()` for one of those keys would compile, run, pass the panic-only
    // test, and lose the data at the first save. These tests exercise the REAL
    // conversion (`Tag -> Id3v2Tag -> Tag` merge/split, and the Vorbis
    // equivalent) so reverting any of the 8 `insert_unchecked` arms in
    // `write_common_tag_to_lofty` back to `insert()` turns the matching
    // assertion RED — the mechanism the correctness review asked for.
    //
    // Refs: lofty 0.22.4 src/tag/mod.rs::{insert,insert_unchecked},
    // src/id3/v2/util/mappings.rs (TIPL_MAPPINGS), src/id3/v2/tag.rs
    // (TIPL merge ~line 1481 / split ~line 1077). #65.
    // ------------------------------------------------------------------

    /// Write one `CommonTag`, round-trip it through a full ID3v2 save (`merge`)
    /// then reload (`split`), and return the value recovered under
    /// `recover_key` (`None` if lofty dropped it). No audio fixture needed —
    /// the conversion is pure in-memory.
    fn id3v2_roundtrip(variant: CommonTag, value: &str, recover_key: &ItemKey) -> Option<String> {
        use lofty::id3::v2::Id3v2Tag;
        let mut tag = Tag::new(TagType::Id3v2);
        write_common_tag_to_lofty(&mut tag, variant, value).expect("value accepted");
        // `Id3v2Tag::from` runs the save-side merge (TIPL synthesis etc.);
        // `Tag::from` runs the reload-side split back into ItemKeys.
        let id3 = Id3v2Tag::from(tag);
        let back = Tag::from(id3);
        back.get_string(recover_key).map(str::to_string)
    }

    /// Vorbis equivalent of `id3v2_roundtrip`. All new role keys DO have a
    /// direct `VORBIS_MAP` entry, so `insert()` would work here — but the
    /// Unknown-key tags (ISWC) still require `insert_unchecked` on Vorbis too.
    fn vorbis_roundtrip(variant: CommonTag, value: &str, recover_key: &ItemKey) -> Option<String> {
        use lofty::ogg::VorbisComments;
        let mut tag = Tag::new(TagType::VorbisComments);
        write_common_tag_to_lofty(&mut tag, variant, value).expect("value accepted");
        let vc = VorbisComments::from(tag);
        let back = Tag::from(vc);
        back.get_string(recover_key).map(str::to_string)
    }

    #[test]
    fn contributor_roles_survive_id3v2_save_reload() {
        // Arranger/Producer/Engineer/Mixer have NO direct ID3v2 frame; lofty
        // synthesises them into TIPL only if the ItemKey is present in the Tag,
        // which requires `insert_unchecked` (insert() drops them first).
        assert_eq!(
            id3v2_roundtrip(CommonTag::Arranger, "Ada Arr", &ItemKey::Arranger).as_deref(),
            Some("Ada Arr"),
            "Arranger dropped — write arm must use insert_unchecked (TIPL)"
        );
        assert_eq!(
            id3v2_roundtrip(CommonTag::Producer, "Pat Prod", &ItemKey::Producer).as_deref(),
            Some("Pat Prod"),
            "Producer dropped — write arm must use insert_unchecked (TIPL)"
        );
        assert_eq!(
            id3v2_roundtrip(CommonTag::Engineer, "Eve Eng", &ItemKey::Engineer).as_deref(),
            Some("Eve Eng"),
            "Engineer dropped — write arm must use insert_unchecked (TIPL)"
        );
        assert_eq!(
            id3v2_roundtrip(CommonTag::Mixer, "Max Mix", &ItemKey::MixEngineer).as_deref(),
            Some("Max Mix"),
            "Mixer dropped — write arm must use insert_unchecked (TIPL)"
        );
    }

    #[test]
    fn iswc_survives_id3v2_save_reload() {
        // ISWC is a freeform Unknown("ISWC") key; insert() rejects all Unknown
        // keys, so only insert_unchecked lands it (serialises as TXXX:ISWC).
        assert_eq!(
            id3v2_roundtrip(
                CommonTag::Iswc,
                "T-345246800-1",
                &ItemKey::Unknown("ISWC".into())
            )
            .as_deref(),
            Some("T-345246800-1"),
            "ISWC dropped — write arm must use insert_unchecked (TXXX)"
        );
    }

    #[test]
    fn acoustid_survives_id3v2_save_reload() {
        // AcoustID is a freeform Unknown("Acoustid Id") key; insert() rejects
        // all Unknown keys, so only insert_unchecked lands it (serialises as
        // TXXX:Acoustid Id). This also proves the exact literal key used on
        // write matches read_tags' freeform-mapping table, so a written
        // AcoustID is recoverable rather than silently one-way lost (#65).
        assert_eq!(
            id3v2_roundtrip(
                CommonTag::AcoustId,
                "eb31d1c3-950e-468b-9e36-e46fa75b1291",
                &ItemKey::Unknown("Acoustid Id".into())
            )
            .as_deref(),
            Some("eb31d1c3-950e-468b-9e36-e46fa75b1291"),
            "AcoustID dropped — write arm must use insert_unchecked (TXXX)"
        );
    }

    #[test]
    fn mapped_roles_survive_id3v2_save_reload() {
        // Control group: Lyricist/Conductor/Remixer DO have direct ID3v2 frames
        // (TEXT/TPE3/TPE4) and legitimately use insert(). Asserting they too
        // round-trip proves the harness recovers real data (not green-because-
        // the-conversion-eats-everything).
        assert_eq!(
            id3v2_roundtrip(CommonTag::Lyricist, "Lee Lyr", &ItemKey::Lyricist).as_deref(),
            Some("Lee Lyr")
        );
        assert_eq!(
            id3v2_roundtrip(CommonTag::Conductor, "Cy Con", &ItemKey::Conductor).as_deref(),
            Some("Cy Con")
        );
        assert_eq!(
            id3v2_roundtrip(CommonTag::Remixer, "Rex Rem", &ItemKey::Remixer).as_deref(),
            Some("Rex Rem")
        );
    }

    #[test]
    fn iswc_and_roles_survive_vorbis_save_reload() {
        assert_eq!(
            vorbis_roundtrip(
                CommonTag::Iswc,
                "T-345246800-1",
                &ItemKey::Unknown("ISWC".into())
            )
            .as_deref(),
            Some("T-345246800-1"),
            "ISWC dropped on Vorbis — write arm must use insert_unchecked"
        );
        // Roles have VORBIS_MAP entries, but insert_unchecked must still work.
        assert_eq!(
            vorbis_roundtrip(CommonTag::Arranger, "Ada", &ItemKey::Arranger).as_deref(),
            Some("Ada")
        );
        assert_eq!(
            vorbis_roundtrip(CommonTag::Producer, "Pat", &ItemKey::Producer).as_deref(),
            Some("Pat")
        );
        assert_eq!(
            vorbis_roundtrip(CommonTag::Engineer, "Eve", &ItemKey::Engineer).as_deref(),
            Some("Eve")
        );
        assert_eq!(
            vorbis_roundtrip(CommonTag::Mixer, "Max", &ItemKey::MixEngineer).as_deref(),
            Some("Max")
        );
    }

    // ------------------------------------------------------------------
    // CommonTag::Language (policy MWBM-MEDIA-LANG, TRACK-070)
    //
    // ID3v2 has no full-tag language field at all (TRACK-070's table
    // shows a dash for it) — only the three-letter `TLAN` frame, which
    // `id3v2_roundtrip` exercises via `ItemKey::Language`. FLAC's
    // container tag type is Vorbis Comments, whose `LANGUAGE` field is
    // free text and takes the canonical BCP 47 tag — `vorbis_roundtrip`
    // stands in for "write to a FLAC file" without needing a real FLAC
    // fixture, the same way the roundtrip helpers above stand in for a
    // real MP3 file. For a SINGLE value that stand-in is faithful; for
    // several values it is not (lofty joins several items into one frame
    // in memory but not when saving a file — see `write_language`), which
    // is why the several-value tests further down also use real files.
    // ------------------------------------------------------------------

    #[test]
    fn language_pt_br_writes_iso639_2_terminology_code_on_id3v2() {
        // `por` is Portuguese's ISO 639-2 terminology code (there is no
        // separate bibliographic form for Portuguese, but TRACK-070 always
        // asks for the terminology form on ID3v2 regardless).
        assert_eq!(
            id3v2_roundtrip(CommonTag::Language, "pt-BR", &ItemKey::Language).as_deref(),
            Some("por")
        );
    }

    #[test]
    fn language_pt_br_writes_canonical_tag_on_flac_vorbis() {
        // Already canonical, so it round-trips unchanged — region and
        // script would be lost if this were forced through the
        // three-letter form instead (TRACK-070's reason the full-tag
        // field must always be written where one exists).
        assert_eq!(
            vorbis_roundtrip(CommonTag::Language, "pt-BR", &ItemKey::Language).as_deref(),
            Some("pt-BR")
        );
    }

    #[test]
    fn language_old_three_letter_code_is_canonicalised_on_flac_vorbis() {
        // `eng` is what MusicBrainz Picard and others write into Vorbis
        // `LANGUAGE` (LANG-002's own example); this format gets the
        // canonical two-letter form back, not the three-letter input
        // echoed unchanged.
        assert_eq!(
            vorbis_roundtrip(CommonTag::Language, "eng", &ItemKey::Language).as_deref(),
            Some("en")
        );
    }

    // ------------------------------------------------------------------
    // Refusing an unrecognised language (policy MWBM-MEDIA-LANG, LANG-002
    // and COMPAT-040; Codex review r7)
    //
    // Until review r7, `zzz` was written unchanged into free-text fields
    // (Vorbis, MP4, APE) and as `und` into ID3v2. Both are wrong: the
    // first stores something that is not a language in a language field,
    // the second silently loses what the caller said. The value is now
    // refused, with a message that says what was wrong and what to give
    // instead, on every format alike.
    // ------------------------------------------------------------------

    /// Every tag type `write_tags` can end up writing a language into.
    const LANGUAGE_TAG_TYPES: [TagType; 4] = [
        TagType::Id3v2,
        TagType::VorbisComments,
        TagType::Mp4Ilst,
        TagType::Ape,
    ];

    /// The refusal `write_common_tag_to_lofty` gives for `value` on a fresh
    /// tag of `tag_type`, as its message; panics if the value was accepted.
    fn language_refusal(tag_type: TagType, value: &str) -> String {
        let mut tag = Tag::new(tag_type);
        match write_common_tag_to_lofty(&mut tag, CommonTag::Language, value) {
            Err(e @ MetadataError::UnrecognisedLanguage { .. }) => {
                assert!(
                    tag.get_strings(&ItemKey::Language).next().is_none(),
                    "{tag_type:?}: a refused value must not leave anything in the tag"
                );
                e.to_string()
            }
            Err(other) => panic!("{tag_type:?}: {value:?} gave the wrong error: {other}"),
            Ok(()) => panic!("{tag_type:?}: {value:?} was accepted, but must be refused"),
        }
    }

    #[test]
    fn language_unrecognised_value_is_refused_on_every_format() {
        // `zzz`: not an ISO 639-2 code, not a registered subtag, not in
        // the local-use range. `English`: a language NAME. `en_GB`: an
        // operating-system locale name, never converted. `""` and `" "`:
        // nothing at all.
        for tag_type in LANGUAGE_TAG_TYPES {
            for value in ["zzz", "English", "en_GB", "", " "] {
                let message = language_refusal(tag_type, value);
                // The message says plainly what to give instead.
                for example in ["`en`", "`pt-BR`", "`und`"] {
                    assert!(
                        message.contains(example),
                        "{tag_type:?}/{value:?}: message lacks {example}: {message}"
                    );
                }
                assert!(message.contains("Nothing was written"), "{message}");
            }
        }
    }

    #[test]
    fn language_refusal_names_the_value_and_hints_at_a_locale_name() {
        let message = language_refusal(TagType::VorbisComments, "English");
        assert!(
            message.contains("\"English\" is not a language tag"),
            "{message}"
        );

        // A locale name gets a hint, but is still refused, never converted.
        let message = language_refusal(TagType::Id3v2, "en_GB");
        assert!(message.contains("\"en_GB\""), "{message}");
        assert!(message.contains("`en-GB`"), "{message}");

        let message = language_refusal(TagType::Mp4Ilst, "");
        assert!(message.contains("is empty"), "{message}");
    }

    #[test]
    fn language_refusal_keeps_the_language_already_in_the_tag() {
        // The tag already says French; a refused write must not disturb
        // it — the refusal happens before anything is removed.
        for tag_type in LANGUAGE_TAG_TYPES {
            let mut tag = Tag::new(tag_type);
            write_common_tag_to_lofty(&mut tag, CommonTag::Language, "fr").expect("fr");
            let before: Vec<String> = tag
                .get_strings(&ItemKey::Language)
                .map(str::to_string)
                .collect();
            assert!(write_common_tag_to_lofty(&mut tag, CommonTag::Language, "zzz").is_err());
            let after: Vec<String> = tag
                .get_strings(&ItemKey::Language)
                .map(str::to_string)
                .collect();
            assert_eq!(before, after, "{tag_type:?}");
        }
    }

    #[test]
    fn language_special_values_are_recognised_and_written() {
        // `und` (not known), `mul` (several), `zxx` (no language), `mis`
        // (no code of its own), local use `qaa`, a private-use tag and a
        // grandfathered one are all real values, and are written normally:
        // on ID3v2 as their ISO 639-2 code (`und` for the last two, which
        // have none — TRACK-070), elsewhere as the canonical tag itself.
        let cases = [
            ("und", "und", "und"),
            ("mul", "mul", "mul"),
            ("zxx", "zxx", "zxx"),
            ("mis", "mis", "mis"),
            ("qaa", "qaa", "qaa"),
            ("x-private", "und", "x-private"),
            ("i-default", "und", "i-default"),
            // ID3's own "not known" marker is read as `und` (LANG-002).
            ("XXX", "und", "und"),
        ];
        for (input, id3v2, vorbis) in cases {
            assert_eq!(
                id3v2_roundtrip(CommonTag::Language, input, &ItemKey::Language).as_deref(),
                Some(id3v2),
                "ID3v2 {input:?}"
            );
            assert_eq!(
                vorbis_roundtrip(CommonTag::Language, input, &ItemKey::Language).as_deref(),
                Some(vorbis),
                "Vorbis {input:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // Several languages in one value (LANG-002; Codex review r7)
    //
    // A language field may hold several languages, separated by a null
    // character (ID3v2.4's multi-value form, and what `read_tags` hands
    // back). Until review r7 only the first was written.
    // ------------------------------------------------------------------

    /// Every language value recovered after an in-memory ID3v2 save
    /// (`merge`) and reload (`split`), plus the raw `TLAN` frame text.
    fn id3v2_languages(value: &str) -> (Vec<String>, Option<String>) {
        use lofty::id3::v2::{FrameId, Id3v2Tag};
        use std::borrow::Cow;
        let mut tag = Tag::new(TagType::Id3v2);
        write_common_tag_to_lofty(&mut tag, CommonTag::Language, value).expect("accepted");
        let id3 = Id3v2Tag::from(tag);
        let frame_text = id3
            .get_text(&FrameId::Valid(Cow::Borrowed("TLAN")))
            .map(str::to_string);
        let back = Tag::from(id3);
        let values = back
            .get_strings(&ItemKey::Language)
            .map(str::to_string)
            .collect();
        (values, frame_text)
    }

    /// Every language value recovered after an in-memory Vorbis round trip.
    fn vorbis_languages(value: &str) -> Vec<String> {
        use lofty::ogg::VorbisComments;
        let mut tag = Tag::new(TagType::VorbisComments);
        write_common_tag_to_lofty(&mut tag, CommonTag::Language, value).expect("accepted");
        let vc = VorbisComments::from(tag);
        assert_eq!(
            vc.get_all("LANGUAGE").count(),
            value.split('\0').count(),
            "one LANGUAGE field per value"
        );
        Tag::from(vc)
            .get_strings(&ItemKey::Language)
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn language_two_values_are_both_written_on_id3v2() {
        // Codex's exact input: French used to be dropped. (This in-memory
        // conversion is not enough on its own — see the real-MP3 test
        // below and `write_language` for why.)
        let (values, frame) = id3v2_languages("eng\0fra");
        assert_eq!(values, ["eng", "fra"]);
        // One TLAN frame, the values separated by a null character.
        assert_eq!(frame.as_deref(), Some("eng\0fra"));
    }

    #[test]
    fn language_three_values_are_all_written_on_id3v2() {
        // Each value becomes its own terminology code; `yue` has no
        // ISO 639-2 code, so it is `und` in its place — never dropped,
        // which would move the values after it up one place.
        let (values, frame) = id3v2_languages("pt-BR\0ger\0yue");
        assert_eq!(values, ["por", "deu", "und"]);
        assert_eq!(frame.as_deref(), Some("por\0deu\0und"));
    }

    #[test]
    fn language_two_and_three_values_are_all_written_on_vorbis() {
        assert_eq!(vorbis_languages("eng\0fr-CA"), ["en", "fr-CA"]);
        assert_eq!(
            vorbis_languages("eng\0ger\0zh-Hant"),
            ["en", "de", "zh-Hant"]
        );
    }

    #[test]
    fn language_several_values_are_one_null_separated_item_on_ape() {
        // An APE tag holds each key once; APEv2 lists several values in
        // one item, separated by null characters.
        use lofty::ape::ApeTag;
        let mut tag = Tag::new(TagType::Ape);
        write_common_tag_to_lofty(&mut tag, CommonTag::Language, "eng\0ger\0zh-Hant")
            .expect("accepted");
        let ape = ApeTag::from(tag);
        let item = ape.get("Language").expect("a Language item");
        assert_eq!(item.value().text(), Some("en\0de\0zh-Hant"));
    }

    #[test]
    fn language_one_unrecognised_value_among_several_refuses_them_all() {
        for tag_type in LANGUAGE_TAG_TYPES {
            let message = language_refusal(tag_type, "eng\0zzz\0fra");
            assert!(message.contains("value 2 of 3, \"zzz\","), "{message}");
            assert!(
                message.contains("none of the 3 values was written"),
                "{message}"
            );
            // Two values, the unrecognised one first.
            let message = language_refusal(tag_type, "English\0fra");
            assert!(message.contains("value 1 of 2, \"English\","), "{message}");
            // An empty value between two nulls is unrecognised too.
            let message = language_refusal(tag_type, "eng\0\0fra");
            assert!(message.contains("value 2 of 3 is empty"), "{message}");
        }
    }

    // ------------------------------------------------------------------
    // The same, through real files (write_tags -> save -> read_tags)
    //
    // The in-memory round trips above prove what lofty's conversion does;
    // these prove what actually lands in, and comes back out of, a file of
    // each kind — including the MP3 the review named.
    // ------------------------------------------------------------------

    /// A minimal untagged MP3: three silent MPEG-1 Layer III frames
    /// (128 kbit/s, 44.1 kHz, stereo — frame header `FF FB 90 00`), each
    /// 417 bytes (144 × 128000 ÷ 44100, no padding) of which all but the
    /// four header bytes are zero.
    fn minimal_untagged_mp3() -> Vec<u8> {
        const FRAME_LEN: usize = 417;
        let mut out = Vec::with_capacity(3 * FRAME_LEN);
        for _ in 0..3 {
            let mut frame = vec![0u8; FRAME_LEN];
            frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            out.extend_from_slice(&frame);
        }
        out
    }

    /// A minimal untagged WavPack file: one 32-byte block header (`wvpk`,
    /// stream version 0x410, first and last block, 44.1 kHz) and no audio.
    /// WavPack's tag is APEv2, so this is how an APE write is tested on a
    /// real file.
    fn minimal_untagged_wavpack() -> Vec<u8> {
        const FLAG_INITIAL_AND_FINAL_BLOCK: u32 = 0x800 | 0x1000;
        const SAMPLE_RATE_44100: u32 = 9 << 23;
        let mut block = Vec::with_capacity(32);
        block.extend_from_slice(b"wvpk");
        block.extend_from_slice(&24u32.to_le_bytes()); // size after these 8 bytes
        block.extend_from_slice(&0x0410u16.to_le_bytes()); // stream version
        block.extend_from_slice(&[0, 0]); // track number, sub-index
        block.extend_from_slice(&0u32.to_le_bytes()); // total samples
        block.extend_from_slice(&0u32.to_le_bytes()); // block index
        block.extend_from_slice(&0u32.to_le_bytes()); // samples in this block
        block.extend_from_slice(&(FLAG_INITIAL_AND_FINAL_BLOCK | SAMPLE_RATE_44100).to_le_bytes());
        block.extend_from_slice(&0u32.to_le_bytes()); // checksum
        block
    }

    /// Writes `languages` (with a title) to a fresh file built by `fixture`,
    /// then reads the file back and returns every language value found.
    fn file_languages(name: &str, fixture: Vec<u8>, languages: &str) -> Vec<String> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(name);
        std::fs::write(&path, fixture).expect("write fixture");
        write_tags(
            &path,
            &[
                (CommonTag::Title, "Fixture Title".into()),
                (CommonTag::Language, languages.into()),
            ],
        )
        .unwrap_or_else(|e| panic!("{name}: write_tags: {e}"));
        let read_back = read_tags(&path).unwrap_or_else(|e| panic!("{name}: read_tags: {e}"));
        assert_eq!(
            read_back.get(&CommonTag::Title).map(Vec::as_slice),
            Some(["Fixture Title".to_string()].as_slice()),
            "{name}: the title must have been written too"
        );
        read_back
            .get(&CommonTag::Language)
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn language_values_survive_a_real_mp3() {
        assert_eq!(
            file_languages("a.mp3", minimal_untagged_mp3(), "eng\0fra"),
            ["eng", "fra"]
        );
        assert_eq!(
            file_languages("b.mp3", minimal_untagged_mp3(), "pt-BR\0ger\0yue"),
            ["por", "deu", "und"]
        );
    }

    #[test]
    fn language_values_survive_a_real_flac_and_opus() {
        assert_eq!(
            file_languages("a.flac", minimal_untagged_flac(), "eng\0fr-CA"),
            ["en", "fr-CA"]
        );
        assert_eq!(
            file_languages("b.opus", minimal_untagged_opus(), "eng\0ger\0zh-Hant"),
            ["en", "de", "zh-Hant"]
        );
    }

    #[test]
    fn language_values_survive_a_real_m4a() {
        assert_eq!(
            file_languages("a.m4a", minimal_untagged_m4a(), "eng\0fr-CA"),
            ["en", "fr-CA"]
        );
        assert_eq!(
            file_languages("b.m4a", minimal_untagged_m4a(), "eng\0ger\0zh-Hant"),
            ["en", "de", "zh-Hant"]
        );
    }

    #[test]
    fn language_values_survive_a_real_wavpack_ape_tag() {
        // `read_tags` returns the APE item as it is stored — one value
        // with null characters between the languages — which is the input
        // shape `from_legacy_three_letter_all` reads.
        assert_eq!(
            file_languages("a.wv", minimal_untagged_wavpack(), "eng\0fr-CA"),
            ["en\0fr-CA"]
        );
        assert_eq!(
            file_languages("b.wv", minimal_untagged_wavpack(), "eng\0ger\0zh-Hant"),
            ["en\0de\0zh-Hant"]
        );
    }

    #[test]
    fn a_refused_language_leaves_a_real_file_byte_for_byte_unchanged() {
        let fixtures: [(&str, Vec<u8>); 5] = [
            ("c.mp3", minimal_untagged_mp3()),
            ("c.flac", minimal_untagged_flac()),
            ("c.opus", minimal_untagged_opus()),
            ("c.m4a", minimal_untagged_m4a()),
            ("c.wv", minimal_untagged_wavpack()),
        ];
        for (name, fixture) in fixtures {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(name);
            std::fs::write(&path, &fixture).expect("write fixture");
            // A good title first, then a language list with one bad value:
            // the whole call is refused and neither is written.
            let result = write_tags(
                &path,
                &[
                    (CommonTag::Title, "Must Not Be Written".into()),
                    (CommonTag::Language, "eng\0zzz".into()),
                ],
            );
            assert!(
                matches!(result, Err(MetadataError::UnrecognisedLanguage { .. })),
                "{name}: {result:?}"
            );
            assert_eq!(
                std::fs::read(&path).expect("read"),
                fixture,
                "{name} was changed"
            );
        }
    }

    // ------------------------------------------------------------------
    // Languages survive every later write (stand-in review of revision 5)
    //
    // Writing several languages worked, but the NEXT write of any kind —
    // a title change, ReplayGain, a registry tag — cut them down: on
    // ID3v2 (MP3, WAV, AIFF) lofty re-saved one `TLAN` frame per language
    // and a reader keeps only the last; on MP4 the languages sat in one
    // atom each (ffprobe showed the LAST as the file's language), and an
    // atom holding several `data` atoms — the usual form — was read as its
    // first value only and lost the rest on the next save. Reproduced on
    // the reviewer's real files before the fix; these tests hold it.
    // ------------------------------------------------------------------

    /// A minimal untagged WAV: `RIFF`/`WAVE`, a 16-bit mono 8 kHz PCM
    /// `fmt ` chunk and a `data` chunk of eight silent samples. lofty
    /// writes a WAV file's ID3v2 tag into an `id3 ` chunk, and ID3v2 is
    /// the tag type it prefers for WAV, so this is how an ID3v2 write to
    /// a WAV file is tested.
    fn minimal_untagged_wav() -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
        fmt.extend_from_slice(&1u16.to_le_bytes()); // one channel
        fmt.extend_from_slice(&8_000u32.to_le_bytes()); // sample rate
        fmt.extend_from_slice(&16_000u32.to_le_bytes()); // bytes per second
        fmt.extend_from_slice(&2u16.to_le_bytes()); // bytes per sample frame
        fmt.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&u32::try_from(fmt.len()).expect("fits").to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&[0u8; 16]);
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&u32::try_from(body.len()).expect("fits").to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// A minimal untagged AIFF: `FORM`/`AIFF`, a `COMM` chunk (one
    /// channel, eight 16-bit sample frames, 8 kHz as an 80-bit extended
    /// number: exponent 16383 + 12, mantissa 8000 << 51) and an `SSND`
    /// chunk. Like WAV, lofty's preferred tag for AIFF is ID3v2 (written
    /// into an `ID3 ` chunk).
    fn minimal_untagged_aiff() -> Vec<u8> {
        let mut comm = Vec::new();
        comm.extend_from_slice(&1u16.to_be_bytes()); // channels
        comm.extend_from_slice(&8u32.to_be_bytes()); // sample frames
        comm.extend_from_slice(&16u16.to_be_bytes()); // bits per sample
        comm.extend_from_slice(&[0x40, 0x0B, 0xFA, 0, 0, 0, 0, 0, 0, 0]); // 8000.0
        let mut ssnd = Vec::new();
        ssnd.extend_from_slice(&0u32.to_be_bytes()); // offset
        ssnd.extend_from_slice(&0u32.to_be_bytes()); // block size
        ssnd.extend_from_slice(&[0u8; 16]);
        let mut body = Vec::new();
        body.extend_from_slice(b"AIFF");
        body.extend_from_slice(b"COMM");
        body.extend_from_slice(&u32::try_from(comm.len()).expect("fits").to_be_bytes());
        body.extend_from_slice(&comm);
        body.extend_from_slice(b"SSND");
        body.extend_from_slice(&u32::try_from(ssnd.len()).expect("fits").to_be_bytes());
        body.extend_from_slice(&ssnd);
        let mut out = Vec::new();
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&u32::try_from(body.len()).expect("fits").to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// How many times `needle` occurs in `haystack` — a byte check of the
    /// saved file, independent of lofty's reading of it.
    fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    }

    /// One write made to a file by path, for the table below.
    type FileWrite = fn(&Path);

    /// A function building an untagged file's bytes.
    type Fixture = fn() -> Vec<u8>;

    /// The three kinds of write the review named, each unrelated to the
    /// language: a title-only `write_tags`, `write_replaygain_tags`, and a
    /// registry write (the registry path saves the file separately from
    /// `write_tags`).
    fn unrelated_writes() -> [(&'static str, FileWrite); 3] {
        fn title(path: &Path) {
            write_tags(path, &[(CommonTag::Title, "Changed Title".into())]).expect("title write");
        }
        fn replaygain(path: &Path) {
            let result = meedya_fingerprint::ReplayGainResult {
                integrated_loudness: -14.2,
                true_peak: 0.933,
                gain_db: -3.8,
                reference_level: -18.0,
            };
            write_replaygain_tags(path, &result, None).expect("ReplayGain write");
        }
        fn registry(path: &Path) {
            let registry = TagRegistry::from_toml(
                "[track.Mood]\njson_path = \"mood\"\nvalue_type = \"string\"\n\
                 atoms = [{ namespace = \"meedya\", name = \"Mood\" }]\n",
            )
            .expect("registry");
            let json = serde_json::json!({ "mood": "calm" });
            let written = write_registry_tags(path, &registry, &json, TagScope::Track)
                .expect("registry write");
            assert_eq!(written, 1);
        }
        [
            ("title-only write_tags", title),
            ("write_replaygain_tags", replaygain),
            ("write_registry_tags", registry),
        ]
    }

    const THREE_LANGUAGES: &str = "pt-BR\0ger\0zh-Hant";

    #[test]
    fn three_languages_survive_every_later_write_on_mp3_wav_and_aiff() {
        let fixtures: [(&str, Fixture); 3] = [
            ("mp3", minimal_untagged_mp3),
            ("wav", minimal_untagged_wav),
            ("aiff", minimal_untagged_aiff),
        ];
        for (extension, fixture) in fixtures {
            for (write_name, unrelated_write) in unrelated_writes() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(format!("f.{extension}"));
                std::fs::write(&path, fixture()).expect("write fixture");
                write_tags(&path, &[(CommonTag::Language, THREE_LANGUAGES.into())])
                    .expect("three languages");
                unrelated_write(&path);

                let read_back = read_tags(&path).expect("read_tags");
                assert_eq!(
                    read_back.get(&CommonTag::Language).map(Vec::as_slice),
                    Some(["por", "deu", "zho"].map(String::from).as_slice()),
                    "{extension} after {write_name}: every language, in order"
                );
                let bytes = std::fs::read(&path).expect("read file");
                assert_eq!(
                    occurrences(&bytes, b"TLAN"),
                    1,
                    "{extension} after {write_name}: exactly one TLAN frame in the saved file"
                );
            }
        }
    }

    #[test]
    fn gather_languages_before_saving_joins_the_items_lofty_splits_on_reading() {
        // What lofty hands back for a file whose one TLAN frame lists
        // three languages: three items. The helper makes them one again.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.mp3");
        std::fs::write(&path, minimal_untagged_mp3()).expect("write fixture");
        write_tags(&path, &[(CommonTag::Language, THREE_LANGUAGES.into())]).expect("write");
        let mut tagged_file = Probe::open(&path).expect("open").read().expect("read");
        let items = |tagged_file: &TaggedFile| -> Vec<String> {
            tagged_file
                .tag(TagType::Id3v2)
                .expect("an ID3v2 tag")
                .get_strings(&ItemKey::Language)
                .map(str::to_string)
                .collect()
        };
        assert_eq!(items(&tagged_file), ["por", "deu", "zho"]);
        gather_languages_before_saving(&mut tagged_file);
        assert_eq!(items(&tagged_file), ["por\0deu\0zho"]);
        // A second run changes nothing.
        gather_languages_before_saving(&mut tagged_file);
        assert_eq!(items(&tagged_file), ["por\0deu\0zho"]);
    }

    /// The language atoms of a saved MP4 file, read through lofty's own
    /// MP4 type: one inner list per `----:com.apple.iTunes:LANGUAGE` atom,
    /// holding that atom's text values in order.
    fn mp4_language_atoms(path: &Path) -> Vec<Vec<String>> {
        let mut file = std::fs::File::open(path).expect("open");
        let mp4 = Mp4File::read_from(&mut file, ParseOptions::default()).expect("an MP4 file");
        let Some(ilst) = mp4.ilst() else {
            return Vec::new();
        };
        ilst.into_iter()
            .filter(|atom| atom.ident() == &MP4_LANGUAGE)
            .map(|atom| mp4_language_texts(std::slice::from_ref(atom)))
            .collect()
    }

    #[test]
    fn mp4_languages_are_one_atom_holding_one_data_atom_each() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.m4a");
        std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");
        write_tags(&path, &[(CommonTag::Language, THREE_LANGUAGES.into())]).expect("write");
        // One atom, three values, the primary language first — the form
        // iTunes and mutagen write, and the one ffprobe takes its first
        // value from (revision 5 wrote three atoms, and ffprobe showed the
        // LAST language).
        assert_eq!(
            mp4_language_atoms(&path),
            [vec!["pt-BR".to_string(), "de".into(), "zh-Hant".into()]]
        );
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"LANGUAGE"),
            1
        );
        assert_eq!(
            read_tags(&path).expect("read").get(&CommonTag::Language),
            Some(&vec!["pt-BR".to_string(), "de".into(), "zh-Hant".into()])
        );
    }

    #[test]
    fn mp4_languages_survive_every_later_write() {
        // The registry write in `unrelated_writes` uses a `MeedyaMeta:`
        // key, which an M4A file cannot store and which is therefore
        // refused there (#103); an M4A file gets a registry write with a
        // proper freeform key instead. And `write_replaygain_tags` always
        // writes the reference loudness, which an M4A file has no atom for,
        // so it is refused there (the stand-in review of revision 9); an
        // M4A file gets the gain and peak written with `write_tags`.
        fn gain_and_peak(path: &Path) {
            write_tags(
                path,
                &[
                    (CommonTag::ReplayGainTrackGain, "-3.80 dB".into()),
                    (CommonTag::ReplayGainTrackPeak, "0.933000".into()),
                ],
            )
            .expect("gain and peak");
        }
        fn freeform_registry(path: &Path) {
            let written = write_registry_tags(
                path,
                &freeform_registry_of_mood(),
                &serde_json::json!({ "mood": "calm" }),
                TagScope::Track,
            )
            .expect("registry write");
            assert_eq!(written, 1);
        }
        let writes = unrelated_writes().map(|(name, write)| match name {
            "write_registry_tags" => (name, freeform_registry as FileWrite),
            "write_replaygain_tags" => (name, gain_and_peak as FileWrite),
            _ => (name, write),
        });
        for (write_name, unrelated_write) in writes {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("f.m4a");
            std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");
            write_tags(&path, &[(CommonTag::Language, THREE_LANGUAGES.into())]).expect("write");
            unrelated_write(&path);
            assert_eq!(
                mp4_language_atoms(&path),
                [vec!["pt-BR".to_string(), "de".into(), "zh-Hant".into()]],
                "after {write_name}"
            );
        }
    }

    /// A registry of one track tag, `Mood`, written under a key an M4A
    /// file can store as a freeform atom: `----:com.apple.iTunes:Mood`.
    fn freeform_registry_of_mood() -> TagRegistry {
        TagRegistry::from_toml(
            "[track.Mood]\njson_path = \"mood\"\nvalue_type = \"string\"\n\
             atoms = [{ namespace = \"----:com.apple.iTunes\", name = \"Mood\" }]\n",
        )
        .expect("registry")
    }

    // ------------------------------------------------------------------
    // #102 and #103 (Codex's review of revisions 5–7, and the stand-in
    // review of revision 8): every M4A save is made on a temporary copy
    // and compared atom by atom with the original; a save that would change
    // anything not asked for, or not store what was asked, is refused, and
    // the original is left byte for byte as it was.
    // ------------------------------------------------------------------

    /// An M4A file holding `atoms` (saved through lofty's own `Ilst`, the
    /// shape mutagen writes), in `dir`.
    fn m4a_with_atoms(dir: &Path, atoms: Vec<Atom<'static>>) -> std::path::PathBuf {
        let path = dir.join("f.m4a");
        std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");
        let mut ilst = Ilst::new();
        for atom in atoms {
            ilst.insert(atom);
        }
        ilst.save_to_path(&path, WriteOptions::default())
            .expect("save");
        path
    }

    /// A tiny PNG-looking picture, `tail` telling two apart.
    fn picture(tail: u8) -> AtomData {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        data.push(tail);
        AtomData::Picture(lofty::picture::Picture::new_unchecked(
            lofty::picture::PictureType::CoverFront,
            Some(lofty::picture::MimeType::Png),
            None,
            data,
        ))
    }

    fn freeform(name: &str) -> AtomIdent<'static> {
        AtomIdent::Freeform {
            mean: Cow::Borrowed("com.apple.iTunes"),
            name: Cow::Owned(name.to_string()),
        }
    }

    fn text(value: &str) -> AtomData {
        AtomData::UTF8(value.to_string())
    }

    #[test]
    fn an_m4a_write_that_would_drop_a_value_is_refused_and_saves_nothing() {
        // Each measured on a real M4A file (written by mutagen) before
        // revision 8's guard: a title-only write kept the first image, the
        // first artist, and cut `Meedya:Mood` to `Meedya`. Now the
        // comparison of the saved copy with the original finds each one —
        // nothing here tells it what to look for.
        let artist = AtomIdent::Fourcc(*b"\xa9ART");
        let cases: [(&str, Atom<'static>, &str); 5] = [
            (
                "two cover images",
                Atom::from_collection(AtomIdent::Fourcc(*b"covr"), vec![picture(1), picture(2)])
                    .expect("two"),
                "covr would change: now 2 values: a PNG image (9 bytes), a PNG image (9 bytes); \
                 after saving, one value, a PNG image (9 bytes)",
            ),
            (
                "two artists in \u{a9}ART",
                Atom::from_collection(artist, vec![text("Alice"), text("Bob")]).expect("two"),
                "\u{a9}ART would change: now 2 values: the text \"Alice\", the text \"Bob\"; \
                 after saving, one value, the text \"Alice\"",
            ),
            (
                "two artists in a freeform ARTISTS atom",
                Atom::from_collection(freeform("ARTISTS"), vec![text("Alice"), text("Bob")])
                    .expect("two"),
                "----:com.apple.iTunes:ARTISTS would change: now 2 values",
            ),
            (
                "a colon inside a freeform name",
                Atom::new(freeform("Meedya:Mood"), text("calm")),
                "----:com.apple.iTunes:Meedya:Mood would change: now one value, the text \
                 \"calm\"; after saving, nothing",
            ),
            (
                "two advisory ratings",
                Atom::from_collection(
                    AtomIdent::Fourcc(*b"rtng"),
                    vec![AtomData::SignedInteger(1), AtomData::SignedInteger(2)],
                )
                .expect("two"),
                "rtng would change: now 2 values: the whole number 1 in 4 bytes, the whole \
                 number 2 in 4 bytes; after saving, one value, the whole number 1 in 4 bytes",
            ),
        ];
        for (what, atom, expected) in cases {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = m4a_with_atoms(dir.path(), vec![atom]);
            let before = std::fs::read(&path).expect("read");
            let message = match write_tags(&path, &[(CommonTag::Title, "T".into())]) {
                Err(MetadataError::WriteError(message)) => message,
                other => panic!("{what}: expected a refusal, got {other:?}"),
            };
            assert!(message.contains(expected), "{what}: {message}");
            assert!(message.contains("#102"), "{what}: {message}");
            assert!(message.contains("Nothing was written"), "{what}: {message}");
            assert_eq!(std::fs::read(&path).expect("read"), before, "{what}");
            assert_only_the_file_is_there(dir.path(), "f.m4a");
        }
    }

    #[test]
    fn an_m4a_whose_atoms_hold_one_value_each_is_written_as_before() {
        // The guard refuses only what would be lost: one image, one
        // artist and a colon-free freeform name are written as before.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = m4a_with_atoms(
            dir.path(),
            vec![
                Atom::new(AtomIdent::Fourcc(*b"covr"), picture(1)),
                Atom::new(AtomIdent::Fourcc(*b"\xa9ART"), text("Alice")),
                Atom::new(freeform("MOOD"), text("calm")),
            ],
        );
        write_tags(&path, &[(CommonTag::Title, "Changed Title".into())]).expect("title");
        let after = read_tags(&path).expect("read");
        assert_eq!(after[&CommonTag::Title], ["Changed Title"]);
        assert_eq!(after[&CommonTag::Artist], ["Alice"]);
        let mut file = std::fs::File::open(&path).expect("open");
        let mp4 = Mp4File::read_from(&mut file, ParseOptions::default()).expect("mp4");
        let ilst = mp4.ilst().expect("an ilst");
        assert_eq!(
            ilst.get(&AtomIdent::Fourcc(*b"covr"))
                .map(|a| a.data().count()),
            Some(1)
        );
        assert!(ilst.get(&freeform("MOOD")).is_some());
    }

    #[test]
    fn a_registry_tag_an_m4a_cannot_store_is_refused_not_reported_written() {
        // #103: `MeedyaMeta:ISRC` is not an MP4 freeform key
        // (`----:mean:name`), and lofty used to leave it out of the saved
        // file while the call returned `Ok(1)`.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.m4a");
        std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");
        let before = std::fs::read(&path).expect("read");
        let registry = TagRegistry::from_toml(
            "[track.ISRC]\njson_path = \"isrc\"\nvalue_type = \"string\"\n\
             atoms = [{ namespace = \"meedya\", name = \"ISRC\" }]\n",
        )
        .expect("registry");
        let json = serde_json::json!({ "isrc": "GBAAA0000001" });
        let message = match write_registry_tags(&path, &registry, &json, TagScope::Track) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(message.contains("\"MeedyaMeta:ISRC\""), "{message}");
        assert!(message.contains("#103"), "{message}");
        assert!(message.contains("Nothing was written"), "{message}");
        assert_eq!(std::fs::read(&path).expect("read"), before);

        // A key in the freeform form is stored, and reported written.
        let written = write_registry_tags(
            &path,
            &freeform_registry_of_mood(),
            &serde_json::json!({ "mood": "calm" }),
            TagScope::Track,
        )
        .expect("a proper freeform key");
        assert_eq!(written, 1);
        let mut file = std::fs::File::open(&path).expect("open");
        let mp4 = Mp4File::read_from(&mut file, ParseOptions::default()).expect("mp4");
        let stored = mp4.ilst().and_then(|ilst| ilst.get(&freeform("Mood")));
        assert_eq!(
            stored.and_then(|atom| atom.data().next()),
            Some(&AtomData::UTF8("calm".into()))
        );
    }

    #[test]
    fn which_registry_keys_an_m4a_can_store() {
        assert!(is_mp4_freeform_key("----:com.apple.iTunes:ISRC"));
        assert!(is_mp4_freeform_key("----:MeedyaMeta:AppleRecordLabel"));
        // The form registry keys have today: no `----` in front.
        assert!(!is_mp4_freeform_key("MeedyaMeta:ISRC"));
        assert!(!is_mp4_freeform_key("com.apple.iTunes:AlbumArtistSort"));
        // Four characters make an ordinary atom, not a freeform one.
        assert!(!is_mp4_freeform_key("a:bc"));
        // A colon inside the name would be cut off.
        assert!(!is_mp4_freeform_key("----:com.apple.iTunes:Meedya:ISRC"));
    }

    // ------------------------------------------------------------------
    // The same check on REAL files: small M4A files made by ffmpeg and
    // tagged by mutagen, kept in `testdata/m4a/` (`make_m4a_fixtures.py`
    // there makes them, and says how each was built). Every result below
    // was also cross-checked with mutagen and ffprobe when this was written.
    // ------------------------------------------------------------------

    /// A copy of the real test file `name`, in `dir` (the file in
    /// `testdata/m4a/` itself is never written to).
    fn real_m4a(dir: &Path, name: &str) -> std::path::PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/m4a")
            .join(name);
        let path = dir.join(name);
        std::fs::copy(&source, &path).expect("copy the test file");
        path
    }

    /// The `ilst` atoms of the M4A file at `path`, read from its bytes.
    fn raw_atoms(path: &Path) -> Vec<RawAtom> {
        mp4_save_check::read_ilst_atoms(path).expect("readable atoms")
    }

    /// The key of the atom named `fourcc`.
    fn fourcc(fourcc: &[u8; 4]) -> RawKey {
        RawKey::Fourcc(*fourcc)
    }

    /// Nothing is left in `dir` but `name`: no temporary copy stays behind,
    /// whether the save went ahead or was refused.
    fn assert_only_the_file_is_there(dir: &Path, name: &str) {
        let names: Vec<String> = std::fs::read_dir(dir)
            .expect("list")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, [name.to_string()]);
    }

    /// `write` on a copy of the real file `name` must be refused, leave the
    /// file byte for byte as it was, and name everything in `named`.
    /// Returns the refusal.
    fn refused_on_real_file(
        name: &str,
        write: impl FnOnce(&Path) -> Result<(), MetadataError>,
        named: &[&str],
    ) -> String {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), name);
        let before = std::fs::read(&path).expect("read");
        let message = match write(&path) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("{name}: expected a refusal, got {other:?}"),
        };
        for part in named {
            assert!(message.contains(part), "{name}: {part:?} not in: {message}");
        }
        assert!(message.contains("Nothing was written"), "{message}");
        assert!(message.contains("#102"), "{message}");
        assert_eq!(
            std::fs::read(&path).expect("read"),
            before,
            "{name}: untouched"
        );
        assert_only_the_file_is_there(dir.path(), name);
        message
    }

    fn title_only(path: &Path) -> Result<(), MetadataError> {
        write_tags(path, &[(CommonTag::Title, "Changed".into())])
    }

    #[test]
    fn the_reviewers_flag_and_freeform_file_is_refused_and_left_untouched() {
        // The stand-in review of revision 8's own file. A title-only write
        // used to turn the flags into the text "0" (mutagen then read
        // `pgap` as TRUE) and rename three freeform atoms to lofty's
        // spelling; revision 8's list of checks caught none of it.
        refused_on_real_file(
            "flags-and-freeform.m4a",
            title_only,
            &[
                "pgap would change: now one value, the whole number 0 in 1 byte; after saving, \
                 one value, the text \"0\"",
                "hdvd would change",
                "shwm would change",
                "----:com.apple.iTunes:Mood would change: now one value, the text \"calm\"; \
                 after saving, nothing",
                "----:com.apple.iTunes:isrc would change",
                "----:com.apple.iTunes:REPLAYGAIN_TRACK_GAIN would change",
                "----:com.apple.iTunes:MOOD would change: now nothing; after saving, one value, \
                 the text \"calm\"",
            ],
        );
    }

    #[test]
    fn cover_art_with_an_unusual_second_type_is_refused_and_left_untouched() {
        // The second value of `covr` is typed as text (1), not an image;
        // lofty drops the whole atom on reading, so every image was lost.
        refused_on_real_file(
            "two-cover-types.m4a",
            title_only,
            &[
                "covr would change: now 2 values: a JPEG image (",
                "the text \"second-image-bytes\"; after saving, nothing",
            ],
        );
    }

    #[test]
    fn a_real_file_with_one_value_per_atom_is_written_and_every_other_atom_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "one-value-each.m4a");
        let before = raw_atoms(&path);
        assert_eq!(before.len(), 14);

        title_only(&path).expect("nothing but the title changes");

        let after = raw_atoms(&path);
        let title = fourcc(b"\xa9nam");
        let changed: Vec<&RawAtom> = after.iter().filter(|a| a.key == title).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(
            changed[0].values,
            [RawValue {
                type_indicator: 1,
                locale: 0,
                value: b"Changed".to_vec()
            }]
        );
        // Every other atom byte for byte as it was, in the same numbers.
        let others = |atoms: &[RawAtom]| {
            let mut kept: Vec<(RawKey, Vec<u8>)> = atoms
                .iter()
                .filter(|a| a.key != title)
                .map(|a| (a.key.clone(), a.bytes.clone()))
                .collect();
            kept.sort();
            kept
        };
        assert_eq!(others(&after), others(&before));
        let read_back = read_tags(&path).expect("read");
        assert_eq!(read_back[&CommonTag::Title], ["Changed"]);
        assert_eq!(read_back[&CommonTag::Language], ["en", "fr"]);
        assert_only_the_file_is_there(dir.path(), "one-value-each.m4a");
    }

    #[test]
    fn replacing_an_atom_that_holds_two_values_is_allowed() {
        // The stand-in review of revision 8, finding 5: writing the artist
        // on a file whose `©ART` holds Alice and Bob replaces the atom on
        // purpose. Allowed; only an atom NOT asked for may not change.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "two-artists.m4a");
        let before = raw_atoms(&path);
        write_tags(&path, &[(CommonTag::Artist, "Carol".into())]).expect("a replacement");
        let after = raw_atoms(&path);
        let artist = fourcc(b"\xa9ART");
        let artists: Vec<&RawAtom> = after.iter().filter(|a| a.key == artist).collect();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].values.len(), 1);
        assert_eq!(artists[0].values[0].value, b"Carol");
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Artist],
            ["Carol"]
        );
        for other in [fourcc(b"\xa9alb"), fourcc(b"\xa9too")] {
            let bytes = |atoms: &[RawAtom]| {
                atoms
                    .iter()
                    .filter(|a| a.key == other)
                    .map(|a| a.bytes.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(bytes(&after), bytes(&before));
        }

        // Writing the artist that is already FIRST is still asking for the
        // whole atom: allowed, and it then holds that one artist. (What was
        // asked for is recorded as the write is made, not worked out from
        // which values changed — the tag lofty hands over holds only the
        // first artist, so nothing would look changed.)
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "two-artists.m4a");
        write_tags(&path, &[(CommonTag::Artist, "Alice".into())]).expect("a replacement");
        let artists: Vec<RawAtom> = raw_atoms(&path)
            .into_iter()
            .filter(|a| a.key == artist)
            .collect();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].values.len(), 1);
        assert_eq!(artists[0].values[0].value, b"Alice");

        // The same file, the title written instead: `©ART` was not asked
        // for, and the save would keep only Alice — refused.
        refused_on_real_file(
            "two-artists.m4a",
            title_only,
            &[
                "\u{a9}ART would change: now 2 values: the text \"Alice\", the text \"Bob\"; \
               after saving, one value, the text \"Alice\"",
            ],
        );
    }

    #[test]
    fn two_track_numbers_in_one_atom_are_refused_unless_the_number_is_written() {
        // `trkn` holding two values (the M17 branch of revision 8's list,
        // which no test pinned). A title-only write would keep the first.
        refused_on_real_file(
            "two-track-numbers.m4a",
            title_only,
            &[
                "trkn would change: now 2 values: untyped data (8 bytes), untyped data (8 bytes); \
               after saving, one value, untyped data (8 bytes)",
            ],
        );
        // Writing the track number replaces the atom on purpose, and keeps
        // the total that was there.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "two-track-numbers.m4a");
        write_tags(&path, &[(CommonTag::TrackNumber, "5".into())]).expect("a replacement");
        let trkn: Vec<RawAtom> = raw_atoms(&path)
            .into_iter()
            .filter(|a| a.key == fourcc(b"trkn"))
            .collect();
        assert_eq!(trkn.len(), 1);
        assert_eq!(
            trkn[0].values,
            [RawValue {
                type_indicator: 0,
                locale: 0,
                value: vec![0, 0, 0, 5, 0, 10, 0, 0]
            }]
        );
    }

    #[test]
    fn a_change_of_data_type_alone_is_refused() {
        // `cpil` stored as an UNSIGNED whole number (22); lofty writes it
        // back as a signed one (21) holding the same byte.
        refused_on_real_file(
            "compilation-typed-unsigned.m4a",
            title_only,
            &[
                "cpil would change: now one value, the unsigned whole number 1 in 1 byte; after \
               saving, one value, the whole number 1 in 1 byte",
            ],
        );
    }

    #[test]
    fn a_change_of_locale_alone_is_refused() {
        // ffmpeg's `©too` with a locale of 1; lofty writes every locale as 0.
        refused_on_real_file(
            "encoder-with-locale.m4a",
            title_only,
            &[
                "\u{a9}too would change: now one value, the text \"Lavf63.1.101\" with locale 1; \
               after saving, one value, the text \"Lavf63.1.101\"",
            ],
        );
    }

    #[test]
    fn a_typical_itunes_style_file_is_refused_until_the_route_keeps_its_atoms() {
        // What iTunes and Apple Music leave in a file: whole numbers in one
        // or two bytes, a six-byte disc number, a gapless flag. lofty
        // rewrites every one of them (four-byte numbers, an eight-byte disc
        // number, the flag as text), so even a title-only write is refused.
        // That is the price of never changing what was not asked for until
        // #102's real fix, a route that keeps these atoms as they are.
        refused_on_real_file(
            "itunes-style.m4a",
            title_only,
            &[
                "stik would change: now one value, the whole number 1 in 1 byte; after saving, \
                 one value, the whole number 1 in 4 bytes",
                "rtng would change",
                "tmpo would change: now one value, the whole number 120 in 2 bytes",
                "akID would change",
                "disk would change: now one value, untyped data (6 bytes); after saving, one \
                 value, untyped data (8 bytes)",
                "pgap would change",
            ],
        );
    }

    /// A registry of one track tag, `x`, written under the freeform key
    /// `----:com.apple.iTunes:{name}`.
    fn registry_of(name: &str) -> TagRegistry {
        TagRegistry::from_toml(&format!(
            "[track.X]\njson_path = \"x\"\nvalue_type = \"string\"\n\
             atoms = [{{ namespace = \"----:com.apple.iTunes\", name = \"{name}\" }}]\n"
        ))
        .expect("registry")
    }

    fn registry_write(path: &Path, name: &str, value: &str) -> Result<(), MetadataError> {
        write_registry_tags(
            path,
            &registry_of(name),
            &serde_json::json!({ "x": value }),
            TagScope::Track,
        )
        .map(|written| assert_eq!(written, 1))
    }

    #[test]
    fn a_registry_write_that_would_be_duplicated_or_overridden_is_refused() {
        // The stand-in review of revision 8, finding 6: both used to be
        // reported written. A file already holding two ISRC atoms would
        // keep both, with the new value as a third beside them (ffprobe
        // shows the last, mutagen all three).
        refused_on_real_file(
            "two-isrc-atoms.m4a",
            |path| registry_write(path, "ISRC", "GBXXX2600001"),
            &[
                "----:com.apple.iTunes:ISRC was asked to hold one value, the text \
               \"GBXXX2600001\", but after saving would hold 3 atoms of that name",
            ],
        );
        // A registry value aimed at the language atom is put back over by
        // the file's own languages: not stored at all.
        refused_on_real_file(
            "language-en.m4a",
            |path| registry_write(path, "LANGUAGE", "fr"),
            &[
                "----:com.apple.iTunes:LANGUAGE was asked to hold one value, the text \"fr\", \
               but after saving would hold one value, the text \"en\"",
            ],
        );
        // On a file with no language at all, the value is not even put
        // back over: it is taken out again, so the file would gain nothing
        // while the call reported one tag written. Refused; the copy
        // holding no such atom where one was asked is the whole difference.
        refused_on_real_file(
            "two-artists.m4a",
            |path| registry_write(path, "LANGUAGE", "fr"),
            &[
                "----:com.apple.iTunes:LANGUAGE was asked to hold one value, the text \"fr\", \
               but after saving would hold nothing",
            ],
        );
        // A key the file does not hold yet is stored, and reported so.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "language-en.m4a");
        registry_write(&path, "ISRC", "GBXXX2600001").expect("a new atom");
        let isrc = RawKey::Freeform {
            mean: b"com.apple.iTunes".to_vec(),
            name: b"ISRC".to_vec(),
        };
        let stored: Vec<RawAtom> = raw_atoms(&path)
            .into_iter()
            .filter(|a| a.key == isrc)
            .collect();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].values[0].value, b"GBXXX2600001");
    }

    #[test]
    fn a_value_lofty_would_leave_out_is_refused_not_reported_written() {
        // lofty stores the compilation flag only as 0 or 1; "yes" would
        // be dropped, and the write used to succeed without storing it.
        // Refused since revision 9 - by the comparison then ("the value
        // given for cpil cannot be stored"), and since the stand-in review
        // of revision 9 (M4) before the save, by the value itself.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "one-value-each.m4a");
        let before = std::fs::read(&path).expect("read");
        let message = match write_tags(&path, &[(CommonTag::Compilation, "yes".into())]) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(message.contains("must be given as 1 or 0"), "{message}");
        assert_eq!(std::fs::read(&path).expect("read"), before);
        // "1" is stored.
        write_tags(&path, &[(CommonTag::Compilation, "1".into())]).expect("a flag");
    }

    /// A sample value an M4A file stores exactly as given, for `common_tag`.
    fn storable_value(common_tag: CommonTag) -> &'static str {
        match common_tag {
            CommonTag::TrackNumber
            | CommonTag::DiscNumber
            | CommonTag::TotalTracks
            | CommonTag::TotalDiscs => "7",
            CommonTag::Year => "2021",
            CommonTag::Compilation => "1",
            _ => "a value",
        }
    }

    #[test]
    fn a_value_an_m4a_would_not_store_as_given_is_refused_whatever_the_file_holds() {
        // The stand-in review of revision 9 (M4): "exactly what was asked"
        // is measured against what the CALLER gave, not lofty's conversion
        // of it. On a file holding track 3 of 12, the track number 70000
        // used to be accepted and the track number lost (lofty's `u16`
        // could not hold it, so it wrote track 0 of 12), and the total
        // "abc" likewise lost the total; on a file with no track atom the
        // same writes were refused by the comparison. Now the same answer
        // for both: refused before anything is written.
        let cases: &[(CommonTag, &str)] = &[
            (CommonTag::TrackNumber, "70000"),
            (CommonTag::TotalTracks, "abc"),
            (CommonTag::DiscNumber, "70000"),
            (CommonTag::TotalDiscs, "0"),
            (CommonTag::TrackNumber, "+5"),
            (CommonTag::TrackNumber, " 5"),
            (CommonTag::TrackNumber, ""),
            (CommonTag::Year, "2019-03-01"),
            (CommonTag::Year, "02019"),
            (CommonTag::Year, "0999"),
            (CommonTag::Year, "999"),
            (CommonTag::Compilation, "yes"),
            (CommonTag::Compilation, "true"),
        ];
        for &(common_tag, value) in cases {
            let mut messages = Vec::new();
            // Track 3 of 12; then no track atom at all.
            for name in ["plain-tone.m4a", "chapters.m4a"] {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = real_m4a(dir.path(), name);
                let before = std::fs::read(&path).expect("read");
                let message = match write_tags(&path, &[(common_tag, value.to_string())]) {
                    Err(MetadataError::WriteError(message)) => message,
                    other => panic!("{common_tag:?} {value:?} on {name}: got {other:?}"),
                };
                assert!(
                    message.starts_with(&format!("cannot write {common_tag:?} {value:?}")),
                    "{message}"
                );
                assert!(message.contains("Nothing was written"), "{message}");
                assert_eq!(std::fs::read(&path).expect("read"), before, "{name}");
                assert_only_the_file_is_there(dir.path(), name);
                messages.push(message);
            }
            assert_eq!(messages[0], messages[1], "the same answer for both files");
        }
    }

    #[test]
    fn values_an_m4a_stores_as_given_are_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "plain-tone.m4a");
        write_tags(&path, &[(CommonTag::TrackNumber, "7".into())]).expect("track");
        let read = read_tags(&path).expect("read");
        assert_eq!(read[&CommonTag::TrackNumber], ["7"]);
        assert_eq!(read[&CommonTag::TotalTracks], ["12"], "the total kept");
        write_tags(&path, &[(CommonTag::TotalTracks, "65535".into())]).expect("total");
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::TotalTracks],
            ["65535"]
        );
        write_tags(&path, &[(CommonTag::Compilation, "0".into())]).expect("flag");
        // A year over a full date: lofty keeps the month and day.
        let path = real_m4a(dir.path(), "one-value-each.m4a");
        write_tags(&path, &[(CommonTag::Year, "2021".into())]).expect("year");
        assert_eq!(read_tags(&path).expect("read")[&CommonTag::Year], ["2021"]);
        let day: Vec<RawAtom> = raw_atoms(&path)
            .into_iter()
            .filter(|atom| atom.key == fourcc(b"\xa9day"))
            .collect();
        assert_eq!(day[0].values[0].value, b"2021-05-01");
    }

    #[test]
    fn a_field_an_m4a_has_no_atom_for_is_refused_by_name() {
        // Found from lofty's own conversion, never from a list: every
        // `CommonTag` but the language (written its own way) is tried.
        // lofty 0.22.4 has an M4A atom for every field but these three
        // (`Producer` and `Engineer`, once thought missing too, are stored
        // as `----:com.apple.iTunes:PRODUCER` and `…:ENGINEER`).
        use strum::IntoEnumIterator;
        let without_an_atom: Vec<CommonTag> = CommonTag::iter()
            .filter(|tag| *tag != CommonTag::Language)
            .filter(
                |tag| match refuse_what_an_m4a_would_not_store(*tag, storable_value(*tag)) {
                    Ok(()) => false,
                    Err(MetadataError::WriteError(message)) => {
                        assert!(message.contains("has no M4A atom for it"), "{message}");
                        true
                    }
                    Err(other) => panic!("{tag:?}: {other:?}"),
                },
            )
            .collect();
        assert_eq!(
            without_an_atom,
            [
                CommonTag::AcoustId,
                CommonTag::ReplayGainReferenceLoudness,
                CommonTag::Arranger
            ]
        );
        // Each refused on a real file, named, the file untouched.
        for (tag, item) in [
            (CommonTag::AcoustId, "Acoustid Id"),
            (
                CommonTag::ReplayGainReferenceLoudness,
                "REPLAYGAIN_REFERENCE_LOUDNESS",
            ),
            (CommonTag::Arranger, "Arranger"),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = real_m4a(dir.path(), "plain-tone.m4a");
            let before = std::fs::read(&path).expect("read");
            let message = match write_tags(&path, &[(tag, "x".into())]) {
                Err(MetadataError::WriteError(message)) => message,
                other => panic!("{tag:?}: got {other:?}"),
            };
            assert!(
                message.contains(&format!("cannot write {tag:?}")),
                "{message}"
            );
            assert!(message.contains(item), "{message}");
            assert_eq!(std::fs::read(&path).expect("read"), before, "{tag:?}");
        }
        // So the two helpers that always write one of them are refused.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "plain-tone.m4a");
        let before = std::fs::read(&path).expect("read");
        let gain = meedya_fingerprint::ReplayGainResult {
            integrated_loudness: -14.2,
            true_peak: 0.933,
            gain_db: -3.8,
            reference_level: -18.0,
        };
        assert!(write_replaygain_tags(&path, &gain, None).is_err());
        let acoustid = meedya_fingerprint::AcoustIdResult {
            acoustid: "0123".into(),
            score: 1.0,
            recording_ids: vec!["abcd".into()],
            fingerprint: String::new(),
            duration_secs: 1,
        };
        assert!(write_acoustid_tags(&path, &acoustid).is_err());
        assert_eq!(std::fs::read(&path).expect("read"), before);
        // Every other field is written on a real file.
        for tag in CommonTag::iter()
            .filter(|tag| *tag != CommonTag::Language && !without_an_atom.contains(tag))
        {
            let path = real_m4a(dir.path(), "plain-tone.m4a");
            write_tags(&path, &[(tag, storable_value(tag).to_string())])
                .unwrap_or_else(|e| panic!("{tag:?}: {e}"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_checked_save_through_a_symbolic_link_replaces_the_file_it_points_to() {
        // The saved copy replaces the real file, not the link: the link is
        // followed first, so it still points at the (now saved) file.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "one-value-each.m4a");
        let link = dir.path().join("link.m4a");
        std::os::unix::fs::symlink(&path, &link).expect("symlink");
        title_only(&link).expect("write through the link");
        assert!(std::fs::symlink_metadata(&link)
            .expect("link")
            .file_type()
            .is_symlink());
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Title],
            ["Changed"]
        );
    }

    // ------------------------------------------------------------------
    // The WHOLE saved file, not just its tags (the stand-in review of
    // revision 9, H1 and M1): real files with real audio - half a second
    // of a tone - made by ffmpeg and tagged by mutagen
    // (`make_m4a_fixtures.py`). What is checked here is read with a small
    // reader of the tests' own, not with the check under test. Every result
    // was also cross-checked with a full ffmpeg decode (the same decoded
    // audio before and after), ffprobe's chapters, mutagen, and Apple's
    // AVFoundation when this was written.
    // ------------------------------------------------------------------

    /// Where each atom named `name` sits between `start` and `end` of
    /// `bytes`: (start, end, contents start). Plain 32-bit sizes only - all
    /// these files use.
    fn atoms_named(
        bytes: &[u8],
        start: usize,
        end: usize,
        name: &[u8],
    ) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        let mut pos = start;
        while pos + 8 <= end {
            let size = u32::from_be_bytes(bytes[pos..pos + 4].try_into().expect("4")) as usize;
            assert!(size >= 8 && pos + size <= end, "a readable test file");
            if &bytes[pos + 4..pos + 8] == name {
                out.push((pos, pos + size, pos + 8));
            }
            pos += size;
        }
        out
    }

    /// The only atom named `name` between `start` and `end`.
    fn only(bytes: &[u8], start: usize, end: usize, name: &[u8]) -> (usize, usize, usize) {
        let found = atoms_named(bytes, start, end, name);
        assert_eq!(found.len(), 1, "one {}", String::from_utf8_lossy(name));
        found[0]
    }

    /// What the tests compare in a saved file: where the sample data
    /// starts, the sample data itself, every chunk offset of every track,
    /// the `udta`'s `chpl` (Nero chapters) if there is one, and the
    /// `meta`'s parts other than the tag list and padding.
    #[derive(Debug, PartialEq)]
    struct Layout {
        data_start: usize,
        data: Vec<u8>,
        offsets: Vec<Vec<u32>>,
        chpl: Option<Vec<u8>>,
        meta_parts: Vec<Vec<u8>>,
    }

    fn layout(path: &Path) -> Layout {
        let bytes = std::fs::read(path).expect("read");
        let mdat = only(&bytes, 0, bytes.len(), b"mdat");
        let moov = only(&bytes, 0, bytes.len(), b"moov");
        let mut offsets = Vec::new();
        for trak in atoms_named(&bytes, moov.2, moov.1, b"trak") {
            let mdia = only(&bytes, trak.2, trak.1, b"mdia");
            let minf = only(&bytes, mdia.2, mdia.1, b"minf");
            let stbl = only(&bytes, minf.2, minf.1, b"stbl");
            let stco = only(&bytes, stbl.2, stbl.1, b"stco");
            let count = u32::from_be_bytes(bytes[stco.2 + 4..stco.2 + 8].try_into().expect("4"));
            offsets.push(
                (0..count as usize)
                    .map(|i| {
                        let at = stco.2 + 8 + 4 * i;
                        u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4"))
                    })
                    .collect(),
            );
        }
        let udta = only(&bytes, moov.2, moov.1, b"udta");
        let chpl = atoms_named(&bytes, udta.2, udta.1, b"chpl")
            .first()
            .map(|chpl| bytes[chpl.0..chpl.1].to_vec());
        let meta = only(&bytes, udta.2, udta.1, b"meta");
        let mut meta_parts = Vec::new();
        let mut pos = meta.2 + 4; // after version and flags
        while pos + 8 <= meta.1 {
            let size = u32::from_be_bytes(bytes[pos..pos + 4].try_into().expect("4")) as usize;
            let name = &bytes[pos + 4..pos + 8];
            if !matches!(name, b"ilst" | b"free" | b"skip") {
                meta_parts.push(bytes[pos..pos + size].to_vec());
            }
            pos += size;
        }
        Layout {
            data_start: mdat.2,
            data: bytes[mdat.2..mdat.1].to_vec(),
            offsets,
            chpl,
            meta_parts,
        }
    }

    #[test]
    fn working_out_what_was_asked_takes_steps_in_step_with_the_tags() {
        // Counted, never timed (the stand-in review of revision 9, M3: a
        // title-only save of a file with 32,000 tags took about two minutes,
        // because every key was looked for by searching the whole tag). Four
        // times the tags must take four times the steps, not sixteen.
        let steps_for = |n: usize| {
            let mut before = Tag::new(TagType::Mp4Ilst);
            for i in 0..n {
                before.push_unchecked(TagItem::new(
                    ItemKey::Unknown(format!("----:com.apple.iTunes:k{i:06}")),
                    ItemValue::Text("v".to_string()),
                ));
            }
            let mut after = before.clone();
            after.set_title("Changed".to_string());
            let asked = [ItemKey::TrackTitle];
            let languages = LanguageField {
                current: Vec::new(),
                as_read: Vec::new(),
                replacement: None,
            };
            let mut steps = 0;
            let expected =
                atoms_asked_for(&before, &after, &asked, &languages, &[], &mut steps).expect("ok");
            assert_eq!(expected.len(), 1, "only the title was asked for");
            steps
        };
        let (small, large) = (steps_for(1000), steps_for(4000));
        // No more than four times the steps (in step, not squared), and more
        // than three times (every tag is counted).
        assert!(
            large <= 4 * small && large > 3 * small,
            "{small} then {large}"
        );
    }

    #[test]
    fn a_fragmented_file_is_refused_before_anything_is_written() {
        // A title-only write, and a long comment: both broke the audio on
        // the reviewer's file (the pieces moved, where each says its audio
        // starts did not). Refused now, the file byte for byte as it was.
        let long_comment =
            |path: &Path| write_tags(path, &[(CommonTag::Comment, "z".repeat(3000))]);
        let message = refused_on_real_file("fragmented.m4a", title_only, &["fragmented", "`moof`"]);
        assert!(message.contains("breaks the audio"), "{message}");
        refused_on_real_file("fragmented.m4a", long_comment, &["fragmented"]);
    }

    #[test]
    fn real_files_with_audio_and_chapters_keep_them_through_a_save() {
        // The plain file and the two chapter files (a QuickTime chapter
        // track and a Nero `chpl`; `moov` after `mdat`, then first), each
        // given a new title and then a long comment - which, with `moov`
        // first, grows the tags past their padding and moves the audio.
        // Accepted, the audio byte for byte the same, every chunk offset
        // moved exactly as far as the audio, the chapters and the handler
        // untouched.
        let mut audio_moved = false;
        for name in ["plain-tone.m4a", "chapters.m4a", "chapters-faststart.m4a"] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = real_m4a(dir.path(), name);
            for (tag, value) in [
                (CommonTag::Title, "Changed".to_string()),
                (CommonTag::Comment, "y".repeat(5000)),
            ] {
                let before = layout(&path);
                write_tags(&path, &[(tag, value.clone())])
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                let after = layout(&path);
                assert_eq!(after.data, before.data, "{name}: the audio");
                let moved = after.data_start as i64 - before.data_start as i64;
                audio_moved |= moved != 0;
                let expected: Vec<Vec<u32>> = before
                    .offsets
                    .iter()
                    .map(|table| {
                        table
                            .iter()
                            .map(|offset| (i64::from(*offset) + moved) as u32)
                            .collect()
                    })
                    .collect();
                assert_eq!(after.offsets, expected, "{name}: the chunk offsets");
                assert_eq!(after.chpl, before.chpl, "{name}: the Nero chapters");
                assert_eq!(after.meta_parts, before.meta_parts, "{name}: the handler");
                assert_eq!(read_tags(&path).expect("read")[&tag], [value], "{name}");
                assert_only_the_file_is_there(dir.path(), name);
            }
        }
        assert!(
            audio_moved,
            "the long comment moved the audio of the faststart file"
        );
        let chapters =
            layout(&Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/m4a/chapters.m4a"));
        assert!(
            chapters.chpl.is_some() && chapters.offsets.len() == 2,
            "the chapter file has both kinds of chapters"
        );
    }

    /// A copy of `plain-tone.m4a`, in `dir`, whose tags were removed by
    /// lofty itself (an empty `Ilst` saved): its `meta` keeps its handler
    /// and has no tag list - the stand-in review of revision 9's
    /// "lofty-cleared" file, made the same way.
    fn tags_removed_by_lofty(dir: &Path) -> std::path::PathBuf {
        let path = real_m4a(dir, "plain-tone.m4a");
        Ilst::default()
            .save_to_path(&path, WriteOptions::default())
            .expect("remove every tag");
        let bytes = std::fs::read(&path).expect("read");
        let moov = only(&bytes, 0, bytes.len(), b"moov");
        let udta = only(&bytes, moov.2, moov.1, b"udta");
        let meta = only(&bytes, udta.2, udta.1, b"meta");
        assert_eq!(atoms_named(&bytes, meta.2 + 4, meta.1, b"hdlr").len(), 1);
        assert!(atoms_named(&bytes, meta.2 + 4, meta.1, b"ilst").is_empty());
        path
    }

    #[test]
    fn a_meta_with_a_handler_and_no_tag_list_is_refused_before_saving() {
        // lofty's save would write the new tag list over the handler, and
        // ffprobe and AVFoundation then read no tags at all (M1).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = tags_removed_by_lofty(dir.path());
        let before = std::fs::read(&path).expect("read");
        let message = match title_only(&path) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(
            message.contains("metadata box has no tag list"),
            "{message}"
        );
        assert!(message.contains("[hdlr, free]"), "{message}");
        assert!(message.contains("Nothing was written"), "{message}");
        assert_eq!(std::fs::read(&path).expect("read"), before);
        assert_only_the_file_is_there(dir.path(), "plain-tone.m4a");
    }

    #[test]
    fn the_whole_file_comparison_alone_finds_the_handler_written_over() {
        // Without the check before saving, would the comparison of the
        // whole file still refuse? lofty's own save of a title, into a
        // copy of the file above, compared with the original: yes - check
        // (c) names the handler that went missing.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = tags_removed_by_lofty(dir.path());
        let saved = dir.path().join("saved.m4a");
        std::fs::copy(&path, &saved).expect("copy");
        let mut ilst = Ilst::default();
        ilst.set_title("Changed".to_string());
        ilst.save_to_path(&saved, WriteOptions::default())
            .expect("lofty's own save");
        let found = mp4_file_check::differences_outside_the_tags(
            &mut std::fs::File::open(&path).expect("open"),
            &mut std::fs::File::open(&saved).expect("open"),
        )
        .expect("readable");
        assert_eq!(
            found,
            ["the parts of moov → udta → meta would change: now [hdlr]; after saving, []"]
        );
    }

    #[test]
    fn a_file_whose_tags_mutagen_cleared_is_saved_with_its_handler() {
        // mutagen's `clear()` and save leaves an EMPTY tag list beside the
        // handler: that is saved, the handler untouched (ffprobe and
        // AVFoundation read the new title - checked when this was written).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = real_m4a(dir.path(), "mutagen-cleared.m4a");
        let before = layout(&path);
        assert!(raw_atoms(&path).is_empty(), "no tags to begin with");
        title_only(&path).expect("saved");
        let after = layout(&path);
        assert_eq!(after.meta_parts, before.meta_parts, "the handler");
        assert_eq!(after.data, before.data);
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Title],
            ["Changed"]
        );
    }

    #[test]
    fn mp4_language_atom_written_by_another_tool_is_read_whole_and_kept() {
        // The usual form, as another tool writes it: ONE atom, several
        // `data` atoms. Built with lofty's own `Ilst` (bypassing this
        // crate), the same shape mutagen and mp4ameta produce. Before the
        // fix `read_tags` saw only `en`, and the title write below
        // deleted `fr` and `de` from the file.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.m4a");
        std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");
        let mut ilst = Ilst::new();
        ilst.insert(
            Atom::from_collection(
                MP4_LANGUAGE,
                vec![
                    AtomData::UTF8("en".into()),
                    AtomData::UTF8("fr".into()),
                    AtomData::UTF8("de".into()),
                ],
            )
            .expect("three values"),
        );
        ilst.save_to_path(&path, WriteOptions::default())
            .expect("save");

        let three = vec!["en".to_string(), "fr".into(), "de".into()];
        assert_eq!(
            read_tags(&path).expect("read").get(&CommonTag::Language),
            Some(&three)
        );
        write_tags(&path, &[(CommonTag::Title, "Changed Title".into())]).expect("title");
        assert_eq!(mp4_language_atoms(&path), std::slice::from_ref(&three));
        assert_eq!(
            read_tags(&path).expect("read").get(&CommonTag::Language),
            Some(&three)
        );
    }

    #[test]
    fn mp4_file_with_one_language_atom_per_value_is_read_whole_and_mended_on_save() {
        // The form revision 5 wrote: a format-neutral `Tag` with one
        // language item per value, saved as it was — one atom per value.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.m4a");
        std::fs::write(&path, minimal_untagged_m4a()).expect("write fixture");
        let mut tag = Tag::new(TagType::Mp4Ilst);
        for value in ["en", "fr", "de"] {
            tag.push(TagItem::new(
                ItemKey::Language,
                ItemValue::Text(value.into()),
            ));
        }
        tag.save_to_path(&path, WriteOptions::default())
            .expect("save");
        assert_eq!(
            mp4_language_atoms(&path).len(),
            3,
            "the old form: three atoms"
        );

        let three = vec!["en".to_string(), "fr".into(), "de".into()];
        assert_eq!(
            read_tags(&path).expect("read").get(&CommonTag::Language),
            Some(&three)
        );
        // Any later save puts them in one atom, in the same order.
        write_tags(&path, &[(CommonTag::Title, "Changed Title".into())]).expect("title");
        assert_eq!(mp4_language_atoms(&path), [three]);
    }

    // ------------------------------------------------------------------
    // A file that ALREADY holds one TLAN frame per language (stand-in
    // review of revision 6). This crate wrote files that way before
    // revision 6, and `meedya-tags-extended`'s `TagFile::save` still does
    // (#100). lofty reads only the last such frame, and the next save here
    // deleted the others — reproduced on the reviewer's real MP3, WAV and
    // AIFF files (split with `TagFile::save` itself) before the fix.
    // ------------------------------------------------------------------

    /// Writes three languages to a fresh file, then splits them into three
    /// `TLAN` frames the way `TagFile::save` does: lofty reads the one
    /// frame as three items and saves one frame per item. (Done here with
    /// lofty 0.22 directly, which splits exactly as `TagFile::save`'s lofty
    /// 0.21 does, because this crate cannot depend on
    /// `meedya-tags-extended` without changing the workspace lock file;
    /// the real `TagFile::save` was used on the reviewer's files.)
    fn file_with_three_tlan_frames(
        dir: &Path,
        extension: &str,
        fixture: Fixture,
    ) -> std::path::PathBuf {
        file_with_split_tlan_frames(dir, extension, fixture, THREE_LANGUAGES)
    }

    /// The same, for any `languages` (null-separated): one `TLAN` frame per
    /// language.
    fn file_with_split_tlan_frames(
        dir: &Path,
        extension: &str,
        fixture: Fixture,
        languages: &str,
    ) -> std::path::PathBuf {
        let path = dir.join(format!("split.{extension}"));
        std::fs::write(&path, fixture()).expect("write fixture");
        write_tags(&path, &[(CommonTag::Language, languages.into())]).expect("write");
        let tagged_file = Probe::open(&path).expect("open").read().expect("read");
        tagged_file
            .save_to_path(&path, WriteOptions::default())
            .expect("split save");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(
            occurrences(&bytes, b"TLAN"),
            languages.split('\0').count(),
            "{extension}: one frame per language"
        );
        path
    }

    #[test]
    fn languages_split_into_several_tlan_frames_survive_every_later_write() {
        let fixtures: [(&str, Fixture); 3] = [
            ("mp3", minimal_untagged_mp3),
            ("wav", minimal_untagged_wav),
            ("aiff", minimal_untagged_aiff),
        ];
        let three = ["por", "deu", "zho"].map(String::from);
        for (extension, fixture) in fixtures {
            for (write_name, unrelated_write) in unrelated_writes() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = file_with_three_tlan_frames(dir.path(), extension, fixture);
                // lofty alone sees only the last frame…
                assert_eq!(languages_of(&path, TagType::Id3v2), ["zho"], "{extension}");
                // …`read_tags` returns every frame's language, in order.
                assert_eq!(
                    read_tags(&path).expect("read")[&CommonTag::Language],
                    three,
                    "{extension}: read_tags"
                );

                unrelated_write(&path);

                assert_eq!(
                    read_tags(&path).expect("read")[&CommonTag::Language],
                    three,
                    "{extension} after {write_name}: every language, in order"
                );
                assert_eq!(
                    occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
                    1,
                    "{extension} after {write_name}: merged into one frame"
                );
            }
        }
    }

    #[test]
    fn split_languages_carried_back_unchanged_are_kept_whole() {
        // Read, change the title, write the languages back as read: they
        // are unchanged (all three, as `read_tags` returns them), and the
        // save merges them into one frame.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_with_three_tlan_frames(dir.path(), "mp3", minimal_untagged_mp3);
        change_title_and_write_back(&path).expect("write");
        let after = read_tags(&path).expect("read");
        assert_eq!(after[&CommonTag::Title], ["New Title"]);
        assert_eq!(after[&CommonTag::Language], ["por", "deu", "zho"]);
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
            1
        );
    }

    // ------------------------------------------------------------------
    // The two public steps for a caller saving its own lofty `TaggedFile`
    // (Codex's review of revisions 5–7): recovering the file's languages
    // straight after reading, and gathering them just before saving. One
    // combined helper, called before saving, used to read the languages
    // from the disk again and put them back over the caller's own change.
    // ------------------------------------------------------------------

    /// What a caller does: read the file (an MP3 holding `eng` and `fra` in
    /// one `TLAN` frame each), recover its languages straight away, make
    /// `change`, gather the languages, save. Returns the saved file's path.
    fn read_change_and_save(dir: &Path, change: impl FnOnce(&mut Tag)) -> std::path::PathBuf {
        let path = file_with_split_tlan_frames(dir, "mp3", minimal_untagged_mp3, "eng\0fra");
        let mut tagged_file = Probe::open(&path).expect("open").read().expect("read");
        recover_languages_after_reading(&mut tagged_file, &path).expect("readable");
        assert_eq!(
            tagged_file
                .tag(TagType::Id3v2)
                .expect("an ID3v2 tag")
                .get_strings(&ItemKey::Language)
                .collect::<Vec<_>>(),
            ["eng", "fra"],
            "both languages recovered before the change"
        );
        change(tagged_file.tag_mut(TagType::Id3v2).expect("an ID3v2 tag"));
        gather_languages_before_saving(&mut tagged_file);
        tagged_file
            .save_to_path(&path, WriteOptions::default())
            .expect("save");
        path
    }

    #[test]
    fn a_language_replaced_by_the_caller_is_saved_as_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = read_change_and_save(dir.path(), |tag| {
            tag.remove_key(&ItemKey::Language);
            tag.insert_text(ItemKey::Language, "deu".into());
        });
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Language],
            ["deu"]
        );
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
            1
        );
    }

    #[test]
    fn a_language_deleted_by_the_caller_stays_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = read_change_and_save(dir.path(), |tag| {
            tag.remove_key(&ItemKey::Language);
        });
        assert_eq!(
            read_tags(&path).expect("read").get(&CommonTag::Language),
            None
        );
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
            0
        );
    }

    #[test]
    fn languages_the_caller_did_not_touch_are_all_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = read_change_and_save(dir.path(), |tag| {
            tag.set_title("Changed Title".into());
        });
        let after = read_tags(&path).expect("read");
        assert_eq!(after[&CommonTag::Title], ["Changed Title"]);
        assert_eq!(after[&CommonTag::Language], ["eng", "fra"]);
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
            1
        );
    }

    #[test]
    fn write_tags_recovers_split_languages_before_the_edit_not_after() {
        // Pins the order inside `edit_and_save`: step 1 (recovering the
        // file's languages) must come straight after reading, before the
        // edit. Moved after the edit, it puts `eng` and `fra` back over the
        // `deu` asked for — and before this test (the stand-in review of
        // revision 8, M10) nothing noticed. Each of the three formats.
        let fixtures: [(&str, Fixture); 3] = [
            ("mp3", minimal_untagged_mp3),
            ("wav", minimal_untagged_wav),
            ("aiff", minimal_untagged_aiff),
        ];
        for (extension, fixture) in fixtures {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = file_with_split_tlan_frames(dir.path(), extension, fixture, "eng\0fra");
            write_tags(&path, &[(CommonTag::Language, "deu".into())]).expect("write");
            assert_eq!(
                read_tags(&path).expect("read")[&CommonTag::Language],
                ["deu"],
                "{extension}: the language asked for, and only it"
            );
            assert_eq!(
                occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
                1,
                "{extension}"
            );
        }
    }

    #[test]
    fn gathering_never_reads_the_file() {
        // Step 2 on a file whose frames are split, WITHOUT step 1: it
        // gathers only what lofty read (the last frame), and never goes
        // back to the disk — so it cannot undo a change either. (Leaving
        // out step 1 is how languages get lost; that is why the doc
        // comments put it straight after reading.)
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_with_split_tlan_frames(dir.path(), "mp3", minimal_untagged_mp3, "eng\0fra");
        let mut tagged_file = Probe::open(&path).expect("open").read().expect("read");
        gather_languages_before_saving(&mut tagged_file);
        assert_eq!(
            tagged_file
                .tag(TagType::Id3v2)
                .expect("an ID3v2 tag")
                .get_strings(&ItemKey::Language)
                .collect::<Vec<_>>(),
            ["fra"]
        );
    }

    /// An MP3 whose ID3v2 tag (version `major`, 3 or 4) holds one Latin-1
    /// language frame per value in `values`, each named `TLA` followed by a
    /// zero byte — ID3v2.2's name in a four-byte frame header, which lofty
    /// reads as `TLAN` in an ID3v2.3 tag but not in an ID3v2.4 one. (Every
    /// size here is below 128, where the plain and synchsafe forms agree.)
    fn mp3_with_tla_frames(dir: &Path, major: u8, values: &[&[u8]]) -> std::path::PathBuf {
        let frame = |text: &[u8]| {
            let mut out = b"TLA\0".to_vec();
            out.extend_from_slice(&u32::try_from(text.len() + 1).expect("fits").to_be_bytes());
            out.extend_from_slice(&[0, 0, 0]);
            out.extend_from_slice(text);
            out
        };
        let mut body: Vec<u8> = values.iter().flat_map(|value| frame(value)).collect();
        body.extend_from_slice(&[0u8; 16]);
        let mut bytes = b"ID3".to_vec();
        bytes.extend_from_slice(&[major, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, u8::try_from(body.len()).expect("fits")]);
        bytes.extend(body);
        bytes.extend(minimal_untagged_mp3());
        let path = dir.join("tla.mp3");
        std::fs::write(&path, bytes).expect("write fixture");
        path
    }

    #[test]
    fn two_v2_3_tla_frames_survive_an_unrelated_write() {
        // Codex's review of revisions 5–7: only frames named `TLAN` were
        // looked for, so this file — two `TLA\0` frames, `eng` and `fra`,
        // in an ID3v2.3 tag — was let through, and a title-only write kept
        // `fra` alone (reproduced before the fix).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = mp3_with_tla_frames(dir.path(), 3, &[b"eng", b"fra"]);
        assert_eq!(
            languages_of(&path, TagType::Id3v2),
            ["fra"],
            "lofty alone sees the last frame only"
        );
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Language],
            ["eng", "fra"]
        );

        write_tags(&path, &[(CommonTag::Title, "Changed Title".into())]).expect("title write");

        let after = read_tags(&path).expect("read");
        assert_eq!(after[&CommonTag::Title], ["Changed Title"]);
        assert_eq!(after[&CommonTag::Language], ["eng", "fra"]);
        // Saved as one frame of the current name, holding both.
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(occurrences(&bytes, b"TLAN"), 1);
        assert_eq!(occurrences(&bytes, b"TLA\0"), 0);
    }

    #[test]
    fn a_v2_4_tla_frame_refuses_the_write_and_leaves_the_file_as_it_was() {
        // In an ID3v2.4 tag lofty does not read a `TLA\0` frame as a
        // language, but mutagen does, and lofty's save would turn it into
        // an ordinary text frame (`TXXX:TLA`) — measured on a real file
        // before this was refused. Refused, even as the only such frame.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = mp3_with_tla_frames(dir.path(), 4, &[b"eng"]);
        let before = std::fs::read(&path).expect("read");
        let message = match write_tags(&path, &[(CommonTag::Title, "T".into())]) {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(message.contains("inside an ID3v2.4 tag"), "{message}");
        assert!(message.contains("Nothing was written"), "{message}");
        assert_eq!(std::fs::read(&path).expect("read"), before);
    }

    /// One small ID3v2.4 tag holding a Latin-1 `TLAN` frame per value
    /// (every size here is below 128, where plain and synchsafe agree).
    fn id3v2_4_tag_of_languages(values: &[&[u8]]) -> Vec<u8> {
        let mut body = Vec::new();
        for value in values {
            body.extend_from_slice(b"TLAN");
            body.extend_from_slice(&[0, 0, 0, u8::try_from(value.len() + 1).expect("fits")]);
            body.extend_from_slice(&[0, 0, 0]);
            body.extend_from_slice(value);
        }
        let mut tag = b"ID3\x04\x00\x00".to_vec();
        tag.extend_from_slice(&[0, 0, 0, u8::try_from(body.len()).expect("fits")]);
        tag.extend(body);
        tag
    }

    /// A copy of the real test file `name` from `testdata/id3/`
    /// (`make_id3_fixtures.py` there makes them), in `dir`.
    fn real_id3_file(dir: &Path, name: &str) -> std::path::PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/id3")
            .join(name);
        let path = dir.join(name);
        std::fs::copy(&source, &path).expect("copy the test file");
        path
    }

    #[test]
    fn a_language_frame_in_just_one_of_two_id3_tags_or_chunks_refuses_the_write() {
        // The stand-in review of revision 9's three files (M2), on real
        // audio: a language frame only in the SECOND of two tags (a write of
        // `deu` used to report success while the file kept `eng`); the
        // languages split over two frames in the second tag (a title-only
        // write used to leave mutagen reading `fra` alone); and a WAV file
        // whose language is only in its second ID3 chunk. Each refused, the
        // file byte for byte as it was - a language write and a title-only
        // write alike.
        for name in [
            "two-tags-lang-in-second.mp3",
            "two-tags-split-in-second.mp3",
            "two-chunks-lang-in-second.wav",
        ] {
            for tags in [
                vec![(CommonTag::Language, "deu".to_string())],
                vec![(CommonTag::Title, "T".to_string())],
            ] {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = real_id3_file(dir.path(), name);
                let before = std::fs::read(&path).expect("read");
                let message = match write_tags(&path, &tags) {
                    Err(MetadataError::WriteError(message)) => message,
                    other => panic!("{name} {tags:?}: expected a refusal, got {other:?}"),
                };
                assert!(message.contains("has 2 separate ID3v2 tags"), "{message}");
                assert!(message.contains("in 1 of them"), "{message}");
                assert!(message.contains("Nothing was written"), "{message}");
                assert_eq!(std::fs::read(&path).expect("read"), before, "{name}");
                assert_only_the_file_is_there(dir.path(), name);
            }
        }
    }

    #[test]
    fn one_id3_tag_with_a_v1_tail_an_ape_tag_or_padding_is_still_written() {
        // Still ONE ID3v2 tag: an ID3v1 tail or an APE tag at the end of the
        // file, and padding inside the tag, are not a second tag.
        for (name, languages) in [
            ("one-tag-split-v1.mp3", &["eng", "fra"][..]),
            ("one-tag-split-ape.mp3", &["eng", "fra"][..]),
            ("one-tag-padded.mp3", &["eng"][..]),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = real_id3_file(dir.path(), name);
            write_tags(&path, &[(CommonTag::Title, "T".into())])
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let read = read_tags(&path).expect("read");
            assert_eq!(read[&CommonTag::Title], ["T"], "{name}");
            assert_eq!(read[&CommonTag::Language], languages, "{name}");
        }
    }

    #[test]
    fn language_frames_in_two_id3_tags_or_chunks_refuse_every_write() {
        // The stand-in review of revision 8: a WAV file with an `id3 `
        // chunk (`eng`) and an `ID3 ` chunk (`fra`) — mutagen read `eng`,
        // then `fra, eng` after one title write, and the two chunks kept
        // swapping — and an MP3 file with two tags one after the other.
        // lofty rewrites only one of them, so the merge left languages in
        // two places. Now refused, the file untouched, whatever the write.
        let mut wav = minimal_untagged_wav();
        for (name, value) in [(b"id3 ", b"eng"), (b"ID3 ", b"fra")] {
            let tag = id3v2_4_tag_of_languages(&[value]);
            wav.extend_from_slice(name);
            wav.extend_from_slice(&u32::try_from(tag.len()).expect("fits").to_le_bytes());
            wav.extend(tag);
        }
        let riff_size = u32::try_from(wav.len() - 8).expect("fits");
        wav[4..8].copy_from_slice(&riff_size.to_le_bytes());
        let mut mp3 = id3v2_4_tag_of_languages(&[b"eng"]);
        mp3.extend(id3v2_4_tag_of_languages(&[b"fra"]));
        mp3.extend(minimal_untagged_mp3());

        for (name, bytes) in [("two-chunks.wav", wav), ("two-tags.mp3", mp3)] {
            let writes: [(&str, Vec<(CommonTag, String)>); 2] = [
                ("title", vec![(CommonTag::Title, "T".into())]),
                ("language", vec![(CommonTag::Language, "deu".into())]),
            ];
            for (write, tags) in writes {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(name);
                std::fs::write(&path, &bytes).expect("write fixture");
                let message = match write_tags(&path, &tags) {
                    Err(MetadataError::WriteError(message)) => message,
                    other => panic!("{name}, {write}: expected a refusal, got {other:?}"),
                };
                assert!(message.contains("has 2 separate ID3v2 tags"), "{message}");
                assert!(message.contains("Nothing was written"), "{message}");
                assert_eq!(
                    std::fs::read(&path).expect("read"),
                    bytes,
                    "{name}, {write}"
                );
            }
        }
    }

    #[test]
    fn several_tlan_frames_that_cannot_be_read_refuse_the_write() {
        // An MP3 whose ID3v2.4 tag is marked "unsynchronised" (an old
        // encoding lofty undoes but `id3v2_language_frames` does not) and
        // holds two TLAN frames: lofty reads only `fra`, so saving would
        // delete `eng`. The write is refused and the file left untouched.
        let frame = |text: &[u8]| {
            let mut out = b"TLAN".to_vec();
            out.extend_from_slice(&[0, 0, 0, u8::try_from(text.len() + 1).expect("fits")]);
            out.extend_from_slice(&[0, 0, 3]);
            out.extend_from_slice(text);
            out
        };
        let body = [frame(b"eng"), frame(b"fra"), vec![0u8; 8]].concat();
        let mut bytes = b"ID3\x04\x00\x80".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, u8::try_from(body.len()).expect("fits")]);
        bytes.extend(body);
        bytes.extend(minimal_untagged_mp3());

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("unsync.mp3");
        std::fs::write(&path, &bytes).expect("write fixture");
        assert_eq!(
            languages_of(&path, TagType::Id3v2),
            ["fra"],
            "lofty sees one"
        );

        let result = write_tags(&path, &[(CommonTag::Title, "T".into())]);
        let message = match result {
            Err(MetadataError::WriteError(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(message.contains("unsynchronised"), "{message}");
        assert!(message.contains("Nothing was written"), "{message}");
        assert_eq!(std::fs::read(&path).expect("read"), bytes);
        // Reading still works: it reports what lofty reads.
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Language],
            ["fra"]
        );
    }

    // ------------------------------------------------------------------
    // Several `Language` entries in one call (stand-in review of
    // revision 5): each used to replace the one before, keeping the last.
    // ------------------------------------------------------------------

    #[test]
    fn several_language_entries_in_one_call_are_all_written_in_order() {
        let entries = [
            (CommonTag::Language, "pt-BR".to_string()),
            (CommonTag::Title, "Fixture Title".to_string()),
            (CommonTag::Language, "ger\0fr-CA".to_string()),
            (CommonTag::Language, "zh-Hant".to_string()),
        ];
        let fixtures: [(&str, Vec<u8>, &[&str]); 5] = [
            (
                "a.mp3",
                minimal_untagged_mp3(),
                &["por", "deu", "fra", "zho"],
            ),
            (
                "b.flac",
                minimal_untagged_flac(),
                &["pt-BR", "de", "fr-CA", "zh-Hant"],
            ),
            (
                "c.opus",
                minimal_untagged_opus(),
                &["pt-BR", "de", "fr-CA", "zh-Hant"],
            ),
            (
                "d.m4a",
                minimal_untagged_m4a(),
                &["pt-BR", "de", "fr-CA", "zh-Hant"],
            ),
            (
                "e.wv",
                minimal_untagged_wavpack(),
                &["pt-BR\0de\0fr-CA\0zh-Hant"],
            ),
        ];
        for (name, fixture, expected) in fixtures {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(name);
            std::fs::write(&path, fixture).expect("write fixture");
            write_tags(&path, &entries).unwrap_or_else(|e| panic!("{name}: {e}"));
            let read_back = read_tags(&path).expect("read");
            assert_eq!(
                read_back.get(&CommonTag::Language).map(Vec::as_slice),
                Some(
                    expected
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .as_slice()
                ),
                "{name}"
            );
            assert_eq!(
                read_back.get(&CommonTag::Title),
                Some(&vec!["Fixture Title".to_string()]),
                "{name}"
            );
        }
    }

    #[test]
    fn one_unrecognised_entry_among_several_refuses_the_whole_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.flac");
        let fixture = minimal_untagged_flac();
        std::fs::write(&path, &fixture).expect("write fixture");
        let result = write_tags(
            &path,
            &[
                (CommonTag::Language, "eng".into()),
                (CommonTag::Title, "Must Not Be Written".into()),
                (CommonTag::Language, "fra\0zzz".into()),
            ],
        );
        let message = match result {
            Err(e @ MetadataError::UnrecognisedLanguage { .. }) => e.to_string(),
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(
            message.contains("in language entry 2 of the 2 given together, value 2 of 2, \"zzz\","),
            "{message}"
        );
        assert_eq!(std::fs::read(&path).expect("read"), fixture);
    }

    // ------------------------------------------------------------------
    // An unchanged value is left alone (stand-in review of revision 5;
    // the rule MeedyaManager adopted, COMPAT-030): reading a file,
    // changing its title and writing every field back must not be refused
    // because another tool wrote a language this crate would not.
    // ------------------------------------------------------------------

    /// A FLAC file whose `LANGUAGE` another tool set to `English` (a
    /// language NAME, which this crate refuses when given as a new value),
    /// written with lofty directly so nothing here checks it.
    fn flac_with_language_english(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("english.flac");
        std::fs::write(&path, minimal_untagged_flac()).expect("write fixture");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title("Old Title".into());
        tag.push(TagItem::new(
            ItemKey::Language,
            ItemValue::Text("English".into()),
        ));
        tag.save_to_path(&path, WriteOptions::default())
            .expect("save");
        path
    }

    #[test]
    fn an_unchanged_unrecognised_language_does_not_block_the_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = flac_with_language_english(dir.path());
        let read_back = read_tags(&path).expect("read");
        let language = read_back[&CommonTag::Language].join("\0");
        assert_eq!(language, "English");

        // Read, change the title, write every field back.
        write_tags(
            &path,
            &[
                (CommonTag::Title, "New Title".into()),
                (CommonTag::Language, language),
            ],
        )
        .expect("an unchanged language must not refuse the write");
        let after = read_tags(&path).expect("read");
        assert_eq!(after[&CommonTag::Title], ["New Title"]);
        // Left exactly as it was — not refused, not converted.
        assert_eq!(after[&CommonTag::Language], ["English"]);
    }

    #[test]
    fn a_changed_unrecognised_language_is_still_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = flac_with_language_english(dir.path());
        let before = std::fs::read(&path).expect("read");
        let result = write_tags(
            &path,
            &[
                (CommonTag::Title, "New Title".into()),
                (CommonTag::Language, "Englisch".into()),
            ],
        );
        assert!(
            matches!(result, Err(MetadataError::UnrecognisedLanguage { .. })),
            "{result:?}"
        );
        assert_eq!(std::fs::read(&path).expect("read"), before);
    }

    #[test]
    fn an_unchanged_value_is_compared_as_read_tags_joins_it() {
        // MP3: another tool's one TLAN frame listing two languages comes
        // back from `read_tags` as two entries; joined with a null
        // character they are the unchanged value, which is left alone —
        // still ONE frame, both languages.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.mp3");
        std::fs::write(&path, minimal_untagged_mp3()).expect("write fixture");
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert(TagItem::new(
            ItemKey::Language,
            ItemValue::Text("eng\0fra".into()),
        ));
        tag.save_to_path(&path, WriteOptions::default())
            .expect("save");

        let joined = read_tags(&path).expect("read")[&CommonTag::Language].join("\0");
        assert_eq!(joined, "eng\0fra");
        write_tags(
            &path,
            &[
                (CommonTag::Title, "New Title".into()),
                (CommonTag::Language, joined),
            ],
        )
        .expect("unchanged");
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Language],
            ["eng", "fra"]
        );
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
            1
        );

        // The comparison is of the text, not of what it means: FLAC
        // holding `eng` (another tool's old three-letter code) keeps it
        // when `eng` is written back, but `en` — the same language, in
        // canonical form — is a change, and is written.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("e.flac");
        std::fs::write(&path, minimal_untagged_flac()).expect("write fixture");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.push(TagItem::new(
            ItemKey::Language,
            ItemValue::Text("eng".into()),
        ));
        tag.save_to_path(&path, WriteOptions::default())
            .expect("save");
        write_tags(&path, &[(CommonTag::Language, "eng".into())]).expect("unchanged");
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Language],
            ["eng"]
        );
        write_tags(&path, &[(CommonTag::Language, "en".into())]).expect("changed");
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Language],
            ["en"]
        );

        // An empty value for a file holding no language is unchanged too.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("g.flac");
        std::fs::write(&path, minimal_untagged_flac()).expect("write fixture");
        write_tags(
            &path,
            &[
                (CommonTag::Title, "T".into()),
                (CommonTag::Language, String::new()),
            ],
        )
        .expect("an empty value for a file with no language is unchanged");
        assert_eq!(
            read_tags(&path).expect("read").get(&CommonTag::Language),
            None
        );
    }

    // ------------------------------------------------------------------
    // A language `read_tags` found in ANOTHER tag (stand-in review of
    // revision 6). A WAV file holding only a RIFF INFO list, or an MP3
    // holding only an APE tag, has no main (ID3v2) tag; `read_tags` reports
    // the other tag's language, but a write goes into a new ID3v2 tag.
    // Writing the same language back used to count as "unchanged", so the
    // new tag got none and `read_tags` then reported none. Reproduced on the
    // reviewer's real files (`ffmpeg -metadata language=eng` for the WAV)
    // before the fix.
    // ------------------------------------------------------------------

    /// A WAV file whose only tag is a RIFF INFO list naming `language`
    /// (the `ILNG` field ffmpeg writes for `-metadata language=…`).
    fn riff_info_only_wav(dir: &Path, language: &str) -> std::path::PathBuf {
        let path = dir.join("riff.wav");
        std::fs::write(&path, minimal_untagged_wav()).expect("write fixture");
        let mut tag = Tag::new(TagType::RiffInfo);
        tag.set_title("Orig".into());
        tag.insert(TagItem::new(
            ItemKey::Language,
            ItemValue::Text(language.into()),
        ));
        tag.save_to_path(&path, WriteOptions::default())
            .expect("save");
        path
    }

    /// An MP3 file whose only tag is an APE tag naming `language`.
    fn ape_only_mp3(dir: &Path, language: &str) -> std::path::PathBuf {
        let path = dir.join("ape.mp3");
        std::fs::write(&path, minimal_untagged_mp3()).expect("write fixture");
        let mut tag = Tag::new(TagType::Ape);
        tag.set_title("Orig".into());
        tag.insert(TagItem::new(
            ItemKey::Language,
            ItemValue::Text(language.into()),
        ));
        tag.save_to_path(&path, WriteOptions::default())
            .expect("save");
        path
    }

    /// The language values of the file's tag of type `tag_type`, read
    /// with lofty directly.
    fn languages_of(path: &Path, tag_type: TagType) -> Vec<String> {
        let tagged_file = Probe::open(path).expect("open").read().expect("read");
        tagged_file
            .tag(tag_type)
            .map(languages_in)
            .unwrap_or_default()
    }

    /// Read, change the title, write every field back — the round trip a
    /// tag editor makes.
    fn change_title_and_write_back(path: &Path) -> Result<(), MetadataError> {
        let read_back = read_tags(path).expect("read");
        let language = read_back
            .get(&CommonTag::Language)
            .expect("a language to carry back")
            .join("\0");
        write_tags(
            path,
            &[
                (CommonTag::Title, "New Title".into()),
                (CommonTag::Language, language),
            ],
        )
    }

    #[test]
    fn a_language_read_from_another_tag_is_written_into_the_main_tag() {
        type Build = fn(&Path, &str) -> std::path::PathBuf;
        let files: [(&str, Build, TagType); 2] = [
            ("RIFF-INFO-only WAV", riff_info_only_wav, TagType::RiffInfo),
            ("APE-only MP3", ape_only_mp3, TagType::Ape),
        ];
        for (name, build, other_tag) in files {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = build(dir.path(), "eng");
            assert_eq!(
                read_tags(&path).expect("read")[&CommonTag::Language],
                ["eng"],
                "{name}: read from the only tag there is"
            );

            change_title_and_write_back(&path).unwrap_or_else(|e| panic!("{name}: {e}"));

            let after = read_tags(&path).expect("read");
            assert_eq!(after[&CommonTag::Title], ["New Title"], "{name}");
            assert_eq!(
                after.get(&CommonTag::Language).map(Vec::as_slice),
                Some(["eng".to_string()].as_slice()),
                "{name}: the language must still be reported"
            );
            // Written into the new ID3v2 tag, as one TLAN frame…
            assert_eq!(languages_of(&path, TagType::Id3v2), ["eng"], "{name}");
            assert_eq!(
                occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
                1,
                "{name}"
            );
            // …and the other tag keeps its own.
            assert_eq!(languages_of(&path, other_tag), ["eng"], "{name}");
        }
    }

    #[test]
    fn an_unrecognised_language_read_from_another_tag_is_skipped_not_refused() {
        // `English` (a language name) in the RIFF INFO list: carrying it
        // back is not the caller choosing it, so the write goes ahead; it
        // cannot be written into the ID3v2 tag, so it is not.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = riff_info_only_wav(dir.path(), "English");
        change_title_and_write_back(&path).expect("not refused");
        assert_eq!(
            read_tags(&path).expect("read")[&CommonTag::Title],
            ["New Title"]
        );
        assert!(languages_of(&path, TagType::Id3v2).is_empty());
        assert_eq!(
            occurrences(&std::fs::read(&path).expect("read"), b"TLAN"),
            0
        );
        assert_eq!(languages_of(&path, TagType::RiffInfo), ["English"]);

        // A CHANGED unrecognised value is still refused, file untouched.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = riff_info_only_wav(dir.path(), "English");
        let before = std::fs::read(&path).expect("read");
        let result = write_tags(
            &path,
            &[
                (CommonTag::Title, "New Title".into()),
                (CommonTag::Language, "Englisch".into()),
            ],
        );
        assert!(
            matches!(result, Err(MetadataError::UnrecognisedLanguage { .. })),
            "{result:?}"
        );
        assert_eq!(std::fs::read(&path).expect("read"), before);
    }
}
