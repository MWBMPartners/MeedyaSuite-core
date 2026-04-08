// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// meedya-metadata — Tag schemas, metadata read/write, TOML tag registry
// =====================================================================
//
// Provides a config-driven metadata tag system that maps API JSON fields
// to file-level metadata atoms. Adding a new tag requires only editing
// the TOML config — zero code changes.
//
// Extracted from MeedyaDL tag_registry.rs + tags.toml and MeedyaManager
// metadata/tag_registry.rs + tags.json5.

pub mod tag_registry;
pub mod json_path;
pub mod common_tags;
mod error;

pub use tag_registry::{
    TagRegistry, TagDefinition, TagValueType, AtomTarget, TagScope,
};
pub use json_path::{extract_json_value, value_to_string};
pub use common_tags::{CommonTag, STANDARD_NAMESPACES};
pub use error::MetadataError;
