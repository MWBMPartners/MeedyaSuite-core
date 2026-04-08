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

/// Spatial audio type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpatialType {
    Stereo,
    DolbyDigital,
    DolbyAtmos,
}

/// Audio codec classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCodec {
    Alac,
    AacLc,
    HeAac,
    Eac3,
    Ac3,
    Flac,
    Opus,
    Vorbis,
    Mp3,
    Wav,
    Unknown,
}

impl AudioCodec {
    pub fn is_lossless(&self) -> bool {
        matches!(self, AudioCodec::Alac | AudioCodec::Flac | AudioCodec::Wav)
    }
}

/// Channel configuration string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channels: u32,
    pub label: String,
}

impl ChannelConfig {
    pub fn from_count(channels: u32) -> Self {
        let label = match channels {
            1 => "1.0".to_string(),
            2 => "2.0".to_string(),
            6 => "5.1".to_string(),
            8 => "7.1".to_string(),
            n => format!("{n}ch"),
        };
        Self { channels, label }
    }
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

/// Capabilities declared by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Media types this provider supports.
    pub media_types: Vec<MediaType>,
    /// Whether this provider supports text search.
    pub supports_search: bool,
    /// Whether this provider supports identifier-based lookup.
    pub supports_identifier_lookup: bool,
    /// Whether this provider can supply cover art.
    pub provides_cover_art: bool,
    /// Whether authentication is required.
    pub requires_auth: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            media_types: vec![MediaType::Music],
            supports_search: true,
            supports_identifier_lookup: false,
            provides_cover_art: false,
            requires_auth: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_lossless_classification() {
        assert!(AudioCodec::Alac.is_lossless());
        assert!(AudioCodec::Flac.is_lossless());
        assert!(!AudioCodec::AacLc.is_lossless());
        assert!(!AudioCodec::Mp3.is_lossless());
    }

    #[test]
    fn channel_config_from_count() {
        assert_eq!(ChannelConfig::from_count(2).label, "2.0");
        assert_eq!(ChannelConfig::from_count(6).label, "5.1");
        assert_eq!(ChannelConfig::from_count(8).label, "7.1");
        assert_eq!(ChannelConfig::from_count(4).label, "4ch");
    }

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
