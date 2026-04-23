// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-codecs — Shared codec, container, and media format definitions
// =====================================================================
//
// Canonical type definitions for audio codecs, video codecs, subtitle
// formats, container formats, HDR formats, spatial audio formats, and
// media classification used across all MeedyaSuite applications.
//
// Consumed by:
//   - MeedyaDL (Rust/Tauri) — direct Cargo dependency
//   - MeedyaManager (Rust) — direct Cargo dependency
//   - MeedyaConverter (Swift) — via bindings/swift C FFI / XCFramework

pub mod audio_codec;
pub mod channel_config;
pub mod classify;
pub mod container;
pub mod ffprobe;
pub mod hdr;
pub mod mediainfo;
pub mod registry;
pub mod spatial;
pub mod spatial_type;
pub mod subtitle_codec;
pub mod tool_path;
pub mod video_codec;
mod error;

pub use audio_codec::AudioCodec;
pub use channel_config::ChannelConfig;
pub use classify::{MediaGroup, MediaFormat, MediaClass, MediaQuality, MediaClassification};
pub use container::ContainerFormat;
pub use error::CodecError;
pub use hdr::HdrFormat;
pub use registry::CodecRegistry;
pub use spatial::SpatialAudioFormat;
pub use spatial_type::SpatialType;
pub use subtitle_codec::SubtitleCodec;
pub use video_codec::VideoCodec;
