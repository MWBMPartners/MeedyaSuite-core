// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::roles — track types and roles (TRACK-010), and TRACK-050's
// role ordering. Kept apart from `tag`, `canonical`, `tracks` and
// `presentation` because the policy's section 9 requires tag parsing, the
// two comparators, matching and role ordering to each be their own job —
// mixing "what a track's tag is" with "what a track is for" is exactly
// the kind of drift that makes an implementation hard to trust.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::canonical::LanguageItem;

/// The kind of track, for TRACK-060 ("each track type is ordered on its
/// own — video, then audio, then subtitles, then anything else") and for
/// picking which role order applies (audio and subtitles order their
/// roles differently — see [`individual_rank`]).
///
/// Declared in this order deliberately: this type derives [`Ord`], and
/// [`tracks::sort_tracks`](crate::tracks::sort_tracks) sorts by it
/// directly, so the declaration order below **is** TRACK-060's ordering.
/// Reordering these variants would silently change stored track order.
///
/// Serialises as `video`, `audio`, `subtitle`, `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackType {
    Video,
    Audio,
    Subtitle,
    Other,
}

/// A track's role, apart from its language (TRACK-010). A track with no
/// roles at all is the main programme (audio) or full subtitles
/// (subtitle) — there is no `Role::Main` or `Role::Full` variant because
/// "no roles" already means that.
///
/// Written and read with the policy's own words — `alternate`,
/// `audio_description`, `commentary`, `sdh`, `forced`, `other` — by
/// [`Role::as_str`], [`FromStr`] and serde alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Alternate,
    AudioDescription,
    Commentary,
    Sdh,
    Forced,
    Other,
}

impl Role {
    /// Every role, in declaration order.
    pub const ALL: [Role; 6] = [
        Role::Alternate,
        Role::AudioDescription,
        Role::Commentary,
        Role::Sdh,
        Role::Forced,
        Role::Other,
    ];

    /// The policy's word for this role (the fixture schema's `role` list):
    /// `alternate`, `audio_description`, `commentary`, `sdh`, `forced`,
    /// `other`. Not a sidecar file name word — TEXT-030 has its own,
    /// shorter list (see [`crate::sidecar`]).
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Alternate => "alternate",
            Role::AudioDescription => "audio_description",
            Role::Commentary => "commentary",
            Role::Sdh => "sdh",
            Role::Forced => "forced",
            Role::Other => "other",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = UnknownWordError;

    /// Reads the policy's word for a role, exactly as [`Role::as_str`]
    /// writes it (lower case, no other spelling). An unknown word is an
    /// error here, so a typo is not silently accepted. When reading roles
    /// from a file or another system, remember TRACK-050: a role you do not
    /// recognise counts as **other** — `word.parse().unwrap_or(Role::Other)`.
    fn from_str(word: &str) -> Result<Self, Self::Err> {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == word)
            .ok_or_else(|| UnknownWordError {
                what: "role",
                word: word.to_string(),
            })
    }
}

/// A word that is not one of the policy's words for the thing being read
/// ([`Role`]'s or [`SubtitleMode`](crate::select::SubtitleMode)'s
/// [`FromStr`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnknownWordError {
    /// What was being read: `"role"` or `"subtitle mode"`.
    pub what: &'static str,
    /// The word as given.
    pub word: String,
}

impl fmt::Display for UnknownWordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a {} word the policy defines",
            self.word, self.what
        )
    }
}

impl std::error::Error for UnknownWordError {}

/// Anything with a language that can also carry roles — the one place
/// `roles()` is declared. [`TrackItem`](crate::tracks::TrackItem),
/// [`PresentationItem`](crate::presentation::PresentationItem) and
/// [`SelectableTrack`](crate::select::SelectableTrack) all build on it.
///
/// Why one shared trait: before policy revision 4 each of those three
/// declared its own `roles()` (and `SelectableTrack` its own `language()`
/// and `is_original()` too). A type implementing all three — an app's own
/// track type, the obvious thing to write — then had three methods called
/// `roles`, and a plain `track.roles()` would not compile until the caller
/// spelled out which trait they meant. Declared once, the call is
/// unambiguous and the three traits cannot drift apart.
pub trait RoleItem: LanguageItem {
    /// The item's roles (TRACK-010). Defaults to none: a track with no
    /// roles is the main programme (audio) or full subtitles, and a plain
    /// language item (a translation in a picker) has none at all.
    fn roles(&self) -> &[Role] {
        &[]
    }
}

/// Where one role sits in TRACK-050's fixed order, for the given track
/// type. A role that TRACK-050 does not mention for this track type
/// (`Sdh` on an audio track, say — not a combination the policy expects,
/// but not one this function refuses either) is treated as "anything
/// else", the same rank as [`Role::Other`].
///
/// * Audio: main (no roles, rank 0) → alternate → audio description →
///   commentary → anything else.
/// * Subtitle: full (no roles, rank 0) → SDH/captions → forced →
///   commentary → anything else.
/// * Video and "anything else" track types: roles don't apply; always 0.
pub(crate) fn individual_rank(track_type: TrackType, role: Role) -> u8 {
    match track_type {
        TrackType::Audio => match role {
            Role::Alternate => 1,
            Role::AudioDescription => 2,
            Role::Commentary => 3,
            Role::Sdh | Role::Forced | Role::Other => 4,
        },
        TrackType::Subtitle => match role {
            Role::Sdh => 1,
            Role::Forced => 2,
            Role::Commentary => 3,
            Role::Alternate | Role::AudioDescription | Role::Other => 4,
        },
        TrackType::Video | TrackType::Other => 0,
    }
}

/// The rank a whole track sorts by, given all of its roles (TRACK-050:
/// "a track with more than one role is placed by the one latest in its
/// list" — the highest-ranked one). A track with no roles ranks 0, the
/// same as the main/full role would.
pub fn role_rank(track_type: TrackType, roles: &[Role]) -> u8 {
    roles
        .iter()
        .map(|&r| individual_rank(track_type, r))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_type_order_matches_track_060() {
        assert!(TrackType::Video < TrackType::Audio);
        assert!(TrackType::Audio < TrackType::Subtitle);
        assert!(TrackType::Subtitle < TrackType::Other);
    }

    #[test]
    fn a_track_with_several_roles_is_ranked_by_the_latest() {
        // TRACK-050: subtitle order is full -> SDH -> forced -> commentary
        // -> anything else, so a track carrying both forced and SDH is
        // placed as if it were forced (the later of the two).
        let roles = [Role::Forced, Role::Sdh];
        assert_eq!(
            role_rank(TrackType::Subtitle, &roles),
            individual_rank(TrackType::Subtitle, Role::Forced)
        );
    }

    #[test]
    fn no_roles_ranks_the_same_as_main_or_full() {
        assert_eq!(role_rank(TrackType::Audio, &[]), 0);
        assert_eq!(role_rank(TrackType::Subtitle, &[]), 0);
    }

    #[test]
    fn video_and_other_track_types_never_rank_by_role() {
        assert_eq!(role_rank(TrackType::Video, &[Role::Commentary]), 0);
        assert_eq!(role_rank(TrackType::Other, &[Role::Forced]), 0);
    }

    #[test]
    fn role_words_round_trip() {
        for role in Role::ALL {
            assert_eq!(role.as_str().parse::<Role>(), Ok(role));
            assert_eq!(role.to_string(), role.as_str());
            assert_eq!(
                serde_json::to_string(&role).unwrap(),
                format!("\"{}\"", role.as_str())
            );
        }
        assert_eq!(Role::AudioDescription.as_str(), "audio_description");
        assert_eq!(
            "cc".parse::<Role>(),
            Err(UnknownWordError {
                what: "role",
                word: "cc".to_string()
            })
        );
        assert!("SDH".parse::<Role>().is_err());
        assert_eq!(
            serde_json::to_string(&TrackType::Subtitle).unwrap(),
            "\"subtitle\""
        );
    }
}
