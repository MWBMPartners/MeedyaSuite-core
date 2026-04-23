// Copyright (c) 2026 MWBMPartners
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
pub mod match_scoring;
pub mod rate_limiter;
pub mod traits;
pub mod types;

pub use cover_art::CoverArtSize;
pub use credentials::{CredentialSource, CredentialStore, ResolvedCredential};
pub use error::CredentialError;
pub use match_scoring::{MatchScorer, ScoringWeights};
pub use rate_limiter::{ProviderRateLimiter, RateLimiterRegistry};
pub use traits::{MetadataProvider, ProviderCapabilities, ProviderError};
pub use types::{CoverArtInfo, MediaType, ProviderResult, SearchQuery};
