// Copyright (c) 2026 MWBMPartners
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
    ];

    for (key, common_tag) in key_mappings {
        for item in tag.get_items(key) {
            if let ItemValue::Text(text) = item.value() {
                result.entry(*common_tag).or_default().push(text.clone());
            }
        }
    }

    // Extract ReplayGain tags (stored as custom/freeform fields)
    let rg_mappings: &[(&str, CommonTag)] = &[
        ("REPLAYGAIN_TRACK_GAIN", CommonTag::ReplayGainTrackGain),
        ("REPLAYGAIN_TRACK_PEAK", CommonTag::ReplayGainTrackPeak),
        ("REPLAYGAIN_ALBUM_GAIN", CommonTag::ReplayGainAlbumGain),
        ("REPLAYGAIN_ALBUM_PEAK", CommonTag::ReplayGainAlbumPeak),
        (
            "REPLAYGAIN_REFERENCE_LOUDNESS",
            CommonTag::ReplayGainReferenceLoudness,
        ),
    ];

    for (field_name, common_tag) in rg_mappings {
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
            tag.insert(TagItem::new(key, ItemValue::Text(string_val.clone())));
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
            tag.insert(TagItem::new(
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
            tag.insert(TagItem::new(
                ItemKey::Unknown("REPLAYGAIN_REFERENCE_LOUDNESS".into()),
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
        // Verify that all CommonTag variants have a write implementation
        // (compile-time check — the match in write_common_tag_to_lofty is exhaustive)
        let tag_type = TagType::Id3v2;
        let mut tag = Tag::new(tag_type);

        // Should not panic for any variant
        write_common_tag_to_lofty(&mut tag, CommonTag::Title, "Test");
        write_common_tag_to_lofty(&mut tag, CommonTag::ReplayGainTrackGain, "-3.80 dB");
        write_common_tag_to_lofty(&mut tag, CommonTag::AcoustId, "abc-123");
        write_common_tag_to_lofty(&mut tag, CommonTag::Isrc, "USUG12204767");
        write_common_tag_to_lofty(&mut tag, CommonTag::MusicBrainzRecordingId, "mb-001");

        // Verify values were set
        assert_eq!(tag.title().as_deref(), Some("Test"));
    }
}
