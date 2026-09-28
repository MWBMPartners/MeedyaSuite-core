// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang conformance test — runs every case in
// tests/fixtures/bcp47-language-policy-v1.json (the master copy of the
// MWBM-MEDIA-LANG test cases) against this crate's public API. Every
// case in every section must pass; this file collects ALL failures
// before asserting, so one run shows the whole picture rather than
// stopping at the first mismatch.
//
// This test deliberately builds its own small wrapper types implementing
// this crate's traits (`LanguageItem`, `TrackItem`, `PresentationItem`,
// `SelectableTrack`) from the fixture JSON, rather than reaching into the
// crate's internals — it exercises exactly the surface a real consumer
// (MeedyaDL, MeedyaManager) would use.
//
// Policy 8.1 requires this runner to FAIL — never quietly pass — on an
// unknown section, a missing or empty section it needs, or a case
// missing a field the schema requires. Two things make that true here:
// serde's derive already refuses to deserialise a struct whose non-
// `Option` field is absent from the JSON, which covers most required
// fields for free; and `require_present`, below, closes the one gap that
// leaves — a field the SCHEMA requires to be present but whose VALUE may
// be `null` (an `Option<T>` field in Rust cannot tell "the key was
// missing" apart from "the key was present and null" on its own).

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use meedya_lang::{
    build_sidecar_name, canonicalise, embedded_data_version, from_legacy_three_letter,
    from_posix_locale, iso639_2_write, label, match_tags, parse_sidecar_name, select_audio,
    select_subtitle, sort_canonical, sort_for_presentation, sort_tracks, subtitle_menu,
    Accessibility, LanguageItem, LanguageTag, MatchLevel, MenuEntry, PresentationContext,
    PresentationItem, PresentationKind, Role, RoleItem, SelectableTrack, SubtitleMode, TrackItem,
    TrackType,
};

// ---------------------------------------------------------------------
// Fixture-shape robustness helpers (policy 8.1)
// ---------------------------------------------------------------------

/// Panics unless every one of `keys` is present (as a JSON object key —
/// `null` counts as present, only a missing key does not) on `case`.
/// This is the check an `Option<T>` struct field cannot do by itself: a
/// case that OMITS a required-but-nullable field would deserialise into
/// exactly the same Rust value (`None`) as one that explicitly writes
/// `"field": null`, which is precisely the ambiguity policy 8.1 requires
/// the harness to resolve rather than paper over.
fn require_present(case: &Value, keys: &[&str], id_hint: &str) {
    let obj = case
        .as_object()
        .unwrap_or_else(|| panic!("{id_hint}: fixture case is not a JSON object"));
    for key in keys {
        assert!(
            obj.contains_key(*key),
            "{id_hint}: fixture case is missing required field {key:?}"
        );
    }
}

fn case_id_hint(case: &Value, section: &str) -> String {
    case.get("id")
        .and_then(Value::as_str)
        .map(|id| format!("{section}/{id}"))
        .unwrap_or_else(|| format!("{section}/<no id>"))
}

// ---------------------------------------------------------------------
// Fixture file shape
// ---------------------------------------------------------------------

#[derive(Deserialize)]
struct Fixtures {
    policy: String,
    policy_version: String,
    #[allow(dead_code)]
    fixtures_version: String,
    data_version: String,
    canonicalise: Vec<CanonicaliseCase>,
    legacy_three_letter: Vec<SimpleCase>,
    iso639_2_write: Vec<Iso639WriteCase>,
    posix_locale: Vec<SimpleCase>,
    sidecar_name: Vec<SidecarCase>,
    canonical_order: Vec<OrderCase>,
    track_order: Vec<TrackOrderCase>,
    presentation_order: Vec<PresentationCase>,
    subtitle_menu: Vec<PresentationCase>,
    label: Vec<LabelCase>,
    #[serde(rename = "match")]
    match_cases: Vec<MatchCase>,
    auto_select_audio: Vec<AutoAudioCase>,
    auto_select_subtitle: Vec<AutoSubtitleCase>,
}

/// Every section name this harness knows how to run, in the order the
/// policy's own table (8.1) lists them. Used for two checks: that the
/// fixture file does not carry a section this harness has never heard of
/// (which would otherwise be silently ignored by `Fixtures`' lack of
/// `deny_unknown_fields` at the field level — checked explicitly instead,
/// see `every_conformance_case_passes`), and that none of the sections
/// this harness needs is empty.
const KNOWN_SECTIONS: &[&str] = &[
    "canonicalise",
    "legacy_three_letter",
    "iso639_2_write",
    "posix_locale",
    "sidecar_name",
    "canonical_order",
    "track_order",
    "presentation_order",
    "subtitle_menu",
    "label",
    "match",
    "auto_select_audio",
    "auto_select_subtitle",
];

#[derive(Deserialize)]
struct CanonicaliseCase {
    id: String,
    input: String,
    expected: Option<String>,
    kind: String,
}

#[derive(Deserialize)]
struct SimpleCase {
    id: String,
    input: String,
    expected: Option<String>,
}

#[derive(Deserialize)]
struct Iso639WriteExpected {
    b: String,
    t: String,
}

#[derive(Deserialize)]
struct Iso639WriteCase {
    id: String,
    input: String,
    expected: Iso639WriteExpected,
}

#[derive(Deserialize, Debug)]
struct SidecarExpected {
    tag: Option<String>,
    unrecognised: Option<String>,
    roles: Vec<String>,
    number: Option<u32>,
    extension: String,
}

/// The fixture's `sidecar_name` cases come in two shapes, told apart by
/// their `mode` field — matches the JSON schema's `oneOf` exactly, so a
/// case with the wrong fields for its own mode fails to deserialise
/// rather than being silently accepted.
///
/// `Build::number` is `i64` (not `u32`, what the field is stored as once
/// validated) precisely so a fixture's `-1` — proving the builder refuses
/// a negative number — has somewhere to go; the same reasoning as
/// `InvalidSidecarNumber` in `sidecar.rs`. `Build::expected` is
/// `Option<String>` because an `error: true` case sets it to `null`.
#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
enum SidecarCase {
    Build {
        id: String,
        stem: String,
        tag: String,
        roles: Vec<String>,
        extension: String,
        number: Option<i64>,
        expected: Option<String>,
        #[serde(default)]
        error: bool,
    },
    Parse {
        id: String,
        stem: String,
        filename: String,
        expected: Option<SidecarExpected>,
    },
}

#[derive(Deserialize)]
struct OrderItem {
    id: Option<String>,
    tag: String,
    original: Option<bool>,
}

#[derive(Deserialize)]
struct OrderCase {
    id: String,
    items: Vec<OrderItem>,
    expected: Vec<String>,
}

#[derive(Deserialize)]
struct TrackDef {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    tag: String,
    roles: Vec<String>,
    original: Option<bool>,
}

#[derive(Deserialize)]
struct TrackOrderCase {
    id: String,
    tracks: Vec<TrackDef>,
    expected: Vec<String>,
}

#[derive(Deserialize, Default)]
struct AccessibilityDef {
    audio_description: Option<bool>,
    captions: Option<bool>,
}

impl AccessibilityDef {
    fn resolve(&self) -> Accessibility {
        Accessibility {
            audio_description: self.audio_description.unwrap_or(false),
            captions: self.captions.unwrap_or(false),
        }
    }
}

#[derive(Deserialize)]
struct PresentationItemDef {
    id: String,
    tag: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
    original: Option<bool>,
}

#[derive(Deserialize)]
struct PresentationCase {
    id: String,
    preferences: Vec<String>,
    accessibility: AccessibilityDef,
    display_names: HashMap<String, String>,
    collation_keys: HashMap<String, String>,
    items: Vec<PresentationItemDef>,
    expected: Vec<String>,
}

#[derive(Deserialize)]
struct LabelCase {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    language_name: String,
    roles: Vec<String>,
    role_names: HashMap<String, String>,
    channels: Option<String>,
    expected: String,
}

#[derive(Deserialize)]
struct MatchExpected {
    level: String,
    distance: usize,
}

#[derive(Deserialize)]
struct MatchCase {
    id: String,
    preference: String,
    candidate: String,
    expected: MatchExpected,
}

#[derive(Deserialize)]
struct SelectTrackDef {
    id: String,
    tag: String,
    #[serde(default)]
    roles: Vec<String>,
    default: Option<bool>,
    original: Option<bool>,
}

#[derive(Deserialize)]
struct AutoAudioCase {
    id: String,
    preferences: Vec<String>,
    accessibility: AccessibilityDef,
    tracks: Vec<SelectTrackDef>,
    expected: Option<String>,
    #[serde(default)]
    error: bool,
}

#[derive(Deserialize)]
struct AutoSubtitleCase {
    id: String,
    mode: String,
    preferences: Vec<String>,
    accessibility: AccessibilityDef,
    audio: Option<String>,
    tracks: Vec<SelectTrackDef>,
    expected: Option<String>,
    #[serde(default)]
    error: bool,
}

// ---------------------------------------------------------------------
// Small helpers shared by several sections
// ---------------------------------------------------------------------

fn role_from_str(s: &str) -> Role {
    match s {
        "alternate" => Role::Alternate,
        "audio_description" => Role::AudioDescription,
        "commentary" => Role::Commentary,
        "sdh" => Role::Sdh,
        "forced" => Role::Forced,
        "other" => Role::Other,
        other => panic!("fixture uses an unknown role {other:?}"),
    }
}

fn roles_from_strs(strs: &[String]) -> Vec<Role> {
    strs.iter().map(|s| role_from_str(s)).collect()
}

fn track_type_from_str(s: &str) -> TrackType {
    match s {
        "video" => TrackType::Video,
        "audio" => TrackType::Audio,
        "subtitle" => TrackType::Subtitle,
        "other" => TrackType::Other,
        other => panic!("fixture uses an unknown track type {other:?}"),
    }
}

fn presentation_kind_from_str(s: &str) -> PresentationKind {
    match s {
        "audio" => PresentationKind::Audio,
        "subtitle" => PresentationKind::Subtitle,
        "text" => PresentationKind::Text,
        other => panic!("fixture uses an unknown item type {other:?}"),
    }
}

fn preferences_from_strs(strs: &[String]) -> Vec<LanguageTag> {
    strs.iter().map(|s| canonicalise(s)).collect()
}

fn match_level_from_str(s: &str) -> MatchLevel {
    match s {
        "exact" => MatchLevel::Exact,
        "general" => MatchLevel::General,
        "specific" => MatchLevel::Specific,
        "related" => MatchLevel::Related,
        "none" => MatchLevel::None,
        other => panic!("fixture uses an unknown match level {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Wrapper item types, one per trait this crate exposes
// ---------------------------------------------------------------------

struct OrderTestItem {
    id: String,
    tag: LanguageTag,
    original: bool,
}

impl LanguageItem for OrderTestItem {
    fn language(&self) -> &LanguageTag {
        &self.tag
    }
    fn is_original(&self) -> bool {
        self.original
    }
}

struct TrackTestItem {
    id: String,
    tag: LanguageTag,
    track_type: TrackType,
    roles: Vec<Role>,
    original: bool,
}

impl LanguageItem for TrackTestItem {
    fn language(&self) -> &LanguageTag {
        &self.tag
    }
    fn is_original(&self) -> bool {
        self.original
    }
}

impl RoleItem for TrackTestItem {
    fn roles(&self) -> &[Role] {
        &self.roles
    }
}

impl TrackItem for TrackTestItem {
    fn track_type(&self) -> TrackType {
        self.track_type
    }
}

struct PresentationTestItem {
    id: String,
    tag: LanguageTag,
    kind: Option<PresentationKind>,
    roles: Vec<Role>,
    original: bool,
}

impl LanguageItem for PresentationTestItem {
    fn language(&self) -> &LanguageTag {
        &self.tag
    }
    fn is_original(&self) -> bool {
        self.original
    }
}

impl RoleItem for PresentationTestItem {
    fn roles(&self) -> &[Role] {
        &self.roles
    }
}

impl PresentationItem for PresentationTestItem {
    fn kind(&self) -> Option<PresentationKind> {
        self.kind
    }
}

#[derive(Clone)]
struct SelectTestTrack {
    id: String,
    tag: LanguageTag,
    roles: Vec<Role>,
    default: bool,
    original: bool,
}

impl LanguageItem for SelectTestTrack {
    fn language(&self) -> &LanguageTag {
        &self.tag
    }
    fn is_original(&self) -> bool {
        self.original
    }
}

impl RoleItem for SelectTestTrack {
    fn roles(&self) -> &[Role] {
        &self.roles
    }
}

impl SelectableTrack for SelectTestTrack {
    type Id = String;
    fn id(&self) -> Self::Id {
        self.id.clone()
    }
    fn is_default(&self) -> bool {
        self.default
    }
}

/// The harness's stand-in for a real localised-name collation (the
/// fixture schema's own words: "the harness's compare_groups compares
/// collation_keys as plain strings then the subtag"). Real callers pass
/// a closure backed by platform locale data instead — this crate holds
/// no name data of its own.
fn compare_groups_for<'a>(
    collation_keys: &'a HashMap<String, String>,
) -> impl Fn(&str, &str) -> Ordering + 'a {
    move |a: &str, b: &str| {
        let ka = collation_keys.get(a).map(String::as_str).unwrap_or(a);
        let kb = collation_keys.get(b).map(String::as_str).unwrap_or(b);
        ka.cmp(kb).then_with(|| a.cmp(b))
    }
}

// ---------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------

#[test]
fn every_conformance_case_passes() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/bcp47-language-policy-v1.json"
    ))
    .expect("could not read tests/fixtures/bcp47-language-policy-v1.json");

    // Parsed twice, deliberately: once loosely (`serde_json::Value`) so
    // this harness can check the fixture's own SHAPE (no unknown
    // section, no section this harness needs left empty, no case missing
    // a field the schema requires — policy 8.1) independently of
    // whichever Rust types happen to make a value optional; once
    // strictly (`Fixtures`) for the typed data every case below actually
    // runs against.
    let raw_value: Value =
        serde_json::from_str(&raw).expect("fixture file is not valid JSON at all");
    let top_level = raw_value
        .as_object()
        .expect("fixture file's top level is not a JSON object");
    for key in top_level.keys() {
        if key == "$schema"
            || key == "policy"
            || key == "policy_version"
            || key == "fixtures_version"
            || key == "data_version"
        {
            continue;
        }
        assert!(
            KNOWN_SECTIONS.contains(&key.as_str()),
            "fixture file has a section {key:?} this harness does not know how to run — \
             policy 8.1 requires failing on an unknown section, not silently ignoring it"
        );
    }
    for section in KNOWN_SECTIONS {
        let arr = top_level
            .get(*section)
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("fixture file has no array section {section:?}"));
        assert!(
            !arr.is_empty(),
            "fixture section {section:?} is empty — policy 8.1 requires failing on an empty \
             section this harness needs, not reporting success having run nothing"
        );
    }

    let fixtures: Fixtures =
        serde_json::from_str(&raw).expect("fixture file did not match the expected schema shape");

    assert_eq!(fixtures.policy, "MWBM-MEDIA-LANG");
    assert_eq!(fixtures.policy_version, "1.0.0");
    assert_eq!(
        fixtures.data_version,
        embedded_data_version(),
        "fixtures were computed against a different reference-data version than this crate embeds"
    );

    let mut failures: Vec<String> = Vec::new();
    let mut cases_run: usize = 0;
    let cases_in_file = fixtures.canonicalise.len()
        + fixtures.legacy_three_letter.len()
        + fixtures.iso639_2_write.len()
        + fixtures.posix_locale.len()
        + fixtures.sidecar_name.len()
        + fixtures.canonical_order.len()
        + fixtures.track_order.len()
        + fixtures.presentation_order.len()
        + fixtures.subtitle_menu.len()
        + fixtures.label.len()
        + fixtures.match_cases.len()
        + fixtures.auto_select_audio.len()
        + fixtures.auto_select_subtitle.len();

    // -- canonicalise (LANG-001, LANG-026) --------------------------------
    // Plus a stability check added in the policy's second revision: for
    // every case whose expected answer is a real tag (not malformed),
    // canonicalising that answer AGAIN must return it completely
    // unchanged — canonical form is meant to be a fixed point, not
    // something that can still be transformed further. Tracked with its
    // own counter, asserted against the file's own count of such cases
    // further down, exactly like the section-skip guard above.
    let mut stability_checks_run: usize = 0;
    for (idx, case) in fixtures.canonicalise.iter().enumerate() {
        require_present(
            &raw_value["canonicalise"][idx],
            &["id", "rules", "input", "expected", "kind"],
            &case_id_hint(&raw_value["canonicalise"][idx], "canonicalise"),
        );
        cases_run += 1;
        let got = canonicalise(&case.input);
        let got_kind = match got.kind {
            meedya_lang::TagKind::Ordinary => "ordinary",
            meedya_lang::TagKind::Grandfathered => "grandfathered",
            meedya_lang::TagKind::PrivateUse => "privateuse",
            meedya_lang::TagKind::Malformed => "malformed",
        };
        let got_expected = if got.is_malformed() {
            None
        } else {
            Some(got.tag.clone())
        };
        if got_expected != case.expected || got_kind != case.kind {
            failures.push(format!(
                "{}: canonicalise({:?}) = ({:?}, {:?}), expected ({:?}, {:?})",
                case.id, case.input, got_expected, got_kind, case.expected, case.kind
            ));
        }

        if let Some(expected) = &case.expected {
            stability_checks_run += 1;
            let restated = canonicalise(expected);
            if restated.is_malformed() || &restated.tag != expected {
                failures.push(format!(
                    "{} (stability): canonicalise({:?}) = {:?}, expected it back unchanged \
                     — canonical form must be a fixed point",
                    case.id,
                    expected,
                    if restated.is_malformed() {
                        None
                    } else {
                        Some(&restated.tag)
                    }
                ));
            }
        }
    }
    let stability_checks_in_file = fixtures
        .canonicalise
        .iter()
        .filter(|c| c.expected.is_some())
        .count();

    // -- legacy_three_letter (LANG-002, LANG-003) -------------------------
    for (idx, case) in fixtures.legacy_three_letter.iter().enumerate() {
        require_present(
            &raw_value["legacy_three_letter"][idx],
            &["id", "rules", "input", "expected"],
            &case_id_hint(
                &raw_value["legacy_three_letter"][idx],
                "legacy_three_letter",
            ),
        );
        cases_run += 1;
        let got = from_legacy_three_letter(&case.input).map(|t| t.tag);
        if got != case.expected {
            failures.push(format!(
                "{}: from_legacy_three_letter({:?}) = {:?}, expected {:?}",
                case.id, case.input, got, case.expected
            ));
        }
    }

    // -- iso639_2_write (TRACK-070) ----------------------------------------
    for (idx, case) in fixtures.iso639_2_write.iter().enumerate() {
        require_present(
            &raw_value["iso639_2_write"][idx],
            &["id", "rules", "input", "expected"],
            &case_id_hint(&raw_value["iso639_2_write"][idx], "iso639_2_write"),
        );
        cases_run += 1;
        let tag = canonicalise(&case.input);
        let got = iso639_2_write(&tag);
        if got.bibliographic != case.expected.b || got.terminology != case.expected.t {
            failures.push(format!(
                "{}: iso639_2_write({:?}) = {{b: {:?}, t: {:?}}}, expected {{b: {:?}, t: {:?}}}",
                case.id,
                case.input,
                got.bibliographic,
                got.terminology,
                case.expected.b,
                case.expected.t
            ));
        }
    }

    // -- posix_locale (LANG-004) ------------------------------------------
    for (idx, case) in fixtures.posix_locale.iter().enumerate() {
        require_present(
            &raw_value["posix_locale"][idx],
            &["id", "rules", "input", "expected"],
            &case_id_hint(&raw_value["posix_locale"][idx], "posix_locale"),
        );
        cases_run += 1;
        let got = from_posix_locale(&case.input).map(|t| t.tag);
        if got != case.expected {
            failures.push(format!(
                "{}: from_posix_locale({:?}) = {:?}, expected {:?}",
                case.id, case.input, got, case.expected
            ));
        }
    }

    // -- sidecar_name (TEXT-030) -------------------------------------------
    for (idx, case) in fixtures.sidecar_name.iter().enumerate() {
        let raw_case = &raw_value["sidecar_name"][idx];
        let id_hint = case_id_hint(raw_case, "sidecar_name");
        cases_run += 1;
        match case {
            SidecarCase::Build {
                id,
                stem,
                tag,
                roles,
                extension,
                number,
                expected,
                error,
            } => {
                require_present(
                    raw_case,
                    &[
                        "id",
                        "rules",
                        "mode",
                        "stem",
                        "tag",
                        "roles",
                        "extension",
                        "number",
                        "expected",
                    ],
                    &id_hint,
                );
                let roles = roles_from_strs(roles);
                let got = build_sidecar_name(stem, tag, &roles, extension, *number);
                if *error {
                    if got.is_ok() {
                        failures.push(format!(
                            "{id}: build_sidecar_name(.., number={number:?}) = {got:?}, \
                             expected an error (this case carries error: true)"
                        ));
                    }
                } else {
                    match (&got, expected) {
                        (Ok(name), Some(exp)) if name == exp => {}
                        _ => failures.push(format!(
                            "{id}: build_sidecar_name({stem:?}, {tag:?}, ..) = {got:?}, expected \
                             {expected:?}"
                        )),
                    }
                }
            }
            SidecarCase::Parse {
                id,
                stem,
                filename,
                expected,
            } => {
                require_present(
                    raw_case,
                    &["id", "rules", "mode", "stem", "filename", "expected"],
                    &id_hint,
                );
                let got = parse_sidecar_name(stem, filename);
                match (&got, expected) {
                    (None, None) => {}
                    (Some(g), Some(e)) => {
                        let expected_roles = roles_from_strs(&e.roles);
                        if g.tag != e.tag
                            || g.unrecognised != e.unrecognised
                            || g.roles != expected_roles
                            || g.number != e.number
                            || g.extension != e.extension
                        {
                            failures.push(format!(
                                "{id}: parse_sidecar_name({stem:?}, {filename:?}) = {g:?}, \
                                 expected {e:?}"
                            ));
                        }
                    }
                    _ => {
                        failures.push(format!(
                            "{id}: parse_sidecar_name({stem:?}, {filename:?}) = {got:?}, \
                             expected {expected:?}"
                        ));
                    }
                }
            }
        }
    }

    // -- canonical_order (LANG-010 to LANG-027) ---------------------------
    for (idx, case) in fixtures.canonical_order.iter().enumerate() {
        require_present(
            &raw_value["canonical_order"][idx],
            &["id", "rules", "description", "items", "expected"],
            &case_id_hint(&raw_value["canonical_order"][idx], "canonical_order"),
        );
        cases_run += 1;
        let mut items: Vec<OrderTestItem> = case
            .items
            .iter()
            .map(|it| OrderTestItem {
                id: it.id.clone().unwrap_or_else(|| it.tag.clone()),
                tag: canonicalise(&it.tag),
                original: it.original.unwrap_or(false),
            })
            .collect();
        sort_canonical(&mut items);
        let got: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        if got != case.expected {
            failures.push(format!(
                "{}: canonical order = {:?}, expected {:?}",
                case.id, got, case.expected
            ));
        }
    }

    // -- track_order (TRACK-050, TRACK-060) --------------------------------
    for (idx, case) in fixtures.track_order.iter().enumerate() {
        require_present(
            &raw_value["track_order"][idx],
            &["id", "rules", "description", "tracks", "expected"],
            &case_id_hint(&raw_value["track_order"][idx], "track_order"),
        );
        cases_run += 1;
        let mut tracks: Vec<TrackTestItem> = case
            .tracks
            .iter()
            .map(|t| TrackTestItem {
                id: t.id.clone(),
                tag: canonicalise(&t.tag),
                track_type: track_type_from_str(&t.kind),
                roles: roles_from_strs(&t.roles),
                original: t.original.unwrap_or(false),
            })
            .collect();
        sort_tracks(&mut tracks);
        let got: Vec<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        if got != case.expected {
            failures.push(format!(
                "{}: track order = {:?}, expected {:?}",
                case.id, got, case.expected
            ));
        }
    }

    // -- presentation_order (UI-020 to UI-050) -----------------------------
    for (idx, case) in fixtures.presentation_order.iter().enumerate() {
        require_present(
            &raw_value["presentation_order"][idx],
            &[
                "id",
                "rules",
                "description",
                "preferences",
                "accessibility",
                "display_names",
                "collation_keys",
                "items",
                "expected",
            ],
            &case_id_hint(&raw_value["presentation_order"][idx], "presentation_order"),
        );
        cases_run += 1;
        let mut items: Vec<PresentationTestItem> = case
            .items
            .iter()
            .map(|it| PresentationTestItem {
                id: it.id.clone(),
                tag: canonicalise(&it.tag),
                kind: it.kind.as_deref().map(presentation_kind_from_str),
                roles: roles_from_strs(&it.roles),
                original: it.original.unwrap_or(false),
            })
            .collect();
        let context = PresentationContext {
            preferences: preferences_from_strs(&case.preferences),
            accessibility: case.accessibility.resolve(),
        };
        sort_for_presentation(
            &mut items,
            &context,
            compare_groups_for(&case.collation_keys),
        );
        let _ = &case.display_names; // supplied by the fixture; names themselves are never checked here (the crate holds no name data — see PresentationCase's schema doc)
        let got: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        if got != case.expected {
            failures.push(format!(
                "{}: presentation order = {:?}, expected {:?}",
                case.id, got, case.expected
            ));
        }
    }

    // -- subtitle_menu (UI-060) --------------------------------------------
    for (idx, case) in fixtures.subtitle_menu.iter().enumerate() {
        require_present(
            &raw_value["subtitle_menu"][idx],
            &[
                "id",
                "rules",
                "description",
                "preferences",
                "accessibility",
                "display_names",
                "collation_keys",
                "items",
                "expected",
            ],
            &case_id_hint(&raw_value["subtitle_menu"][idx], "subtitle_menu"),
        );
        cases_run += 1;
        let mut items: Vec<PresentationTestItem> = case
            .items
            .iter()
            .map(|it| PresentationTestItem {
                id: it.id.clone(),
                tag: canonicalise(&it.tag),
                kind: it.kind.as_deref().map(presentation_kind_from_str),
                roles: roles_from_strs(&it.roles),
                original: it.original.unwrap_or(false),
            })
            .collect();
        let context = PresentationContext {
            preferences: preferences_from_strs(&case.preferences),
            accessibility: case.accessibility.resolve(),
        };
        let menu = subtitle_menu(
            &mut items,
            &context,
            compare_groups_for(&case.collation_keys),
        );
        let got: Vec<String> = menu
            .iter()
            .map(|entry| match entry {
                MenuEntry::Off => "off".to_string(),
                MenuEntry::Track(t) => t.id.clone(),
            })
            .collect();
        if got != case.expected {
            failures.push(format!(
                "{}: subtitle menu = {:?}, expected {:?}",
                case.id, got, case.expected
            ));
        }
    }

    // -- label (UI-070) ------------------------------------------------------
    for (idx, case) in fixtures.label.iter().enumerate() {
        require_present(
            &raw_value["label"][idx],
            &[
                "id",
                "rules",
                "type",
                "language_name",
                "roles",
                "role_names",
                "channels",
                "expected",
            ],
            &case_id_hint(&raw_value["label"][idx], "label"),
        );
        cases_run += 1;
        let track_type = match case.kind.as_str() {
            "audio" => TrackType::Audio,
            "subtitle" => TrackType::Subtitle,
            other => panic!("label case {} has an unknown type {other:?}", case.id),
        };
        let roles = roles_from_strs(&case.roles);
        let role_names = &case.role_names;
        let got = label(
            track_type,
            &case.language_name,
            &roles,
            |r| {
                let key = match r {
                    Role::Alternate => "alternate",
                    Role::AudioDescription => "audio_description",
                    Role::Commentary => "commentary",
                    Role::Sdh => "sdh",
                    Role::Forced => "forced",
                    Role::Other => "other",
                };
                role_names
                    .get(key)
                    .unwrap_or_else(|| {
                        panic!("label case {} has no role_names entry for {key}", case.id)
                    })
                    .clone()
            },
            case.channels.as_deref(),
        );
        if got != case.expected {
            failures.push(format!(
                "{}: label = {:?}, expected {:?}",
                case.id, got, case.expected
            ));
        }
    }

    // -- match (MATCH-010 to MATCH-040) --------------------------------------
    for (idx, case) in fixtures.match_cases.iter().enumerate() {
        require_present(
            &raw_value["match"][idx],
            &["id", "rules", "preference", "candidate", "expected"],
            &case_id_hint(&raw_value["match"][idx], "match"),
        );
        cases_run += 1;
        let pref = canonicalise(&case.preference);
        let cand = canonicalise(&case.candidate);
        let got = match_tags(&pref, &cand);
        let expected_level = match_level_from_str(&case.expected.level);
        if got.level != expected_level || got.distance != case.expected.distance {
            failures.push(format!(
                "{}: match({:?}, {:?}) = ({:?}, {}), expected ({:?}, {})",
                case.id,
                case.preference,
                case.candidate,
                got.level,
                got.distance,
                expected_level,
                case.expected.distance
            ));
        }
    }

    // -- auto_select_audio (AUTO-010, AUTO-020, AUTO-040) --------------------
    for (idx, case) in fixtures.auto_select_audio.iter().enumerate() {
        require_present(
            &raw_value["auto_select_audio"][idx],
            &[
                "id",
                "rules",
                "description",
                "preferences",
                "accessibility",
                "tracks",
                "expected",
            ],
            &case_id_hint(&raw_value["auto_select_audio"][idx], "auto_select_audio"),
        );
        cases_run += 1;
        let tracks: Vec<SelectTestTrack> = case
            .tracks
            .iter()
            .map(|t| SelectTestTrack {
                id: t.id.clone(),
                tag: canonicalise(&t.tag),
                roles: roles_from_strs(&t.roles),
                default: t.default.unwrap_or(false),
                original: t.original.unwrap_or(false),
            })
            .collect();
        let preferences = preferences_from_strs(&case.preferences);
        let accessibility = case.accessibility.resolve();

        let got = select_audio(&tracks, &preferences, &accessibility);
        if case.error {
            if got.is_ok() {
                failures.push(format!(
                    "{}: select_audio = {:?}, expected an error (this case carries error: true)",
                    case.id, got
                ));
            }
        } else {
            match &got {
                Ok(id) if id.as_deref() == case.expected.as_deref() => {}
                _ => failures.push(format!(
                    "{}: select_audio = {:?}, expected Ok({:?})",
                    case.id, got, case.expected
                )),
            }

            // AUTO-010: the same answer whatever order the tracks are
            // listed in (only checked for non-error cases; a duplicate-id
            // error is order-independent by construction, since the
            // uniqueness check does not care where the duplicate sits).
            let mut reversed = tracks.clone();
            reversed.reverse();
            let got_reversed = select_audio(&reversed, &preferences, &accessibility);
            if got_reversed != got {
                failures.push(format!(
                    "{} (reversed): select_audio = {:?}, expected the same answer as forward \
                     order ({:?})",
                    case.id, got_reversed, got
                ));
            }
        }
    }

    // -- auto_select_subtitle (AUTO-010, AUTO-030, AUTO-040) -----------------
    for (idx, case) in fixtures.auto_select_subtitle.iter().enumerate() {
        require_present(
            &raw_value["auto_select_subtitle"][idx],
            &[
                "id",
                "rules",
                "description",
                "mode",
                "preferences",
                "accessibility",
                "audio",
                "tracks",
                "expected",
            ],
            &case_id_hint(
                &raw_value["auto_select_subtitle"][idx],
                "auto_select_subtitle",
            ),
        );
        cases_run += 1;
        let tracks: Vec<SelectTestTrack> = case
            .tracks
            .iter()
            .map(|t| SelectTestTrack {
                id: t.id.clone(),
                tag: canonicalise(&t.tag),
                roles: roles_from_strs(&t.roles),
                default: t.default.unwrap_or(false),
                original: t.original.unwrap_or(false),
            })
            .collect();
        let preferences = preferences_from_strs(&case.preferences);
        let accessibility = case.accessibility.resolve();
        let mode = match case.mode.as_str() {
            "automatic" => SubtitleMode::Automatic,
            "always" => SubtitleMode::Always,
            "forced_only" => SubtitleMode::ForcedOnly,
            "off" => SubtitleMode::Off,
            other => panic!("case {} has an unknown mode {other:?}", case.id),
        };
        let audio = case.audio.as_ref().map(|a| canonicalise(a));

        let got = select_subtitle(&tracks, audio.as_ref(), &preferences, mode, &accessibility);
        if case.error {
            if got.is_ok() {
                failures.push(format!(
                    "{}: select_subtitle = {:?}, expected an error (this case carries error: \
                     true)",
                    case.id, got
                ));
            }
        } else {
            match &got {
                Ok(id) if id.as_deref() == case.expected.as_deref() => {}
                _ => failures.push(format!(
                    "{}: select_subtitle = {:?}, expected Ok({:?})",
                    case.id, got, case.expected
                )),
            }

            let mut reversed = tracks.clone();
            reversed.reverse();
            let got_reversed = select_subtitle(
                &reversed,
                audio.as_ref(),
                &preferences,
                mode,
                &accessibility,
            );
            if got_reversed != got {
                failures.push(format!(
                    "{} (reversed): select_subtitle = {:?}, expected the same answer as forward \
                     order ({:?})",
                    case.id, got_reversed, got
                ));
            }
        }
    }

    assert_eq!(
        cases_run, cases_in_file,
        "ran {cases_run} cases but the fixture file has {cases_in_file} — a section was skipped"
    );
    assert_eq!(
        stability_checks_run, stability_checks_in_file,
        "ran {stability_checks_run} canonical-form stability checks but the file has \
         {stability_checks_in_file} canonicalise cases with a non-null expected answer"
    );

    assert!(
        failures.is_empty(),
        "{} of {} conformance cases (including {} stability checks) failed:\n{}",
        failures.len(),
        cases_in_file + stability_checks_in_file,
        stability_checks_in_file,
        failures.join("\n")
    );
}
