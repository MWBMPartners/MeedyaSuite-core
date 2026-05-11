// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-metadata — Config-driven metadata tagging for M4A/MP4 files
//
// This crate provides the shared metadata tagging logic for the MeedyaSuite
// family of applications. It is sandboxable (no subprocess spawning) and
// App Store safe.
//
// ## Modules
//
// - `registry` — Tag definitions from tags.toml, JSON path extraction,
//   value type conversion. The `TAG_REGISTRY` static provides cached access.
// - `writer` — Writes registry-driven tags to M4A files, plus ISRC vendor
//   extraction and always-on local tags.
// - `codec_tags` — Codec-specific identification tags (lossless, Atmos,
//   binaural, downmix) and the `CodecKind` enum.
// - `playback_bounds` — User-supplied soft playback start/stop atoms,
//   honored by MeedyaSuite tools only.

pub mod codec_tags;
pub mod playback_bounds;
pub mod registry;
pub mod writer;
