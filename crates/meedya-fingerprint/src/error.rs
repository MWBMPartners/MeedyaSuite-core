// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.

use thiserror::Error;

/// Errors from fingerprinting and loudness analysis.
#[derive(Debug, Error)]
pub enum FingerprintError {
    #[error("audio decode failed: {0}")]
    DecodeError(String),

    #[error("fingerprint generation failed: {0}")]
    FingerprintFailed(String),

    #[error("AcoustID API error: {0}")]
    AcoustIdApiError(String),

    #[error("AcoustID API key not configured")]
    ApiKeyMissing,

    #[error("rate limited — retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("no AcoustID match found")]
    NoMatch,

    #[error("FFmpeg not found at expected path: {0}")]
    FfmpegNotFound(String),

    #[error("FFmpeg analysis failed: {0}")]
    FfmpegError(String),

    #[error("loudness parse error: {0}")]
    LoudnessParseError(String),

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
