// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// meedya-fingerprint — AcoustID fingerprinting and ReplayGain analysis
// ====================================================================
//
// Provides:
// - Chromaprint audio fingerprint generation (pure Rust, no fpcalc binary)
// - AcoustID API lookup with rate limiting
// - EBU R128 loudness measurement and ReplayGain calculation
//
// This crate produces analysis *results*. It does NOT write tags to files —
// that's the consumer's responsibility (via meedya-metadata or direct
// mp4ameta/lofty usage), since different apps write to different formats.
//
// Extracted from MeedyaDL acoustid_service.rs + replaygain_service.rs.

pub mod acoustid;
mod error;
pub mod replaygain;

pub use acoustid::{AcoustIdClient, AcoustIdResult};
pub use error::FingerprintError;
pub use replaygain::{
    AlbumGainResult, ReplayGainAnalyzer, ReplayGainResult, DEFAULT_REFERENCE_LEVEL,
};
