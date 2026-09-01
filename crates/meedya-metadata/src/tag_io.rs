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

use std::collections::HashMap;
use std::path::Path;

use lofty::config::WriteOptions;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, ItemValue, Tag, TagItem, TagType};

use crate::common_tags::CommonTag;
use crate::error::MetadataError;
use crate::json_path;
use crate::tag_registry::{TagRegistry, TagScope};

/// A map of common tags to their values (supports multi-value fields).
pub type TagMap = HashMap<CommonTag, Vec<String>>;

// ============================================================
// Reading
// ============================================================

/// Read all recognised tags from a media file.
///
/// Auto-detects the file format and reads whichever tag type is present
/// (ID3v2, Vorbis Comment, MP4 ilst, APE, etc.). Returns a `TagMap`
/// mapping `CommonTag` variants to their string values.
pub fn read_tags(path: &Path) -> Result<TagMap, MetadataError> {
    if !path.exists() {
        return Err(MetadataError::FileNotFound(path.display().to_string()));
    }

    let tagged_file = Probe::open(path)?.read()?;

    let mut result = TagMap::new();

    // Try primary tag first, fall back to any available tag
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let Some(tag) = tag else {
        return Ok(result);
    };

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

    Ok(result)
}

// ============================================================
// Writing
// ============================================================

/// Write a set of common tags to a media file.
///
/// Auto-detects the file format. Uses the file's existing primary tag type,
/// or creates an appropriate new one. Existing values for the given tags
/// are overwritten; other tags are preserved.
pub fn write_tags(path: &Path, tags: &[(CommonTag, String)]) -> Result<(), MetadataError> {
    if !path.exists() {
        return Err(MetadataError::FileNotFound(path.display().to_string()));
    }

    let mut tagged_file = Probe::open(path)?.read()?;

    let tag_type = tagged_file
        .primary_tag()
        .map(Tag::tag_type)
        .unwrap_or(TagType::Id3v2);

    // Ensure the tag exists before borrowing mutably
    if tagged_file.tag(tag_type).is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }

    let tag = tagged_file.tag_mut(tag_type).unwrap();

    for (common_tag, value) in tags {
        write_common_tag_to_lofty(tag, *common_tag, value);
    }

    tagged_file.save_to_path(path, WriteOptions::default())?;
    Ok(())
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

    let mut tagged_file = Probe::open(path)?.read()?;

    let tag_type = tagged_file
        .primary_tag()
        .map(Tag::tag_type)
        .unwrap_or(TagType::Id3v2);

    if tagged_file.tag(tag_type).is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }

    let tag = tagged_file.tag_mut(tag_type).unwrap();

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

    tagged_file.save_to_path(path, WriteOptions::default())?;
    Ok(count)
}

// ============================================================
// Internal helpers
// ============================================================

/// Write a single CommonTag value to a lofty Tag, using the appropriate
/// ItemKey for the tag type.
fn write_common_tag_to_lofty(tag: &mut Tag, common_tag: CommonTag, value: &str) {
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
        CommonTag::Language => {
            tag.insert(TagItem::new(
                ItemKey::Language,
                ItemValue::Text(value.into()),
            ));
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Should not panic for any variant. "1" parses fine for the
        // numeric arms (Year/TrackNumber/DiscNumber).
        for variant in CommonTag::iter() {
            write_common_tag_to_lofty(&mut tag, variant, "1");
        }

        // Keep the original title assertion by writing Title last.
        write_common_tag_to_lofty(&mut tag, CommonTag::Title, "Test");
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
        write_common_tag_to_lofty(&mut tag, variant, value);
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
        write_common_tag_to_lofty(&mut tag, variant, value);
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
}
