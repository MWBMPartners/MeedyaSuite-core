//! # meedya-core
//!
//! Unified re-export crate for the MeedyaSuite shared library.
//!
//! This crate provides a single dependency for MeedyaSuite applications
//! to access all shared functionality:
//!
//! ```toml
//! [dependencies]
//! meedya-core = { git = "https://github.com/MWBMPartners/MeedyaSuite-core", features = ["full"] }
//! ```
//!
//! ## Feature Flags
//!
//! - `metadata` — Tag registry, metadata I/O, provider traits (default)
//! - `codecs` — FFprobe/MediaInfo codec detection (default)
//! - `fingerprint` — Audio fingerprinting and ReplayGain (default)
//! - `providers` — Provider registry, credentials, rate limiting, match scoring
//! - `keyring` — OS keyring credential storage
//! - `chromaprint` — Chromaprint fingerprint generation
//! - `acoustid` — AcoustID API client
//! - `full` — Everything

#[cfg(feature = "metadata")]
pub use meedya_metadata as metadata;

#[cfg(feature = "codecs")]
pub use meedya_codecs as codecs;

#[cfg(feature = "fingerprint")]
pub use meedya_fingerprint as fingerprint;

#[cfg(feature = "metadata")]
pub mod prelude {
    //! Convenient imports for common types.
    pub use meedya_metadata::{
        error::{CredentialError, ProviderError, TagError},
        tag_registry::{self, TagRegistry, TagScope},
        traits::MetadataProvider,
        types::*,
    };

    #[cfg(feature = "providers")]
    pub use meedya_metadata::{
        credentials::CredentialStore,
        match_scoring::MatchScorer,
        rate_limiter::{ProviderRateLimiter, RateLimiterRegistry},
        registry::ProviderRegistry,
    };
}
