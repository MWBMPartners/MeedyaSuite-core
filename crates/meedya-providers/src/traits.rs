// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Core provider traits.
// Merged from MeedyaManager mm-providers/src/traits.rs (BaseProvider)
// and MeedyaConverter MetadataLookup.swift provider abstractions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

/// A metadata search result from any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResult {
    pub provider_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u16>,
    pub cover_art_url: Option<String>,
    pub external_id: Option<String>,
    pub confidence: f64,
    pub metadata: serde_json::Value,
}

/// Core trait for all metadata providers.
///
/// Implementing this trait allows a provider to be registered in the
/// central provider registry and used by any MeedyaSuite application.
#[async_trait::async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Unique identifier for this provider (e.g., "musicbrainz", "tmdb").
    fn id(&self) -> &str;

    /// Human-readable display name.
    fn display_name(&self) -> &str;

    /// Capabilities this provider supports.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Search for metadata matching a query string.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<ProviderResult>, ProviderError>;

    /// Look up a specific item by its provider-specific ID.
    async fn lookup(&self, id: &str) -> Result<ProviderResult, ProviderError>;
}
