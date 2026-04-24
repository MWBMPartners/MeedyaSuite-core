// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Core provider traits.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::{ProviderResult, SearchQuery};

/// Errors from metadata provider operations.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider not configured: {0}")]
    NotConfigured(String),

    #[error("authentication failed for provider {provider}: {reason}")]
    AuthenticationFailed { provider: String, reason: String },

    #[error("rate limited by provider {0}")]
    RateLimited(String),

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("operation not supported: {0}")]
    NotSupported(String),

    #[error("provider returned no results")]
    NoResults,

    #[error("provider error: {0}")]
    Other(String),
}

/// Capabilities that a metadata provider supports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub music_search: bool,
    pub video_search: bool,
    pub podcast_search: bool,
    pub cover_art: bool,
    pub lyrics: bool,
    pub fingerprint_lookup: bool,
    pub identifier_lookup: bool,
}

/// Core trait for all metadata providers.
#[async_trait::async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Unique identifier for this provider (e.g., "musicbrainz", "tmdb").
    fn id(&self) -> &str;

    /// Human-readable display name.
    fn display_name(&self) -> &str;

    /// Capabilities this provider supports.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Search for metadata matching the given query.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError>;

    /// Look up a specific item by provider-specific ID.
    async fn lookup(&self, id: &str) -> Result<Option<ProviderResult>, ProviderError> {
        let _ = id;
        Err(ProviderError::NotSupported(format!(
            "{} does not support direct lookup",
            self.display_name()
        )))
    }
}
