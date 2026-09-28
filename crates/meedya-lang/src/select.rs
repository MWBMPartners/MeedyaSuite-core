// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::select — AUTO-020 (choosing audio) and AUTO-030 (choosing
// subtitles): which single track plays automatically, as opposed to
// `presentation.rs`'s job of ordering a menu a person picks from
// themselves (AUTO-010: "choosing which track plays is a different job
// from ordering the menu").

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::canonical::{self, GroupKey};
use crate::matching::{match_tags, MatchLevel};
use crate::presentation::Accessibility;
use crate::roles::{self, Role, RoleItem, TrackType, UnknownWordError};
use crate::tag::LanguageTag;

/// A track that can be chosen automatically: it has a language, an
/// original flag and roles (via [`RoleItem`], which builds on
/// [`LanguageItem`](crate::LanguageItem)), an identifier stable enough to
/// break ties by (AUTO-010: "the track identifier decides, the number or ID
/// the file gives the track — never its position in a list"), and a
/// default flag.
///
/// `language()`, `is_original()` and `roles()` come from the shared
/// traits, not from this one, so a track type that also implements
/// [`TrackItem`](crate::TrackItem) and
/// [`PresentationItem`](crate::PresentationItem) has exactly one of each.
/// (Before policy revision 4 this trait declared its own copies, and such a
/// type could not call `track.roles()` without naming a trait.)
///
/// `Id` is required to render as text (`AsRef<str>`) rather than simply
/// to be `Ord`: AUTO-010 defines a specific comparison — identifiers made
/// of ASCII digits only come first, compared as numbers, then (when
/// numerically equal) as plain text; everything else follows in plain
/// byte-string order — and this crate applies that rule itself
/// ([`compare_identifiers`]) rather than trusting whatever `Ord`
/// implementation the caller's `Id` type happens to have. That
/// difference matters in practice: `"01"` and `"1"` are equal as
/// integers but different strings, and a naive numeric-aware comparison
/// (as several dynamically-typed languages' own comparison operators do)
/// would treat them as interchangeable, silently discarding one.
pub trait SelectableTrack: RoleItem {
    type Id: Clone + Eq + std::hash::Hash + AsRef<str>;

    fn id(&self) -> Self::Id;
    /// The container's "default" flag. Defaults to `false`.
    fn is_default(&self) -> bool {
        false
    }
}

/// AUTO-010: two tracks in the same selection call had the same
/// identifier. Selection refuses to guess which one was meant, rather
/// than silently overwriting one candidate's canonical position with the
/// other's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DuplicateIdentifierError {
    pub identifier: String,
}

impl fmt::Display for DuplicateIdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "duplicate track identifier {:?} — AUTO-010 requires identifiers to be unique",
            self.identifier
        )
    }
}

impl std::error::Error for DuplicateIdentifierError {}

/// True for an identifier made only of ASCII digits (and at least one of
/// them). Such identifiers come first in AUTO-010's order.
fn is_digits_only(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

/// Compares two digits-only identifiers as numbers, of any length,
/// without converting them to a number type: leading zeros are removed,
/// then the longer is the bigger number, and two of the same length
/// compare as text (for digit strings with no leading zero, text order and
/// number order are the same). Returns [`Ordering::Equal`] for two ways of
/// writing one number (`"01"` and `"1"`) — the caller breaks that tie.
fn compare_as_numbers(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Compares two identifiers per AUTO-010. Every tie-break in this module
/// that would otherwise fall back to a track's position in a list uses
/// this instead:
///
/// 1. digits-only identifiers before every other identifier;
/// 2. two digits-only identifiers as numbers, of any length (`9` before
///    `10`; a 40-digit identifier is still a number);
/// 3. two that are the same number written differently, as plain text
///    (`01` before `1`);
/// 4. two other identifiers as plain text (byte order).
///
/// Before policy revision 4 step 2 converted the digits to a 128-bit
/// number and, for 40 or more digits, where that overflows, fell back to
/// treating the identifier as ordinary text — which moved it after `0abc`,
/// against AUTO-010's order. No number type is used now, so there is no
/// length at which the rule changes.
pub fn compare_identifiers(a: &str, b: &str) -> Ordering {
    match (is_digits_only(a), is_digits_only(b)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => compare_as_numbers(a, b).then_with(|| a.cmp(b)),
        (false, false) => a.cmp(b),
    }
}

fn check_unique_ids<T: SelectableTrack>(tracks: &[&T]) -> Result<(), DuplicateIdentifierError> {
    let mut seen: HashSet<String> = HashSet::new();
    for t in tracks {
        let id_text = t.id().as_ref().to_string();
        if !seen.insert(id_text.clone()) {
            return Err(DuplicateIdentifierError {
                identifier: id_text,
            });
        }
    }
    Ok(())
}

/// The user's subtitle preference (AUTO-030). Defaults to
/// [`SubtitleMode::Automatic`], the policy's usual default.
///
/// Written and read with the policy's own words — `automatic`, `always`,
/// `forced_only`, `off` — by [`SubtitleMode::as_str`], [`FromStr`] and
/// serde alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    /// If the chosen audio's language matches a preference (or there are
    /// no preferences at all — malformed ones do not count), act as
    /// [`SubtitleMode::ForcedOnly`]; otherwise act as
    /// [`SubtitleMode::Always`]. The usual default.
    #[default]
    Automatic,
    /// The best-matching subtitle track placed as full or SDH, by
    /// preference order; else the default one of those; else none.
    Always,
    /// Only the forced track matching the chosen audio's language, if
    /// any.
    ForcedOnly,
    /// Never choose a subtitle track.
    Off,
}

impl SubtitleMode {
    /// Every mode, in declaration order.
    pub const ALL: [SubtitleMode; 4] = [
        SubtitleMode::Automatic,
        SubtitleMode::Always,
        SubtitleMode::ForcedOnly,
        SubtitleMode::Off,
    ];

    /// The policy's word for this mode: `automatic`, `always`,
    /// `forced_only`, `off`.
    pub fn as_str(self) -> &'static str {
        match self {
            SubtitleMode::Automatic => "automatic",
            SubtitleMode::Always => "always",
            SubtitleMode::ForcedOnly => "forced_only",
            SubtitleMode::Off => "off",
        }
    }
}

impl fmt::Display for SubtitleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SubtitleMode {
    type Err = UnknownWordError;

    /// Reads the policy's word for a mode, exactly as
    /// [`SubtitleMode::as_str`] writes it. An unknown word is an error,
    /// never a guess at a mode.
    fn from_str(word: &str) -> Result<Self, Self::Err> {
        SubtitleMode::ALL
            .into_iter()
            .find(|m| m.as_str() == word)
            .ok_or_else(|| UnknownWordError {
                what: "subtitle mode",
                word: word.to_string(),
            })
    }
}

/// AUTO-010 (as clarified in policy revision 4): a malformed preference is
/// ignored in selection, as it is in menus — it matches nothing — and a
/// user whose preferences are ALL malformed counts as having none. Only
/// that second part changes an answer (see [`select_subtitle`]'s automatic
/// mode), but filtering once here keeps every step below honest about
/// which preferences exist.
fn usable_preferences(preferences: &[LanguageTag]) -> Vec<&LanguageTag> {
    preferences.iter().filter(|p| !p.is_malformed()).collect()
}

/// AUTO-020 point 1's "special" test, as revised: a track counts as
/// commentary/other when that is its PLACEMENT role — TRACK-050's rule,
/// the latest of its roles — not merely when commentary or other happens
/// to be somewhere in its role list. Commentary and other are the two
/// highest-ranked audio roles (TRACK-050 orders audio main < alternate <
/// audio description < commentary < other), so "placed at or above
/// commentary" is exactly "placed as commentary or other": a track
/// carrying both audio description and commentary is placed by
/// commentary (the later of the two) and is correctly treated as
/// special, while one carrying only audio description is not.
fn is_special_audio_role(roles: &[Role]) -> bool {
    roles::role_rank(TrackType::Audio, roles)
        >= roles::individual_rank(TrackType::Audio, Role::Commentary)
}

/// Each track's position in stored order (TRACK-050, TRACK-060) among
/// `tracks`, which must be EVERY track of the type — including tracks that
/// cannot be chosen (AUTO-020, as clarified in policy revision 4). An
/// original track that is never chosen (commentary, say) still brings its
/// language group forward, exactly as it does in stored order. Before
/// revision 4 the audio positions were worked out from the choosable
/// tracks only, which lost that promotion.
///
/// Worked out from a copy of `tracks` **sorted by id first** (using
/// [`compare_identifiers`], not the caller's own `Ord`) — this is what
/// makes the result independent of the order the caller's slice happens
/// to be in (AUTO-010): two tracks stored order leaves level (equal tags,
/// or two malformed values) get positions in identifier order, whichever
/// order they arrived in.
fn canonical_positions<T: SelectableTrack>(
    tracks: &[&T],
    track_type: TrackType,
) -> HashMap<T::Id, usize> {
    let mut by_id: Vec<&&T> = tracks.iter().collect();
    by_id.sort_by(|a, b| compare_identifiers(a.id().as_ref(), b.id().as_ref()));

    let promoted: HashSet<GroupKey> =
        canonical::promoted_groups(by_id.iter().map(|t| (t.language(), t.is_original())));

    let mut order: Vec<usize> = (0..by_id.len()).collect();
    order.sort_by(|&i, &j| {
        let a = by_id[i];
        let b = by_id[j];
        canonical::compare_ranked(
            a.language(),
            a.is_original(),
            roles::role_rank(track_type, a.roles()),
            b.language(),
            b.is_original(),
            roles::role_rank(track_type, b.roles()),
            &promoted,
        )
    });

    order
        .into_iter()
        .enumerate()
        .map(|(position, idx)| (by_id[idx].id(), position))
        .collect()
}

/// The role rank AUTO-020 uses as the first tie-break among candidates,
/// built from the track's PLACING role (TRACK-050: the latest of its
/// roles — exactly [`roles::role_rank`]'s own definition). Lower is
/// better.
///
/// * Ordinarily that rank unchanged: main (0), alternate (1), audio
///   description (2), commentary (3), other (4).
/// * When the user has asked for audio description: audio description,
///   then main, then alternate — and then commentary before other, as
///   ever. Commentary and other only reach this comparison when every
///   audio track is one of them (AUTO-020 point 1), and the policy settles
///   (revision 4) that commentary still ranks before other then, whether
///   or not audio description was asked for.
fn audio_role_preference(roles: &[Role], accessibility: &Accessibility) -> u8 {
    let placing = roles::role_rank(TrackType::Audio, roles);
    if !accessibility.audio_description {
        return placing;
    }
    let audio_description = roles::individual_rank(TrackType::Audio, Role::AudioDescription);
    let alternate = roles::individual_rank(TrackType::Audio, Role::Alternate);
    match placing {
        p if p == audio_description => 0,
        0 => 1, // main programme
        p if p == alternate => 2,
        // Commentary (3) and other (4) keep TRACK-050's order and stay
        // behind the three above.
        other => other,
    }
}

/// Chooses which audio track plays automatically (AUTO-020).
///
/// `Ok(None)` only when `tracks` is empty. `Err` when two tracks share an
/// identifier (AUTO-010) — this function never guesses which one was
/// meant. A malformed preference is ignored (it matches nothing).
///
/// Never influenced by list order: reversing `tracks` and calling this
/// again gives the same answer back, because every tie-break bottoms out
/// in either the track's own id (compared per [`compare_identifiers`]) or
/// a canonical position computed from an id-sorted copy (see
/// [`canonical_positions`]) — never the position `tracks` happened to be
/// given in.
pub fn select_audio<T: SelectableTrack>(
    tracks: &[T],
    preferences: &[LanguageTag],
    accessibility: &Accessibility,
) -> Result<Option<T::Id>, DuplicateIdentifierError> {
    if tracks.is_empty() {
        return Ok(None);
    }
    let refs: Vec<&T> = tracks.iter().collect();
    check_unique_ids(&refs)?;
    let preferences = usable_preferences(preferences);

    // AUTO-020 point 1: tracks placed as commentary/other are never
    // chosen automatically unless every track is one.
    let eligible: Vec<&T> = {
        let non_special: Vec<&T> = refs
            .iter()
            .filter(|t| !is_special_audio_role(t.roles()))
            .copied()
            .collect();
        if non_special.is_empty() {
            refs.clone()
        } else {
            non_special
        }
    };

    // Canonical order counts ALL audio tracks, not only the eligible ones
    // (AUTO-020, as clarified in policy revision 4).
    let positions = canonical_positions(&refs, TrackType::Audio);

    // AUTO-020 point 2: for each preference in order, the best match.
    for preference in preferences {
        let mut candidates: Vec<(&&T, MatchLevel, usize)> = eligible
            .iter()
            .filter_map(|t| {
                let m = match_tags(preference, t.language());
                (m.level <= MatchLevel::Related).then_some((t, m.level, m.distance))
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by(|(t1, l1, d1), (t2, l2, d2)| {
            audio_role_preference(t1.roles(), accessibility)
                .cmp(&audio_role_preference(t2.roles(), accessibility))
                .then_with(|| l1.cmp(l2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| t2.is_default().cmp(&t1.is_default()))
                .then_with(|| t2.is_original().cmp(&t1.is_original()))
                .then_with(|| positions[&t1.id()].cmp(&positions[&t2.id()]))
                .then_with(|| compare_identifiers(t1.id().as_ref(), t2.id().as_ref()))
        });
        return Ok(Some(candidates[0].0.id()));
    }

    // AUTO-020 points 3-4: no preference matched — the original, else the
    // default, with the same role/default/canonical-order/id tie-breaks.
    for flag_is_set in [
        (|t: &&T| t.is_original()) as fn(&&T) -> bool,
        (|t: &&T| t.is_default()) as fn(&&T) -> bool,
    ] {
        let mut flagged: Vec<&&T> = eligible.iter().filter(|t| flag_is_set(t)).collect();
        if flagged.is_empty() {
            continue;
        }
        flagged.sort_by(|t1, t2| {
            audio_role_preference(t1.roles(), accessibility)
                .cmp(&audio_role_preference(t2.roles(), accessibility))
                .then_with(|| t2.is_default().cmp(&t1.is_default()))
                .then_with(|| positions[&t1.id()].cmp(&positions[&t2.id()]))
                .then_with(|| compare_identifiers(t1.id().as_ref(), t2.id().as_ref()))
        });
        return Ok(Some(flagged[0].id()));
    }

    // AUTO-020 point 5 (as revised): best by role first — so, with
    // nothing else to go on, a main-programme track still beats an
    // audio-description track nobody asked for — then canonical order,
    // then identifier.
    Ok(eligible
        .iter()
        .min_by(|t1, t2| {
            audio_role_preference(t1.roles(), accessibility)
                .cmp(&audio_role_preference(t2.roles(), accessibility))
                .then_with(|| positions[&t1.id()].cmp(&positions[&t2.id()]))
                .then_with(|| compare_identifiers(t1.id().as_ref(), t2.id().as_ref()))
        })
        .map(|t| t.id()))
}

/// AUTO-030's "forced only": true when the audio's language gives a forced
/// track nothing to match — a malformed value (it matches nothing,
/// MATCH-010), or a primary language of `und` (not known), `mul` (several)
/// or `zxx` (none), with or without further subtags.
///
/// A private-use tag (`x-foo`) or a grandfathered tag with no replacement
/// (`i-default`) DOES have something to match: a forced track with exactly
/// that tag (MATCH-040; settled in policy revision 4). Before revision 4
/// this function refused every tag that was not an ordinary one, so such
/// audio never got its forced subtitles.
fn audio_has_nothing_to_match(tag: &LanguageTag) -> bool {
    tag.is_malformed()
        || (tag.is_ordinary() && matches!(tag.language.as_deref(), Some("und" | "mul" | "zxx")))
}

/// AUTO-030's "forced only": a track's PLACING role (TRACK-050) must be
/// exactly forced — a `["forced", "sdh"]` track qualifies (placed as
/// forced, the later of the two), but a `["forced", "commentary"]` track
/// does not (placed as commentary, which ranks later still).
fn is_placed_as_forced(roles: &[Role]) -> bool {
    roles::role_rank(TrackType::Subtitle, roles)
        == roles::individual_rank(TrackType::Subtitle, Role::Forced)
}

/// AUTO-030's "always": a track's PLACING role must be full (no roles)
/// or SDH — never forced, commentary, or anything this format has no
/// word for (which places as "other", per [`roles::individual_rank`]'s
/// wildcard arm).
fn is_placed_as_full_or_sdh(roles: &[Role]) -> bool {
    let placing = roles::role_rank(TrackType::Subtitle, roles);
    placing == 0 || placing == roles::individual_rank(TrackType::Subtitle, Role::Sdh)
}

/// The forced track that best matches `audio`'s language, per AUTO-030's
/// "forced only": `None` if there is no audio, the audio's language gives
/// nothing to match (see [`audio_has_nothing_to_match`]), or no forced
/// track matches it at all.
fn forced_only<T: SelectableTrack>(
    subtitles: &[&T],
    audio: Option<&LanguageTag>,
    positions: &HashMap<T::Id, usize>,
) -> Option<T::Id> {
    let audio = audio?;
    if audio_has_nothing_to_match(audio) {
        return None;
    }
    let mut candidates: Vec<(&&T, MatchLevel, usize)> = subtitles
        .iter()
        .filter(|t| is_placed_as_forced(t.roles()))
        .filter_map(|t| {
            let m = match_tags(audio, t.language());
            (m.level <= MatchLevel::Related).then_some((t, m.level, m.distance))
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|(t1, l1, d1), (t2, l2, d2)| {
        l1.cmp(l2)
            .then_with(|| d1.cmp(d2))
            .then_with(|| t2.is_default().cmp(&t1.is_default()))
            .then_with(|| positions[&t1.id()].cmp(&positions[&t2.id()]))
            .then_with(|| compare_identifiers(t1.id().as_ref(), t2.id().as_ref()))
    });
    Some(candidates[0].0.id())
}

/// The best-matching subtitle track placed as full or SDH, per AUTO-030's
/// "always": tries each preference in order, then falls back to the
/// default one of those tracks, else `None`.
fn always<T: SelectableTrack>(
    subtitles: &[&T],
    preferences: &[&LanguageTag],
    accessibility: &Accessibility,
    positions: &HashMap<T::Id, usize>,
) -> Option<T::Id> {
    let ok: Vec<&&T> = subtitles
        .iter()
        .filter(|t| is_placed_as_full_or_sdh(t.roles()))
        .collect();

    let role_preference = |roles: &[Role]| -> u8 {
        let placing = roles::role_rank(TrackType::Subtitle, roles);
        if accessibility.captions {
            // ok only ever contains placing 0 (full) or the SDH rank —
            // swap them so SDH leads when captions were asked for.
            if placing == roles::individual_rank(TrackType::Subtitle, Role::Sdh) {
                0
            } else {
                1
            }
        } else {
            placing
        }
    };

    for preference in preferences {
        let mut candidates: Vec<(&&&T, MatchLevel, usize)> = ok
            .iter()
            .filter_map(|t| {
                let m = match_tags(preference, t.language());
                (m.level <= MatchLevel::Related).then_some((t, m.level, m.distance))
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by(|(t1, l1, d1), (t2, l2, d2)| {
            role_preference(t1.roles())
                .cmp(&role_preference(t2.roles()))
                .then_with(|| l1.cmp(l2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| t2.is_default().cmp(&t1.is_default()))
                .then_with(|| positions[&t1.id()].cmp(&positions[&t2.id()]))
                .then_with(|| compare_identifiers(t1.id().as_ref(), t2.id().as_ref()))
        });
        return Some(candidates[0].0.id());
    }

    let mut defaults: Vec<&&&T> = ok.iter().filter(|t| t.is_default()).collect();
    if defaults.is_empty() {
        return None;
    }
    defaults.sort_by(|t1, t2| {
        positions[&t1.id()]
            .cmp(&positions[&t2.id()])
            .then_with(|| compare_identifiers(t1.id().as_ref(), t2.id().as_ref()))
    });
    Some(defaults[0].id())
}

/// Chooses which subtitle track plays automatically (AUTO-030), given
/// which audio track was chosen (or `None` if none was — a silent video,
/// or nothing has been chosen yet; AUTO-030 treats this the same as an
/// audio language that is not known). `mode` is the user's subtitle
/// preference.
///
/// A malformed preference is ignored, and a user whose preferences are all
/// malformed counts as having none — so automatic mode then acts as
/// forced only (AUTO-010, as clarified in policy revision 4).
///
/// `Err` when two tracks share an identifier (AUTO-010) — see
/// [`select_audio`]'s doc comment; the same guarantee about list order
/// applies here too.
pub fn select_subtitle<T: SelectableTrack>(
    subtitles: &[T],
    audio: Option<&LanguageTag>,
    preferences: &[LanguageTag],
    mode: SubtitleMode,
    accessibility: &Accessibility,
) -> Result<Option<T::Id>, DuplicateIdentifierError> {
    let refs: Vec<&T> = subtitles.iter().collect();
    check_unique_ids(&refs)?;

    if mode == SubtitleMode::Off {
        return Ok(None);
    }
    let preferences = usable_preferences(preferences);
    // Every subtitle track counts for canonical order, as for audio.
    let positions = canonical_positions(&refs, TrackType::Subtitle);

    Ok(match mode {
        SubtitleMode::Off => unreachable!("handled by the early return above"),
        SubtitleMode::ForcedOnly => forced_only(&refs, audio, &positions),
        SubtitleMode::Always => always(&refs, &preferences, accessibility, &positions),
        SubtitleMode::Automatic => {
            if preferences.is_empty() {
                forced_only(&refs, audio, &positions)
            } else {
                let audio_matches_a_preference = audio.is_some_and(|a| {
                    preferences
                        .iter()
                        .any(|p| match_tags(p, a).level <= MatchLevel::Related)
                });
                if audio_matches_a_preference {
                    forced_only(&refs, audio, &positions)
                } else {
                    always(&refs, &preferences, accessibility, &positions)
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::LanguageItem;
    use crate::tag::canonicalise;

    #[derive(Clone)]
    struct Track {
        id: &'static str,
        tag: LanguageTag,
        roles: Vec<Role>,
        default: bool,
        original: bool,
    }

    impl LanguageItem for Track {
        fn language(&self) -> &LanguageTag {
            &self.tag
        }
        fn is_original(&self) -> bool {
            self.original
        }
    }

    impl RoleItem for Track {
        fn roles(&self) -> &[Role] {
            &self.roles
        }
    }

    impl SelectableTrack for Track {
        type Id = &'static str;
        fn id(&self) -> Self::Id {
            self.id
        }
        fn is_default(&self) -> bool {
            self.default
        }
    }

    fn track(id: &'static str, tag: &str, roles: &[Role]) -> Track {
        Track {
            id,
            tag: canonicalise(tag),
            roles: roles.to_vec(),
            default: false,
            original: false,
        }
    }

    fn with_default(mut t: Track) -> Track {
        t.default = true;
        t
    }

    fn with_original(mut t: Track) -> Track {
        t.original = true;
        t
    }

    fn prefs(tags: &[&str]) -> Vec<LanguageTag> {
        tags.iter().map(|t| canonicalise(t)).collect()
    }

    #[test]
    fn exact_preference_wins() {
        let tracks = vec![
            with_default(track("t1", "en", &[])),
            track("t2", "en-GB", &[]),
            track("t3", "fr", &[]),
        ];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en-GB"]), &a11y),
            Ok(Some("t2"))
        );
    }

    #[test]
    fn tie_is_broken_by_identifier_not_list_position() {
        let tracks = vec![track("t2", "en", &[]), track("t1", "en", &[])];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en"]), &a11y),
            Ok(Some("t1"))
        );
    }

    #[test]
    fn digits_only_identifiers_compare_as_numbers() {
        let tracks = vec![track("10", "en", &[]), track("9", "en", &[])];
        let a11y = Accessibility::default();
        assert_eq!(select_audio(&tracks, &prefs(&["en"]), &a11y), Ok(Some("9")));
    }

    #[test]
    fn duplicate_identifiers_are_refused() {
        let tracks = vec![track("a", "en", &[]), track("a", "fr", &[])];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en"]), &a11y),
            Err(DuplicateIdentifierError {
                identifier: "a".to_string()
            })
        );
    }

    #[test]
    fn reversing_the_track_list_never_changes_the_answer() {
        let tracks = vec![
            track("t1", "en-US", &[]),
            track("t2", "en", &[]),
            track("t3", "fr", &[]),
        ];
        let a11y = Accessibility::default();
        let forward = select_audio(&tracks, &prefs(&["en-GB"]), &a11y);
        let mut reversed = tracks.clone();
        reversed.reverse();
        let backward = select_audio(&reversed, &prefs(&["en-GB"]), &a11y);
        assert_eq!(forward, backward);
        assert_eq!(forward, Ok(Some("t2")));
    }

    #[test]
    fn commentary_only_chosen_when_everything_is_commentary() {
        let tracks = vec![
            track("t1", "en", &[Role::Commentary]),
            track("t2", "fr", &[]),
        ];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en"]), &a11y),
            Ok(Some("t2"))
        );

        let all_commentary = vec![
            track("t1", "en", &[Role::Commentary]),
            track("t2", "fr", &[Role::Commentary]),
        ];
        assert_eq!(
            select_audio(&all_commentary, &prefs(&["en"]), &a11y),
            Ok(Some("t1"))
        );
    }

    #[test]
    fn audio_description_leads_only_when_asked_for() {
        let tracks = vec![
            track("t1", "en-GB", &[Role::AudioDescription]),
            track("t2", "en", &[]),
        ];
        let plain = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en-GB"]), &plain),
            Ok(Some("t2"))
        );

        let wants_ad = Accessibility {
            audio_description: true,
            captions: false,
        };
        assert_eq!(
            select_audio(&tracks, &prefs(&["en-GB"]), &wants_ad),
            Ok(Some("t1"))
        );
    }

    #[test]
    fn placing_role_wins_over_a_role_merely_present() {
        // A track carrying both alternate and audio description is
        // PLACED as audio description (the later of the two) — with no
        // accessibility request, that ranks worse than a plain alternate
        // track, even though "alternate" is also technically one of its
        // roles.
        let tracks = vec![
            track("a", "en-GB", &[Role::Alternate, Role::AudioDescription]),
            track("b", "en", &[Role::Alternate]),
        ];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en-GB"]), &a11y),
            Ok(Some("b"))
        );
    }

    #[test]
    fn subtitle_automatic_with_understood_audio_picks_the_forced_track() {
        let subs = vec![
            track("s1", "en", &[]),
            track("s2", "en", &[Role::Forced]),
            track("s3", "fr", &[]),
        ];
        let a11y = Accessibility::default();
        let en = canonicalise("en");
        assert_eq!(
            select_subtitle(
                &subs,
                Some(&en),
                &prefs(&["en"]),
                SubtitleMode::Automatic,
                &a11y
            ),
            Ok(Some("s2"))
        );
    }

    #[test]
    fn subtitle_automatic_with_unfamiliar_audio_never_picks_forced() {
        let subs = vec![
            track("s2", "en", &[Role::Forced]),
            track("s1", "en", &[]),
            track("s3", "ja", &[Role::Forced]),
        ];
        let a11y = Accessibility::default();
        let ja = canonicalise("ja");
        assert_eq!(
            select_subtitle(
                &subs,
                Some(&ja),
                &prefs(&["en"]),
                SubtitleMode::Automatic,
                &a11y
            ),
            Ok(Some("s1"))
        );
    }

    #[test]
    fn subtitle_off_means_off() {
        let subs = vec![track("s1", "en", &[Role::Forced])];
        let a11y = Accessibility::default();
        let en = canonicalise("en");
        assert_eq!(
            select_subtitle(&subs, Some(&en), &prefs(&["en"]), SubtitleMode::Off, &a11y),
            Ok(None)
        );
    }

    #[test]
    fn forced_only_with_unknown_audio_language_matches_nothing() {
        let subs = vec![track("s1", "en", &[Role::Forced])];
        let a11y = Accessibility::default();
        let und = canonicalise("und");
        assert_eq!(
            select_subtitle(
                &subs,
                Some(&und),
                &prefs(&[]),
                SubtitleMode::ForcedOnly,
                &a11y
            ),
            Ok(None)
        );
    }

    #[test]
    fn a_forced_sdh_track_is_placed_as_forced_not_chosen_by_always() {
        let subs = vec![track("s1", "en", &[Role::Forced, Role::Sdh])];
        let a11y = Accessibility {
            audio_description: false,
            captions: true,
        };
        assert_eq!(
            select_subtitle(
                &subs,
                Some(&canonicalise("en")),
                &prefs(&["en"]),
                SubtitleMode::Always,
                &a11y
            ),
            Ok(None)
        );
    }

    // ---- policy revision 4 -------------------------------------------

    #[test]
    fn digits_only_identifiers_of_any_length_come_first() {
        // AUTO-010: 40 digits overflow a 128-bit number; before revision 4
        // such an identifier fell back to text order and lost to "0abc".
        let forty = "1234567890123456789012345678901234567890";
        assert_eq!(compare_identifiers(forty, "0abc"), Ordering::Less);
        assert_eq!(compare_identifiers("0abc", forty), Ordering::Greater);
        let tracks = vec![
            track("0abc", "en", &[]),
            Track {
                id: forty,
                ..track("x", "en", &[])
            },
        ];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["en"]), &a11y),
            Ok(Some(forty))
        );
    }

    #[test]
    fn long_digit_identifiers_compare_as_numbers_by_length() {
        let small = "99999999999999999999999999999999999999999"; // 41 nines
        let big = "100000000000000000000000000000000000000000"; // 42 digits
        assert_eq!(compare_identifiers(small, big), Ordering::Less);
        // Leading zeros do not make a number bigger.
        assert_eq!(
            compare_identifiers("0000000000000000000000000000000000000009", "10"),
            Ordering::Less
        );
        // Same number, different text: plain text decides ("01" before "1").
        assert_eq!(compare_identifiers("01", "1"), Ordering::Less);
        assert_eq!(compare_identifiers("0", "000"), Ordering::Less);
        assert_eq!(compare_identifiers("9", "10"), Ordering::Less);
        assert_eq!(compare_identifiers("", "0"), Ordering::Greater);
    }

    #[test]
    fn a_malformed_preference_matches_nothing_in_selection() {
        // MATCH-010 / AUTO-010 (revision 4): "English" used to match the
        // track "english" exactly.
        let tracks = vec![
            with_default(track("1", "fr", &[])),
            track("2", "english", &[]),
        ];
        let a11y = Accessibility::default();
        assert_eq!(
            select_audio(&tracks, &prefs(&["English"]), &a11y),
            Ok(Some("1"))
        );
    }

    #[test]
    fn all_malformed_preferences_count_as_none_for_automatic_subtitles() {
        // AUTO-010 (revision 4): automatic mode acts as forced only.
        let subs = vec![
            with_default(track("1", "en", &[])),
            track("2", "en", &[Role::Forced]),
        ];
        let a11y = Accessibility::default();
        let en = canonicalise("en");
        assert_eq!(
            select_subtitle(
                &subs,
                Some(&en),
                &prefs(&["English", "en_US"]),
                SubtitleMode::Automatic,
                &a11y
            ),
            Ok(Some("2"))
        );
    }

    #[test]
    fn canonical_order_counts_tracks_that_cannot_be_chosen() {
        // AUTO-020 (revision 4): the original Japanese commentary is never
        // chosen, but it brings the Japanese group forward, so of the two
        // default tracks the Japanese one wins.
        let tracks = vec![
            with_default(track("1", "en", &[])),
            with_default(track("2", "ja", &[])),
            with_original(track("3", "ja", &[Role::Commentary])),
        ];
        let a11y = Accessibility::default();
        assert_eq!(select_audio(&tracks, &[], &a11y), Ok(Some("2")));
    }

    #[test]
    fn commentary_ranks_before_other_even_when_audio_description_is_asked_for() {
        // AUTO-020 (revision 4): every track is commentary or other.
        let tracks = vec![
            with_default(track("1", "en", &[Role::Other])),
            track("2", "en", &[Role::Commentary]),
        ];
        let wants_ad = Accessibility {
            audio_description: true,
            captions: false,
        };
        assert_eq!(
            select_audio(&tracks, &prefs(&["en"]), &wants_ad),
            Ok(Some("2"))
        );
        assert_eq!(
            select_audio(&tracks, &prefs(&["en"]), &Accessibility::default()),
            Ok(Some("2"))
        );
    }

    #[test]
    fn forced_only_matches_a_private_use_or_grandfathered_audio_tag_exactly() {
        // AUTO-030 (revision 4): these used to give nothing.
        let subs = vec![
            track("1", "x-bar", &[Role::Forced]),
            track("2", "x-foo", &[Role::Forced]),
            track("3", "i-default", &[Role::Forced]),
        ];
        let a11y = Accessibility::default();
        for (audio, expected) in [
            ("x-foo", Some("2")),
            ("i-default", Some("3")),
            ("x-baz", None),
        ] {
            let audio = canonicalise(audio);
            assert_eq!(
                select_subtitle(&subs, Some(&audio), &[], SubtitleMode::ForcedOnly, &a11y),
                Ok(expected)
            );
        }
        // A malformed audio value still has nothing to match.
        let malformed = canonicalise("English");
        assert_eq!(
            select_subtitle(
                &subs,
                Some(&malformed),
                &[],
                SubtitleMode::ForcedOnly,
                &a11y
            ),
            Ok(None)
        );
    }

    #[test]
    fn subtitle_mode_words_default_and_round_trip() {
        assert_eq!(SubtitleMode::default(), SubtitleMode::Automatic);
        for mode in SubtitleMode::ALL {
            assert_eq!(mode.as_str().parse::<SubtitleMode>(), Ok(mode));
            assert_eq!(
                serde_json::to_string(&mode).unwrap(),
                format!("\"{}\"", mode.as_str())
            );
        }
        assert!("Forced_Only".parse::<SubtitleMode>().is_err());
    }
}
