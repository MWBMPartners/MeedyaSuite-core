// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// meedya-metadata — Tag schemas, metadata read/write, TOML tag registry
// =====================================================================
//
// Provides a config-driven metadata tag system that maps API JSON fields
// to file-level metadata atoms. Two parallel surfaces co-exist:
//
// **Lofty-backed (extracted from MeedyaDL/MeedyaManager):**
// - `common_tags` — `CommonTag` enum + standard-namespace mapping.
// - `tag_io` — read/write via `lofty` (ID3v2 / Vorbis / MP4 ilst / APE / ...).
// - `tag_registry` — TOML-driven definitions (`TagDefinition`, `TagScope`).
// - `json_path` — dot-path extraction with array indexing.
//
// **mp4ameta-backed (sandbox / App Store safe — no subprocess spawning):**
// - `registry` — Tag definitions from `tags.toml`, JSON path extraction,
//   value type conversion. The `TAG_REGISTRY` static provides cached access.
// - `writer` — Writes registry-driven tags to M4A files, plus ISRC vendor
//   extraction and always-on local tags.
// - `codec_tags` — Codec-specific identification tags (lossless, Atmos,
//   binaural, downmix) and the `CodecKind` enum.
// - `playback_bounds` — User-supplied soft playback start/stop atoms,
//   honored by MeedyaSuite tools only.

pub mod codec_tags;
pub mod common_tags;
mod error;
pub mod json_path;
pub mod playback_bounds;
pub mod registry;
pub mod tag_io;
pub mod tag_registry;
pub mod template;
pub mod writer;

pub use common_tags::{CommonTag, STANDARD_NAMESPACES};
pub use error::MetadataError;
pub use json_path::{extract_json_value, value_to_string};
pub use tag_io::{
    read_tags, write_acoustid_tags, write_registry_tags, write_replaygain_tags, write_tags, TagMap,
};
pub use tag_registry::{AtomTarget, TagDefinition, TagRegistry, TagScope, TagValueType};
pub use template::{TagSource, Template, TemplateError};
