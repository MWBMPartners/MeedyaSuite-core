// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::tracks — TRACK-050 and TRACK-060: stored order for
// container tracks, which is Part A's canonical order (`canonical.rs`)
// with two things added — track types are never interleaved, and roles
// break ties before specificity does.

use std::collections::{HashMap, HashSet};

use crate::canonical::{self, GroupKey, LanguageItem};
use crate::roles::{self, Role, TrackType};

/// Anything that is a container track: it has a language (via
/// [`LanguageItem`]), a [`TrackType`], and a list of [`Role`]s. Implement
/// this on your own track type to sort it into stored order with
/// [`sort_tracks`].
pub trait TrackItem: LanguageItem {
    /// Which kind of track this is (TRACK-060).
    fn track_type(&self) -> TrackType;
    /// The track's roles (TRACK-010). Most tracks have none.
    fn roles(&self) -> &[Role];
}

/// Sorts `items` into stored order per TRACK-050 and TRACK-060:
///
/// * track types are never interleaved — every video track, then every
///   audio track, then every subtitle track, then anything else
///   (TRACK-060);
/// * within a type, LANG-010's original-group promotion is worked out
///   **separately for each type** — a German audio track marked original
///   does not promote a German *subtitle* track's group, because the two
///   types are never compared against each other in the first place
///   (TRACK-020 + TRACK-060 together);
/// * within a language group, role order comes before specificity
///   (TRACK-050: main/full first, alternates and accessibility roles
///   after, in the order that track type's roles are defined);
/// * ties keep their original relative order (LANG-027), via Rust's
///   stable [`slice::sort_by`].
pub fn sort_tracks<T: TrackItem>(items: &mut [T]) {
    // TRACK-020: which language groups LANG-010 promotes is decided once
    // per track type, from only that type's items — never globally.
    let mut promoted_by_type: HashMap<TrackType, HashSet<GroupKey>> = HashMap::new();
    for ty in [
        TrackType::Video,
        TrackType::Audio,
        TrackType::Subtitle,
        TrackType::Other,
    ] {
        let set = canonical::promoted_groups(
            items
                .iter()
                .filter(|it| it.track_type() == ty)
                .map(|it| (it.language(), it.is_original())),
        );
        promoted_by_type.insert(ty, set);
    }

    items.sort_by(|a, b| {
        a.track_type().cmp(&b.track_type()).then_with(|| {
            // Reached only when a and b share a track type (the primary
            // key above was Equal), so looking up a's type's promoted set
            // is correct for both sides of this comparison.
            let promoted = &promoted_by_type[&a.track_type()];
            let a_rank = roles::role_rank(a.track_type(), a.roles());
            let b_rank = roles::role_rank(b.track_type(), b.roles());
            canonical::compare_ranked(
                a.language(),
                a.is_original(),
                a_rank,
                b.language(),
                b.is_original(),
                b_rank,
                promoted,
            )
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{canonicalise, LanguageTag};

    struct Track {
        id: &'static str,
        tag: LanguageTag,
        track_type: TrackType,
        roles: Vec<Role>,
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

    impl TrackItem for Track {
        fn track_type(&self) -> TrackType {
            self.track_type
        }
        fn roles(&self) -> &[Role] {
            &self.roles
        }
    }

    fn track(
        id: &'static str,
        tag: &str,
        track_type: TrackType,
        roles: &[Role],
        original: bool,
    ) -> Track {
        Track {
            id,
            tag: canonicalise(tag),
            track_type,
            roles: roles.to_vec(),
            original,
        }
    }

    #[test]
    fn types_are_kept_apart_and_roles_ordered_within_a_language() {
        let mut tracks = vec![
            track("a1", "en", TrackType::Audio, &[Role::Commentary], false),
            track("s1", "en", TrackType::Subtitle, &[Role::Forced], false),
            track("v1", "und", TrackType::Video, &[], false),
            track("a4", "de", TrackType::Audio, &[], false),
            track("s2", "en", TrackType::Subtitle, &[Role::Sdh], false),
            track("a2", "en", TrackType::Audio, &[], false),
            track("s3", "en", TrackType::Subtitle, &[], false),
            track(
                "a3",
                "en",
                TrackType::Audio,
                &[Role::AudioDescription],
                false,
            ),
            track("s4", "de", TrackType::Subtitle, &[], false),
        ];
        sort_tracks(&mut tracks);
        let order: Vec<&str> = tracks.iter().map(|t| t.id).collect();
        assert_eq!(
            order,
            ["v1", "a4", "a2", "a3", "a1", "s4", "s3", "s2", "s1"]
        );
    }

    #[test]
    fn original_flags_promote_separately_per_track_type() {
        let mut tracks = vec![
            track("a1", "en", TrackType::Audio, &[], false),
            track("a2", "ja", TrackType::Audio, &[], true),
            track("a3", "ja", TrackType::Audio, &[Role::Commentary], false),
            track("a4", "fr", TrackType::Audio, &[], false),
            track("s1", "en", TrackType::Subtitle, &[], false),
            track("s2", "ja", TrackType::Subtitle, &[], true),
            track("s3", "en", TrackType::Subtitle, &[Role::Forced], false),
        ];
        sort_tracks(&mut tracks);
        let order: Vec<&str> = tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, ["a2", "a3", "a1", "a4", "s2", "s1", "s3"]);
    }

    #[test]
    fn a_track_with_several_roles_is_placed_by_the_latest() {
        let mut tracks = vec![
            track("s1", "en", TrackType::Subtitle, &[Role::Commentary], false),
            track(
                "s2",
                "en",
                TrackType::Subtitle,
                &[Role::Forced, Role::Sdh],
                false,
            ),
            track("s3", "en", TrackType::Subtitle, &[Role::Sdh], false),
            track("s4", "en", TrackType::Subtitle, &[], false),
            track("a1", "en", TrackType::Audio, &[Role::Other], false),
            track(
                "a2",
                "en",
                TrackType::Audio,
                &[Role::AudioDescription],
                false,
            ),
            track("a3", "en", TrackType::Audio, &[], false),
            track("a4", "en", TrackType::Audio, &[Role::Alternate], false),
        ];
        sort_tracks(&mut tracks);
        let order: Vec<&str> = tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, ["a3", "a4", "a2", "a1", "s4", "s3", "s2", "s1"]);
    }

    #[test]
    fn identical_tracks_keep_their_order() {
        let mut tracks = vec![
            track("x", "en", TrackType::Audio, &[], false),
            track("y", "en", TrackType::Audio, &[], false),
        ];
        sort_tracks(&mut tracks);
        let order: Vec<&str> = tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, ["x", "y"]);
    }
}
