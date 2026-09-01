// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Common/standard metadata tag key definitions.
// These are industry-standard tag names recognised by popular players
// and tag editors (MusicBrainz Picard, Mp3tag, foobar2000, beets).

use serde::{Deserialize, Serialize};
use strum::EnumIter;

/// Standard namespace aliases used across MeedyaSuite.
pub const STANDARD_NAMESPACES: &[(&str, &str)] =
    &[("itunes", "com.apple.iTunes"), ("meedya", "MeedyaMeta")];

/// Well-known metadata tag identifiers.
///
/// These are the canonical tag names recognised by industry-standard
/// tools. Each variant includes the common freeform atom name and
/// equivalent names in other tagging systems.
///
/// ELI5: `#[non_exhaustive]` (#65) means code in OTHER crates can't write
/// an exhaustive `match` over every `CommonTag` variant — they must add a
/// catch-all `_ =>` arm. Code inside THIS crate is unaffected (matches
/// here stay exhaustive, so adding a variant still forces every in-crate
/// mapping method below to handle it).
/// Why: before #65, adding a variant was a breaking change for any
/// downstream exhaustive match. After `#[non_exhaustive]`, new variants
/// are non-breaking (workspace version 0.1.0 → 0.2.0 marks the one-time
/// breaking transition itself — MeedyaSuite-core#65 §2.2). Serde is
/// unaffected: `CommonTag` still (de)serializes as the variant-name
/// string, so an OLDER consumer reading a NEWER producer's payload will
/// still error on an unrecognised variant name — cross-version payload
/// tolerance remains the consumer's concern, not something this attribute
/// grants. New identifier-carrying variants are reserved for tags with a
/// genuine per-container frame mapping (ID3v2/Vorbis/MP4 ilst); a bare
/// external identifier with no container frame belongs in the
/// `identifier_types` registry instead (see `identifier_slug()` below).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
#[non_exhaustive]
pub enum CommonTag {
    // --- Core identifiers ---
    /// International Standard Recording Code
    Isrc,
    /// Universal Product Code (barcode)
    Upc,
    /// MusicBrainz Recording ID
    MusicBrainzRecordingId,
    /// MusicBrainz Release ID
    MusicBrainzReleaseId,
    /// AcoustID fingerprint identifier
    AcoustId,

    // --- Basic metadata ---
    Title,
    Artist,
    AlbumArtist,
    Album,
    Genre,
    Year,
    TrackNumber,
    DiscNumber,
    TotalTracks,
    TotalDiscs,
    Composer,
    Comment,

    // --- Extended metadata ---
    Label,
    Copyright,
    ReleaseDate,
    Compilation,
    Lyrics,
    Description,
    Encoder,

    // --- ReplayGain ---
    ReplayGainTrackGain,
    ReplayGainTrackPeak,
    ReplayGainAlbumGain,
    ReplayGainAlbumPeak,
    ReplayGainReferenceLoudness,

    // --- Catalog / IDs (promoted to first-class) ---
    /// Label's release catalog code (e.g. `SCR-001`). Widely supported by
    /// Mp3tag, MusicBrainz Picard, foobar2000, beets.
    CatalogNumber,
    /// EAN/UPC barcode — separate from `Upc` so callers expecting the
    /// industry-canonical `BARCODE` tag name find it.
    Barcode,
    /// Original release date (for re-releases / remasters) — when the
    /// original was released vs. `ReleaseDate` which refers to this
    /// pressing's date.
    OriginalDate,

    // --- Work / release-group identifiers (#65) ---
    /// MusicBrainz Release Group ID
    MusicBrainzReleaseGroupId,
    /// MusicBrainz Work ID
    MusicBrainzWorkId,
    /// International Standard Musical Work Code (ISO 15707). Canonical
    /// compact form `T` + 10 digits; see `identifier_types.toml` slug `iswc`.
    Iswc,

    // --- Core info (#65) ---
    /// Track subtitle / version qualifier (ID3v2 `TIT3`).
    Subtitle,
    /// Primary content language (ID3v2 `TLAN`; ISO 639-2 recommended).
    Language,

    // --- Contributor roles beyond Composer (#65) ---
    /// Lyric writer (ID3v2 `TEXT`).
    Lyricist,
    /// Conductor (ID3v2 `TPE3`).
    Conductor,
    /// Remixer / "interpreted or remixed by" (ID3v2 `TPE4`).
    Remixer,
    /// Arranger (ID3v2.4 `TIPL` role "arranger"; no MP4 ilst mapping in
    /// lofty 0.22 — MP4 writes via `write_tags` are dropped, freeform
    /// `ARRANGER` is the documented atom name).
    Arranger,
    /// Producer (ID3v2.4 `TIPL` role "producer").
    Producer,
    /// (Recording) engineer (ID3v2.4 `TIPL` role "engineer").
    Engineer,
    /// Mix engineer (Picard "mixer"; ID3v2.4 `TIPL` role "mix").
    Mixer,
}

impl CommonTag {
    /// The standard freeform atom name for MP4/M4A containers
    /// (used in `com.apple.iTunes` namespace).
    pub fn itunes_atom_name(&self) -> &'static str {
        match self {
            Self::Isrc => "ISRC",
            Self::Upc => "UPC",
            Self::MusicBrainzRecordingId => "MusicBrainz Track Id",
            Self::MusicBrainzReleaseId => "MusicBrainz Album Id",
            Self::AcoustId => "Acoustid Id",
            Self::Title => "TITLE",
            Self::Artist => "ARTIST",
            Self::AlbumArtist => "ALBUMARTIST",
            Self::Album => "ALBUM",
            Self::Genre => "GENRE",
            Self::Year => "DATE",
            Self::TrackNumber => "TRACKNUMBER",
            Self::DiscNumber => "DISCNUMBER",
            Self::TotalTracks => "TOTALTRACKS",
            Self::TotalDiscs => "TOTALDISCS",
            Self::Composer => "COMPOSER",
            Self::Comment => "COMMENT",
            Self::Label => "LABEL",
            Self::Copyright => "COPYRIGHT",
            Self::ReleaseDate => "ReleaseDate",
            Self::Compilation => "COMPILATION",
            Self::Lyrics => "LYRICS",
            Self::Description => "DESCRIPTION",
            Self::Encoder => "ENCODER",
            Self::ReplayGainTrackGain => "REPLAYGAIN_TRACK_GAIN",
            Self::ReplayGainTrackPeak => "REPLAYGAIN_TRACK_PEAK",
            Self::ReplayGainAlbumGain => "REPLAYGAIN_ALBUM_GAIN",
            Self::ReplayGainAlbumPeak => "REPLAYGAIN_ALBUM_PEAK",
            Self::ReplayGainReferenceLoudness => "REPLAYGAIN_REFERENCE_LOUDNESS",
            Self::CatalogNumber => "CATALOGNUMBER",
            Self::Barcode => "BARCODE",
            Self::OriginalDate => "ORIGINALDATE",
            Self::MusicBrainzReleaseGroupId => "MusicBrainz Release Group Id",
            Self::MusicBrainzWorkId => "MusicBrainz Work Id",
            Self::Iswc => "ISWC",
            Self::Subtitle => "SUBTITLE",
            Self::Language => "LANGUAGE",
            Self::Lyricist => "LYRICIST",
            Self::Conductor => "CONDUCTOR",
            Self::Remixer => "REMIXER",
            Self::Arranger => "ARRANGER",
            Self::Producer => "PRODUCER",
            Self::Engineer => "ENGINEER",
            Self::Mixer => "MIXER",
        }
    }

    /// The Vorbis Comment field name (used in FLAC, OGG, Opus).
    pub fn vorbis_comment_name(&self) -> &'static str {
        match self {
            Self::Isrc => "ISRC",
            Self::Upc => "UPC",
            Self::MusicBrainzRecordingId => "MUSICBRAINZ_TRACKID",
            Self::MusicBrainzReleaseId => "MUSICBRAINZ_ALBUMID",
            Self::AcoustId => "ACOUSTID_ID",
            Self::Title => "TITLE",
            Self::Artist => "ARTIST",
            Self::AlbumArtist => "ALBUMARTIST",
            Self::Album => "ALBUM",
            Self::Genre => "GENRE",
            Self::Year => "DATE",
            Self::TrackNumber => "TRACKNUMBER",
            Self::DiscNumber => "DISCNUMBER",
            Self::TotalTracks => "TOTALTRACKS",
            Self::TotalDiscs => "TOTALDISCS",
            Self::Composer => "COMPOSER",
            Self::Comment => "COMMENT",
            Self::Label => "LABEL",
            Self::Copyright => "COPYRIGHT",
            Self::ReleaseDate => "DATE",
            Self::Compilation => "COMPILATION",
            Self::Lyrics => "LYRICS",
            Self::Description => "DESCRIPTION",
            Self::Encoder => "ENCODER",
            Self::ReplayGainTrackGain => "REPLAYGAIN_TRACK_GAIN",
            Self::ReplayGainTrackPeak => "REPLAYGAIN_TRACK_PEAK",
            Self::ReplayGainAlbumGain => "REPLAYGAIN_ALBUM_GAIN",
            Self::ReplayGainAlbumPeak => "REPLAYGAIN_ALBUM_PEAK",
            Self::ReplayGainReferenceLoudness => "REPLAYGAIN_REFERENCE_LOUDNESS",
            Self::CatalogNumber => "CATALOGNUMBER",
            Self::Barcode => "BARCODE",
            Self::OriginalDate => "ORIGINALDATE",
            Self::MusicBrainzReleaseGroupId => "MUSICBRAINZ_RELEASEGROUPID",
            Self::MusicBrainzWorkId => "MUSICBRAINZ_WORKID",
            Self::Iswc => "ISWC",
            Self::Subtitle => "SUBTITLE",
            Self::Language => "LANGUAGE",
            Self::Lyricist => "LYRICIST",
            Self::Conductor => "CONDUCTOR",
            Self::Remixer => "REMIXER",
            Self::Arranger => "ARRANGER",
            Self::Producer => "PRODUCER",
            Self::Engineer => "ENGINEER",
            Self::Mixer => "MIXER",
        }
    }

    /// The ID3v2 frame ID (used in MP3). Returns the 4-character frame ID
    /// or TXXX descriptor for freeform fields.
    pub fn id3v2_frame(&self) -> &'static str {
        match self {
            Self::Isrc => "TSRC",
            Self::Upc => "TXXX:UPC",
            Self::MusicBrainzRecordingId => "TXXX:MusicBrainz Track Id",
            Self::MusicBrainzReleaseId => "TXXX:MusicBrainz Album Id",
            Self::AcoustId => "TXXX:Acoustid Id",
            Self::Title => "TIT2",
            Self::Artist => "TPE1",
            Self::AlbumArtist => "TPE2",
            Self::Album => "TALB",
            Self::Genre => "TCON",
            Self::Year => "TDRC",
            Self::TrackNumber => "TRCK",
            Self::DiscNumber => "TPOS",
            Self::TotalTracks => "TRCK", // encoded as "N/M" in TRCK
            Self::TotalDiscs => "TPOS",  // encoded as "N/M" in TPOS
            Self::Composer => "TCOM",
            Self::Comment => "COMM",
            Self::Label => "TPUB",
            Self::Copyright => "TCOP",
            Self::ReleaseDate => "TDRC",
            Self::Compilation => "TCMP",
            Self::Lyrics => "USLT",
            Self::Description => "COMM",
            Self::Encoder => "TSSE",
            Self::ReplayGainTrackGain => "TXXX:REPLAYGAIN_TRACK_GAIN",
            Self::ReplayGainTrackPeak => "TXXX:REPLAYGAIN_TRACK_PEAK",
            Self::ReplayGainAlbumGain => "TXXX:REPLAYGAIN_ALBUM_GAIN",
            Self::ReplayGainAlbumPeak => "TXXX:REPLAYGAIN_ALBUM_PEAK",
            Self::ReplayGainReferenceLoudness => "TXXX:REPLAYGAIN_REFERENCE_LOUDNESS",
            Self::CatalogNumber => "TXXX:CATALOGNUMBER",
            Self::Barcode => "TXXX:BARCODE",
            Self::OriginalDate => "TDOR",
            Self::MusicBrainzReleaseGroupId => "TXXX:MusicBrainz Release Group Id",
            Self::MusicBrainzWorkId => "TXXX:MusicBrainz Work Id",
            Self::Iswc => "TXXX:ISWC",
            Self::Subtitle => "TIT3",
            Self::Language => "TLAN",
            Self::Lyricist => "TEXT",
            Self::Conductor => "TPE3",
            Self::Remixer => "TPE4",
            Self::Arranger => "TIPL:arranger",
            Self::Producer => "TIPL:producer",
            Self::Engineer => "TIPL:engineer",
            Self::Mixer => "TIPL:mix",
        }
    }

    /// The `identifier_types.toml` registry slug for identifier-carrying
    /// variants; `None` for descriptive tags. Total match on purpose (#65):
    /// a new variant must decide, at compile time, whether it is an
    /// external identifier (and therefore must exist in the registry —
    /// enforced by tests/identifier_registry_guard.rs).
    ///
    /// ELI5: for the handful of tags that ARE an external ID number (ISRC,
    /// UPC, MusicBrainz IDs, ISWC), this says which row of
    /// `identifier_types.toml` they correspond to; everything else (title,
    /// artist, lyrics, ...) returns `None`.
    /// Why: this is the compile-time bridge between the `CommonTag`
    /// per-container-frame world (#65 §2–§4) and the `identifier_types`
    /// registry (#65 §1) — a NO-WILDCARD match so a future identifier
    /// variant can't be added without an explicit decision here, which the
    /// guard test `common_tag_identifier_variants_are_registered_and_active`
    /// then checks resolves to a real, active registry slug.
    /// `CatalogNumber` deliberately maps to `None` — label catalogue codes
    /// are not a global identifier scheme (#65 §10.6).
    pub fn identifier_slug(&self) -> Option<&'static str> {
        match self {
            Self::Isrc => Some("isrc"),
            Self::Upc | Self::Barcode => Some("upc"),
            Self::AcoustId => Some("acoustid"),
            Self::MusicBrainzRecordingId => Some("musicbrainz-recording"),
            Self::MusicBrainzReleaseId => Some("musicbrainz-release"),
            Self::MusicBrainzReleaseGroupId => Some("musicbrainz-release-group"),
            Self::MusicBrainzWorkId => Some("musicbrainz-work"),
            Self::Iswc => Some("iswc"),
            Self::Title
            | Self::Artist
            | Self::AlbumArtist
            | Self::Album
            | Self::Genre
            | Self::Year
            | Self::TrackNumber
            | Self::DiscNumber
            | Self::TotalTracks
            | Self::TotalDiscs
            | Self::Composer
            | Self::Comment
            | Self::Label
            | Self::Copyright
            | Self::ReleaseDate
            | Self::Compilation
            | Self::Lyrics
            | Self::Description
            | Self::Encoder
            | Self::ReplayGainTrackGain
            | Self::ReplayGainTrackPeak
            | Self::ReplayGainAlbumGain
            | Self::ReplayGainAlbumPeak
            | Self::ReplayGainReferenceLoudness
            | Self::CatalogNumber
            | Self::OriginalDate
            | Self::Subtitle
            | Self::Language
            | Self::Lyricist
            | Self::Conductor
            | Self::Remixer
            | Self::Arranger
            | Self::Producer
            | Self::Engineer
            | Self::Mixer => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isrc_across_formats() {
        assert_eq!(CommonTag::Isrc.itunes_atom_name(), "ISRC");
        assert_eq!(CommonTag::Isrc.vorbis_comment_name(), "ISRC");
        assert_eq!(CommonTag::Isrc.id3v2_frame(), "TSRC");
    }

    #[test]
    fn catalog_number_across_formats() {
        assert_eq!(CommonTag::CatalogNumber.itunes_atom_name(), "CATALOGNUMBER");
        assert_eq!(
            CommonTag::CatalogNumber.vorbis_comment_name(),
            "CATALOGNUMBER"
        );
        assert_eq!(CommonTag::CatalogNumber.id3v2_frame(), "TXXX:CATALOGNUMBER");
    }

    #[test]
    fn barcode_across_formats() {
        assert_eq!(CommonTag::Barcode.itunes_atom_name(), "BARCODE");
        assert_eq!(CommonTag::Barcode.vorbis_comment_name(), "BARCODE");
        assert_eq!(CommonTag::Barcode.id3v2_frame(), "TXXX:BARCODE");
    }

    #[test]
    fn original_date_across_formats() {
        assert_eq!(CommonTag::OriginalDate.itunes_atom_name(), "ORIGINALDATE");
        assert_eq!(
            CommonTag::OriginalDate.vorbis_comment_name(),
            "ORIGINALDATE"
        );
        // TDOR is the dedicated ID3v2.4 frame for "original release time".
        assert_eq!(CommonTag::OriginalDate.id3v2_frame(), "TDOR");
    }

    #[test]
    fn promoted_ids_distinct_from_musicbrainz_ids() {
        // CatalogNumber is the label's catalog code (e.g., SCR-001),
        // NOT the MusicBrainz release ID. They should map to different
        // atoms in every format.
        assert_ne!(
            CommonTag::CatalogNumber.itunes_atom_name(),
            CommonTag::MusicBrainzReleaseId.itunes_atom_name()
        );
        assert_ne!(
            CommonTag::CatalogNumber.vorbis_comment_name(),
            CommonTag::MusicBrainzReleaseId.vorbis_comment_name()
        );
        assert_ne!(
            CommonTag::CatalogNumber.id3v2_frame(),
            CommonTag::MusicBrainzReleaseId.id3v2_frame()
        );
    }

    #[test]
    fn replaygain_names() {
        assert_eq!(
            CommonTag::ReplayGainTrackGain.vorbis_comment_name(),
            "REPLAYGAIN_TRACK_GAIN"
        );
        assert_eq!(
            CommonTag::ReplayGainTrackGain.id3v2_frame(),
            "TXXX:REPLAYGAIN_TRACK_GAIN"
        );
    }

    #[test]
    fn release_group_and_work_ids_across_formats() {
        assert_eq!(
            CommonTag::MusicBrainzReleaseGroupId.itunes_atom_name(),
            "MusicBrainz Release Group Id"
        );
        assert_eq!(
            CommonTag::MusicBrainzReleaseGroupId.vorbis_comment_name(),
            "MUSICBRAINZ_RELEASEGROUPID"
        );
        assert_eq!(
            CommonTag::MusicBrainzReleaseGroupId.id3v2_frame(),
            "TXXX:MusicBrainz Release Group Id"
        );

        assert_eq!(
            CommonTag::MusicBrainzWorkId.itunes_atom_name(),
            "MusicBrainz Work Id"
        );
        assert_eq!(
            CommonTag::MusicBrainzWorkId.vorbis_comment_name(),
            "MUSICBRAINZ_WORKID"
        );
        assert_eq!(
            CommonTag::MusicBrainzWorkId.id3v2_frame(),
            "TXXX:MusicBrainz Work Id"
        );
    }

    #[test]
    fn iswc_across_formats() {
        assert_eq!(CommonTag::Iswc.itunes_atom_name(), "ISWC");
        assert_eq!(CommonTag::Iswc.vorbis_comment_name(), "ISWC");
        assert_eq!(CommonTag::Iswc.id3v2_frame(), "TXXX:ISWC");
    }

    #[test]
    fn subtitle_and_language_across_formats() {
        assert_eq!(CommonTag::Subtitle.itunes_atom_name(), "SUBTITLE");
        assert_eq!(CommonTag::Subtitle.vorbis_comment_name(), "SUBTITLE");
        assert_eq!(CommonTag::Subtitle.id3v2_frame(), "TIT3");

        assert_eq!(CommonTag::Language.itunes_atom_name(), "LANGUAGE");
        assert_eq!(CommonTag::Language.vorbis_comment_name(), "LANGUAGE");
        assert_eq!(CommonTag::Language.id3v2_frame(), "TLAN");
    }

    #[test]
    fn contributor_roles_across_formats() {
        let roles: &[(CommonTag, &str, &str, &str)] = &[
            (CommonTag::Lyricist, "LYRICIST", "LYRICIST", "TEXT"),
            (CommonTag::Conductor, "CONDUCTOR", "CONDUCTOR", "TPE3"),
            (CommonTag::Remixer, "REMIXER", "REMIXER", "TPE4"),
            (CommonTag::Arranger, "ARRANGER", "ARRANGER", "TIPL:arranger"),
            (CommonTag::Producer, "PRODUCER", "PRODUCER", "TIPL:producer"),
            (CommonTag::Engineer, "ENGINEER", "ENGINEER", "TIPL:engineer"),
            (CommonTag::Mixer, "MIXER", "MIXER", "TIPL:mix"),
        ];

        for (tag, itunes, vorbis, id3) in roles {
            assert_eq!(tag.itunes_atom_name(), *itunes);
            assert_eq!(tag.vorbis_comment_name(), *vorbis);
            assert_eq!(tag.id3v2_frame(), *id3);
        }
    }
}
