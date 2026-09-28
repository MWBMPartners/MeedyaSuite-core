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

use crate::canonical::{self, GroupKey};
use crate::matching::{match_tags, MatchLevel};
use crate::presentation::Accessibility;
use crate::roles::{self, Role, TrackType};
use crate::tag::LanguageTag;

/// A track that can be chosen automatically: it has an identifier stable
/// enough to break ties by (AUTO-010: "the track identifier decides, the
/// number or ID the file gives the track — never its position in a
/// list"), a language, roles, and default/original flags.
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
pub trait SelectableTrack {
    type Id: Clone + Eq + std::hash::Hash + AsRef<str>;

    fn id(&self) -> Self::Id;
    fn language(&self) -> &LanguageTag;
    fn roles(&self) -> &[Role];
    /// The container's "default" flag. Defaults to `false`.
    fn is_default(&self) -> bool {
        false
    }
    /// The container's "original language" flag. Defaults to `false`.
    fn is_original(&self) -> bool {
        false
    }
}

/// AUTO-010: two tracks in the same selection call had the same
/// identifier. Selection refuses to guess which one was meant, rather
/// than silently overwriting one candidate's canonical position with the
/// other's.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// AUTO-010's identifier comparison: `(is-not-all-digits, numeric value
/// when all-digits, the text itself)`. Digits-only identifiers sort
/// first (bucket 0), by their numeric value, then — for two identifiers
/// that are numerically equal but not textually identical, such as `"1"`
/// and `"01"` — by the text itself. Anything else sorts after (bucket 1),
/// in plain byte-string order.
///
/// An identifier is more digits than any real track count will ever
/// need (`u128` overflows only past 39 digits) is treated as plain text
/// instead of failing — comparing it as text is still a defined, useful
/// answer, and refusing outright over an identifier this policy never
/// expected to matter to a human would be a worse failure mode than a
/// slightly surprising sort position.
fn identifier_key(id: &str) -> (u8, u128, &str) {
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(n) = id.parse::<u128>() {
            return (0, n, id);
        }
    }
    (1, 0, id)
}

/// Compares two identifiers per AUTO-010 (see [`identifier_key`]). Every
/// tie-break in this module that would otherwise fall back to a track's
/// position in a list uses this instead.
pub fn compare_identifiers(a: &str, b: &str) -> Ordering {
    identifier_key(a).cmp(&identifier_key(b))
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

/// The user's subtitle preference (AUTO-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleMode {
    /// If the chosen audio's language matches a preference (or there are
    /// no preferences at all), act as [`SubtitleMode::ForcedOnly`];
    /// otherwise act as [`SubtitleMode::Always`]. The usual default.
    Automatic,
    /// The best-matching non-forced, non-commentary subtitle track, by
    /// preference order.
    Always,
    /// Only the forced track matching the chosen audio's language, if
    /// any.
    ForcedOnly,
    /// Never choose a subtitle track.
    Off,
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

/// Builds each eligible track's position in Part A's stored order,
/// computed from a copy of `tracks` **sorted by id first** (using
/// [`compare_identifiers`], not the caller's own `Ord`, since `Id` no
/// longer needs one) — this is what makes the result independent of the
/// order the caller's slice happens to be in (AUTO-010): whichever order
/// `tracks` arrives in, sorting by id first before working out canonical
/// positions means two callers who disagree only about track order still
/// agree about positions, and therefore about ties.
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

/// The role-preference rank AUTO-020 point 2 uses as the first tie-break
/// among candidates, built from the track's PLACING role (TRACK-050: the
/// latest of its roles — exactly [`roles::role_rank`]'s own definition).
/// Ordinarily this is that rank unchanged (main first, then alternate,
/// then audio description, worst); when the user has asked for audio
/// description, audio description is promoted ahead of both.
fn audio_role_preference(roles: &[Role], accessibility: &Accessibility) -> u8 {
    let placing = roles::role_rank(TrackType::Audio, roles);
    if accessibility.audio_description {
        match placing {
            2 => 0,         // audio description promoted to the front
            0 => 1,         // main
            1 => 2,         // alternate
            other => other, // commentary/other: unaffected, already excluded upstream
        }
    } else {
        placing
    }
}

/// Chooses which audio track plays automatically (AUTO-020).
///
/// `Ok(None)` only when `tracks` is empty. `Err` when two tracks share an
/// identifier (AUTO-010) — this function never guesses which one was
/// meant.
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

    // AUTO-020 point 1: tracks placed as commentary/other are never
    // chosen automatically unless every track is one.
    let eligible: Vec<&T> = {
        let non_special: Vec<&T> = refs
            .iter()
            .filter(|t| !is_special_audio_role(t.roles()))
            .copied()
            .collect();
        if non_special.is_empty() {
            refs
        } else {
            non_special
        }
    };

    let positions = canonical_positions(&eligible, TrackType::Audio);

    // AUTO-020 point 2: for each preference in order, the best match.
    for preference in preferences {
        let mut candidates: Vec<(&&T, MatchLevel, u8)> = eligible
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

fn is_ineligible_for_matching(tag: &LanguageTag) -> bool {
    !tag.is_ordinary() || matches!(tag.language.as_deref(), Some("und" | "mul" | "zxx"))
}

/// AUTO-030's "forced only": a track's PLACING role (TRACK-050) must be
/// exactly forced — a `["forced", "sdh"]` track qualifies (placed as
/// forced, the later of the two), but a `["sdh", "audio_description"]`
/// track would not (placed as something else entirely, per whichever
/// role ranks highest for subtitles).
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
/// "forced only": `None` if there is no audio, the audio's language is
/// `und`/`mul`/`zxx`, or no forced track matches it at all.
fn forced_only<T: SelectableTrack>(
    subtitles: &[&T],
    audio: Option<&LanguageTag>,
    positions: &HashMap<T::Id, usize>,
) -> Option<T::Id> {
    let audio = audio?;
    if is_ineligible_for_matching(audio) {
        return None;
    }
    let mut candidates: Vec<(&&T, MatchLevel, u8)> = subtitles
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

/// The best-matching non-forced, non-commentary subtitle track, per
/// AUTO-030's "always": tries each preference in order, then falls back
/// to the default full track, else `None`.
fn always<T: SelectableTrack>(
    subtitles: &[&T],
    preferences: &[LanguageTag],
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
        let mut candidates: Vec<(&&&T, MatchLevel, u8)> = ok
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
    let positions = canonical_positions(&refs, TrackType::Subtitle);

    Ok(match mode {
        SubtitleMode::Off => unreachable!("handled by the early return above"),
        SubtitleMode::ForcedOnly => forced_only(&refs, audio, &positions),
        SubtitleMode::Always => always(&refs, preferences, accessibility, &positions),
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
                    always(&refs, preferences, accessibility, &positions)
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::canonicalise;

    #[derive(Clone)]
    struct Track {
        id: &'static str,
        tag: LanguageTag,
        roles: Vec<Role>,
        default: bool,
        original: bool,
    }

    impl SelectableTrack for Track {
        type Id = &'static str;
        fn id(&self) -> Self::Id {
            self.id
        }
        fn language(&self) -> &LanguageTag {
            &self.tag
        }
        fn roles(&self) -> &[Role] {
            &self.roles
        }
        fn is_default(&self) -> bool {
            self.default
        }
        fn is_original(&self) -> bool {
            self.original
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
}
