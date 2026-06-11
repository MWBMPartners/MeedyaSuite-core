// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Provider-layer value types: search queries, results, cover art.
//
// Adopted from interesting-mirzakhani's meedya-metadata/types.rs. AudioCodec /
// ChannelConfig / SpatialType from that file were not included — those live
// in meedya-codecs here and have a different variant set.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Media type that a provider supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Music,
    Video,
    Podcast,
    Identifier,
}

/// Cover art metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverArtInfo {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// A unified search query accepted by all providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub media_type: Option<MediaType>,
    /// Maximum results to return per provider.
    pub max_results: Option<usize>,
    // Identifiers
    pub isrc: Option<String>,
    pub upc: Option<String>,
    pub iswc: Option<String>,
    pub eidr: Option<String>,
    pub musicbrainz_id: Option<String>,
}

impl SearchQuery {
    /// Create a simple title + artist music query (max 10 results).
    pub fn music(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            artist: Some(artist.into()),
            media_type: Some(MediaType::Music),
            max_results: Some(10),
            ..Default::default()
        }
    }

    /// Create a video / film lookup query (max 10 results).
    pub fn video(title: impl Into<String>, year: Option<u32>) -> Self {
        Self {
            title: Some(title.into()),
            year,
            media_type: Some(MediaType::Video),
            max_results: Some(10),
            ..Default::default()
        }
    }

    /// Create a query by ISRC identifier (max 5 results).
    pub fn by_isrc(isrc: impl Into<String>) -> Self {
        Self {
            isrc: Some(isrc.into()),
            media_type: Some(MediaType::Identifier),
            max_results: Some(5),
            ..Default::default()
        }
    }

    /// Create a query by ISWC identifier (max 5 results).
    pub fn by_iswc(iswc: impl Into<String>) -> Self {
        Self {
            iswc: Some(iswc.into()),
            media_type: Some(MediaType::Identifier),
            max_results: Some(5),
            ..Default::default()
        }
    }

    /// Create a query by EIDR identifier (max 5 results).
    pub fn by_eidr(eidr: impl Into<String>) -> Self {
        Self {
            eidr: Some(eidr.into()),
            media_type: Some(MediaType::Identifier),
            max_results: Some(5),
            ..Default::default()
        }
    }
}

/// A standardized result returned by any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResult {
    pub provider_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub genre: Option<String>,
    /// Confidence score 0.0–1.0.
    pub score: f64,
    /// Cover art options.
    #[serde(default)]
    pub cover_art: Vec<CoverArtInfo>,
    /// Additional metadata fields (provider-specific).
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    // Identifiers
    pub isrc: Option<String>,
    pub upc: Option<String>,
    pub musicbrainz_id: Option<String>,
}

impl ProviderResult {
    pub fn new(provider_name: impl Into<String>) -> Self {
        Self {
            provider_name: provider_name.into(),
            title: None,
            artist: None,
            album: None,
            year: None,
            track_number: None,
            disc_number: None,
            genre: None,
            score: 0.0,
            cover_art: Vec::new(),
            metadata: HashMap::new(),
            isrc: None,
            upc: None,
            musicbrainz_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_result_builder() {
        let result = ProviderResult::new("test");
        assert_eq!(result.provider_name, "test");
        assert_eq!(result.score, 0.0);
        assert!(result.metadata.is_empty());
    }

    #[test]
    fn search_query_defaults() {
        let q = SearchQuery::default();
        assert!(q.title.is_none());
        assert!(q.media_type.is_none());
    }

    #[test]
    fn media_type_serialization() {
        let json = serde_json::to_string(&MediaType::Music).unwrap();
        assert_eq!(json, r#""music""#);
        let parsed: MediaType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MediaType::Music);
    }
}
