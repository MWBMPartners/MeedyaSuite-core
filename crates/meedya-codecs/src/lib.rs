//! # meedya-codecs
//!
//! Codec detection and format handling for the MeedyaSuite ecosystem.
//!
//! Provides:
//! - **FFprobe integration**: Async wrapper for audio stream analysis
//! - **MediaInfo integration**: Dolby Atmos and advanced codec detection
//! - **Tool resolution**: Auto-discovery of FFprobe/MediaInfo binaries
//! - **Codec types**: Enums and utilities for audio codec classification

pub mod ffprobe;
pub mod mediainfo;
pub mod tool_path;

// Re-export core types from meedya-metadata
pub use meedya_metadata::types::{AudioCodec, ChannelConfig, SpatialType};
