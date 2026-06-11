// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Quick Tag schema — predefined bucket vocabularies for rapid manual
// tagging. Modelled on OneTagger's Quick Tag interface; the *schema*
// (allowed values + multi-flag per category) lives in core so it's
// shared across MeedyaSuite apps. The *keybinding UI* lives in the
// downstream apps (MeedyaManager).
//
// Storage: each category's values write to `MeedyaMeta:QuickTag<Category>`
// as a comma-separated list (single value when `multi = false`).
//
// No standards-first override: there is no widely-supported standard
// for DJ mood/role/energy categorical tagging across formats. All Quick
// Tag atoms live in MeedyaMeta.

use std::collections::HashMap;

use lofty::tag::Tag;
use serde::Deserialize;

use crate::meedya_atom::{clear_meedya_atom, read_meedya_atom, write_meedya_atom};

const ATOM_PREFIX: &str = "QuickTag";

/// Compiled-in default schema. Mirrors `quick_tags.toml`.
const DEFAULT_SCHEMA_TOML: &str = include_str!("../quick_tags.toml");

// ============================================================
// Schema
// ============================================================

/// Schema describing valid Quick Tag categories + values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickTagSchema {
    /// Categories in TOML-declaration order.
    pub categories: Vec<QuickTagCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QuickTagCategory {
    /// Category key — the part after `QuickTag` in the atom name.
    /// E.g., `mood` → `MeedyaMeta:QuickTagMood`. Stored in PascalCase
    /// for the atom name regardless of TOML key casing.
    #[serde(skip_deserializing)]
    pub key: String,
    /// Ordered list of valid values.
    pub values: Vec<String>,
    /// Whether the category accepts multiple values (comma-separated)
    /// or only one.
    pub multi: bool,
}

impl QuickTagSchema {
    /// Load the bundled default schema (`quick_tags.toml`).
    pub fn load_default() -> Self {
        Self::load_from_str(DEFAULT_SCHEMA_TOML).expect("default quick_tags.toml is valid")
    }

    /// Load from a TOML string. Returns a parse error if the input
    /// doesn't match the documented structure.
    pub fn load_from_str(toml_text: &str) -> Result<Self, String> {
        // TOML doesn't natively give us insertion order for top-level
        // tables, but `toml`'s deserialize-as-IndexMap-via-feature is
        // overkill here. Parse to Value first, then walk top-level tables.
        let parsed: toml::Value = toml::from_str(toml_text).map_err(|e| e.to_string())?;
        let table = parsed
            .as_table()
            .ok_or_else(|| "quick_tags.toml root must be a table".to_owned())?;

        let mut categories = Vec::with_capacity(table.len());
        for (key, value) in table {
            let raw: QuickTagCategory = value
                .clone()
                .try_into()
                .map_err(|e: toml::de::Error| format!("category `{key}`: {e}"))?;
            categories.push(QuickTagCategory {
                key: snake_to_pascal(key),
                values: raw.values,
                multi: raw.multi,
            });
        }
        Ok(Self { categories })
    }

    /// Look up a category by atom key (PascalCase, e.g. `\"Mood\"`).
    pub fn category(&self, key: &str) -> Option<&QuickTagCategory> {
        self.categories.iter().find(|c| c.key == key)
    }
}

// ============================================================
// Values
// ============================================================

/// Quick Tag values keyed by category (atom-key, e.g., `\"Mood\"`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickTagValues {
    pub by_category: HashMap<String, Vec<String>>,
}

impl QuickTagValues {
    /// Set a category's values, dropping any duplicates while preserving
    /// the first-seen order.
    pub fn set(&mut self, category: impl Into<String>, values: Vec<String>) {
        let mut seen = std::collections::HashSet::new();
        let dedup = values
            .into_iter()
            .filter(|v| seen.insert(v.clone()))
            .collect();
        self.by_category.insert(category.into(), dedup);
    }

    /// Get the values for a category (empty Vec if not set).
    pub fn get(&self, category: &str) -> &[String] {
        self.by_category
            .get(category)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.by_category.values().all(|values| values.is_empty())
    }
}

/// Validation errors when applying values against a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickTagValidationError {
    /// Category not declared in the schema.
    UnknownCategory(String),
    /// Value not in the category's allowed list.
    UnknownValue { category: String, value: String },
    /// Category is `multi = false` but multiple values were supplied.
    MultiNotAllowed { category: String, count: usize },
}

impl std::fmt::Display for QuickTagValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCategory(c) => write!(f, "unknown category `{c}`"),
            Self::UnknownValue { category, value } => {
                write!(f, "value `{value}` not allowed in category `{category}`")
            }
            Self::MultiNotAllowed { category, count } => write!(
                f,
                "category `{category}` is single-value but received {count} values"
            ),
        }
    }
}

impl std::error::Error for QuickTagValidationError {}

/// Check `values` against `schema`. Returns first error or `Ok(())`.
pub fn validate(
    values: &QuickTagValues,
    schema: &QuickTagSchema,
) -> Result<(), QuickTagValidationError> {
    for (category, vals) in &values.by_category {
        let Some(cat) = schema.category(category) else {
            return Err(QuickTagValidationError::UnknownCategory(category.clone()));
        };
        if !cat.multi && vals.len() > 1 {
            return Err(QuickTagValidationError::MultiNotAllowed {
                category: category.clone(),
                count: vals.len(),
            });
        }
        for v in vals {
            if !cat.values.contains(v) {
                return Err(QuickTagValidationError::UnknownValue {
                    category: category.clone(),
                    value: v.clone(),
                });
            }
        }
    }
    Ok(())
}

// ============================================================
// Read / Write
// ============================================================

/// Read all Quick Tag values from `tag`, against the given schema.
/// Categories not in the schema are ignored (atoms preserved on disk
/// but not surfaced).
pub fn read_quick_tags(tag: &Tag, schema: &QuickTagSchema) -> QuickTagValues {
    let mut out = QuickTagValues::default();
    for cat in &schema.categories {
        let atom_name = format!("{ATOM_PREFIX}{}", cat.key);
        let Some(raw) = read_meedya_atom(tag, &atom_name) else {
            continue;
        };
        let vals: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if !vals.is_empty() {
            out.by_category.insert(cat.key.clone(), vals);
        }
    }
    out
}

/// Write `values` to `tag` after validating against `schema`. Returns
/// `Err` on validation failure with no partial-write side-effects.
pub fn write_quick_tags(
    tag: &mut Tag,
    values: &QuickTagValues,
    schema: &QuickTagSchema,
) -> Result<(), QuickTagValidationError> {
    validate(values, schema)?;
    for cat in &schema.categories {
        let atom_name = format!("{ATOM_PREFIX}{}", cat.key);
        match values.by_category.get(&cat.key) {
            Some(vals) if !vals.is_empty() => {
                write_meedya_atom(tag, &atom_name, &vals.join(","));
            }
            _ => clear_meedya_atom(tag, &atom_name),
        }
    }
    Ok(())
}

/// Write without validating against the schema. Use only for restoring
/// known-good data or testing.
pub fn write_quick_tags_unchecked(tag: &mut Tag, values: &QuickTagValues, schema: &QuickTagSchema) {
    for cat in &schema.categories {
        let atom_name = format!("{ATOM_PREFIX}{}", cat.key);
        match values.by_category.get(&cat.key) {
            Some(vals) if !vals.is_empty() => {
                write_meedya_atom(tag, &atom_name, &vals.join(","));
            }
            _ => clear_meedya_atom(tag, &atom_name),
        }
    }
}

/// Remove all Quick Tag atoms for categories declared in the schema.
pub fn clear_quick_tags(tag: &mut Tag, schema: &QuickTagSchema) {
    for cat in &schema.categories {
        clear_meedya_atom(tag, &format!("{ATOM_PREFIX}{}", cat.key));
    }
}

// ============================================================
// Helpers
// ============================================================

fn snake_to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            upper_next = true;
        } else if upper_next {
            for c in ch.to_uppercase() {
                out.push(c);
            }
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::tag::TagType;

    fn fresh() -> Tag {
        Tag::new(TagType::Id3v2)
    }

    // ---- Snake → Pascal ----

    #[test]
    fn snake_to_pascal_basics() {
        assert_eq!(snake_to_pascal("mood"), "Mood");
        assert_eq!(snake_to_pascal("crowd_response"), "CrowdResponse");
        assert_eq!(snake_to_pascal("mixing_role"), "MixingRole");
        assert_eq!(snake_to_pascal("energy"), "Energy");
    }

    // ---- Schema ----

    #[test]
    fn default_schema_loads() {
        let s = QuickTagSchema::load_default();
        assert_eq!(s.categories.len(), 4);
        assert!(s.category("Mood").is_some());
        assert!(s.category("Energy").is_some());
        assert!(s.category("CrowdResponse").is_some());
        assert!(s.category("MixingRole").is_some());
    }

    #[test]
    fn default_schema_mood_is_multi() {
        let s = QuickTagSchema::load_default();
        assert!(s.category("Mood").unwrap().multi);
    }

    #[test]
    fn default_schema_energy_is_single() {
        let s = QuickTagSchema::load_default();
        assert!(!s.category("Energy").unwrap().multi);
    }

    #[test]
    fn custom_schema_loads() {
        let toml_text = r#"
[custom_cat]
values = ["a", "b", "c"]
multi = false
"#;
        let s = QuickTagSchema::load_from_str(toml_text).unwrap();
        assert_eq!(s.categories.len(), 1);
        let cat = s.category("CustomCat").unwrap();
        assert!(!cat.multi);
        assert_eq!(cat.values, vec!["a", "b", "c"]);
    }

    #[test]
    fn malformed_toml_returns_err() {
        assert!(QuickTagSchema::load_from_str("not =}{toml").is_err());
    }

    // ---- Validation ----

    #[test]
    fn validate_accepts_known_values() {
        let schema = QuickTagSchema::load_default();
        let mut values = QuickTagValues::default();
        values.set("Mood", vec!["euphoric".into(), "uplifting".into()]);
        values.set("Energy", vec!["7".into()]);
        assert!(validate(&values, &schema).is_ok());
    }

    #[test]
    fn validate_rejects_unknown_category() {
        let schema = QuickTagSchema::load_default();
        let mut values = QuickTagValues::default();
        values.set("MadeUp", vec!["x".into()]);
        assert_eq!(
            validate(&values, &schema),
            Err(QuickTagValidationError::UnknownCategory("MadeUp".into()))
        );
    }

    #[test]
    fn validate_rejects_unknown_value() {
        let schema = QuickTagSchema::load_default();
        let mut values = QuickTagValues::default();
        values.set("Mood", vec!["bouncy".into()]);
        assert_eq!(
            validate(&values, &schema),
            Err(QuickTagValidationError::UnknownValue {
                category: "Mood".into(),
                value: "bouncy".into(),
            })
        );
    }

    #[test]
    fn validate_rejects_multi_on_single_category() {
        let schema = QuickTagSchema::load_default();
        let mut values = QuickTagValues::default();
        values.set("Energy", vec!["7".into(), "8".into()]);
        assert_eq!(
            validate(&values, &schema),
            Err(QuickTagValidationError::MultiNotAllowed {
                category: "Energy".into(),
                count: 2,
            })
        );
    }

    // ---- Round trip ----

    #[test]
    fn round_trip_full_values() {
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        let mut values = QuickTagValues::default();
        values.set("Mood", vec!["euphoric".into(), "uplifting".into()]);
        values.set("Energy", vec!["7".into()]);
        values.set("MixingRole", vec!["intro".into(), "build".into()]);
        write_quick_tags(&mut tag, &values, &schema).unwrap();
        assert_eq!(read_quick_tags(&tag, &schema), values);
    }

    #[test]
    fn round_trip_via_atoms() {
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        let mut values = QuickTagValues::default();
        values.set("Mood", vec!["funky".into(), "dreamy".into()]);
        write_quick_tags(&mut tag, &values, &schema).unwrap();
        // Atom carries comma-separated form
        assert_eq!(read_meedya_atom(&tag, "QuickTagMood"), Some("funky,dreamy"));
    }

    #[test]
    fn read_tolerates_whitespace_in_atoms() {
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        write_meedya_atom(&mut tag, "QuickTagMood", "  funky , dreamy , ");
        let values = read_quick_tags(&tag, &schema);
        assert_eq!(values.get("Mood"), &["funky", "dreamy"]);
    }

    #[test]
    fn empty_category_value_clears_atom_on_write() {
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        // Seed an existing value.
        write_meedya_atom(&mut tag, "QuickTagMood", "euphoric");
        // Now write empty.
        let empty = QuickTagValues::default();
        write_quick_tags(&mut tag, &empty, &schema).unwrap();
        assert_eq!(read_meedya_atom(&tag, "QuickTagMood"), None);
    }

    #[test]
    fn write_fails_atomically_on_validation_error() {
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        // Seed an existing valid value first.
        write_quick_tags(
            &mut tag,
            &{
                let mut v = QuickTagValues::default();
                v.set("Mood", vec!["chill".into()]);
                v
            },
            &schema,
        )
        .unwrap();

        // Attempt to write a partially-invalid set.
        let mut bad = QuickTagValues::default();
        bad.set("Mood", vec!["aggressive".into()]); // valid
        bad.set("Energy", vec!["1".into(), "10".into()]); // invalid: multi
        let result = write_quick_tags(&mut tag, &bad, &schema);
        assert!(result.is_err());
        // Existing data preserved (no partial write).
        assert_eq!(read_meedya_atom(&tag, "QuickTagMood"), Some("chill"));
    }

    #[test]
    fn set_dedupes_values() {
        let mut values = QuickTagValues::default();
        values.set("Mood", vec!["chill".into(), "chill".into(), "dark".into()]);
        assert_eq!(values.get("Mood"), &["chill", "dark"]);
    }

    #[test]
    fn clear_quick_tags_removes_all_known() {
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        let mut values = QuickTagValues::default();
        values.set("Mood", vec!["dark".into()]);
        values.set("Energy", vec!["3".into()]);
        write_quick_tags(&mut tag, &values, &schema).unwrap();
        clear_quick_tags(&mut tag, &schema);
        let read = read_quick_tags(&tag, &schema);
        assert!(read.is_empty());
    }

    #[test]
    fn read_skips_unknown_categories_on_disk() {
        // An atom written for a category not in our current schema
        // should be left alone (not surfaced, not removed).
        let schema = QuickTagSchema::load_default();
        let mut tag = fresh();
        write_meedya_atom(&mut tag, "QuickTagLegacyCat", "old-value");
        let read = read_quick_tags(&tag, &schema);
        assert!(read.get("LegacyCat").is_empty());
        // Atom still on the tag.
        assert_eq!(
            read_meedya_atom(&tag, "QuickTagLegacyCat"),
            Some("old-value")
        );
    }
}
