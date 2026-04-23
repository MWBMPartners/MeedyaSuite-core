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
//! - `metadata` — Tag schemas, lofty tag I/O, tag registry (default)
//! - `codecs` — Audio/video/subtitle codec definitions, FFprobe/MediaInfo
//!   detection, tool-path resolver (default)
//! - `fingerprint` — AcoustID client, ReplayGain analyzer (default)
//! - `lyrics` — LRCLIB client, LRC I/O, sidecar + tag-embed writes (default,
//!   pulls in `metadata`)
//! - `providers` — Provider traits, credentials, rate limiter, match scoring,
//!   cover art utilities (default)
//! - `db` — MeedyaDB client scaffolding
//! - `keyring` — OS keyring credential storage (pulls in `providers`)
//! - `full` — Everything

#[cfg(feature = "metadata")]
pub use meedya_metadata as metadata;

#[cfg(feature = "codecs")]
pub use meedya_codecs as codecs;

#[cfg(feature = "fingerprint")]
pub use meedya_fingerprint as fingerprint;

#[cfg(feature = "lyrics")]
pub use meedya_lyrics as lyrics;

#[cfg(feature = "providers")]
pub use meedya_providers as providers;

#[cfg(feature = "db")]
pub use meedya_db as db;

pub mod prelude {
    //! Convenient imports for common types.

    #[cfg(feature = "metadata")]
    pub use meedya_metadata::{CommonTag, MetadataError, TagRegistry};

    #[cfg(feature = "codecs")]
    pub use meedya_codecs::{AudioCodec, ChannelConfig, CodecRegistry, ContainerFormat, SpatialType};

    #[cfg(feature = "providers")]
    pub use meedya_providers::{
        CredentialStore, MetadataProvider, ProviderCapabilities, ProviderRateLimiter,
        ProviderResult, SearchQuery,
    };

    #[cfg(feature = "lyrics")]
    pub use meedya_lyrics::{Lyrics, LyricsProvider, SyncedLine, TrackQuery};
}
