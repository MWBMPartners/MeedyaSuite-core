// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::roles — track types and roles (TRACK-010), and TRACK-050's
// role ordering. Kept apart from `tag`, `canonical`, `tracks` and
// `presentation` because the policy's section 9 requires tag parsing, the
// two comparators, matching and role ordering to each be their own job —
// mixing "what a track's tag is" with "what a track is for" is exactly
// the kind of drift that makes an implementation hard to trust.

/// The kind of track, for TRACK-060 ("each track type is ordered on its
/// own — video, then audio, then subtitles, then anything else") and for
/// picking which role order applies (audio and subtitles order their
/// roles differently — see [`individual_rank`]).
///
/// Declared in this order deliberately: this type derives [`Ord`], and
/// [`tracks::sort_tracks`](crate::tracks::sort_tracks) sorts by it
/// directly, so the declaration order below **is** TRACK-060's ordering.
/// Reordering these variants would silently change stored track order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Alternate,
    AudioDescription,
    Commentary,
    Sdh,
    Forced,
    Other,
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
}
