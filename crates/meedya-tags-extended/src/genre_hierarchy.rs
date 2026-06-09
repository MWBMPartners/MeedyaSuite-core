// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Hierarchical genre schema.
//
// Beatport, Traxsource, Juno Download and most electronic-music platforms
// expose hierarchical genre taxonomies: a track is *House → Tech House →
// Deep Tech*, not just "Tech House". The standard `Genre` tag is flat —
// best we can do there is write the leaf. This module preserves the full
// hierarchy in `MeedyaMeta:*` atoms while still writing the leaf to the
// standard tag for player visibility.
//
// Standards-first compliance: the leaf goes to the standard `Genre` /
// `TCON` / `©gen` / `GENRE` slot (which is what consumers see by default).
// MeedyaMeta carries the structured hierarchy because no standard format
// for hierarchical genres exists in ID3v2 / Vorbis / MP4.

use lofty::tag::{ItemKey, Tag};

use crate::meedya_atom::{clear_meedya_atom, read_meedya_atom, write_meedya_atom};

const ATOM_ROOT: &str = "GenreRoot";
const ATOM_SUBGENRE: &str = "GenreSubgenre";
const ATOM_STYLE: &str = "GenreStyle";
const ATOM_SOURCE: &str = "GenreSource";
const ATOM_FULL: &str = "GenreFull";

const FULL_SEPARATOR: &str = " > ";

// ============================================================
// Public Type
// ============================================================

/// A three-level genre hierarchy (root → subgenre → style).
///
/// Most platforms expose at most three levels; deeper trees are rare and
/// usually flattened into the style field by the source itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenreHierarchy {
    /// Root genre (e.g., "House"). The only required level.
    pub root: String,
    /// Optional intermediate genre (e.g., "Tech House").
    pub subgenre: Option<String>,
    /// Optional leaf style (e.g., "Deep Tech").
    pub style: Option<String>,
    /// Source taxonomy reference. Recommended values:
    /// `beatport-v2`, `discogs`, `musicbrainz`, `traxsource`, `manual`.
    pub source: Option<String>,
}

impl GenreHierarchy {
    /// Build from just a root.
    pub fn root(name: impl Into<String>) -> Self {
        Self {
            root: name.into(),
            subgenre: None,
            style: None,
            source: None,
        }
    }

    /// The deepest set level — `style` if present, else `subgenre`, else
    /// `root`. This is what gets written to the standard `Genre` tag for
    /// player visibility.
    pub fn leaf(&self) -> &str {
        self.style
            .as_deref()
            .or(self.subgenre.as_deref())
            .unwrap_or(&self.root)
    }

    /// Human-readable breadcrumb (`House > Tech House > Deep Tech`).
    pub fn full(&self) -> String {
        let mut parts = vec![self.root.as_str()];
        if let Some(s) = &self.subgenre {
            parts.push(s);
        }
        if let Some(s) = &self.style {
            parts.push(s);
        }
        parts.join(FULL_SEPARATOR)
    }

    /// Parse a breadcrumb string (`Root > Subgenre > Style`) into a
    /// hierarchy. Tolerates extra whitespace around `>`. Empty input
    /// returns `None`.
    pub fn parse_full(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s
            .split('>')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if parts.is_empty() {
            return None;
        }
        Some(Self {
            root: parts[0].to_owned(),
            subgenre: parts.get(1).map(|s| (*s).to_owned()),
            style: parts.get(2).map(|s| (*s).to_owned()),
            source: None,
        })
    }
}

// ============================================================
// Public API
// ============================================================

/// Read genre hierarchy from `tag`. Prefers MeedyaMeta atoms (canonical);
/// falls back to parsing the standard `Genre` tag as a leaf-only hierarchy
/// when MeedyaMeta is absent.
pub fn read_genre_hierarchy(tag: &Tag) -> Option<GenreHierarchy> {
    if let Some(root) = read_meedya_atom(tag, ATOM_ROOT) {
        if !root.is_empty() {
            return Some(GenreHierarchy {
                root: root.to_owned(),
                subgenre: read_meedya_atom(tag, ATOM_SUBGENRE)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                style: read_meedya_atom(tag, ATOM_STYLE)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                source: read_meedya_atom(tag, ATOM_SOURCE)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            });
        }
    }
    // Fall back to the standard Genre tag as a leaf-only hierarchy.
    tag.get_string(&ItemKey::Genre)
        .filter(|s| !s.is_empty())
        .map(|s| GenreHierarchy {
            root: s.to_owned(),
            subgenre: None,
            style: None,
            source: None,
        })
}

/// Write the hierarchy to `tag`:
/// - Standard `Genre` tag gets the leaf (most-specific) so existing players
///   show meaningful info.
/// - MeedyaMeta atoms carry the structured levels + source + full breadcrumb.
pub fn write_genre_hierarchy(tag: &mut Tag, hierarchy: &GenreHierarchy) {
    tag.insert_text(ItemKey::Genre, hierarchy.leaf().to_owned());
    write_meedya_atom(tag, ATOM_ROOT, &hierarchy.root);
    if let Some(s) = &hierarchy.subgenre {
        write_meedya_atom(tag, ATOM_SUBGENRE, s);
    } else {
        clear_meedya_atom(tag, ATOM_SUBGENRE);
    }
    if let Some(s) = &hierarchy.style {
        write_meedya_atom(tag, ATOM_STYLE, s);
    } else {
        clear_meedya_atom(tag, ATOM_STYLE);
    }
    if let Some(s) = &hierarchy.source {
        write_meedya_atom(tag, ATOM_SOURCE, s);
    } else {
        clear_meedya_atom(tag, ATOM_SOURCE);
    }
    write_meedya_atom(tag, ATOM_FULL, &hierarchy.full());
}

/// Remove all genre-hierarchy MeedyaMeta atoms. Does NOT touch the
/// standard `Genre` tag — callers should call this then either
/// `tag.remove_key(&ItemKey::Genre)` or rewrite as desired.
pub fn clear_genre_hierarchy(tag: &mut Tag) {
    for atom in [ATOM_ROOT, ATOM_SUBGENRE, ATOM_STYLE, ATOM_SOURCE, ATOM_FULL] {
        clear_meedya_atom(tag, atom);
    }
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

    fn deep_tech() -> GenreHierarchy {
        GenreHierarchy {
            root: "House".into(),
            subgenre: Some("Tech House".into()),
            style: Some("Deep Tech".into()),
            source: Some("beatport-v2".into()),
        }
    }

    // ---- leaf / full ----

    #[test]
    fn leaf_returns_deepest_set_level() {
        assert_eq!(deep_tech().leaf(), "Deep Tech");

        let no_style = GenreHierarchy {
            style: None,
            ..deep_tech()
        };
        assert_eq!(no_style.leaf(), "Tech House");

        let root_only = GenreHierarchy::root("Ambient");
        assert_eq!(root_only.leaf(), "Ambient");
    }

    #[test]
    fn full_renders_breadcrumb() {
        assert_eq!(deep_tech().full(), "House > Tech House > Deep Tech");
        assert_eq!(GenreHierarchy::root("Ambient").full(), "Ambient");
    }

    #[test]
    fn parse_full_round_trip() {
        let h = GenreHierarchy {
            root: "Drum & Bass".into(),
            subgenre: Some("Liquid".into()),
            style: None,
            source: None,
        };
        let parsed = GenreHierarchy::parse_full(&h.full()).unwrap();
        // source is not present in breadcrumb, but the rest matches.
        assert_eq!(parsed.root, h.root);
        assert_eq!(parsed.subgenre, h.subgenre);
        assert_eq!(parsed.style, h.style);
    }

    #[test]
    fn parse_full_tolerates_extra_whitespace() {
        let h = GenreHierarchy::parse_full("  House  >  Tech House  >  Deep Tech  ").unwrap();
        assert_eq!(h.root, "House");
        assert_eq!(h.subgenre.as_deref(), Some("Tech House"));
        assert_eq!(h.style.as_deref(), Some("Deep Tech"));
    }

    #[test]
    fn parse_full_drops_empty_segments() {
        let h = GenreHierarchy::parse_full("House >  > Deep Tech").unwrap();
        assert_eq!(h.root, "House");
        // After dropping the empty middle, "Deep Tech" becomes position 1 (subgenre).
        assert_eq!(h.subgenre.as_deref(), Some("Deep Tech"));
        assert_eq!(h.style, None);
    }

    #[test]
    fn parse_full_empty_returns_none() {
        assert!(GenreHierarchy::parse_full("").is_none());
        assert!(GenreHierarchy::parse_full("   ").is_none());
        assert!(GenreHierarchy::parse_full("  >  >  ").is_none());
    }

    // ---- read / write ----

    #[test]
    fn write_then_read_full_hierarchy() {
        let mut tag = fresh();
        let h = deep_tech();
        write_genre_hierarchy(&mut tag, &h);
        assert_eq!(read_genre_hierarchy(&tag), Some(h));
    }

    #[test]
    fn write_sets_standard_genre_to_leaf() {
        let mut tag = fresh();
        write_genre_hierarchy(&mut tag, &deep_tech());
        assert_eq!(tag.get_string(&ItemKey::Genre), Some("Deep Tech"));
    }

    #[test]
    fn write_sets_full_breadcrumb_atom() {
        let mut tag = fresh();
        write_genre_hierarchy(&mut tag, &deep_tech());
        assert_eq!(
            read_meedya_atom(&tag, ATOM_FULL),
            Some("House > Tech House > Deep Tech")
        );
    }

    #[test]
    fn read_falls_back_to_standard_genre_as_leaf_only() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::Genre, "Ambient".into());
        let h = read_genre_hierarchy(&tag).unwrap();
        assert_eq!(h.root, "Ambient");
        assert_eq!(h.subgenre, None);
        assert_eq!(h.style, None);
    }

    #[test]
    fn read_prefers_meedyameta_over_standard_genre() {
        let mut tag = fresh();
        // Standard tag says "Wrong"; MeedyaMeta hierarchy says House > Deep Tech.
        tag.insert_text(ItemKey::Genre, "Wrong".into());
        write_meedya_atom(&mut tag, ATOM_ROOT, "House");
        write_meedya_atom(&mut tag, ATOM_STYLE, "Deep Tech");
        let h = read_genre_hierarchy(&tag).unwrap();
        assert_eq!(h.root, "House");
        assert_eq!(h.style.as_deref(), Some("Deep Tech"));
    }

    #[test]
    fn read_empty_tag_returns_none() {
        assert_eq!(read_genre_hierarchy(&fresh()), None);
    }

    #[test]
    fn clear_removes_meedyameta_but_not_standard() {
        let mut tag = fresh();
        write_genre_hierarchy(&mut tag, &deep_tech());
        clear_genre_hierarchy(&mut tag);
        // Standard tag preserved (per documented contract — caller's choice
        // whether to also clear it).
        assert_eq!(tag.get_string(&ItemKey::Genre), Some("Deep Tech"));
        // MeedyaMeta gone — falling back to leaf-only behaviour.
        let h = read_genre_hierarchy(&tag).unwrap();
        assert_eq!(h.root, "Deep Tech");
        assert_eq!(h.subgenre, None);
    }

    #[test]
    fn root_only_round_trip() {
        let mut tag = fresh();
        write_genre_hierarchy(&mut tag, &GenreHierarchy::root("Ambient"));
        let h = read_genre_hierarchy(&tag).unwrap();
        assert_eq!(h.root, "Ambient");
        assert_eq!(h.subgenre, None);
        assert_eq!(h.style, None);
        assert_eq!(read_meedya_atom(&tag, ATOM_FULL), Some("Ambient"));
    }
}
