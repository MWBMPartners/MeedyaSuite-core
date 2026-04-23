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
pub mod video_codec;
pub mod subtitle_codec;
pub mod container;
pub mod hdr;
pub mod spatial;
pub mod classify;
pub mod registry;
mod error;

pub use audio_codec::AudioCodec;
pub use video_codec::VideoCodec;
pub use subtitle_codec::SubtitleCodec;
pub use container::ContainerFormat;
pub use hdr::HdrFormat;
pub use spatial::SpatialAudioFormat;
pub use classify::{MediaGroup, MediaFormat, MediaClass, MediaQuality, MediaClassification};
pub use registry::CodecRegistry;
pub use error::CodecError;
