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
use lofty::tag::{Accessor, ItemKey, ItemValue, Tag, TagItem};

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

    // #79 — an untagged file has no `primary_tag()`, and the old fallback
    // hardcoded `TagType::Id3v2` here regardless of container. `insert_tag`
    // silently no-ops when the container doesn't support the tag type it's
    // given (lofty file/tagged_file.rs), so on e.g. a fresh untagged .m4a
    // (the standard MeedyaDL download product) the Id3v2 insert was dropped
    // on the floor and the `tag_mut(tag_type).unwrap()` below panicked on
    // the resulting `None`. `primary_tag_type()` derives the correct tag
    // type from the FILE type instead (Mp4 -> Mp4Ilst, Flac/Opus/Vorbis/
    // Speex -> VorbisComments, etc.), which is always write-supported for
    // its own format, so the insert always lands.
    let tag_type = tagged_file
        .primary_tag()
        .map(Tag::tag_type)
        .unwrap_or_else(|| tagged_file.primary_tag_type());

    // Ensure the tag exists before borrowing mutably
    if tagged_file.tag(tag_type).is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }

    // Unreachable now that tag_type always comes from an existing tag or
    // primary_tag_type() (both write-supported), but house style forbids
    // unwrap — surface a proper error instead of assuming it can't happen.
    let tag = tagged_file.tag_mut(tag_type).ok_or_else(|| {
        MetadataError::UnsupportedFormat(format!(
            "cannot create a {tag_type:?} tag in this container"
        ))
    })?;

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

    // #79 — see the matching comment in write_tags above: fall back to the
    // container's own primary tag type (always write-supported) instead of
    // hardcoding Id3v2, which insert_tag silently drops on non-ID3v2
    // containers.
    let tag_type = tagged_file
        .primary_tag()
        .map(Tag::tag_type)
        .unwrap_or_else(|| tagged_file.primary_tag_type());

    if tagged_file.tag(tag_type).is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }

    // Unreachable — see write_tags above — but house style forbids unwrap.
    let tag = tagged_file.tag_mut(tag_type).ok_or_else(|| {
        MetadataError::UnsupportedFormat(format!(
            "cannot create a {tag_type:?} tag in this container"
        ))
    })?;

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
    // Only used by tests below — kept out of the module-level import so a
    // non-test build doesn't warn about it being unused.
    use lofty::tag::TagType;

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
