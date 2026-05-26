// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// meedya-fingerprint — AcoustID fingerprinting and ReplayGain analysis
// ====================================================================
//
// Provides:
// - Chromaprint audio fingerprint generation (pure Rust, no fpcalc
//   binary; opt-in via the `chromaprint` cargo feature so consumers
//   that only want AcoustID lookup or ReplayGain analysis don't pay
//   the compile-time cost of `rusty-chromaprint` + `symphonia`).
// - AcoustID API lookup with rate limiting
// - EBU R128 loudness measurement and ReplayGain calculation
//
// This crate produces analysis *results*. It does NOT write tags to files —
// that's the consumer's responsibility (via meedya-metadata or direct
// mp4ameta/lofty usage), since different apps write to different formats.
//
// Extracted from MeedyaDL acoustid_service.rs + replaygain_service.rs.

pub mod acoustid;
#[cfg(feature = "chromaprint")]
pub mod chromaprint;
mod error;
pub mod replaygain;

pub use acoustid::{AcoustIdClient, AcoustIdResult};
#[cfg(feature = "chromaprint")]
pub use chromaprint::generate_fingerprint;
pub use error::FingerprintError;
pub use replaygain::{
    AlbumGainResult, ReplayGainAnalyzer, ReplayGainResult, DEFAULT_REFERENCE_LEVEL,
};
