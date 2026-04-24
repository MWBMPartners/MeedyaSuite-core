// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Core media record types shared across all MeedyaSuite applications.

use serde::{Deserialize, Serialize};

/// A media record that can be a track, album, or artist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaRecord {
    Track(Track),
    Album(Album),
    Artist(Artist),
}

/// A single audio/video track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration_ms: Option<u64>,
    pub isrc: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u16>,
    pub composer: Option<String>,
    pub release_date: Option<String>,
    pub cover_art_url: Option<String>,
    /// Provider-specific external IDs (e.g., {"musicbrainz": "...", "spotify": "..."}).
    #[serde(default)]
    pub external_ids: std::collections::HashMap<String, String>,
    /// Additional metadata as key-value pairs.
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// An album / release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub track_count: Option<u32>,
    pub disc_count: Option<u32>,
    pub genre: Option<String>,
    pub year: Option<u16>,
    pub release_date: Option<String>,
    pub upc: Option<String>,
    pub label: Option<String>,
    pub copyright: Option<String>,
    pub cover_art_url: Option<String>,
    pub is_compilation: Option<bool>,
    #[serde(default)]
    pub external_ids: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

/// An artist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub id: Option<String>,
    pub name: String,
    pub sort_name: Option<String>,
    pub genres: Vec<String>,
    pub cover_art_url: Option<String>,
    #[serde(default)]
    pub external_ids: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_serialization() {
        let track = Track {
            id: Some("t1".into()),
            title: "Lavender Haze".into(),
            artist: Some("Taylor Swift".into()),
            album: Some("Midnights".into()),
            album_artist: Some("Taylor Swift".into()),
            track_number: Some(1),
            disc_number: Some(1),
            duration_ms: Some(202395),
            isrc: Some("USUG12204767".into()),
            genre: Some("Pop".into()),
            year: Some(2022),
            composer: Some("Taylor Swift".into()),
            release_date: Some("2022-10-21".into()),
            cover_art_url: None,
            external_ids: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
        };
        let json = serde_json::to_string(&track).unwrap();
        let back: Track = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "Lavender Haze");
        assert_eq!(back.isrc, Some("USUG12204767".into()));
    }

    #[test]
    fn media_record_enum_tags() {
        let record = MediaRecord::Track(Track {
            id: None,
            title: "Test".into(),
            artist: None,
            album: None,
            album_artist: None,
            track_number: None,
            disc_number: None,
            duration_ms: None,
            isrc: None,
            genre: None,
            year: None,
            composer: None,
            release_date: None,
            cover_art_url: None,
            external_ids: Default::default(),
            metadata: Default::default(),
        });
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains(r#""type":"track""#));
    }
}
