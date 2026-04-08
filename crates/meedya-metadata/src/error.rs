// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.

use thiserror::Error;

/// Errors that can occur in metadata operations.
#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("failed to parse tag registry TOML: {0}")]
    RegistryParseError(String),

    #[error("unknown value_type '{value_type}' for tag '{tag_id}'")]
    UnknownValueType { tag_id: String, value_type: String },

    #[error("unknown namespace '{namespace}' for tag '{tag_id}' (expected one of: {expected})")]
    UnknownNamespace {
        tag_id: String,
        namespace: String,
        expected: String,
    },

    #[error("JSON path extraction failed for path '{path}'")]
    PathExtractionFailed { path: String },

    #[error("value conversion failed for tag '{tag_id}': {reason}")]
    ValueConversionFailed { tag_id: String, reason: String },

    // --- File I/O errors ---

    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("unsupported file format: {0}")]
    UnsupportedFormat(String),

    #[error("failed to read tags from file: {0}")]
    ReadError(String),

    #[error("failed to write tags to file: {0}")]
    WriteError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("lofty error: {0}")]
    LoftyError(#[from] lofty::error::LoftyError),
}
