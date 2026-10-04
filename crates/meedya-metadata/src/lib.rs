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
// - identifier_types — cross-repo identifier-type registry (identifier_types.toml, #65).

pub mod codec_tags;
pub mod common_tags;
mod error;
// Private: reads a file's ID3v2 language (`TLAN`) frames straight from
// its bytes, for `tag_io` (see that file's top comment for why).
mod id3v2_language_frames;
pub mod identifier_types;
pub mod json_path;
// Private: checks an M4A save atom by atom, on a temporary copy, before it
// replaces the file (issue #102; see that file's top comment for why).
mod mp4_save_check;
pub mod playback_bounds;
pub mod registry;
// Private: how that temporary copy is made and how it takes the file's
// place — and what a copy-then-rename costs (see that file's top comment).
mod save_by_copy;
pub mod tag_io;
pub mod tag_registry;
pub mod template;
pub mod writer;

pub use common_tags::{CommonTag, STANDARD_NAMESPACES};
pub use error::MetadataError;
pub use identifier_types::{
    active_identifier_slugs, identifier_type, identifier_types, IdentifierScope, IdentifierStatus,
    IdentifierType, IdentifierValidation, IDENTIFIER_TYPES_TOML,
};
pub use json_path::{extract_json_value, value_to_string};
pub use tag_io::{
    read_tags, write_acoustid_tags, write_registry_tags, write_replaygain_tags, write_tags, TagMap,
};
pub use tag_registry::{AtomTarget, TagDefinition, TagRegistry, TagScope, TagValueType};
pub use template::{TagSource, Template, TemplateError};
