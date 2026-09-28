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
// `serde_json` (needed only to parse the reference data this crate
// embeds). Eight building blocks, kept apart on purpose (policy section
// 9): tag parsing and canonical form (`tag`); the canonical, stored-order
// comparator (`canonical`, Part A); track types and roles (`roles`); the
// track-aware variant of stored order (`tracks`); the presentation
// comparator (`presentation`, Part B — a genuinely different algorithm
// from Part A, not the same one wearing a different name); preference
// matching (`matching`); automatic selection (`select`); and sidecar file
// naming (`sidecar`, added in the policy's first revision, TEXT-030).

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
pub use roles::{role_rank, Role, TrackType};
pub use select::{
    compare_identifiers, select_audio, select_subtitle, DuplicateIdentifierError, SelectableTrack,
    SubtitleMode,
};
pub use sidecar::{build_sidecar_name, parse_sidecar_name, InvalidSidecarNumber, SidecarParts};
pub use tag::{
    canonicalise, from_legacy_three_letter, from_posix_locale, iso639_2_code, iso639_2_write,
    Extension, Iso639Form, Iso639Write, LanguageTag, TagKind, TagNote,
};
pub use tracks::{sort_tracks, TrackItem};
