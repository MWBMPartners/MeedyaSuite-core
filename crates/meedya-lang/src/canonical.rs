// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::canonical — Part A of the policy: the order of what is
// *stored* (LANG-010 to LANG-027). Depends only on the language tags
// themselves, so it gives the same answer on every machine, in every
// interface language, for every user — unlike `presentation`, which is
// for what a *person sees* and depends on their preferences and interface
// language. The policy requires these to stay two separate comparators
// (docs/standards/media-language-bcp47-policy.md, "How to read this
// document"); this module and `presentation.rs` share only tag-shape
// primitives (bucket/group/specificity), never a comparison function.

use std::cmp::{Ordering, Reverse};
use std::collections::HashSet;

use crate::tag::{LanguageTag, TagKind};

/// Where a tag's language group sits, before ordinary languages are
/// compared by code (LANG-025): ordinary languages are bucket 0 (compared
/// by primary language subtag); the six fixed special buckets follow in
/// this exact order; malformed values are always last.
const BUCKET_MUL: u8 = 1;
const BUCKET_MIS: u8 = 2;
const BUCKET_LOCAL_USE: u8 = 3;
const BUCKET_UND: u8 = 4;
const BUCKET_ZXX: u8 = 5;
const BUCKET_GRANDFATHERED: u8 = 6;
const BUCKET_PRIVATE_USE: u8 = 7;
/// Exposed to other modules (`presentation.rs` needs to recognise "the
/// malformed group" when building group order) so nobody has to repeat
/// the literal `8` and risk it drifting from this list.
pub(crate) const BUCKET_MALFORMED: u8 = 8;

/// A language group's place in the fixed ordering of LANG-025: the bucket
/// number, then (for buckets that need one) a secondary string to compare
/// within the bucket — the primary language code for ordinary/special
/// codes, the whole tag text for grandfathered/private-use (each such tag
/// is its own group), and nothing for malformed (every malformed value is
/// one single group, per LANG-026 — order among them is decided by
/// stability, not by their text).
pub(crate) type GroupKey = (u8, String);

/// The tuple LANG-021 to LANG-023 sort by, within one language group:
/// `(carries_variants_or_extensions_or_privateuse, specificity_level,
/// script, region_kind, region, everything_else)`. The first element
/// dominates on purpose — LANG-021 rule 5 says a tag with variants,
/// extensions or private use is ordered "among themselves" by the other
/// rules, which means as a group *after* every plainer tag, not
/// interleaved by how specific it happens to be.
pub(crate) type SpecificityKey = (u8, u8, String, u8, String, String);

fn bucket(tag: &LanguageTag) -> u8 {
    match tag.kind {
        TagKind::Malformed => BUCKET_MALFORMED,
        TagKind::PrivateUse => BUCKET_PRIVATE_USE,
        TagKind::Grandfathered => BUCKET_GRANDFATHERED,
        TagKind::Ordinary => {
            let language = tag.language.as_deref().unwrap_or("");
            match language {
                "mul" => BUCKET_MUL,
                "mis" => BUCKET_MIS,
                "und" => BUCKET_UND,
                "zxx" => BUCKET_ZXX,
                _ if language.len() == 3 && ("qaa"..="qtz").contains(&language) => BUCKET_LOCAL_USE,
                _ => 0,
            }
        }
    }
}

/// The language group a tag belongs to (LANG-025). Two tags with the same
/// canonical primary language are always the same group regardless of
/// script/region/variants — that grouping is exactly what lets LANG-010
/// promote "the whole language group" of an original item.
pub(crate) fn group_key(tag: &LanguageTag) -> GroupKey {
    let b = bucket(tag);
    match b {
        BUCKET_GRANDFATHERED | BUCKET_PRIVATE_USE => (b, tag.tag.to_ascii_lowercase()),
        BUCKET_MALFORMED => (b, String::new()),
        _ => (b, tag.language.clone().unwrap_or_default()),
    }
}

/// LANG-021 to LANG-023's specificity ordering for one tag, within its
/// language group. Safe to call on any tag, including grandfathered,
/// private-use and malformed ones — those only ever get compared against
/// another tag sharing the *same* [`group_key`] (which for those kinds
/// means the identical tag text, or "any other malformed value"), so this
/// function's exact answer for them only has to be internally consistent,
/// not meaningful on its own.
pub(crate) fn specificity_key(tag: &LanguageTag) -> SpecificityKey {
    // LANG-021 rule 5 (as revised): an unregistered extlang that survived
    // canonicalisation unfolded (`zh-abc`) counts as an "additional"
    // subtag too, exactly like a variant, extension or private-use part —
    // it did not exist in the registry's eyes, so it cannot be treated as
    // an ordinary, well-understood part of the tag's specificity.
    let has_extra = u8::from(
        tag.extlang.is_some()
            || !tag.variants.is_empty()
            || !tag.extensions.is_empty()
            || !tag.private_use.is_empty(),
    );
    let base: u8 = 1 + u8::from(tag.script.is_some()) + 2 * u8::from(tag.region.is_some());
    let region_kind: u8 = match &tag.region {
        None => 0,
        Some(r) if r.chars().all(|c| c.is_ascii_alphabetic()) => 1, // country/territory
        Some(_) => 2,                                               // UN M.49 area code
    };

    // Everything else that can distinguish two tags at the same
    // specificity level: the leftover extlang (if any), variants, each
    // extension (singleton + its own subtags joined), then private use —
    // flattened into one list and joined, matching how these tags are
    // written out (LANG-001 step 3's ordering), so the comparison reads
    // the same subtags a person would.
    let mut rest_parts: Vec<String> = Vec::new();
    if let Some(extlang) = &tag.extlang {
        rest_parts.push(extlang.clone());
    }
    rest_parts.extend(tag.variants.iter().cloned());
    for ext in &tag.extensions {
        let mut joined = ext.singleton.to_string();
        for s in &ext.subtags {
            joined.push('-');
            joined.push_str(s);
        }
        rest_parts.push(joined);
    }
    if !tag.private_use.is_empty() {
        rest_parts.push("x".to_string());
        rest_parts.extend(tag.private_use.iter().cloned());
    }
    let rest = rest_parts.join("-");

    (
        has_extra,
        base,
        tag.script.clone().unwrap_or_default(),
        region_kind,
        tag.region.clone().unwrap_or_default(),
        rest,
    )
}

/// The set of language groups LANG-010 promotes to the front: every
/// group containing at least one item marked original, excluding
/// malformed values (LANG-026 always puts those last regardless of any
/// flag). Computed once before sorting, then consulted for every
/// comparison — recomputing it per-comparison would be needlessly
/// quadratic and would also be wrong the moment a caller wants the
/// promotion computed per track type (see `tracks.rs`), since this
/// function only ever sees the items it is given.
pub(crate) fn promoted_groups<'a>(
    items: impl Iterator<Item = (&'a LanguageTag, bool)>,
) -> HashSet<GroupKey> {
    items
        .filter(|(_, is_original)| *is_original)
        .map(|(tag, _)| group_key(tag))
        .filter(|g| g.0 != BUCKET_MALFORMED)
        .collect()
}

/// The heart of Part A's ordering (and, via `tracks::sort_tracks`, of
/// TRACK-050's role-aware variant of it): compares two items by —
///
/// 1. whether their language group was promoted by LANG-010 (promoted
///    first);
/// 2. the group itself, in LANG-025's fixed order (bucket, then primary
///    language code / whole tag text as appropriate);
/// 3. within a promoted group, the item(s) actually marked original
///    (LANG-010: "the item marked original leads");
/// 4. the role rank the caller supplies — always 0 for plain Part A
///    ordering; TRACK-050's per-role-type rank for tracks;
/// 5. specificity (LANG-021 to LANG-023).
///
/// Returns [`Ordering::Equal`] for two items this policy does not
/// distinguish, leaving LANG-027's stability requirement to whichever
/// *stable* sort the caller uses (every sort in this crate is
/// [`slice::sort_by`], which Rust guarantees is stable) — there is no
/// separate "original index" field here because a stable sort already
/// provides it for free.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compare_ranked(
    a_tag: &LanguageTag,
    a_original: bool,
    a_role_rank: u8,
    b_tag: &LanguageTag,
    b_original: bool,
    b_role_rank: u8,
    promoted: &HashSet<GroupKey>,
) -> Ordering {
    let a_group = group_key(a_tag);
    let b_group = group_key(b_tag);
    let a_promoted = promoted.contains(&a_group);
    let b_promoted = promoted.contains(&b_group);

    Reverse(a_promoted)
        .cmp(&Reverse(b_promoted))
        .then_with(|| a_group.cmp(&b_group))
        .then_with(|| {
            let a_leads = a_promoted && a_original;
            let b_leads = b_promoted && b_original;
            Reverse(a_leads).cmp(&Reverse(b_leads))
        })
        .then_with(|| a_role_rank.cmp(&b_role_rank))
        .then_with(|| specificity_key(a_tag).cmp(&specificity_key(b_tag)))
}

/// Anything that carries a language tag and can say whether it is the
/// original. Implement this to sort a list of your own items into Part
/// A's stored order with [`sort_canonical`] — the trait exists so this
/// crate never has to know your item's concrete type.
pub trait LanguageItem {
    /// The item's canonical language tag.
    fn language(&self) -> &LanguageTag;
    /// Whether structured data marks this item as the original-language
    /// one (LANG-010). Defaults to `false` — most items in a list are not
    /// the original.
    fn is_original(&self) -> bool {
        false
    }
}

/// Sorts `items` into the canonical stored order defined by LANG-010 to
/// LANG-027: the original language group(s) first, then every other
/// group ordered by primary language code, each group internally ordered
/// by specificity, ties kept in their original relative order.
///
/// This is Part A's comparator, and only Part A's — it does not know
/// about roles (that is `tracks::sort_tracks`, for TRACK-050) or about a
/// user's preferences and interface language (that is
/// `presentation::sort_for_presentation`, for Part B). Using this
/// function to build a menu, or the presentation comparator to decide
/// what gets written into a file, is exactly the mistake the policy's
/// "Two orders, on purpose" section warns against.
pub fn sort_canonical<T: LanguageItem>(items: &mut [T]) {
    let promoted = promoted_groups(items.iter().map(|it| (it.language(), it.is_original())));
    items.sort_by(|a, b| {
        compare_ranked(
            a.language(),
            a.is_original(),
            0,
            b.language(),
            b.is_original(),
            0,
            &promoted,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::canonicalise;

    struct Item {
        id: &'static str,
        tag: LanguageTag,
        original: bool,
    }

    impl LanguageItem for Item {
        fn language(&self) -> &LanguageTag {
            &self.tag
        }
        fn is_original(&self) -> bool {
            self.original
        }
    }

    fn item(id: &'static str, tag: &str, original: bool) -> Item {
        Item {
            id,
            tag: canonicalise(tag),
            original,
        }
    }

    #[test]
    fn basic_primary_language_order() {
        let mut items = vec![
            item("nl", "nl", false),
            item("de", "de", false),
            item("fr", "fr", false),
            item("en", "en", false),
        ];
        sort_canonical(&mut items);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["de", "en", "fr", "nl"]);
    }

    #[test]
    fn original_group_is_promoted_and_leads() {
        let mut items = vec![
            item("fr", "fr", false),
            item("de", "de", false),
            item("ja", "ja", true),
            item("en", "en", false),
            item("ja-Latn", "ja-Latn", false),
        ];
        sort_canonical(&mut items);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["ja", "ja-Latn", "de", "en", "fr"]);
    }

    #[test]
    fn malformed_values_sort_last_in_the_order_found() {
        let mut items = vec![
            item("a", "English", false),
            item("b", "fr", false),
            item("c", "en_US", false),
            item("d", "de", false),
        ];
        sort_canonical(&mut items);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["d", "b", "a", "c"]);
    }

    #[test]
    fn equal_tags_keep_their_relative_order() {
        let mut items = vec![
            item("a1", "en", false),
            item("d", "de", false),
            item("a2", "EN", false),
        ];
        sort_canonical(&mut items);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["d", "a1", "a2"]);
    }
}
