// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Identifier-registry guard (#65).
//
// ELI5: this file's job is to notice if the identifier_types.toml artifact
// and the code that consumes it (CommonTag) ever drift apart, and to fail
// the build loudly when they do.
//
// Why this shape: the `EXPECTED_*` constants below are the DELIBERATE,
// reviewed DECLARATION side of a change-detector — a human decided the
// active-slug set should be exactly this, on this date, for this reason.
// The OTHER side of every comparison is always DERIVED — parsed straight
// from `identifier_types.toml` via `identifier_types()`, or iterated from
// the `CommonTag` enum via `strum::EnumIter` — never a second hand-typed
// copy of the same list. A second hand-typed copy is exactly the failure
// mode this guard exists to prevent (see project CLAUDE.md rule #34: "a
// guard must be mutation-tested, and derived from the tree rather than
// from a list you typed" — and this repo's own §6.5 mutation proof is
// the practical demonstration that these tests actually catch drift
// rather than just restating it).
//
// Integration tests in this file compile as a DOWNSTREAM crate (not
// meedya-metadata itself), which is exactly the environment we want to
// exercise: they see the crate's public surface under the
// `#[non_exhaustive]` contract, the same way MeedyaManager or any other
// consumer would.
//
// Each downstream repo that mirrors `identifier_types.toml` (MeedyaManager,
// iHymns) holds its OWN copy of this kind of guard against its OWN mirror
// — this repo ships the canonical artifact plus this guard; it does not,
// and cannot, verify another repo's mirror stayed in sync.

// NOTE (#65 deviation from spec §6.1's verbatim import list): the spec's
// import line includes `IdentifierValidation`, but none of the 5 tests
// below construct or match on it, and `cargo clippy -D warnings` (§8 step
// 6) fails the build on an unused import. Dropped here rather than adding
// a dead reference just to keep an unused symbol imported.
use meedya_metadata::{identifier_type, identifier_types, CommonTag, IdentifierStatus};
use strum::IntoEnumIterator;

/// The deliberate, reviewed declaration of the active-slug set.
/// Changing identifier_types.toml without updating this list (or vice
/// versa) MUST fail CI — that is the point. Keep sorted.
const EXPECTED_ACTIVE_SLUGS: &[&str] = &[
    "acoustid",
    "bowi",
    "eidr",
    "grid",
    "icpn",
    "ipi",
    "isni",
    "isrc",
    "iswc",
    "musicbrainz-artist",
    "musicbrainz-recording",
    "musicbrainz-release",
    "musicbrainz-release-group",
    "musicbrainz-work",
    "upc",
];

/// Reserved slugs — claimed shapes with no storage surface yet.
const EXPECTED_RESERVED_SLUGS: &[&str] = &["dpid", "hfa", "ipn", "label-code"];

/// Catches: a slug added/removed from the TOML, a slug renamed, or a
/// `status` flip in either direction (active<->reserved) that this
/// declaration wasn't updated to match.
#[test]
fn active_slug_set_matches_expected() {
    let mut active: Vec<&str> = identifier_types()
        .iter()
        .filter(|t| t.status == IdentifierStatus::Active)
        .map(|t| t.slug.as_str())
        .collect();
    active.sort_unstable();
    assert_eq!(active, EXPECTED_ACTIVE_SLUGS);
}

/// The reserved-side mirror of the test above.
#[test]
fn reserved_slug_set_matches_expected() {
    let mut reserved: Vec<&str> = identifier_types()
        .iter()
        .filter(|t| t.status == IdentifierStatus::Reserved)
        .map(|t| t.slug.as_str())
        .collect();
    reserved.sort_unstable();
    assert_eq!(reserved, EXPECTED_RESERVED_SLUGS);
}

/// Derived from the enum (via EnumIter), not a hand-typed variant list: a
/// new CommonTag variant whose `identifier_slug()` points at a slug that
/// is missing from the registry, or that exists but is only `reserved`,
/// goes red here with NO test edit required.
#[test]
fn common_tag_identifier_variants_are_registered_and_active() {
    for variant in CommonTag::iter() {
        let Some(slug) = variant.identifier_slug() else {
            continue;
        };
        let entry = identifier_type(slug).unwrap_or_else(|| {
            panic!("CommonTag::{variant:?}.identifier_slug() = {slug:?} is not a registered identifier_types.toml slug")
        });
        assert_eq!(
            entry.status,
            IdentifierStatus::Active,
            "CommonTag::{variant:?} points at slug {slug:?}, which is only `reserved`",
        );
    }
}

/// Documents/exercises the downstream `#[non_exhaustive]` contract from an
/// external crate: a match here MUST carry a wildcard arm or this file
/// fails to compile (an exhaustive match on a `#[non_exhaustive]` foreign
/// enum is a hard compiler error, not merely a lint) — that failure mode
/// is what protects every future variant addition from silently breaking
/// downstream builds without the compiler flagging it here first.
#[test]
fn downstream_match_requires_wildcard() {
    assert_eq!(
        CommonTag::MusicBrainzReleaseGroupId.vorbis_comment_name(),
        "MUSICBRAINZ_RELEASEGROUPID"
    );

    let tag = CommonTag::Isrc;
    let described = match tag {
        CommonTag::Isrc => "isrc",
        _ => "other",
    };
    assert_eq!(described, "isrc");
}

/// Tree-derived tripwire so `#[non_exhaustive]` cannot be silently dropped
/// from `CommonTag` in a later edit — reads the actual source file rather
/// than relying on a compile-time property this test file can't observe
/// directly (removing the attribute is a semver-relevant but
/// compiler-silent change from a downstream crate's point of view).
#[test]
fn common_tag_is_marked_non_exhaustive() {
    // Tree-derived AND reorder-tolerant: find the `pub enum CommonTag`
    // declaration, then walk backwards over the contiguous decorator block
    // (attributes / doc-comments / blank lines) that sits directly above it
    // and assert `#[non_exhaustive]` is one of those attributes.
    //
    // The previous check — `contains("#[non_exhaustive]\npub enum CommonTag")`
    // — was too blunt (rule #34): it demanded the attribute be the *single*
    // line immediately above the enum, so a correct edit that merely reordered
    // it relative to `#[derive(...)]` or slipped a doc-comment between the two
    // would have gone RED on working code, and a guard that fails on correct
    // code gets weakened or deleted rather than fixed. This version tolerates
    // any attribute ordering / interleaved comments, yet still goes RED if the
    // attribute is removed outright or detached from the enum.
    let src = include_str!("../src/common_tags.rs");
    let lines: Vec<&str> = src.lines().collect();
    let enum_line = lines
        .iter()
        .position(|l| l.trim_start().starts_with("pub enum CommonTag"))
        .expect("`pub enum CommonTag` declaration not found in common_tags.rs");

    let mut has_non_exhaustive = false;
    for line in lines[..enum_line].iter().rev() {
        let t = line.trim();
        // Skip the parts of a decorator block that are not the attribute.
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("#[") {
            if t == "#[non_exhaustive]" {
                has_non_exhaustive = true;
            }
            continue;
        }
        // First line that is neither attribute, comment, nor blank ends the
        // decorator block — stop so a stray `#[non_exhaustive]` elsewhere in
        // the file can't satisfy the check.
        break;
    }
    assert!(
        has_non_exhaustive,
        "CommonTag must carry #[non_exhaustive] as one of the attributes on \
         `pub enum CommonTag` (semver: downstream matches must keep a wildcard \
         arm). #65"
    );
}
