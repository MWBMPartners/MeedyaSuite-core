// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// TOML-driven codec registry for service-specific flag mapping.
// Extracted from MeedyaDL codec_registry.rs + codecs.toml.
//
// This module allows each consuming app to define service-specific
// CLI flags for codecs via a TOML configuration file, while the
// canonical codec identity comes from the AudioCodec/VideoCodec enums.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::error::CodecError;

/// A TOML-driven codec registry that maps canonical codec IDs to
/// per-service CLI flags and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecRegistry {
    #[serde(default)]
    pub audio: HashMap<String, AudioCodecEntry>,
    #[serde(default)]
    pub video: HashMap<String, VideoCodecEntry>,
    #[serde(default)]
    pub meta: HashMap<String, MetaCodecEntry>,
    #[serde(default)]
    pub lyrics: HashMap<String, LyricsFormatEntry>,
}

/// An audio codec entry in the registry with per-service flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCodecEntry {
    pub display_name: String,
    #[serde(default)]
    pub lossless: bool,
    #[serde(default)]
    pub spatial: bool,
    #[serde(default)]
    pub filename_suffix: Option<String>,
    #[serde(default)]
    pub services: HashMap<String, ServiceFlags>,
}

/// A video codec entry in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoCodecEntry {
    pub display_name: String,
    #[serde(default)]
    pub filename_suffix: Option<String>,
    #[serde(default)]
    pub services: HashMap<String, ServiceFlags>,
}

/// A meta-codec entry (e.g., "lossless" resolves to ALAC on Apple, FLAC on Spotify).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaCodecEntry {
    pub display_name: String,
    #[serde(default)]
    pub resolves_to: HashMap<String, String>,
}

/// A lyrics format entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsFormatEntry {
    pub display_name: String,
    #[serde(default)]
    pub services: HashMap<String, ServiceFlags>,
}

/// Per-service CLI flags for a codec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceFlags {
    /// The CLI flag value this service expects (e.g., "--codec aac-lc").
    #[serde(default)]
    pub flag: Option<String>,
    /// Whether this codec is available on this service.
    #[serde(default = "default_true")]
    pub available: bool,
}

fn default_true() -> bool {
    true
}

impl CodecRegistry {
    /// Parse a codec registry from TOML content.
    pub fn from_toml(toml_content: &str) -> Result<Self, CodecError> {
        toml::from_str(toml_content)
            .map_err(|e| CodecError::RegistryParseError(e.to_string()))
    }

    /// Look up the service-specific flag for an audio codec.
    pub fn audio_flag(&self, codec_id: &str, service: &str) -> Option<&str> {
        self.audio
            .get(codec_id)?
            .services
            .get(service)?
            .flag
            .as_deref()
    }

    /// Look up the service-specific flag for a video codec.
    pub fn video_flag(&self, codec_id: &str, service: &str) -> Option<&str> {
        self.video
            .get(codec_id)?
            .services
            .get(service)?
            .flag
            .as_deref()
    }

    /// Resolve a meta-codec (e.g., "lossless") to a concrete codec for a service.
    pub fn resolve_meta(&self, meta_id: &str, service: &str) -> Option<&str> {
        self.meta
            .get(meta_id)?
            .resolves_to
            .get(service)
            .map(|s| s.as_str())
    }

    /// Get the filename suffix for an audio codec.
    pub fn audio_suffix(&self, codec_id: &str) -> Option<&str> {
        self.audio
            .get(codec_id)?
            .filename_suffix
            .as_deref()
    }

    /// Check if an audio codec is available on a given service.
    pub fn is_audio_available(&self, codec_id: &str, service: &str) -> bool {
        self.audio
            .get(codec_id)
            .and_then(|entry| entry.services.get(service))
            .map(|flags| flags.available)
            .unwrap_or(false)
    }

    /// List all audio codec IDs.
    pub fn audio_codec_ids(&self) -> Vec<&str> {
        self.audio.keys().map(|s| s.as_str()).collect()
    }

    /// List all video codec IDs.
    pub fn video_codec_ids(&self) -> Vec<&str> {
        self.video.keys().map(|s| s.as_str()).collect()
    }
}

/// Global default registry (empty). Apps should load their own from TOML.
static EMPTY_REGISTRY: LazyLock<CodecRegistry> = LazyLock::new(|| CodecRegistry {
    audio: HashMap::new(),
    video: HashMap::new(),
    meta: HashMap::new(),
    lyrics: HashMap::new(),
});

/// Get a reference to an empty default registry.
/// Apps should call `CodecRegistry::from_toml()` with their own config instead.
pub fn empty_registry() -> &'static CodecRegistry {
    &EMPTY_REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
[audio.aac-lc]
display_name = "AAC-LC"
lossless = false
filename_suffix = "AAC"

[audio.aac-lc.services.gamdl]
flag = "aac"

[audio.aac-lc.services.votify]
flag = "aac"
available = true

[audio.alac]
display_name = "ALAC"
lossless = true
filename_suffix = "ALAC"

[audio.alac.services.gamdl]
flag = "alac"

[meta.lossless]
display_name = "Lossless (best available)"

[meta.lossless.resolves_to]
apple_music = "alac"
spotify = "flac"
"#;

    #[test]
    fn test_parse_registry() {
        let registry = CodecRegistry::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(registry.audio.len(), 2);
        assert_eq!(registry.meta.len(), 1);
    }

    #[test]
    fn test_audio_flag_lookup() {
        let registry = CodecRegistry::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(registry.audio_flag("aac-lc", "gamdl"), Some("aac"));
        assert_eq!(registry.audio_flag("alac", "gamdl"), Some("alac"));
        assert_eq!(registry.audio_flag("aac-lc", "unknown"), None);
    }

    #[test]
    fn test_meta_resolution() {
        let registry = CodecRegistry::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(registry.resolve_meta("lossless", "apple_music"), Some("alac"));
        assert_eq!(registry.resolve_meta("lossless", "spotify"), Some("flac"));
    }

    #[test]
    fn test_filename_suffix() {
        let registry = CodecRegistry::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(registry.audio_suffix("alac"), Some("ALAC"));
    }
}
