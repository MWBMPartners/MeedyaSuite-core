use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Built-in default tag definitions.
const DEFAULT_TAGS_TOML: &str = include_str!("../config/tags.toml");

/// Global default tag registry, loaded once at first access.
pub static DEFAULT_REGISTRY: LazyLock<TagRegistry> = LazyLock::new(|| {
    TagRegistry::from_toml(DEFAULT_TAGS_TOML).expect("built-in tags.toml must be valid")
});

/// Value type for a tag, controlling how JSON values are converted to strings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagValueType {
    String,
    Bool,
    U32,
    U64,
    Array,
    FirstOfArray,
}

/// Target atom in a specific namespace.
#[derive(Debug, Clone, Deserialize)]
pub struct AtomTarget {
    pub namespace: String,
    pub name: String,
}

impl AtomTarget {
    /// Resolve namespace shorthand to full namespace string.
    pub fn resolved_namespace(&self) -> &str {
        match self.namespace.as_str() {
            "itunes" => "com.apple.iTunes",
            "meedya" => "MeedyaMeta",
            other => other,
        }
    }
}

/// A single tag definition loaded from configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct TagDefinition {
    /// Dot-separated path into API JSON response.
    pub json_path: String,
    /// How to convert the JSON value to a string.
    pub value_type: TagValueType,
    /// Target atoms to write.
    pub atoms: Vec<AtomTarget>,
}

/// Raw TOML structure for deserialization.
#[derive(Debug, Deserialize)]
struct RawTagFile {
    #[serde(default)]
    album: HashMap<String, TagDefinition>,
    #[serde(default)]
    track: HashMap<String, TagDefinition>,
}

/// Registry of tag definitions, split by scope (album vs track).
#[derive(Debug, Clone)]
pub struct TagRegistry {
    pub album_tags: Vec<(String, TagDefinition)>,
    pub track_tags: Vec<(String, TagDefinition)>,
}

impl TagRegistry {
    /// Parse a TOML string into a tag registry.
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        let raw: RawTagFile = toml::from_str(toml_str).map_err(|e| e.to_string())?;

        let mut album_tags: Vec<_> = raw.album.into_iter().collect();
        album_tags.sort_by(|a, b| a.0.cmp(&b.0));

        let mut track_tags: Vec<_> = raw.track.into_iter().collect();
        track_tags.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(Self {
            album_tags,
            track_tags,
        })
    }

    /// Get a tag definition by scope and ID.
    pub fn get(&self, scope: TagScope, id: &str) -> Option<&TagDefinition> {
        let tags = match scope {
            TagScope::Album => &self.album_tags,
            TagScope::Track => &self.track_tags,
        };
        tags.iter().find(|(tag_id, _)| tag_id == id).map(|(_, def)| def)
    }

    /// List all tag IDs and their JSON paths.
    pub fn all_paths(&self) -> Vec<(TagScope, String, String)> {
        let mut paths = Vec::new();
        for (id, def) in &self.album_tags {
            paths.push((TagScope::Album, id.clone(), def.json_path.clone()));
        }
        for (id, def) in &self.track_tags {
            paths.push((TagScope::Track, id.clone(), def.json_path.clone()));
        }
        paths.sort();
        paths
    }
}

/// Tag scope: album-level or track-level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TagScope {
    Album,
    Track,
}

/// Extract a value from a JSON object using a dot-separated path.
///
/// Supports array indexing (e.g., `previews[0].url`).
///
/// Returns `None` for missing paths, out-of-bounds indices, or malformed
/// path syntax (e.g., unclosed brackets).
pub fn extract_json_value(json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = json;

    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }

        // Check for array index: e.g., "previews[0]"
        if let Some(bracket_pos) = segment.find('[') {
            // Validate bracket syntax: must end with ']'
            let idx_str = segment
                .get(bracket_pos + 1..)?
                .strip_suffix(']')?;
            let idx: usize = idx_str.parse().ok()?;

            let key = &segment[..bracket_pos];
            if !key.is_empty() {
                current = current.get(key)?;
            }
            current = current.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }

    Some(current.clone())
}

/// Convert a JSON value to a string based on the declared value type.
pub fn value_to_string(value: &serde_json::Value, value_type: &TagValueType) -> Option<String> {
    match value_type {
        TagValueType::String => match value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            _ => Some(value.to_string()),
        },
        TagValueType::Bool => match value {
            serde_json::Value::Bool(b) => Some(if *b { "1" } else { "0" }.to_string()),
            serde_json::Value::String(s) => {
                let lower = s.to_lowercase();
                Some(
                    if lower == "true" || lower == "1" { "1" } else { "0" }.to_string(),
                )
            }
            _ => None,
        },
        TagValueType::U32 | TagValueType::U64 => match value {
            serde_json::Value::Number(n) => n.as_u64().map(|n| n.to_string()),
            serde_json::Value::String(s) => s.parse::<u64>().ok().map(|n| n.to_string()),
            _ => None,
        },
        TagValueType::Array => match value {
            serde_json::Value::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect();
                Some(items.join(", "))
            }
            _ => Some(value.to_string()),
        },
        TagValueType::FirstOfArray => match value {
            serde_json::Value::Array(arr) => arr.first().map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }),
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None,
        },
    }
}

/// Well-known tag constants for standardized access.
pub mod tags {
    pub const TITLE: &str = "title";
    pub const ARTIST: &str = "artist";
    pub const ALBUM: &str = "album";
    pub const ALBUM_ARTIST: &str = "album_artist";
    pub const GENRE: &str = "genre";
    pub const YEAR: &str = "year";
    pub const TRACK_NUMBER: &str = "track_number";
    pub const DISC_NUMBER: &str = "disc_number";
    pub const COMPOSER: &str = "composer";
    pub const COMMENT: &str = "comment";
    pub const COPYRIGHT: &str = "copyright";
    pub const LYRICS: &str = "lyrics";
    pub const ISRC: &str = "isrc";
    pub const UPC: &str = "upc";
    pub const RECORD_LABEL: &str = "record_label";
    pub const RELEASE_DATE: &str = "release_date";
    pub const IS_COMPILATION: &str = "is_compilation";
    pub const CONTENT_RATING: &str = "content_rating";
    pub const ENCODER: &str = "encoder";
    // Sort keys
    pub const TITLE_SORT: &str = "title_sort";
    pub const ARTIST_SORT: &str = "artist_sort";
    pub const ALBUM_SORT: &str = "album_sort";
    pub const ALBUM_ARTIST_SORT: &str = "album_artist_sort";
    pub const COMPOSER_SORT: &str = "composer_sort";
    // Classical
    pub const WORK: &str = "work";
    pub const MOVEMENT: &str = "movement";
    pub const MOVEMENT_NUMBER: &str = "movement_number";
    pub const MOVEMENT_COUNT: &str = "movement_count";
    // ReplayGain
    pub const REPLAYGAIN_TRACK_GAIN: &str = "replaygain_track_gain";
    pub const REPLAYGAIN_TRACK_PEAK: &str = "replaygain_track_peak";
    pub const REPLAYGAIN_ALBUM_GAIN: &str = "replaygain_album_gain";
    pub const REPLAYGAIN_ALBUM_PEAK: &str = "replaygain_album_peak";
    // MeedyaSuite-specific
    pub const SOURCE_STORE: &str = "source_store";
    pub const ENCODE_SOURCE: &str = "encode_source";
    pub const SPATIAL_TYPE: &str = "spatial_type";
    pub const CHANNEL_CONFIG: &str = "channel_config";
    pub const IS_LOSSLESS: &str = "is_lossless";
    pub const IS_BINAURAL: &str = "is_binaural";
    pub const IS_DOWNMIX: &str = "is_downmix";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_registry() {
        let reg = &*DEFAULT_REGISTRY;
        assert!(!reg.album_tags.is_empty(), "should have album tags");
        assert!(!reg.track_tags.is_empty(), "should have track tags");
    }

    #[test]
    fn get_tag_by_scope_and_id() {
        let reg = &*DEFAULT_REGISTRY;
        let upc = reg.get(TagScope::Album, "upc");
        assert!(upc.is_some(), "should find album.upc tag");
        assert_eq!(upc.unwrap().value_type, TagValueType::String);
    }

    #[test]
    fn all_paths_sorted() {
        let reg = &*DEFAULT_REGISTRY;
        let paths = reg.all_paths();
        assert!(!paths.is_empty());
        // Verify sorted
        for w in paths.windows(2) {
            assert!(w[0] <= w[1], "paths should be sorted");
        }
    }

    #[test]
    fn extract_simple_path() {
        let json: serde_json::Value = serde_json::json!({
            "attributes": {
                "name": "Test Song",
                "artistName": "Test Artist"
            }
        });
        let val = extract_json_value(&json, "attributes.name").unwrap();
        assert_eq!(val.as_str().unwrap(), "Test Song");
    }

    #[test]
    fn extract_array_index_path() {
        let json: serde_json::Value = serde_json::json!({
            "previews": [
                { "url": "https://example.com/preview.m4a" }
            ]
        });
        let val = extract_json_value(&json, "previews[0].url").unwrap();
        assert_eq!(val.as_str().unwrap(), "https://example.com/preview.m4a");
    }

    #[test]
    fn extract_missing_path() {
        let json = serde_json::json!({"a": 1});
        assert!(extract_json_value(&json, "b.c").is_none());
    }

    #[test]
    fn value_to_string_conversions() {
        assert_eq!(
            value_to_string(&serde_json::json!("hello"), &TagValueType::String),
            Some("hello".to_string())
        );
        assert_eq!(
            value_to_string(&serde_json::json!(true), &TagValueType::Bool),
            Some("1".to_string())
        );
        assert_eq!(
            value_to_string(&serde_json::json!(false), &TagValueType::Bool),
            Some("0".to_string())
        );
        assert_eq!(
            value_to_string(&serde_json::json!(42), &TagValueType::U32),
            Some("42".to_string())
        );
        assert_eq!(
            value_to_string(&serde_json::json!(["a", "b", "c"]), &TagValueType::Array),
            Some("a, b, c".to_string())
        );
        assert_eq!(
            value_to_string(&serde_json::json!(["first", "second"]), &TagValueType::FirstOfArray),
            Some("first".to_string())
        );
    }

    #[test]
    fn atom_target_namespace_resolution() {
        let itunes = AtomTarget {
            namespace: "itunes".to_string(),
            name: "UPC".to_string(),
        };
        assert_eq!(itunes.resolved_namespace(), "com.apple.iTunes");

        let meedya = AtomTarget {
            namespace: "meedya".to_string(),
            name: "AppleUPC".to_string(),
        };
        assert_eq!(meedya.resolved_namespace(), "MeedyaMeta");

        let custom = AtomTarget {
            namespace: "com.example".to_string(),
            name: "Foo".to_string(),
        };
        assert_eq!(custom.resolved_namespace(), "com.example");
    }
}
