// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Identifier-types registry — the canonical cross-repo vocabulary of
// external/catalogue identifier types (scope -> slug -> validation shape).
//
// Loaded from the compiled-in `identifier_types.toml` (#65). This is DATA,
// not an enum: adding an identifier type is a TOML edit plus a one-line
// update to the expected-slug guard (tests/identifier_registry_guard.rs).
// MeedyaManager/MeedyaDL consume it via `identifier_types()`; the Swift
// bindings and cross-repo CI diffs consume the raw artifact via
// `IDENTIFIER_TYPES_TOML`; iHymns mirrors the artifact and appends its own
// domain-only extension list (ccli, hymnary-tune, ...) in its own repo.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// Raw compiled-in registry artifact. Exported so downstream repos and the
/// planned Swift/WASM bindings can pass through or CI-diff the artifact
/// byte-for-byte instead of re-typing it.
pub const IDENTIFIER_TYPES_TOML: &str = include_str!("../identifier_types.toml");

/// The Luminate-model entity an identifier attaches to.
///
/// ELI5: this says WHAT KIND OF THING an ID number is stuck to — a
/// recording, an artist, a whole product, etc.
/// Why: the Luminate industry data model separates these concepts (a
/// "song" is not the same thing as one specific "recording" of it, and
/// neither is the same as a "release"), and identifier schemes are scoped
/// to exactly one of these — mixing them up is how cross-repo ID drift
/// starts. `#[non_exhaustive]` because new Luminate scopes may need
/// representing without a semver break (mirrors `CommonTag`, #65 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum IdentifierScope {
    Artist,
    Song,
    Recording,
    Work,
    ReleaseGroup,
    Release,
    Product,
    Party,
    AudiovisualWork,
}

/// Whether MeedyaSuite consumers may store/exchange under the slug today.
///
/// ELI5: "active" means you can actually use this ID type right now;
/// "reserved" means the name is saved for later but nothing stores it yet.
/// Why: some identifier schemes are claimed in the registry ahead of any
/// consumer having a storage column/JSON key for them — recording the slug
/// and validation shape costs nothing and prevents a later ad-hoc name
/// (see `identifier_types.toml` field reference, §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum IdentifierStatus {
    /// Consumers may store and exchange values under this slug now.
    Active,
    /// Slug + shape claimed at zero cost; no storage surface consumes it yet.
    Reserved,
}

/// Syntax-validation shape for an identifier's canonical compact form.
///
/// ELI5: how to check if an ID "looks right" — either match it against a
/// regex, or (if there's no public spec) just accept anything non-empty.
/// Why: `check` digits (e.g. GS1 mod-10) are NOT verified here — v1 ships
/// syntax validation only (identifier_types.toml §1.2 `check` field); a
/// check-digit engine is tracked as a follow-up (#65 §9.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum IdentifierValidation {
    /// Anchored regex over the canonical compact form (normalise first:
    /// uppercase, strip '-', '.', spaces).
    Regex { pattern: String },
    /// No public format specification — any non-empty string.
    Free,
}

/// One identifier-type definition from `identifier_types.toml`.
///
/// ELI5: one row of the registry — "ISRC is a recording-scope ID that
/// looks like `USUG12204767`."
/// Why: `String`/`Option<String>` fields (not `&'static str`) because this
/// type is `Deserialize`d from TOML at first access (see `IDENTIFIER_TYPES`
/// below) — the data does not live in the binary as `&'static` string
/// literals. See §1.6 for why this stays Rust-only (no `#[repr(C)]`) for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifierType {
    pub slug: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard: Option<String>,
    pub scope: IdentifierScope,
    pub status: IdentifierStatus,
    pub validation: IdentifierValidation,
    /// Advisory check-digit algorithm name (data only; not executed in v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl IdentifierType {
    /// Whether `value` matches this identifier's canonical compact syntax.
    ///
    /// ELI5: does this string look like a valid ID of this type?
    /// Why: callers normalise first (uppercase, strip '-', '.', spaces) —
    /// this function does not normalise for them, matching iHymns'
    /// normaliser convention so both repos validate the same shape.
    /// `Free` accepts any non-empty value. Check digits are NOT verified.
    pub fn matches_format(&self, value: &str) -> bool {
        match &self.validation {
            IdentifierValidation::Regex { pattern } => match regex::Regex::new(pattern) {
                Ok(re) => re.is_match(value),
                // Unreachable for the compiled-in artifact (guard-tested);
                // defensive false for a hypothetically bad pattern.
                Err(_) => false,
            },
            IdentifierValidation::Free => !value.is_empty(),
        }
    }
}

#[derive(Deserialize)]
struct RawIdentifierTypesToml {
    registry: RegistryMeta,
    identifier: Vec<IdentifierType>,
}

#[derive(Deserialize)]
struct RegistryMeta {
    schema_version: u32,
}

/// Lazily-parsed registry, sorted by slug. Parsed once on first access.
///
/// ELI5: reads the TOML file text into real Rust data structures, the
/// first time anyone asks for it, and remembers the answer after that.
/// Why: follows the `registry.rs` `TAG_REGISTRY` idiom already established
/// in this crate (`include_str!` + `LazyLock` + panic-on-malformed-
/// compiled-in-artifact) — a malformed artifact is a build-time bug that
/// unit tests (below) catch at CI time, not a runtime error path.
static IDENTIFIER_TYPES: LazyLock<Vec<IdentifierType>> = LazyLock::new(|| {
    let raw: RawIdentifierTypesToml = toml::from_str(IDENTIFIER_TYPES_TOML)
        .expect("Failed to parse compiled-in identifier_types.toml");
    assert_eq!(
        raw.registry.schema_version, 1,
        "identifier_types.toml schema_version mismatch — update the loader alongside the artifact"
    );
    let mut types = raw.identifier;
    types.sort_by(|a, b| a.slug.cmp(&b.slug));
    types
});

/// All identifier types, sorted by slug.
pub fn identifier_types() -> &'static [IdentifierType] {
    &IDENTIFIER_TYPES
}

/// Look up one identifier type by its canonical slug.
pub fn identifier_type(slug: &str) -> Option<&'static IdentifierType> {
    IDENTIFIER_TYPES.iter().find(|t| t.slug == slug)
}

/// Slugs with `status = "active"`, sorted.
pub fn active_identifier_slugs() -> Vec<&'static str> {
    IDENTIFIER_TYPES
        .iter()
        .filter(|t| t.status == IdentifierStatus::Active)
        .map(|t| t.slug.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_parses_and_is_nonempty() {
        assert!(!identifier_types().is_empty());
    }

    #[test]
    fn slugs_are_unique_and_kebab_case() {
        let mut seen = HashSet::new();
        for t in identifier_types() {
            assert!(seen.insert(t.slug.as_str()), "duplicate slug: {}", t.slug);
            assert!(
                t.slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "slug not kebab-case charset: {}",
                t.slug
            );
            assert!(!t.slug.starts_with('-'), "slug leading hyphen: {}", t.slug);
            assert!(!t.slug.ends_with('-'), "slug trailing hyphen: {}", t.slug);
            assert!(!t.slug.contains("--"), "slug double hyphen: {}", t.slug);
        }
    }

    #[test]
    fn regex_patterns_compile_and_are_anchored() {
        for t in identifier_types() {
            if let IdentifierValidation::Regex { pattern } = &t.validation {
                let re = regex::Regex::new(pattern)
                    .unwrap_or_else(|e| panic!("{} pattern failed to compile: {e}", t.slug));
                let _ = re;
                assert!(
                    pattern.starts_with('^') && pattern.ends_with('$'),
                    "{} pattern not anchored: {}",
                    t.slug,
                    pattern
                );
            }
        }
    }

    #[test]
    fn regex_entries_have_matching_examples() {
        for t in identifier_types() {
            if matches!(t.validation, IdentifierValidation::Regex { .. }) {
                let example = t
                    .example
                    .as_deref()
                    .unwrap_or_else(|| panic!("{} is regex-validated but has no example", t.slug));
                assert!(
                    t.matches_format(example),
                    "{} example {:?} does not match its own pattern",
                    t.slug,
                    example
                );
            }
        }
    }

    #[test]
    fn lookup_by_slug() {
        let isrc = identifier_type("isrc").expect("isrc must exist");
        assert_eq!(isrc.scope, IdentifierScope::Recording);
        assert!(identifier_type("no-such-slug").is_none());
    }

    #[test]
    fn active_and_reserved_partition() {
        let active = identifier_types()
            .iter()
            .filter(|t| t.status == IdentifierStatus::Active)
            .count();
        let reserved = identifier_types()
            .iter()
            .filter(|t| t.status == IdentifierStatus::Reserved)
            .count();
        assert_eq!(active, 15);
        assert_eq!(reserved, 4);
        assert_eq!(identifier_types().len(), 19);
    }

    #[test]
    fn matches_format_accepts_known_good() {
        assert!(identifier_type("isrc")
            .unwrap()
            .matches_format("USUG12204767"));
        assert!(identifier_type("iswc")
            .unwrap()
            .matches_format("T0712043421"));
        assert!(identifier_type("isni")
            .unwrap()
            .matches_format("000000012146438X"));
        assert!(identifier_type("bowi").unwrap().matches_format("anything"));
    }

    #[test]
    fn matches_format_rejects_malformed() {
        let isrc = identifier_type("isrc").unwrap();
        assert!(!isrc.matches_format("US-UG1-22-04767"));
        assert!(!isrc.matches_format("XX123"));

        let iswc = identifier_type("iswc").unwrap();
        assert!(!iswc.matches_format("T-070.244.078-1"));

        let bowi = identifier_type("bowi").unwrap();
        assert!(!bowi.matches_format(""));
    }
}
