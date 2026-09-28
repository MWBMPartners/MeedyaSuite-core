// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang — the shared Rust implementation of policy MWBM-MEDIA-LANG
// 1.0.0 (docs/standards/media-language-bcp47-policy.md). Read that
// document first; this crate deliberately does not repeat its rules in
// comments beyond citing rule IDs, so the two can never quietly drift
// apart. Every public item below names the rule ID it exists for.
//
// Synchronous, no I/O, no network, no dependency beyond `serde` and
// `serde_json` (`serde` for the derives on the public data types, so an
// app can store them or send them over IPC; `serde_json` to parse the
// reference data this crate embeds). Eight building blocks, kept apart on
// purpose (policy section 9): tag parsing and canonical form (`tag`); the
// canonical, stored-order comparator (`canonical`, Part A); track types
// and roles (`roles`); the track-aware variant of stored order (`tracks`);
// the presentation comparator (`presentation`, Part B — a genuinely
// different algorithm from Part A, not the same one wearing a different
// name); preference matching (`matching`); automatic selection
// (`select`); and sidecar file naming (`sidecar`, TEXT-030 — part of
// policy 1.0.0).

//! # meedya-lang
//!
//! Shared implementation of the **Media Language & BCP 47 Policy**
//! (`MWBM-MEDIA-LANG` 1.0.0) — see
//! `docs/standards/media-language-bcp47-policy.md` in this repository for
//! the normative rules this crate follows. If this crate's behaviour and
//! that document ever disagree, the document is the source of truth (or
//! this crate has a bug — see the document's "Changing this policy"
//! section for how a genuine rule change gets made).
//!
//! ## What this crate is for
//!
//! Identifying, ordering, naming, matching and selecting languages for
//! audio tracks, subtitle tracks, lyrics, translations and multilingual
//! metadata — the shared logic every MeedyaSuite app that touches
//! languages needs, written once so the Rust, PHP and Swift
//! implementations can never quietly disagree with each other.
//!
//! ## The two orders
//!
//! The policy defines two different ways of ordering languages, and this
//! crate keeps them as two different algorithms on purpose:
//!
//! * [`canonical::sort_canonical`] — Part A, **stored** order: the order
//!   tracks are written into a file, or translations are saved in a
//!   database. Depends only on the language tags, so it is the same
//!   everywhere.
//! * [`presentation::sort_for_presentation`] — Part B, **presentation**
//!   order: the order a menu shows to a person. Depends on their
//!   preferences and interface language, so it differs between people —
//!   which is exactly why it must never be written back into stored
//!   content.
//!
//! Reaching for the wrong one, or trying to implement both with one
//! comparator, is the mistake the policy's "Two orders, on purpose"
//! section exists to prevent.

pub mod canonical;
mod data;
pub mod matching;
pub mod presentation;
pub mod roles;
pub mod select;
pub mod sidecar;
pub mod tag;
pub mod tracks;

pub use canonical::{sort_canonical, LanguageItem};
pub use data::embedded_data_version;
pub use matching::{match_tags, MatchLevel, TagMatch};
pub use presentation::{
    label, sort_for_presentation, subtitle_menu, Accessibility, MenuEntry, PresentationContext,
    PresentationItem, PresentationKind,
};
pub use roles::{role_rank, Role, RoleItem, TrackType, UnknownWordError};
pub use select::{
    compare_identifiers, select_audio, select_subtitle, DuplicateIdentifierError, SelectableTrack,
    SubtitleMode,
};
pub use sidecar::{build_sidecar_name, parse_sidecar_name, InvalidSidecarNumber, SidecarParts};
pub use tag::{
    canonicalise, from_legacy_three_letter, from_legacy_three_letter_all, from_posix_locale,
    iso639_2_code, iso639_2_write, Extension, Iso639Form, Iso639Write, LanguageTag, TagKind,
    TagNote,
};
pub use tracks::{sort_tracks, TrackItem};

#[cfg(test)]
mod tests {
    use super::*;

    /// One app track type implementing every track trait this crate has —
    /// the obvious thing for a player to write.
    struct AppTrack {
        id: String,
        tag: LanguageTag,
        roles: Vec<Role>,
        original: bool,
    }

    impl LanguageItem for AppTrack {
        fn language(&self) -> &LanguageTag {
            &self.tag
        }
        fn is_original(&self) -> bool {
            self.original
        }
    }

    impl RoleItem for AppTrack {
        fn roles(&self) -> &[Role] {
            &self.roles
        }
    }

    impl TrackItem for AppTrack {
        fn track_type(&self) -> TrackType {
            TrackType::Audio
        }
    }

    impl PresentationItem for AppTrack {
        fn kind(&self) -> Option<PresentationKind> {
            Some(PresentationKind::Audio)
        }
    }

    impl SelectableTrack for AppTrack {
        type Id = String;
        fn id(&self) -> String {
            self.id.clone()
        }
    }

    #[test]
    fn one_type_can_implement_every_track_trait_and_call_its_methods_plainly() {
        // Before policy revision 4, TrackItem, PresentationItem and
        // SelectableTrack each declared `roles()` (and SelectableTrack
        // `language()` too), so the two plain calls below did not compile
        // for a type implementing all three. This test is that proof: it
        // only has to build.
        let track = AppTrack {
            id: "1".to_string(),
            tag: canonicalise("en"),
            roles: vec![Role::Commentary],
            original: true,
        };
        assert_eq!(track.language().tag, "en");
        assert_eq!(track.roles(), &[Role::Commentary]);
        assert!(track.is_original());

        let mut tracks = vec![track];
        sort_tracks(&mut tracks);
        sort_for_presentation(
            &mut tracks,
            &PresentationContext::default(),
            |a: &str, b: &str| a.cmp(b),
        );
        assert_eq!(
            select_audio(&tracks, &[], &Accessibility::default()),
            Ok(Some("1".to_string()))
        );
    }
}
