// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// meedya-providers — Shared metadata provider framework
// ======================================================
//
// Centralised provider traits, registry, rate limiting, credential
// management, and shared implementations for metadata lookup services
// (MusicBrainz, TMDB, TheTVDB, AcoustID, Discogs, etc.) used across
// all MeedyaSuite applications.

pub mod cover_art;
pub mod credentials;
pub mod error;
pub mod extra_keys;
pub mod match_scoring;
pub mod rate_limiter;
pub mod traits;
pub mod types;

// Conditionally compiled provider implementations — each gated behind its
// own `provider-<name>` Cargo feature so downstream apps can opt into only
// the providers they need.
#[cfg(any(
    feature = "provider-musicbrainz",
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-apple-podcasts",
    feature = "provider-isrc",
    feature = "provider-eidr",
    feature = "provider-iswc",
))]
pub mod providers;

pub use cover_art::{best_cover_art, has_cover_art, CoverArtSize};
pub use credentials::{CredentialSource, CredentialStore, ResolvedCredential};
pub use error::CredentialError;
pub use match_scoring::{MatchScorer, ScoringWeights};
pub use rate_limiter::{ProviderRateLimiter, RateLimiterRegistry};
pub use traits::{is_retryable, MetadataProvider, ProviderCapabilities, ProviderError};
pub use types::{CoverArtInfo, MediaType, ProviderResult, SearchQuery};
