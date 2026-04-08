//! # meedya-fingerprint
//!
//! Audio fingerprinting and loudness analysis for the MeedyaSuite ecosystem.
//!
//! Provides:
//! - **Chromaprint**: Pure Rust audio fingerprint generation via symphonia + rusty-chromaprint
//! - **AcoustID**: API client for fingerprint-based music identification
//! - **ReplayGain**: Loudness normalization value computation

#[cfg(feature = "chromaprint")]
pub mod chromaprint;

#[cfg(feature = "acoustid")]
pub mod acoustid;

pub mod replaygain;

use serde::{Deserialize, Serialize};

/// A generated audio fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    /// The raw fingerprint data (compressed).
    pub fingerprint: String,
    /// Duration of the analyzed audio in seconds.
    pub duration: u32,
}

/// An AcoustID match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcoustIdMatch {
    /// AcoustID identifier.
    pub id: String,
    /// Confidence score 0.0–1.0.
    pub score: f64,
    /// Associated MusicBrainz recording IDs.
    pub recordings: Vec<AcoustIdRecording>,
}

/// A MusicBrainz recording associated with an AcoustID match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcoustIdRecording {
    pub id: String,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub duration: Option<u32>,
}

/// ReplayGain analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayGainResult {
    /// Track gain in dB (e.g., "-6.5 dB").
    pub track_gain_db: f64,
    /// Track peak level (0.0–1.0).
    pub track_peak: f64,
}

/// Album-level ReplayGain result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumReplayGain {
    /// Individual track results.
    pub tracks: Vec<ReplayGainResult>,
    /// Album-level gain in dB.
    pub album_gain_db: f64,
    /// Album peak level.
    pub album_peak: f64,
}
