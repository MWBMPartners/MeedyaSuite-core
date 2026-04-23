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

pub mod credentials;
pub mod error;
pub mod rate_limiter;
pub mod traits;

pub use credentials::{CredentialSource, CredentialStore, ResolvedCredential};
pub use error::CredentialError;
pub use rate_limiter::{ProviderRateLimiter, RateLimiterRegistry};
pub use traits::MetadataProvider;
