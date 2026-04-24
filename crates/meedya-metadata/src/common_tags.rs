// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Common/standard metadata tag key definitions.
// These are industry-standard tag names recognised by popular players
// and tag editors (MusicBrainz Picard, Mp3tag, foobar2000, beets).

use serde::{Deserialize, Serialize};

/// Standard namespace aliases used across MeedyaSuite.
pub const STANDARD_NAMESPACES: &[(&str, &str)] =
    &[("itunes", "com.apple.iTunes"), ("meedya", "MeedyaMeta")];

/// Well-known metadata tag identifiers.
///
/// These are the canonical tag names recognised by industry-standard
/// tools. Each variant includes the common freeform atom name and
/// equivalent names in other tagging systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}
