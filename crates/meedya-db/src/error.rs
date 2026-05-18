// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("MeedyaDB API error: {0}")]
    ApiError(String),

    #[error("authentication failed: {0}")]
    AuthError(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("database export error: {0}")]
    ExportError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),
}
