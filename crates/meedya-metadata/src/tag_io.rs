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

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{FileType, TaggedFile};
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst, Mp4File};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, ItemValue, Tag, TagItem, TagType};

use crate::common_tags::CommonTag;
use crate::error::MetadataError;
use crate::json_path;
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
/// atom is returned (see the top of this file for why that needed care).
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
        OpenedFile::Other(tagged_file) => {
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
///   joined with null characters, are exactly what the file already holds
///   (`read_tags`' `Language` values joined the same way), the language
///   field is not touched at all — not checked, not converted, not
///   rewritten — exactly as if no `Language` entry had been given. This is
///   what lets a caller read a file, change its title, and write every
///   field back: a file whose `LANGUAGE` another tool set to `English`
///   used to have that whole save refused. (The same rule MeedyaManager
///   adopted, COMPAT-030.) An empty value given for a file that holds no
///   language is likewise unchanged.
/// - **A changed value this crate does not recognise refuses the WHOLE
///   call** with [`MetadataError::UnrecognisedLanguage`], before anything
///   is saved, so the file is left exactly as it was — none of the other
///   tags in `tags` are written either.
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

    edit_and_save(path, |tag, languages| {
        // Every value is applied to the in-memory tag first; the file is
        // only saved once all of them have been accepted. A refused value
        // (only a language can be refused) therefore returns here with
        // the file untouched.
        for (common_tag, value) in tags {
            if *common_tag != CommonTag::Language {
                write_common_tag_to_lofty(tag, *common_tag, value)?;
            }
        }
        if !language_entries.is_empty()
            && language_entries.join("\0") != languages.current.join("\0")
        {
            languages.replacement =
                Some(language_values_to_write(&language_entries, tag.tag_type())?);
        }
        Ok(())
    })
}

/// Write ReplayGain analysis results to a media file.
///
/// Writes track-level gain and peak. Optionally writes album-level values
/// and reference loudness if `album_result` is provided.
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
    edit_and_save(path, |tag, _languages| {
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
                let key = ItemKey::Unknown(format!("{}:{}", atom.namespace, atom.name));
                // #65 — insert_unchecked: lofty's insert() rejects ItemKey::Unknown (re_map allow_unknown=false), silently dropping freeform atoms; insert_unchecked is lofty's documented API for Unknown keys.
                tag.insert_unchecked(TagItem::new(key, ItemValue::Text(string_val.clone())));
            }
            count += 1;
        }

        Ok(count)
    })
}

/// Gathers every language item in `tagged_file`'s ID3v2 tag (and APE tag)
/// into ONE item, the values separated by null characters, so that saving
/// writes one `TLAN` frame (one APE `Language` item) holding every
/// language, instead of one frame per language. **Call it just before
/// every save of a `TaggedFile` that may carry an ID3v2 tag** — every save
/// in `tag_io` does, and so does `meedya-lyrics`' `embed_synced`.
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
/// bring back languages a file had already lost — a file saved before
/// this fix may hold several `TLAN` frames, and lofty reads only the last
/// of them, so the others are gone before this function ever sees them.
/// Nor does it reach `meedya-tags-extended`'s `TagFile::save`: that crate
/// is built on an older lofty (0.21), whose `TaggedFile` is a different
/// type this function cannot take, so a file saved through it still has
/// several languages split into several frames (measured on a real MP3;
/// left for a separate change, since moving that crate to lofty 0.22
/// changes the lofty types its public API hands out).
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
    /// Every language value the file holds now, exactly as `read_tags`
    /// returns them.
    current: Vec<String>,
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
/// - **MP4**: the language atom is taken out of the file's `Ilst` first
///   and held on one side; the rest is split into lofty's format-neutral
///   `Tag` for `edit` (`split_tag`), merged back (`merge_tag` — together,
///   exactly the conversion lofty's own `TaggedFile` save performs), and
///   the language atom is put back as ONE atom holding every value: the
///   replacement if `edit` asked for one, otherwise every `data` atom the
///   file had. A file written before this fix, with one atom per
///   language, is therefore rewritten in the usual one-atom form.
/// - **Every other format**: `edit` gets the main tag (created if the file
///   has none), a replacement is stored with `put_languages`, and
///   [`gather_languages_before_saving`] runs just before the save.
fn edit_and_save<T>(
    path: &Path,
    edit: impl FnOnce(&mut Tag, &mut LanguageField) -> Result<T, MetadataError>,
) -> Result<T, MetadataError> {
    match open_file(path)? {
        OpenedFile::Mp4(mut mp4) => {
            let mut ilst = mp4.remove_ilst().unwrap_or_default();
            let held = take_mp4_languages(&mut ilst);
            let mut languages = LanguageField {
                current: mp4_language_texts(&held),
                replacement: None,
            };
            let (remainder, mut tag) = ilst.split_tag();
            let out = edit(&mut tag, &mut languages)?;
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
            merged.save_to_path(path, WriteOptions::default())?;
            Ok(out)
        }
        OpenedFile::Other(mut tagged_file) => {
            let current = read_source(&tagged_file)
                .map(|tag| {
                    tag.get_strings(&ItemKey::Language)
                        .map(str::to_string)
                        .collect()
                })
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
                replacement: None,
            };
            let out = edit(tag, &mut languages)?;
            if let Some(values) = languages.replacement {
                put_languages(tag, values);
            }

            gather_languages_before_saving(&mut tagged_file);
            tagged_file.save_to_path(path, WriteOptions::default())?;
            Ok(out)
        }
    }
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
/// only a CHANGED value: one identical to what the file already holds is
/// left alone before it ever gets here — see its doc comment.)
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
        for (write_name, unrelated_write) in unrelated_writes() {
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
}
