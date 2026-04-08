// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.

use thiserror::Error;

/// Errors that can occur in codec/format operations.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("unknown audio codec: {0}")]
    UnknownAudioCodec(String),

    #[error("unknown video codec: {0}")]
    UnknownVideoCodec(String),

    #[error("unknown container format: {0}")]
    UnknownContainerFormat(String),

    #[error("unknown subtitle codec: {0}")]
    UnknownSubtitleCodec(String),

    #[error("codec {codec} is not compatible with container {container}")]
    IncompatibleCodecContainer { codec: String, container: String },

    #[error("failed to parse codec registry: {0}")]
    RegistryParseError(String),

    #[error("unknown file extension: {0}")]
    UnknownExtension(String),
}
