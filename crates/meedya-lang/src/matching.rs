// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::matching — MATCH-010 to MATCH-040: how well a candidate
// tag (a track, a translation) matches one preference. Deliberately its
// own module (policy section 9 keeps "preference matching" apart from
// ordering and selection) — `select.rs` calls into this for every
// candidate it considers, but this module has no idea what a preference
// list, a track, or a subtitle mode is.

use crate::tag::LanguageTag;

/// How well a candidate tag matches a preference, best first (so that
/// deriving [`Ord`] makes the best match sort first, which is exactly
/// what MATCH-010 to MATCH-040 need). `und`, `mul`, `mis` and `zxx`,
/// private-use-only tags, and grandfathered tags with no replacement only
/// ever match themselves exactly — see [`match_tags`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchLevel {
    /// The canonical tags are identical (MATCH-010).
    Exact,
    /// The candidate is a shorter form of the preference — the preference
    /// with subtags removed from the end (MATCH-020).
    General,
    /// The preference is a shorter form of the candidate (MATCH-030).
    Specific,
    /// Same primary language, no script conflict, but neither a shorter
    /// nor a longer form of the other (MATCH-040).
    Related,
    /// Different primary languages, a script conflict, or either tag is
    /// not usable for matching at all (malformed, or one of `und`/`mul`/
    /// `mis`/`zxx`/private-use/no-replacement-grandfathered matched
    /// against something other than itself).
    None,
}

/// The result of comparing one preference against one candidate:
/// [`MatchLevel::General`] and [`MatchLevel::Specific`] carry how many
/// subtags were removed or added; every other level carries 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagMatch {
    pub level: MatchLevel,
    pub distance: u8,
}

fn hyphen_subtags(tag: &LanguageTag) -> Vec<String> {
    tag.tag
        .to_ascii_lowercase()
        .split('-')
        .map(String::from)
        .collect()
}

/// Whether a tag's primary language is one of the codes that "only ever
/// match themselves exactly" (MATCH-040's closing paragraph): `und` (not
/// known), `mul` (several languages), `mis` (a language with no code —
/// two "uncoded" tracks need not be the same language), `zxx` (no
/// language at all). Private-use-only tags and grandfathered tags with no
/// replacement are handled separately, by [`TagKind`](crate::tag::TagKind)
/// rather than by primary language, since neither has one at all.
fn is_self_match_only_special(tag: &LanguageTag) -> bool {
    matches!(tag.language.as_deref(), Some("und" | "mul" | "mis" | "zxx"))
}

/// Compares `preference` against `candidate` per MATCH-010 to MATCH-040.
/// Both must already be canonical — this function never canonicalises
/// its arguments, so a caller comparing raw strings needs to call
/// [`crate::canonicalise`] first (once per tag, not once per comparison).
///
/// Never changes either tag (MATCH-040's closing paragraph): the two
/// [`LanguageTag`]s are only read, never modified or replaced with a
/// guessed-at fuller form. In particular this function does **not** use
/// likely-subtag inference — `zh-TW` and `zh-Hans` come back "related",
/// not "none" and not "general/specific", because nothing in either tag
/// says `zh-TW` is written in Simplified script. That is a known,
/// deliberate limitation of this policy version (see its changelog).
pub fn match_tags(preference: &LanguageTag, candidate: &LanguageTag) -> TagMatch {
    // Exact match is checked before anything else, including the
    // "must both be ordinary" gate below — this is what lets two
    // identical private-use tags (`x-foo` vs `x-foo`) match exactly while
    // two different ones (`x-foo` vs `x-bar`) match not at all.
    if preference.tag.eq_ignore_ascii_case(&candidate.tag) {
        return TagMatch {
            level: MatchLevel::Exact,
            distance: 0,
        };
    }

    if !preference.is_ordinary() || !candidate.is_ordinary() {
        return TagMatch {
            level: MatchLevel::None,
            distance: 0,
        };
    }
    if is_self_match_only_special(preference) || is_self_match_only_special(candidate) {
        return TagMatch {
            level: MatchLevel::None,
            distance: 0,
        };
    }
    if preference.language != candidate.language {
        return TagMatch {
            level: MatchLevel::None,
            distance: 0,
        };
    }
    // A script conflict rules out a match entirely, even at the "related"
    // level — a reader of one script may not be able to read the other
    // (MATCH-040).
    if let (Some(p_script), Some(c_script)) = (&preference.script, &candidate.script) {
        if p_script != c_script {
            return TagMatch {
                level: MatchLevel::None,
                distance: 0,
            };
        }
    }

    let p_subtags = hyphen_subtags(preference);
    let c_subtags = hyphen_subtags(candidate);

    if c_subtags.len() < p_subtags.len() && p_subtags[..c_subtags.len()] == c_subtags[..] {
        return TagMatch {
            level: MatchLevel::General,
            distance: (p_subtags.len() - c_subtags.len()) as u8,
        };
    }
    if p_subtags.len() < c_subtags.len() && c_subtags[..p_subtags.len()] == p_subtags[..] {
        return TagMatch {
            level: MatchLevel::Specific,
            distance: (c_subtags.len() - p_subtags.len()) as u8,
        };
    }
    TagMatch {
        level: MatchLevel::Related,
        distance: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::canonicalise;

    fn m(pref: &str, cand: &str) -> TagMatch {
        match_tags(&canonicalise(pref), &canonicalise(cand))
    }

    #[test]
    fn exact_match() {
        assert_eq!(m("en-GB", "en-GB").level, MatchLevel::Exact);
    }

    #[test]
    fn general_and_specific_are_mirror_images() {
        assert_eq!(
            m("en-GB", "en"),
            TagMatch {
                level: MatchLevel::General,
                distance: 1
            }
        );
        assert_eq!(
            m("en", "en-GB"),
            TagMatch {
                level: MatchLevel::Specific,
                distance: 1
            }
        );
    }

    #[test]
    fn related_when_neither_is_a_form_of_the_other() {
        assert_eq!(m("en-GB", "en-US").level, MatchLevel::Related);
    }

    #[test]
    fn script_conflict_is_never_a_match() {
        assert_eq!(m("zh-Hant", "zh-Hans").level, MatchLevel::None);
        assert_eq!(m("sr-Latn", "sr-Cyrl-RS").level, MatchLevel::None);
    }

    #[test]
    fn a_region_only_tag_is_related_to_a_script_form_never_inferred() {
        // Known limitation (MATCH-040 / changelog): no likely-subtag
        // inference, so zh-TW is "related" to zh-Hant, not general/none.
        assert_eq!(m("zh-Hant", "zh-TW").level, MatchLevel::Related);
    }

    #[test]
    fn special_codes_only_match_themselves() {
        assert_eq!(m("und", "und").level, MatchLevel::Exact);
        assert_eq!(m("und", "en").level, MatchLevel::None);
        assert_eq!(m("en", "und").level, MatchLevel::None);
        assert_eq!(m("mul", "mul").level, MatchLevel::Exact);
        assert_eq!(m("mis", "mis-Latn").level, MatchLevel::None);
    }

    #[test]
    fn private_use_tags_only_match_themselves() {
        assert_eq!(m("x-foo", "x-foo").level, MatchLevel::Exact);
        assert_eq!(m("x-foo", "x-bar").level, MatchLevel::None);
    }

    #[test]
    fn grandfathered_tags_with_no_replacement_only_match_themselves() {
        assert_eq!(m("i-default", "I-DEFAULT").level, MatchLevel::Exact);
        assert_eq!(m("i-default", "i-mingo").level, MatchLevel::None);
    }

    #[test]
    fn best_match_sorts_first_via_derived_ord() {
        let mut levels = [
            MatchLevel::Related,
            MatchLevel::Exact,
            MatchLevel::None,
            MatchLevel::General,
        ];
        levels.sort();
        assert_eq!(
            levels,
            [
                MatchLevel::Exact,
                MatchLevel::General,
                MatchLevel::Related,
                MatchLevel::None
            ]
        );
    }
}
