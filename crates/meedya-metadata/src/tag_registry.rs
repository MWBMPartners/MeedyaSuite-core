// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Config-driven tag registry.
// Extracted from MeedyaDL tag_registry.rs and MeedyaManager metadata/tag_registry.rs.
//
// Adding a new metadata tag requires only editing the TOML config —
// zero Rust code changes.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::MetadataError;

// ============================================================
// Public Types
// ============================================================

/// Which scope a tag belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagScope {
    /// Written to every file in a collection (same value per file).
    Album,
    /// Written per-track (matched by track/disc number).
    Track,
}

/// Complete tag registry containing album-scope and track-scope definitions.
#[derive(Debug, Clone)]
pub struct TagRegistry {
    /// Tags written to every track (same value per file).
    pub album_tags: Vec<TagDefinition>,
    /// Tags written per-track.
    pub track_tags: Vec<TagDefinition>,
    /// Namespace aliases (e.g., "itunes" -> "com.apple.iTunes").
    pub namespaces: HashMap<String, String>,
}

/// A single tag definition.
#[derive(Debug, Clone)]
pub struct TagDefinition {
    /// Unique identifier (e.g., "content_rating", "isrc").
    pub id: String,
    /// Dot-separated path into the source JSON (e.g., "attributes.isrc").
    pub json_path: String,
    /// How to convert the JSON value to a UTF-8 string.
    pub value_type: TagValueType,
    /// Target atoms to write this value to.
    pub atoms: Vec<AtomTarget>,
}

/// Rules for converting a JSON value to a metadata string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagValueType {
    /// Direct string value.
    String,
    /// JSON bool -> "true" or "false".
    Bool,
    /// JSON number -> decimal string (u32 range).
    U32,
    /// JSON number -> decimal string (u64 range).
    U64,
    /// JSON array of strings -> comma-separated.
    Array,
    /// First element of a JSON string array.
    FirstOfArray,
}

/// A target metadata atom/field (namespace + name).
#[derive(Debug, Clone)]
pub struct AtomTarget {
    /// Full namespace string (e.g., "com.apple.iTunes" or "MeedyaMeta").
    pub namespace: String,
    /// Atom/field name (e.g., "ISRC", "AlbumReleaseDate").
    pub name: String,
}

// ============================================================
// TOML deserialization types (private)
// ============================================================

#[derive(Deserialize)]
struct RawTagsToml {
    #[serde(default)]
    namespaces: HashMap<String, String>,
    #[serde(default)]
    album: HashMap<String, RawTagDefinition>,
    #[serde(default)]
    track: HashMap<String, RawTagDefinition>,
}

#[derive(Deserialize)]
struct RawTagDefinition {
    json_path: String,
    value_type: String,
    atoms: Vec<RawAtom>,
}

#[derive(Deserialize)]
struct RawAtom {
    namespace: String,
    name: String,
}

// ============================================================
// Default namespace aliases
// ============================================================

/// Standard namespace aliases used across MeedyaSuite apps.
fn default_namespaces() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("itunes".into(), "com.apple.iTunes".into());
    m.insert("meedya".into(), "MeedyaMeta".into());
    m
}

// ============================================================
// Loading
// ============================================================

impl TagRegistry {
    /// Parse a tag registry from TOML content.
    ///
    /// The TOML should have optional `[namespaces]`, `[album.*]`, and
    /// `[track.*]` sections. See the `tags.toml` bundled with MeedyaDL
    /// for the canonical format.
    pub fn from_toml(toml_content: &str) -> Result<Self, MetadataError> {
        let raw: RawTagsToml = toml::from_str(toml_content)
            .map_err(|e| MetadataError::RegistryParseError(e.to_string()))?;

        // Merge user-defined namespaces with defaults
        let mut namespaces = default_namespaces();
        for (alias, full) in &raw.namespaces {
            namespaces.insert(alias.clone(), full.clone());
        }

        let mut album_tags: Vec<TagDefinition> = raw
            .album
            .into_iter()
            .map(|(id, def)| convert_definition(id, def, &namespaces))
            .collect::<Result<_, _>>()?;
        album_tags.sort_by(|a, b| a.id.cmp(&b.id));

        let mut track_tags: Vec<TagDefinition> = raw
            .track
            .into_iter()
            .map(|(id, def)| convert_definition(id, def, &namespaces))
            .collect::<Result<_, _>>()?;
        track_tags.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(TagRegistry {
            album_tags,
            track_tags,
            namespaces,
        })
    }

    /// Look up a tag definition by ID in either scope.
    pub fn find_tag(&self, id: &str) -> Option<(&TagDefinition, TagScope)> {
        self.album_tags
            .iter()
            .find(|t| t.id == id)
            .map(|t| (t, TagScope::Album))
            .or_else(|| {
                self.track_tags
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| (t, TagScope::Track))
            })
    }

    /// Get all JSON paths defined in the registry.
    ///
    /// Returns sorted `(scope, tag_id, json_path)` tuples.
    pub fn all_known_paths(&self) -> Vec<(TagScope, String, String)> {
        let mut paths = Vec::new();
        for def in &self.album_tags {
            paths.push((TagScope::Album, def.id.clone(), def.json_path.clone()));
        }
        for def in &self.track_tags {
            paths.push((TagScope::Track, def.id.clone(), def.json_path.clone()));
        }
        paths.sort_by(|a, b| a.1.cmp(&b.1));
        paths
    }

    /// Total number of tag definitions.
    pub fn len(&self) -> usize {
        self.album_tags.len() + self.track_tags.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.album_tags.is_empty() && self.track_tags.is_empty()
    }
}

fn convert_definition(
    id: String,
    raw: RawTagDefinition,
    namespaces: &HashMap<String, String>,
) -> Result<TagDefinition, MetadataError> {
    let value_type = match raw.value_type.as_str() {
        "string" => TagValueType::String,
        "bool" => TagValueType::Bool,
        "u32" => TagValueType::U32,
        "u64" => TagValueType::U64,
        "array" => TagValueType::Array,
        "first_of_array" => TagValueType::FirstOfArray,
        other => {
            return Err(MetadataError::UnknownValueType {
                tag_id: id,
                value_type: other.into(),
            })
        }
    };

    let atoms = raw
        .atoms
        .into_iter()
        .map(|raw_atom| {
            let full_ns = namespaces
                .get(&raw_atom.namespace)
                .cloned()
                .unwrap_or_else(|| raw_atom.namespace.clone());
            Ok(AtomTarget {
                namespace: full_ns,
                name: raw_atom.name,
            })
        })
        .collect::<Result<Vec<_>, MetadataError>>()?;

    Ok(TagDefinition {
        id,
        json_path: raw.json_path,
        value_type,
        atoms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
[namespaces]
itunes = "com.apple.iTunes"
meedya = "MeedyaMeta"
custom = "com.example.custom"

[album.upc]
json_path = "attributes.upc"
value_type = "string"
atoms = [
    { namespace = "itunes", name = "UPC" },
    { namespace = "itunes", name = "Barcode" },
    { namespace = "meedya", name = "AppleUPC" },
]

[album.genre]
json_path = "attributes.genreNames"
value_type = "first_of_array"
atoms = [
    { namespace = "itunes", name = "AlbumGenre" },
]

[album.track_count]
json_path = "attributes.trackCount"
value_type = "u32"
atoms = [
    { namespace = "itunes", name = "TOTALTRACKS" },
]

[track.isrc]
json_path = "attributes.isrc"
value_type = "string"
atoms = [
    { namespace = "itunes", name = "ISRC" },
]

[track.duration_in_millis]
json_path = "attributes.durationInMillis"
value_type = "u64"
atoms = [
    { namespace = "itunes", name = "DurationMs" },
]

[track.custom_tag]
json_path = "attributes.custom"
value_type = "string"
atoms = [
    { namespace = "custom", name = "MyTag" },
]
"#;

    #[test]
    fn parse_sample_registry() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(registry.album_tags.len(), 3);
        assert_eq!(registry.track_tags.len(), 3);
        assert_eq!(registry.len(), 6);
    }

    #[test]
    fn namespace_resolution() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        let (upc, scope) = registry.find_tag("upc").unwrap();
        assert_eq!(scope, TagScope::Album);
        assert_eq!(upc.atoms[0].namespace, "com.apple.iTunes");
        assert_eq!(upc.atoms[2].namespace, "MeedyaMeta");
    }

    #[test]
    fn custom_namespace_resolution() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        let (tag, _) = registry.find_tag("custom_tag").unwrap();
        assert_eq!(tag.atoms[0].namespace, "com.example.custom");
    }

    #[test]
    fn upc_has_three_atoms() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        let (upc, _) = registry.find_tag("upc").unwrap();
        assert_eq!(upc.atoms.len(), 3);
        assert_eq!(upc.value_type, TagValueType::String);
    }

    #[test]
    fn isrc_is_track_scope() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        let (_, scope) = registry.find_tag("isrc").unwrap();
        assert_eq!(scope, TagScope::Track);
    }

    #[test]
    fn all_known_paths_covers_both_scopes() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        let paths = registry.all_known_paths();
        assert_eq!(paths.len(), 6);
        assert!(paths.iter().any(|(s, _, _)| *s == TagScope::Album));
        assert!(paths.iter().any(|(s, _, _)| *s == TagScope::Track));
    }

    #[test]
    fn sorted_output() {
        let registry = TagRegistry::from_toml(SAMPLE_TOML).unwrap();
        let ids: Vec<&str> = registry.album_tags.iter().map(|t| t.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "album_tags should be sorted by id");
    }

    #[test]
    fn unknown_value_type_errors() {
        let bad_toml = r#"
[track.bad]
json_path = "x"
value_type = "invalid_type"
atoms = [{ namespace = "itunes", name = "X" }]
"#;
        let result = TagRegistry::from_toml(bad_toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid_type"));
    }
}
