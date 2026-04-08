//! # meedya-metadata
//!
//! Unified metadata provider infrastructure for the MeedyaSuite ecosystem.
//!
//! This crate provides:
//! - **Provider traits**: Async `MetadataProvider` trait with search/lookup capabilities
//! - **Provider registry**: Central dispatch across multiple metadata sources
//! - **Tag registry**: Config-driven tag definitions loaded from TOML
//! - **Tag I/O**: Read/write metadata across MP4, ID3v2, Vorbis, APE, and RIFF formats
//! - **Credential management**: 4-tier credential resolution (env → config → keyring → file)
//! - **Rate limiting**: Per-provider token-bucket rate limiter
//! - **Match scoring**: Weighted fuzzy matching for search result ranking
//! - **Cover art utilities**: Selection, classification, deduplication

pub mod types;
pub mod error;
pub mod traits;
pub mod tag_registry;
pub mod tag_io;

#[cfg(feature = "providers")]
pub mod registry;
#[cfg(feature = "providers")]
pub mod credentials;
#[cfg(feature = "providers")]
pub mod rate_limiter;
#[cfg(feature = "providers")]
pub mod match_scoring;
#[cfg(feature = "providers")]
pub mod cover_art;

// Re-exports for convenience
pub use types::*;
pub use error::*;
pub use traits::MetadataProvider;
pub use tag_registry::TagRegistry;

#[cfg(feature = "providers")]
pub use registry::ProviderRegistry;
#[cfg(feature = "providers")]
pub use credentials::CredentialStore;
#[cfg(feature = "providers")]
pub use rate_limiter::{ProviderRateLimiter, RateLimiterRegistry};
#[cfg(feature = "providers")]
pub use match_scoring::MatchScorer;
