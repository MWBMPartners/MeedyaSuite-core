// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// JSON path extraction and value conversion.
// Extracted from MeedyaDL tag_registry.rs.

use crate::tag_registry::TagValueType;

/// Extract a value from a `serde_json::Value` using a dot-separated path.
///
/// Supports:
/// - Simple paths: `"attributes.name"` -> `json["attributes"]["name"]`
/// - Nested objects: `"attributes.editorialNotes.short"`
/// - Array indexing: `"attributes.previews[0].url"`
/// - Relationship paths: `"relationships.artists.data[0].id"`
///
/// Returns `None` if any segment is missing or an index is out of bounds.
pub fn extract_json_value(json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = json;

    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        if let Some(bracket_pos) = segment.find('[') {
            let key = &segment[..bracket_pos];
            let index_str = segment[bracket_pos + 1..].strip_suffix(']')?;
            let index: usize = index_str.parse().ok()?;

            if !key.is_empty() {
                current = current.get(key)?;
            }
            current = current.as_array()?.get(index)?;
        } else {
            current = current.get(segment)?;
        }
    }

    Some(current.clone())
}

/// Convert a JSON value to a UTF-8 string according to `TagValueType` rules.
///
/// Returns `None` if the value cannot be converted (null, wrong type, empty array).
pub fn value_to_string(value: &serde_json::Value, value_type: &TagValueType) -> Option<String> {
    match value_type {
        TagValueType::String => value.as_str().map(ToString::to_string).or_else(|| {
            if value.is_null() {
                None
            } else {
                Some(value.to_string())
            }
        }),
        TagValueType::Bool => value.as_bool().map(|b| b.to_string()),
        TagValueType::U32 | TagValueType::U64 => value.as_u64().map(|n| n.to_string()),
        TagValueType::Array => {
            let arr = value.as_array()?;
            let items: Vec<&str> = arr.iter().filter_map(serde_json::Value::as_str).collect();
            if items.is_empty() {
                None
            } else {
                Some(items.join(", "))
            }
        }
        TagValueType::FirstOfArray => value
            .as_array()?
            .first()
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_path() {
        let j = json!({"attributes": {"name": "Midnights"}});
        assert_eq!(
            extract_json_value(&j, "attributes.name"),
            Some(json!("Midnights"))
        );
    }

    #[test]
    fn nested_path() {
        let j = json!({"attributes": {"editorialNotes": {"short": "Brilliant."}}});
        assert_eq!(
            extract_json_value(&j, "attributes.editorialNotes.short"),
            Some(json!("Brilliant."))
        );
    }

    #[test]
    fn array_index() {
        let j = json!({"data": [{"id": "123"}, {"id": "456"}]});
        assert_eq!(extract_json_value(&j, "data[0].id"), Some(json!("123")));
        assert_eq!(extract_json_value(&j, "data[1].id"), Some(json!("456")));
    }

    #[test]
    fn missing_path_returns_none() {
        let j = json!({"attributes": {"name": "Test"}});
        assert!(extract_json_value(&j, "attributes.nonexistent").is_none());
        assert!(extract_json_value(&j, "missing.path").is_none());
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let j = json!({"data": []});
        assert!(extract_json_value(&j, "data[0].id").is_none());
    }

    #[test]
    fn top_level_key() {
        let j = json!({"id": "42"});
        assert_eq!(extract_json_value(&j, "id"), Some(json!("42")));
    }

    #[test]
    fn value_string() {
        assert_eq!(
            value_to_string(&json!("hello"), &TagValueType::String),
            Some("hello".into())
        );
    }

    #[test]
    fn value_null_returns_none() {
        assert!(value_to_string(&json!(null), &TagValueType::String).is_none());
    }

    #[test]
    fn value_bool() {
        assert_eq!(
            value_to_string(&json!(true), &TagValueType::Bool),
            Some("true".into())
        );
        assert_eq!(
            value_to_string(&json!(false), &TagValueType::Bool),
            Some("false".into())
        );
    }

    #[test]
    fn value_u64() {
        assert_eq!(
            value_to_string(&json!(202395), &TagValueType::U64),
            Some("202395".into())
        );
    }

    #[test]
    fn value_array() {
        assert_eq!(
            value_to_string(&json!(["Pop", "Music"]), &TagValueType::Array),
            Some("Pop, Music".into())
        );
    }

    #[test]
    fn value_empty_array_returns_none() {
        assert!(value_to_string(&json!([]), &TagValueType::Array).is_none());
    }

    #[test]
    fn value_first_of_array() {
        assert_eq!(
            value_to_string(&json!(["Pop", "Music"]), &TagValueType::FirstOfArray),
            Some("Pop".into())
        );
    }
}
