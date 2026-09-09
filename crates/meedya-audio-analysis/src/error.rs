// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.

use thiserror::Error;

/// Errors from tempo and key analysis.
#[derive(Debug, Error)]
pub enum AnalysisError {
    /// The file could not be opened or read. Carries the original
    /// `std::io::Error` (via `#[from]`) rather than a stringified copy, so a
    /// caller can still match on `ErrorKind` (e.g. `NotFound`) if it wants to.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The file opened fine but symphonia could not make sense of it —
    /// unrecognised container, no usable codec, or a decode failure partway
    /// through. Carries a human-readable string rather than symphonia's own
    /// error type, so this crate's public API never leaks a dependency's
    /// error type into a consumer's `match` arms (the same reason
    /// `meedya-fingerprint::FingerprintError::DecodeError` is a `String`).
    #[error("audio decode failed: {0}")]
    Decode(String),

    /// The container was readable but had no audio track at all — e.g. a
    /// video file with only a subtitle stream, or a corrupt header that
    /// zeroed out the track list.
    #[error("no audio track found")]
    NoAudioTrack,

    /// There was not enough audio to say anything useful about tempo or key.
    /// Below this, an autocorrelation-based tempo estimate has too few
    /// candidate periods to distinguish a real beat from noise, and a chroma
    /// average has too few frames to be anything but sample noise — a
    /// confident-looking number here would be actively misleading rather
    /// than merely absent. `seconds` is how much audio was actually found,
    /// for a useful error message ("only 3.0s of audio").
    #[error("audio too short to analyse: {seconds:.1}s (need at least 5s)")]
    TooShort { seconds: f64 },

    /// The reported sample rate was unusable (zero, or missing from the
    /// container and impossible to infer). Every later constant in this
    /// crate is derived from the working sample rate, so there is no safe
    /// default to fall back to — better to refuse than to silently divide
    /// by a made-up number.
    #[error("unsupported sample rate: {0} Hz")]
    UnsupportedSampleRate(u32),
}
