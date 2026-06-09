// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// AI content disclosure tags.
//
// As AI-generated and AI-touched media becomes prevalent, distribution
// platforms (Spotify, Apple Music, YouTube) require AI disclosure. This
// module reads/writes a small canonical set of flags so MeedyaSuite tools
// can filter, sort, preserve, and ultimately translate to platform-
// specific AI metadata at distribution time.
//
// **Dual-write strategy**: each boolean writes to BOTH a generic freeform
// tag (e.g., `IS_AI` via ItemKey::Unknown) AND a `MeedyaMeta:*` canonical.
// Third-party tools reading either format pick up the value. When a
// formal ID3v2 / Vorbis / MP4 standard AI field emerges, callers should
// migrate the canonical to the standard slot and keep `MeedyaMeta:*` as
// an audit-trail mirror (same pattern as `LABEL` and `COPYRIGHT` in
// `meedya-metadata/tags.toml`).
//
// **Truthy boolean parsing**: reads accept `1`/`TRUE`/`true`/`YES`/`yes`/
// `Y`/`y`/`T`/`t`/`on` as true; `0`/`FALSE`/`false`/`NO`/`no`/`N`/`n`/`F`/
// `f`/`off`/empty as false; anything else as `None` (unknown/garbage —
// we don't guess). Writes always emit `\"1\"` / `\"0\"` for consistency.

use lofty::tag::{ItemKey, ItemValue, Tag, TagItem};
use serde::{Deserialize, Serialize};

use crate::meedya_atom::{clear_meedya_atom, read_meedya_atom, write_meedya_atom};

// Atom names (MeedyaMeta canonical)
const ATOM_IS_AI: &str = "IsAI";
const ATOM_AI_USED: &str = "AIUsed";
const ATOM_AI_ENHANCED: &str = "AIEnhanced";
const ATOM_AI_ENHANCE_DETAIL: &str = "AIEnhanceDetail";

// Generic freeform names (dual-write target — for cross-tool visibility)
const GENERIC_IS_AI: &str = "IS_AI";
const GENERIC_AI_USED: &str = "AI_USED";
const GENERIC_AI_ENHANCED: &str = "AI_ENHANCED";
const GENERIC_AI_ENHANCE_DETAIL: &str = "AI_ENHANCE_DETAIL";

// ============================================================
// Public Type
// ============================================================

/// AI content disclosure flags for a single file.
///
/// All fields default to `None` (not set). Writers emit only the fields
/// that are `Some`; readers leave absent fields as `None` rather than
/// defaulting to false, so callers can distinguish \"explicitly not AI\"
/// from \"unspecified\".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiContentFlags {
    /// Content was created **fully** using AI (e.g. Suno, Udio output with
    /// no human stems).
    pub is_ai: Option<bool>,
    /// Content used AI on **some** elements (AI vocal stem, AI-generated
    /// drums, AI mastering).
    pub ai_used: Option<bool>,
    /// Content was processed through AI **after** creation — AI upscaling,
    /// AI HDR / colour grading, AI channel upmixing, AI noise reduction.
    pub ai_enhanced: Option<bool>,
    /// Freetext: what AI models + settings were used (e.g. \"Topaz Photo AI
    /// v3.0 + Gigapixel\", \"iZotope RX 11 voice de-noise\").
    pub ai_enhance_detail: Option<String>,
}

impl AiContentFlags {
    pub fn is_empty(&self) -> bool {
        self.is_ai.is_none()
            && self.ai_used.is_none()
            && self.ai_enhanced.is_none()
            && self.ai_enhance_detail.is_none()
    }
}

// ============================================================
// Public API
// ============================================================

/// Read AI content flags from `tag`. Prefers `MeedyaMeta:*` (canonical);
/// falls back to the generic freeform tag if MeedyaMeta is absent.
pub fn read_ai_content(tag: &Tag) -> AiContentFlags {
    AiContentFlags {
        is_ai: read_bool_field(tag, ATOM_IS_AI, GENERIC_IS_AI),
        ai_used: read_bool_field(tag, ATOM_AI_USED, GENERIC_AI_USED),
        ai_enhanced: read_bool_field(tag, ATOM_AI_ENHANCED, GENERIC_AI_ENHANCED),
        ai_enhance_detail: read_string_field(
            tag,
            ATOM_AI_ENHANCE_DETAIL,
            GENERIC_AI_ENHANCE_DETAIL,
        ),
    }
}

/// Write AI content flags to `tag`. Each `Some` field writes BOTH the
/// `MeedyaMeta:*` canonical AND the generic freeform tag. `None` fields
/// are not written (use [`clear_ai_content`] to actively remove).
pub fn write_ai_content(tag: &mut Tag, flags: &AiContentFlags) {
    if let Some(v) = flags.is_ai {
        write_bool_field(tag, ATOM_IS_AI, GENERIC_IS_AI, v);
    }
    if let Some(v) = flags.ai_used {
        write_bool_field(tag, ATOM_AI_USED, GENERIC_AI_USED, v);
    }
    if let Some(v) = flags.ai_enhanced {
        write_bool_field(tag, ATOM_AI_ENHANCED, GENERIC_AI_ENHANCED, v);
    }
    if let Some(detail) = flags.ai_enhance_detail.as_deref() {
        write_string_field(
            tag,
            ATOM_AI_ENHANCE_DETAIL,
            GENERIC_AI_ENHANCE_DETAIL,
            detail,
        );
    }
}

/// Remove all AI content atoms (both `MeedyaMeta:*` and generic).
pub fn clear_ai_content(tag: &mut Tag) {
    clear_pair(tag, ATOM_IS_AI, GENERIC_IS_AI);
    clear_pair(tag, ATOM_AI_USED, GENERIC_AI_USED);
    clear_pair(tag, ATOM_AI_ENHANCED, GENERIC_AI_ENHANCED);
    clear_pair(tag, ATOM_AI_ENHANCE_DETAIL, GENERIC_AI_ENHANCE_DETAIL);
}

/// Parse a string as a truthy/falsy boolean per the documented rules.
/// Returns `None` for values that aren't recognisable as either.
pub fn parse_bool_truthy(s: &str) -> Option<bool> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some(false);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "t" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "f" | "off" => Some(false),
        _ => None,
    }
}

// ============================================================
// Field-Level Helpers
// ============================================================

fn read_bool_field(tag: &Tag, meedya_name: &str, generic_name: &str) -> Option<bool> {
    if let Some(s) = read_meedya_atom(tag, meedya_name) {
        if let Some(v) = parse_bool_truthy(s) {
            return Some(v);
        }
    }
    if let Some(s) = read_generic_freeform(tag, generic_name) {
        if let Some(v) = parse_bool_truthy(&s) {
            return Some(v);
        }
    }
    None
}

fn read_string_field(tag: &Tag, meedya_name: &str, generic_name: &str) -> Option<String> {
    if let Some(s) = read_meedya_atom(tag, meedya_name) {
        if !s.is_empty() {
            return Some(s.to_owned());
        }
    }
    if let Some(s) = read_generic_freeform(tag, generic_name) {
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

fn write_bool_field(tag: &mut Tag, meedya_name: &str, generic_name: &str, value: bool) {
    let canonical = if value { "1" } else { "0" };
    write_meedya_atom(tag, meedya_name, canonical);
    write_generic_freeform(tag, generic_name, canonical);
}

fn write_string_field(tag: &mut Tag, meedya_name: &str, generic_name: &str, value: &str) {
    write_meedya_atom(tag, meedya_name, value);
    write_generic_freeform(tag, generic_name, value);
}

fn clear_pair(tag: &mut Tag, meedya_name: &str, generic_name: &str) {
    clear_meedya_atom(tag, meedya_name);
    tag.remove_key(&generic_item_key(generic_name));
}

/// Generic freeform tag uses `ItemKey::Unknown` with just the name (no
/// `MeedyaMeta:` namespace prefix). External tools recognise these as
/// custom TXXX (ID3v2) / freeform Vorbis-comment / freeform MP4 atoms
/// keyed by name.
fn generic_item_key(name: &str) -> ItemKey {
    ItemKey::Unknown(name.to_owned())
}

fn read_generic_freeform(tag: &Tag, name: &str) -> Option<String> {
    tag.get_string(&generic_item_key(name)).map(str::to_owned)
}

fn write_generic_freeform(tag: &mut Tag, name: &str, value: &str) {
    tag.insert_unchecked(TagItem::new(
        generic_item_key(name),
        ItemValue::Text(value.to_owned()),
    ));
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

    // ---- parse_bool_truthy ----

    #[test]
    fn parse_truthy_accepts_1() {
        assert_eq!(parse_bool_truthy("1"), Some(true));
    }

    #[test]
    fn parse_truthy_accepts_true_case_insensitive() {
        assert_eq!(parse_bool_truthy("TRUE"), Some(true));
        assert_eq!(parse_bool_truthy("True"), Some(true));
        assert_eq!(parse_bool_truthy("true"), Some(true));
    }

    #[test]
    fn parse_truthy_accepts_short_forms() {
        assert_eq!(parse_bool_truthy("y"), Some(true));
        assert_eq!(parse_bool_truthy("Y"), Some(true));
        assert_eq!(parse_bool_truthy("yes"), Some(true));
        assert_eq!(parse_bool_truthy("YES"), Some(true));
        assert_eq!(parse_bool_truthy("t"), Some(true));
        assert_eq!(parse_bool_truthy("on"), Some(true));
    }

    #[test]
    fn parse_falsy_accepts_0() {
        assert_eq!(parse_bool_truthy("0"), Some(false));
    }

    #[test]
    fn parse_falsy_accepts_false_case_insensitive() {
        assert_eq!(parse_bool_truthy("FALSE"), Some(false));
        assert_eq!(parse_bool_truthy("False"), Some(false));
    }

    #[test]
    fn parse_falsy_accepts_short_forms() {
        assert_eq!(parse_bool_truthy("n"), Some(false));
        assert_eq!(parse_bool_truthy("no"), Some(false));
        assert_eq!(parse_bool_truthy("f"), Some(false));
        assert_eq!(parse_bool_truthy("off"), Some(false));
    }

    #[test]
    fn parse_empty_is_false() {
        assert_eq!(parse_bool_truthy(""), Some(false));
        assert_eq!(parse_bool_truthy("   "), Some(false));
    }

    #[test]
    fn parse_garbage_is_none() {
        assert_eq!(parse_bool_truthy("maybe"), None);
        assert_eq!(parse_bool_truthy("2"), None);
        assert_eq!(parse_bool_truthy("affirmative"), None);
    }

    #[test]
    fn parse_handles_whitespace() {
        assert_eq!(parse_bool_truthy("  yes  "), Some(true));
        assert_eq!(parse_bool_truthy("\ttrue\n"), Some(true));
    }

    // ---- read/write round-trips ----

    #[test]
    fn round_trip_is_ai_true() {
        let mut tag = fresh();
        write_ai_content(
            &mut tag,
            &AiContentFlags {
                is_ai: Some(true),
                ..Default::default()
            },
        );
        let read = read_ai_content(&tag);
        assert_eq!(read.is_ai, Some(true));
        assert_eq!(read.ai_used, None);
    }

    #[test]
    fn round_trip_all_flags() {
        let mut tag = fresh();
        let flags = AiContentFlags {
            is_ai: Some(false),
            ai_used: Some(true),
            ai_enhanced: Some(true),
            ai_enhance_detail: Some("Topaz Audio AI v2.1, music-restoration preset".to_owned()),
        };
        write_ai_content(&mut tag, &flags);
        assert_eq!(read_ai_content(&tag), flags);
    }

    #[test]
    fn writes_canonical_1_or_0() {
        let mut tag = fresh();
        write_ai_content(
            &mut tag,
            &AiContentFlags {
                is_ai: Some(true),
                ai_used: Some(false),
                ..Default::default()
            },
        );
        // MeedyaMeta side
        assert_eq!(read_meedya_atom(&tag, ATOM_IS_AI), Some("1"));
        assert_eq!(read_meedya_atom(&tag, ATOM_AI_USED), Some("0"));
        // Generic freeform side
        assert_eq!(tag.get_string(&generic_item_key(GENERIC_IS_AI)), Some("1"));
        assert_eq!(
            tag.get_string(&generic_item_key(GENERIC_AI_USED)),
            Some("0")
        );
    }

    #[test]
    fn read_falls_back_to_generic_when_meedyameta_absent() {
        let mut tag = fresh();
        // Write only to the generic freeform slot.
        tag.insert_unchecked(TagItem::new(
            generic_item_key(GENERIC_IS_AI),
            ItemValue::Text("yes".to_owned()),
        ));
        let read = read_ai_content(&tag);
        assert_eq!(read.is_ai, Some(true));
    }

    #[test]
    fn read_prefers_meedyameta_over_generic_on_disagreement() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_IS_AI, "1");
        tag.insert_unchecked(TagItem::new(
            generic_item_key(GENERIC_IS_AI),
            ItemValue::Text("0".to_owned()),
        ));
        assert_eq!(read_ai_content(&tag).is_ai, Some(true));
    }

    #[test]
    fn read_garbage_meedyameta_falls_back_to_generic() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_IS_AI, "maybe");
        tag.insert_unchecked(TagItem::new(
            generic_item_key(GENERIC_IS_AI),
            ItemValue::Text("YES".to_owned()),
        ));
        assert_eq!(read_ai_content(&tag).is_ai, Some(true));
    }

    #[test]
    fn read_returns_none_when_both_garbage() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_IS_AI, "maybe");
        tag.insert_unchecked(TagItem::new(
            generic_item_key(GENERIC_IS_AI),
            ItemValue::Text("perhaps".to_owned()),
        ));
        assert_eq!(read_ai_content(&tag).is_ai, None);
    }

    #[test]
    fn clear_removes_both_sides() {
        let mut tag = fresh();
        write_ai_content(
            &mut tag,
            &AiContentFlags {
                is_ai: Some(true),
                ai_enhanced: Some(true),
                ai_enhance_detail: Some("Topaz".into()),
                ..Default::default()
            },
        );
        clear_ai_content(&mut tag);
        let read = read_ai_content(&tag);
        assert!(read.is_empty());
    }

    #[test]
    fn write_skips_none_fields() {
        let mut tag = fresh();
        write_ai_content(
            &mut tag,
            &AiContentFlags {
                is_ai: Some(true),
                ai_used: None,
                ai_enhanced: None,
                ai_enhance_detail: None,
            },
        );
        let read = read_ai_content(&tag);
        assert_eq!(read.is_ai, Some(true));
        assert_eq!(read.ai_used, None);
        assert_eq!(read.ai_enhanced, None);
        assert_eq!(read.ai_enhance_detail, None);
    }

    #[test]
    fn detail_preserves_unicode() {
        let mut tag = fresh();
        let detail = "🤖 Sübstantielles Modell — Mistral Mídium v3.0 ✨";
        write_ai_content(
            &mut tag,
            &AiContentFlags {
                ai_enhance_detail: Some(detail.to_owned()),
                ..Default::default()
            },
        );
        assert_eq!(
            read_ai_content(&tag).ai_enhance_detail.as_deref(),
            Some(detail)
        );
    }

    #[test]
    fn flags_is_empty_works() {
        assert!(AiContentFlags::default().is_empty());
        assert!(!AiContentFlags {
            is_ai: Some(false),
            ..Default::default()
        }
        .is_empty());
    }
}
