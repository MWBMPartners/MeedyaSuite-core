// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.

use thiserror::Error;

/// Errors from credential operations.
#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential not found for {provider}: {key}")]
    NotFound { provider: String, key: String },

    #[error("keyring error: {0}")]
    KeyringError(String),

    #[error("file I/O error: {0}")]
    IoError(String),
}
