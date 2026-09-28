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
// this crate's traits (`LanguageItem`, `RoleItem`, `TrackItem`,
// `PresentationItem`, `SelectableTrack`) from the fixture JSON, rather
// than reaching into the crate's internals — it exercises exactly the
// surface a real consumer (MeedyaDL, MeedyaManager) would use.
//
// Policy 8.1 requires this runner to FAIL — never quietly pass — on an
// unknown section, a missing or empty section it needs, or a case
// missing a field the schema requires. How that is made true here:
//
// * Each case is read on its own (`parse_section`), so a failure names
//   the case. Every case struct, and every object nested inside one,
//   carries `#[serde(deny_unknown_fields)]`, mirroring the schema's
//   `additionalProperties: false`: a field the schema does not allow —
//   an `error: true` flag in a section that has no refusal cases, say —
//   fails loudly instead of being ignored.
// * A field the schema requires but allows to be `null` is read with
//   `deserialize_with = "Option::deserialize"`. A plain `Option<T>` field
//   quietly reads a MISSING key as `None`, exactly like an explicit
//   `null`; with that attribute a missing key is an error. Before policy
//   revision 4 the nested `expected` object of a sidecar parse case, and
//   the `roles` of a selection track (`#[serde(default)]`), could be left
//   out and still pass — an independent review found both.
// * `error`, where the schema allows it, may only be `true` (its schema
//   `const`), and a case carrying it must expect `null`.
// * Required fields that may not be null are ordinary (non-`Option`)
//   fields, which serde already refuses to leave out.
// * A field the schema makes OPTIONAL but never nullable (a `description`,
//   a track's `original`, an accessibility preference ...) is read with
//   `default, deserialize_with = "present_not_null"`: absent is `None`,
//   an explicit `null` is refused. A plain `Option<T>` reads `null` as
//   `None`, exactly like an absent key, so until Codex's review r7 a null
//   the schema forbids passed unnoticed here. Every other wrong TYPE (a
//   number where a string belongs, `roles: null`, a fraction where a whole
//   number belongs) serde already refuses, because each field is typed.
// * Every struct this runner builds from a JSON object — each case, and
//   every object nested inside one (a track, an item, an `expected`
//   object, `accessibility`) — is read through `object`, `objects` or
//   `object_or_null`, which refuse anything but a JSON object. serde reads
//   a struct from a JSON LIST as well (its fields in order), so until the
//   stand-in review of revision 5 `accessibility: []` passed as "no
//   preferences" and `expected: ["eng", "eng"]` as `{b, t}`. Maps
//   (`display_names`, `role_names`, `collation_keys`) and lists already
//   refused the wrong kind of container.
// * The file's own top-level fields are checked too: `policy` and
//   `policy_version` exactly, `fixtures_version` as three dot-separated
//   numbers, `data_version` against the data this crate embeds, and
//   `$schema`, if present, as a string.
// * The `harness_refuses` tests at the bottom prove each of those by
//   running this same harness on a deliberately damaged copy of the case
//   file and requiring it to fail, for the stated reason.

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

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

fn case_id_hint(case: &Value, section: &str, index: usize) -> String {
    case.get("id")
        .and_then(Value::as_str)
        .map(|id| format!("{section}/{id}"))
        .unwrap_or_else(|| format!("{section}/<case {index}, no id>"))
}

/// Reads every case of one section into `T`, one case at a time, so a
/// failure names the case (serde cannot give a line number for some of
/// these shapes). Panics — failing the run — on the first case that does
/// not match `T`, which mirrors the schema.
fn parse_section<T: DeserializeOwned>(top: &Map<String, Value>, section: &str) -> Vec<T> {
    let cases = top
        .get(section)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("fixture file has no array section {section:?}"));
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            from_object(case.clone()).unwrap_or_else(|e| {
                panic!(
                    "{}: fixture case does not match the schema — {e}. Policy 8.1: a harness \
                     must fail, not quietly default or ignore, when a case lacks a field the \
                     schema requires or carries one it does not allow",
                    case_id_hint(case, section, index)
                )
            })
        })
        .collect()
}

/// `error` may only ever be `true` (the schema's `const`): a case that
/// should not be refused leaves the field out.
fn only_true<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    if bool::deserialize(deserializer)? {
        Ok(true)
    } else {
        Err(D::Error::custom(
            "`error` may only be true (the schema's const); leave it out rather than writing false",
        ))
    }
}

/// For a field the schema makes optional but never nullable: absent is
/// `None` (the field also carries `#[serde(default)]`), a real value is
/// `Some`, and an explicit `null` is refused. See the header comment for
/// why a plain `Option<T>` is not enough.
fn present_not_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        Some(value) => Ok(Some(value)),
        None => Err(D::Error::custom(
            "a field is null, which the schema does not allow here - leave it out instead",
        )),
    }
}

/// Reads `value` into `T` only when it is a JSON object. serde reads a
/// struct from a JSON list too (its fields in order), which the schema
/// never allows — see the header comment.
fn from_object<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    let kind = match &value {
        Value::Object(_) => return serde_json::from_value(value).map_err(|e| e.to_string()),
        Value::Array(_) => "a list",
        Value::Null => "null",
        Value::Bool(_) => "true/false",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
    };
    Err(format!("a JSON object belongs here, but this is {kind}"))
}

/// For a struct-typed field: the value must be a JSON object.
fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    from_object(Value::deserialize(deserializer)?).map_err(D::Error::custom)
}

/// For a list of structs: every item must be a JSON object. (The message
/// does not number the item: serde's own messages for a wrong field inside
/// an item do not either, and this runner's refusal tests match those
/// messages as they are.)
fn objects<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Vec::<Value>::deserialize(deserializer)?
        .into_iter()
        .map(|item| from_object(item).map_err(D::Error::custom))
        .collect()
}

/// For a struct-typed field the schema requires but allows to be `null`:
/// `null` is `None`, anything else must be a JSON object. (Like
/// `Option::deserialize`, a missing key is still an error, because the
/// field carries no `default`.)
fn object_or_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    match Option::<Value>::deserialize(deserializer)? {
        None => Ok(None),
        Some(value) => from_object(value).map(Some).map_err(D::Error::custom),
    }
}

/// A case that carries `error: true` must expect `null` (the schema's
/// `if error then expected: null`) — otherwise it is unclear what it tests.
fn require_null_expected_on_error(id: &str, error: bool, expected_is_null: bool) {
    assert!(
        !error || expected_is_null,
        "{id}: fixture case carries error: true but its expected answer is not null"
    );
}

// ---------------------------------------------------------------------
// Fixture file shape — one struct per case shape in the schema
// ---------------------------------------------------------------------

/// Every section name this harness knows how to run, in the order the
/// policy's own table (8.1) lists them. Used for two checks: that the
/// fixture file does not carry a section this harness has never heard of,
/// and that none of the sections this harness needs is missing or empty.
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

/// The top-level keys that describe the file rather than holding cases.
const METADATA_KEYS: &[&str] = &[
    "$schema",
    "policy",
    "policy_version",
    "fixtures_version",
    "data_version",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicaliseCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    input: String,
    #[serde(deserialize_with = "Option::deserialize")]
    expected: Option<String>,
    kind: String,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "present_not_null")]
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    input: String,
    #[serde(deserialize_with = "Option::deserialize")]
    expected: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PosixCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "present_not_null")]
    description: Option<String>,
    input: String,
    #[serde(deserialize_with = "Option::deserialize")]
    expected: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Iso639WriteExpected {
    b: String,
    t: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Iso639WriteCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "present_not_null")]
    description: Option<String>,
    input: String,
    #[serde(deserialize_with = "object")]
    expected: Iso639WriteExpected,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct SidecarExpected {
    #[serde(deserialize_with = "Option::deserialize")]
    tag: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    unrecognised: Option<String>,
    roles: Vec<String>,
    #[serde(deserialize_with = "Option::deserialize")]
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
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
enum SidecarCase {
    Build {
        id: String,
        #[allow(dead_code)]
        rules: Vec<String>,
        #[allow(dead_code)]
        #[serde(default, deserialize_with = "present_not_null")]
        description: Option<String>,
        stem: String,
        tag: String,
        roles: Vec<String>,
        extension: String,
        #[serde(deserialize_with = "Option::deserialize")]
        number: Option<i64>,
        #[serde(deserialize_with = "Option::deserialize")]
        expected: Option<String>,
        #[serde(default, deserialize_with = "only_true")]
        error: bool,
    },
    Parse {
        id: String,
        #[allow(dead_code)]
        rules: Vec<String>,
        stem: String,
        filename: String,
        #[serde(deserialize_with = "object_or_null")]
        expected: Option<SidecarExpected>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderItem {
    #[serde(default, deserialize_with = "present_not_null")]
    id: Option<String>,
    tag: String,
    #[serde(default, deserialize_with = "present_not_null")]
    original: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    description: String,
    #[serde(deserialize_with = "objects")]
    items: Vec<OrderItem>,
    expected: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackDef {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    tag: String,
    roles: Vec<String>,
    #[serde(default, deserialize_with = "present_not_null")]
    original: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackOrderCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    description: String,
    #[serde(deserialize_with = "objects")]
    tracks: Vec<TrackDef>,
    expected: Vec<String>,
}

/// Accessibility preferences; an absent key means false (the schema's own
/// convention for these test cases).
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AccessibilityDef {
    #[serde(default, deserialize_with = "present_not_null")]
    audio_description: Option<bool>,
    #[serde(default, deserialize_with = "present_not_null")]
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
#[serde(deny_unknown_fields)]
struct PresentationItemDef {
    id: String,
    tag: String,
    #[serde(rename = "type")]
    #[serde(default, deserialize_with = "present_not_null")]
    kind: Option<String>,
    // Optional in the schema: "Absent means none."
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default, deserialize_with = "present_not_null")]
    original: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    description: String,
    preferences: Vec<String>,
    #[serde(deserialize_with = "object")]
    accessibility: AccessibilityDef,
    /// Present only to show that selection changes nothing (UI-050); the
    /// ordering function takes no selection input, so it is never read.
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "present_not_null")]
    selected: Option<String>,
    #[allow(dead_code)]
    display_names: HashMap<String, String>,
    collation_keys: HashMap<String, String>,
    #[serde(deserialize_with = "objects")]
    items: Vec<PresentationItemDef>,
    expected: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "present_not_null")]
    description: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    language_name: String,
    roles: Vec<String>,
    role_names: HashMap<String, String>,
    #[serde(deserialize_with = "Option::deserialize")]
    channels: Option<String>,
    expected: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchExpected {
    level: String,
    distance: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "present_not_null")]
    description: Option<String>,
    preference: String,
    candidate: String,
    #[serde(deserialize_with = "object")]
    expected: MatchExpected,
}

/// One track in a selection case. `roles` is required by the schema and so
/// is required here: before policy revision 4 it carried
/// `#[serde(default)]`, and a track that left it out quietly counted as
/// having no roles.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectTrackDef {
    id: String,
    tag: String,
    roles: Vec<String>,
    #[serde(default, deserialize_with = "present_not_null")]
    default: Option<bool>,
    #[serde(default, deserialize_with = "present_not_null")]
    original: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoAudioCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    description: String,
    preferences: Vec<String>,
    #[serde(deserialize_with = "object")]
    accessibility: AccessibilityDef,
    #[serde(deserialize_with = "objects")]
    tracks: Vec<SelectTrackDef>,
    #[serde(deserialize_with = "Option::deserialize")]
    expected: Option<String>,
    #[serde(default, deserialize_with = "only_true")]
    error: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoSubtitleCase {
    id: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    #[allow(dead_code)]
    description: String,
    mode: String,
    preferences: Vec<String>,
    #[serde(deserialize_with = "object")]
    accessibility: AccessibilityDef,
    #[serde(deserialize_with = "Option::deserialize")]
    audio: Option<String>,
    #[serde(deserialize_with = "objects")]
    tracks: Vec<SelectTrackDef>,
    #[serde(deserialize_with = "Option::deserialize")]
    expected: Option<String>,
    #[serde(default, deserialize_with = "only_true")]
    error: bool,
}

// ---------------------------------------------------------------------
// Small helpers shared by several sections
// ---------------------------------------------------------------------

fn role_from_str(s: &str) -> Role {
    s.parse()
        .unwrap_or_else(|_| panic!("fixture uses an unknown role {s:?}"))
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

fn select_tracks_from(defs: &[SelectTrackDef]) -> Vec<SelectTestTrack> {
    defs.iter()
        .map(|t| SelectTestTrack {
            id: t.id.clone(),
            tag: canonicalise(&t.tag),
            roles: roles_from_strs(&t.roles),
            default: t.default.unwrap_or(false),
            original: t.original.unwrap_or(false),
        })
        .collect()
}

fn presentation_items_from(defs: &[PresentationItemDef]) -> Vec<PresentationTestItem> {
    defs.iter()
        .map(|it| PresentationTestItem {
            id: it.id.clone(),
            tag: canonicalise(&it.tag),
            kind: it.kind.as_deref().map(presentation_kind_from_str),
            roles: roles_from_strs(&it.roles),
            original: it.original.unwrap_or(false),
        })
        .collect()
}

/// The harness's stand-in for a real localised-name collation: it compares
/// the case's `collation_keys` as plain strings, and NOTHING else. Equal
/// keys compare `Equal`, and it is the crate's job to break that tie by
/// primary language code (UI-040) — as it is for a real collator, which
/// compares names only. (Before policy revision 4 this closure broke the
/// tie itself, which hid a crate that did not; `present-23` now checks the
/// crate.) Real callers pass a closure backed by platform locale data; the
/// crate holds no name data of its own.
fn compare_names_for<'a>(
    collation_keys: &'a HashMap<String, String>,
) -> impl Fn(&str, &str) -> Ordering + 'a {
    move |a: &str, b: &str| {
        let ka = collation_keys.get(a).map(String::as_str).unwrap_or(a);
        let kb = collation_keys.get(b).map(String::as_str).unwrap_or(b);
        ka.cmp(kb)
    }
}

// ---------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------

/// What one run of the harness found.
struct Report {
    failures: Vec<String>,
    cases_in_file: usize,
    stability_checks_in_file: usize,
}

fn fixture_text() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/bcp47-language-policy-v1.json"
    ))
    .expect("could not read tests/fixtures/bcp47-language-policy-v1.json")
}

/// Runs every case in `raw` (the text of a fixture file). Panics — failing
/// the run outright — on anything wrong with the FILE (policy 8.1);
/// returns every case whose ANSWER was wrong in `Report::failures`.
fn run_conformance(raw: &str) -> Report {
    let raw_value: Value =
        serde_json::from_str(raw).expect("fixture file is not valid JSON at all");
    let top = raw_value
        .as_object()
        .expect("fixture file's top level is not a JSON object");
    for key in top.keys() {
        assert!(
            METADATA_KEYS.contains(&key.as_str()) || KNOWN_SECTIONS.contains(&key.as_str()),
            "fixture file has a section {key:?} this harness does not know how to run — \
             policy 8.1 requires failing on an unknown section, not silently ignoring it"
        );
    }
    for section in KNOWN_SECTIONS {
        let arr = top
            .get(*section)
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("fixture file has no array section {section:?}"));
        assert!(
            !arr.is_empty(),
            "fixture section {section:?} is empty — policy 8.1 requires failing on an empty \
             section this harness needs, not reporting success having run nothing"
        );
    }

    let text = |key: &str| {
        top.get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("fixture file has no string {key:?}"))
    };
    assert_eq!(text("policy"), "MWBM-MEDIA-LANG");
    assert_eq!(text("policy_version"), "1.0.0");
    // The schema's pattern for a version: three dot-separated numbers.
    // (Until Codex's review r7 this field was only required to be a
    // string, so "1.0" passed.)
    let fixtures_version = text("fixtures_version");
    let parts: Vec<&str> = fixtures_version.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "fixtures_version {fixtures_version:?} is not a version of three dot-separated numbers"
    );
    // `$schema` is optional, but when present the schema says it is a string.
    if let Some(schema) = top.get("$schema") {
        assert!(
            schema.is_string(),
            "the fixture file's \"$schema\" must be a string, not {schema}"
        );
    }
    assert_eq!(
        text("data_version"),
        embedded_data_version(),
        "fixtures were computed against a different reference-data version than this crate embeds"
    );

    let canonicalise_cases: Vec<CanonicaliseCase> = parse_section(top, "canonicalise");
    let legacy_cases: Vec<LegacyCase> = parse_section(top, "legacy_three_letter");
    let iso_cases: Vec<Iso639WriteCase> = parse_section(top, "iso639_2_write");
    let posix_cases: Vec<PosixCase> = parse_section(top, "posix_locale");
    let sidecar_cases: Vec<SidecarCase> = parse_section(top, "sidecar_name");
    let order_cases: Vec<OrderCase> = parse_section(top, "canonical_order");
    let track_cases: Vec<TrackOrderCase> = parse_section(top, "track_order");
    let presentation_cases: Vec<PresentationCase> = parse_section(top, "presentation_order");
    let menu_cases: Vec<PresentationCase> = parse_section(top, "subtitle_menu");
    let label_cases: Vec<LabelCase> = parse_section(top, "label");
    let match_cases: Vec<MatchCase> = parse_section(top, "match");
    let audio_cases: Vec<AutoAudioCase> = parse_section(top, "auto_select_audio");
    let subtitle_cases: Vec<AutoSubtitleCase> = parse_section(top, "auto_select_subtitle");

    let mut failures: Vec<String> = Vec::new();
    let mut cases_run: usize = 0;
    let cases_in_file: usize = KNOWN_SECTIONS
        .iter()
        .map(|s| top[*s].as_array().map_or(0, Vec::len))
        .sum();

    // -- canonicalise (LANG-001, LANG-026) --------------------------------
    // Plus a stability check: for every case whose expected answer is a
    // real tag (not malformed), canonicalising that answer AGAIN must
    // return it completely unchanged — canonical form is a fixed point.
    // Tracked with its own counter, asserted against the file's own count
    // of such cases further down, exactly like the section-skip guard.
    let mut stability_checks_run: usize = 0;
    for case in &canonicalise_cases {
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
    let stability_checks_in_file = canonicalise_cases
        .iter()
        .filter(|c| c.expected.is_some())
        .count();

    // -- legacy_three_letter (LANG-002, LANG-003) -------------------------
    for case in &legacy_cases {
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
    for case in &iso_cases {
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
    for case in &posix_cases {
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
    for case in &sidecar_cases {
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
                ..
            } => {
                require_null_expected_on_error(id, *error, expected.is_none());
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
                    assert!(
                        expected.is_some(),
                        "{id}: a build case that is not refused must expect a file name, not null"
                    );
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
                ..
            } => {
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
    for case in &order_cases {
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
    for case in &track_cases {
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
    for case in &presentation_cases {
        cases_run += 1;
        let mut items = presentation_items_from(&case.items);
        let context = PresentationContext {
            preferences: preferences_from_strs(&case.preferences),
            accessibility: case.accessibility.resolve(),
        };
        sort_for_presentation(
            &mut items,
            &context,
            compare_names_for(&case.collation_keys),
        );
        let got: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        if got != case.expected {
            failures.push(format!(
                "{}: presentation order = {:?}, expected {:?}",
                case.id, got, case.expected
            ));
        }
    }

    // -- subtitle_menu (UI-060) --------------------------------------------
    for case in &menu_cases {
        cases_run += 1;
        let mut items = presentation_items_from(&case.items);
        let context = PresentationContext {
            preferences: preferences_from_strs(&case.preferences),
            accessibility: case.accessibility.resolve(),
        };
        let menu = subtitle_menu(
            &mut items,
            &context,
            compare_names_for(&case.collation_keys),
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
    for case in &label_cases {
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
                role_names
                    .get(r.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "label case {} has no role_names entry for {}",
                            case.id,
                            r.as_str()
                        )
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
    for case in &match_cases {
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
    for case in &audio_cases {
        cases_run += 1;
        require_null_expected_on_error(&case.id, case.error, case.expected.is_none());
        let tracks = select_tracks_from(&case.tracks);
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
    for case in &subtitle_cases {
        cases_run += 1;
        require_null_expected_on_error(&case.id, case.error, case.expected.is_none());
        let tracks = select_tracks_from(&case.tracks);
        let preferences = preferences_from_strs(&case.preferences);
        let accessibility = case.accessibility.resolve();
        let mode: SubtitleMode = case
            .mode
            .parse()
            .unwrap_or_else(|_| panic!("case {} has an unknown mode {:?}", case.id, case.mode));
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

    Report {
        failures,
        cases_in_file,
        stability_checks_in_file,
    }
}

#[test]
fn every_conformance_case_passes() {
    let report = run_conformance(&fixture_text());
    assert!(
        report.failures.is_empty(),
        "{} of {} conformance cases (including {} stability checks) failed:\n{}",
        report.failures.len(),
        report.cases_in_file + report.stability_checks_in_file,
        report.stability_checks_in_file,
        report.failures.join("\n")
    );
}

// ---------------------------------------------------------------------
// The harness refuses a damaged case file (policy 8.1)
// ---------------------------------------------------------------------
//
// Each test damages a copy of the real case file in one way and requires
// the harness to FAIL, for the stated reason (`should_panic(expected =
// …)` checks the message). If a future change made the harness quietly
// accept any of these, the test would fail — which is the point.

mod harness_refuses {
    use super::*;

    /// The real case file with one case edited by `edit`.
    fn with_case(section: &str, id: &str, edit: impl FnOnce(&mut Value)) -> String {
        let mut fixtures: Value = serde_json::from_str(&fixture_text()).unwrap();
        let case = fixtures[section]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["id"] == id)
            .unwrap_or_else(|| panic!("no case {section}/{id} to damage"));
        edit(case);
        serde_json::to_string(&fixtures).unwrap()
    }

    /// The real case file with its top level edited by `edit`.
    fn with_top(edit: impl FnOnce(&mut Map<String, Value>)) -> String {
        let mut fixtures: Value = serde_json::from_str(&fixture_text()).unwrap();
        edit(fixtures.as_object_mut().unwrap());
        serde_json::to_string(&fixtures).unwrap()
    }

    fn remove(value: &mut Value, key: &str) {
        assert!(
            value.as_object_mut().unwrap().remove(key).is_some(),
            "no {key} to remove"
        );
    }

    #[test]
    fn the_undamaged_file_is_accepted() {
        // The control: the same path the damaged copies take, undamaged.
        let report = run_conformance(&with_top(|_| {}));
        assert!(report.failures.is_empty(), "{:?}", report.failures);
    }

    #[test]
    #[should_panic(
        expected = "sidecar_name/sidecar-07: fixture case does not match the schema — missing field `number`"
    )]
    fn a_parse_case_missing_its_expected_number() {
        run_conformance(&with_case("sidecar_name", "sidecar-07", |c| {
            remove(&mut c["expected"], "number")
        }));
    }

    #[test]
    #[should_panic(expected = "missing field `unrecognised`")]
    fn a_parse_case_missing_its_expected_unrecognised() {
        run_conformance(&with_case("sidecar_name", "sidecar-07", |c| {
            remove(&mut c["expected"], "unrecognised")
        }));
    }

    #[test]
    #[should_panic(expected = "missing field `tag`")]
    fn a_parse_case_missing_its_expected_tag() {
        run_conformance(&with_case("sidecar_name", "sidecar-07", |c| {
            remove(&mut c["expected"], "tag")
        }));
    }

    #[test]
    #[should_panic(
        expected = "auto_select_audio/audio-01: fixture case does not match the schema — missing field `roles`"
    )]
    fn a_selection_track_missing_its_roles() {
        run_conformance(&with_case("auto_select_audio", "audio-01", |c| {
            remove(&mut c["tracks"][0], "roles")
        }));
    }

    #[test]
    #[should_panic(expected = "missing field `roles`")]
    fn a_subtitle_selection_track_missing_its_roles() {
        run_conformance(&with_case("auto_select_subtitle", "subs-01", |c| {
            remove(&mut c["tracks"][0], "roles")
        }));
    }

    #[test]
    #[should_panic(
        expected = "legacy_three_letter/legacy-18: fixture case does not match the schema — unknown field `error`"
    )]
    fn an_error_flag_in_a_section_without_refusal_cases() {
        run_conformance(&with_case("legacy_three_letter", "legacy-18", |c| {
            c["error"] = Value::Bool(true);
        }));
    }

    #[test]
    #[should_panic(expected = "unknown field `error`")]
    fn an_error_flag_on_a_parse_case() {
        run_conformance(&with_case("sidecar_name", "sidecar-07", |c| {
            c["error"] = Value::Bool(true);
        }));
    }

    #[test]
    #[should_panic(expected = "unknown field `error`")]
    fn an_error_flag_in_the_match_section() {
        run_conformance(&with_case("match", "match-01", |c| {
            c["error"] = Value::Bool(true);
        }));
    }

    #[test]
    #[should_panic(expected = "`error` may only be true")]
    fn an_error_flag_set_to_false() {
        run_conformance(&with_case("auto_select_audio", "audio-01", |c| {
            c["error"] = Value::Bool(false);
        }));
    }

    #[test]
    #[should_panic(expected = "carries error: true but its expected answer is not null")]
    fn an_error_case_that_still_expects_an_answer() {
        run_conformance(&with_case("auto_select_audio", "audio-01", |c| {
            c["error"] = Value::Bool(true);
        }));
    }

    #[test]
    #[should_panic(expected = "missing field `channels`")]
    fn a_label_case_missing_its_nullable_channels() {
        run_conformance(&with_case("label", "label-02", |c| remove(c, "channels")));
    }

    #[test]
    #[should_panic(expected = "missing field `audio`")]
    fn a_subtitle_case_missing_its_nullable_audio() {
        run_conformance(&with_case("auto_select_subtitle", "subs-15", |c| {
            remove(c, "audio")
        }));
    }

    #[test]
    #[should_panic(expected = "unknown field `lang`")]
    fn an_unknown_field_inside_a_track() {
        run_conformance(&with_case("track_order", "tracks-01", |c| {
            c["tracks"][0]["lang"] = Value::String("en".into());
        }));
    }

    #[test]
    #[should_panic(expected = "missing field `t`")]
    fn a_write_case_missing_one_form() {
        run_conformance(&with_case("iso639_2_write", "write-01", |c| {
            remove(&mut c["expected"], "t")
        }));
    }

    #[test]
    #[should_panic(expected = "does not know how to run")]
    fn an_unknown_section() {
        run_conformance(&with_top(|top| {
            top.insert(
                "frobnicate".into(),
                serde_json::json!([{ "id": "frob-01" }]),
            );
        }));
    }

    #[test]
    #[should_panic(expected = "fixture section \"label\" is empty")]
    fn an_empty_section() {
        run_conformance(&with_top(|top| {
            top.insert("label".into(), serde_json::json!([]));
        }));
    }

    #[test]
    #[should_panic(expected = "fixture file has no array section \"match\"")]
    fn a_missing_section() {
        run_conformance(&with_top(|top| {
            top.remove("match");
        }));
    }

    // -----------------------------------------------------------------
    // Top-level fields and field TYPES (Codex review r7, finding 3)
    //
    // Codex showed the PHP runner passing a file with the four required
    // top-level fields removed, and one with a selection track's `roles`
    // set to null. The same damaged copies, and more (a wrong type in each
    // kind of field; null where the schema allows none), are run here
    // against this runner. Each must be refused before any case runs, with
    // a message naming what was wrong. Table-driven: one row per damaged
    // copy, so a new kind of damage is one more row.
    // -----------------------------------------------------------------

    /// Runs the harness on `raw` and returns the message it stopped with,
    /// or `None` if it accepted the file (ran to the end without stopping).
    fn refusal(raw: &str) -> Option<String> {
        let payload = std::panic::catch_unwind(|| run_conformance(raw)).err()?;
        Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default(),
        )
    }

    /// Checks every row: the harness refuses the damaged copy, and its
    /// message contains the row's expected fragment. Collects every wrong
    /// row before failing, so one run shows them all.
    fn check_refusals(rows: Vec<(&str, String, &str)>) {
        let mut wrong = Vec::new();
        for (name, raw, fragment) in rows {
            match refusal(&raw) {
                None => wrong.push(format!("{name}: ACCEPTED - it must be refused")),
                Some(message) if !message.contains(fragment) => {
                    wrong.push(format!("{name}: expected {fragment:?} in {message:?}"))
                }
                Some(_) => {}
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn damaged_top_level_fields_are_refused() {
        check_refusals(vec![
            (
                "all four required top-level fields removed (Codex's input)",
                with_top(|top| {
                    for key in [
                        "policy",
                        "policy_version",
                        "fixtures_version",
                        "data_version",
                    ] {
                        top.remove(key);
                    }
                }),
                "fixture file has no string \"policy\"",
            ),
            (
                "policy removed",
                with_top(|top| {
                    top.remove("policy");
                }),
                "no string \"policy\"",
            ),
            (
                "policy_version removed",
                with_top(|top| {
                    top.remove("policy_version");
                }),
                "no string \"policy_version\"",
            ),
            (
                "fixtures_version removed",
                with_top(|top| {
                    top.remove("fixtures_version");
                }),
                "no string \"fixtures_version\"",
            ),
            (
                "data_version removed",
                with_top(|top| {
                    top.remove("data_version");
                }),
                "no string \"data_version\"",
            ),
            (
                "policy has the wrong value",
                with_top(|top| {
                    top.insert("policy".into(), Value::String("OTHER".into()));
                }),
                "MWBM-MEDIA-LANG",
            ),
            (
                "policy_version is a number",
                with_top(|top| {
                    top.insert("policy_version".into(), serde_json::json!(1));
                }),
                "no string \"policy_version\"",
            ),
            (
                "fixtures_version is not three numbers",
                with_top(|top| {
                    top.insert("fixtures_version".into(), Value::String("1.0".into()));
                }),
                "fixtures_version \"1.0\" is not a version",
            ),
            (
                "data_version is not the embedded data's",
                with_top(|top| {
                    top.insert("data_version".into(), Value::String("9.9.9".into()));
                }),
                "different reference-data version",
            ),
            (
                "$schema is a number",
                with_top(|top| {
                    top.insert("$schema".into(), serde_json::json!(5));
                }),
                "\"$schema\" must be a string",
            ),
        ]);
    }

    #[test]
    fn damaged_field_types_are_refused() {
        use serde_json::json;
        let first = |section: &str| -> String {
            let fixtures: Value = serde_json::from_str(&fixture_text()).unwrap();
            fixtures[section][0]["id"].as_str().unwrap().to_string()
        };
        let set = |section: &str, id: &str, pointer: &str, value: Value| -> String {
            with_case(section, id, |c| {
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                let target = if parent.is_empty() {
                    c
                } else {
                    c.pointer_mut(parent).unwrap()
                };
                target
                    .as_object_mut()
                    .unwrap()
                    .insert(key.to_string(), value);
            })
        };
        check_refusals(vec![
            (
                "a selection track's roles is null (Codex's input)",
                set(
                    "auto_select_audio",
                    "audio-01",
                    "/tracks/0/roles",
                    Value::Null,
                ),
                "invalid type: null, expected a sequence",
            ),
            (
                "a canonicalise input is a number",
                set("canonicalise", &first("canonicalise"), "/input", json!(5)),
                "invalid type: integer `5`, expected a string",
            ),
            (
                "a label's channels is a number",
                set("label", "label-01", "/channels", json!(5)),
                "invalid type: integer `5`, expected a string",
            ),
            (
                "an optional description is null",
                set(
                    "posix_locale",
                    &first("posix_locale"),
                    "/description",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a track's original is a string",
                set(
                    "track_order",
                    &first("track_order"),
                    "/tracks/0/original",
                    json!("yes"),
                ),
                "expected a boolean",
            ),
            (
                "a track's original is null",
                set(
                    "track_order",
                    &first("track_order"),
                    "/tracks/0/original",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a match distance is a string",
                set("match", "match-01", "/expected/distance", json!("1")),
                "expected usize",
            ),
            (
                "a match distance has a fraction",
                set("match", "match-01", "/expected/distance", json!(1.5)),
                "invalid type: floating point",
            ),
            (
                "rules is a string",
                set(
                    "legacy_three_letter",
                    &first("legacy_three_letter"),
                    "/rules",
                    json!("LANG-002"),
                ),
                "expected a sequence",
            ),
            (
                "rules holds a number",
                set(
                    "legacy_three_letter",
                    &first("legacy_three_letter"),
                    "/rules",
                    json!([1]),
                ),
                "expected a string",
            ),
            (
                "a display name is a number",
                set(
                    "presentation_order",
                    &first("presentation_order"),
                    "/display_names",
                    json!({"en": 5}),
                ),
                "expected a string",
            ),
            (
                "preferences is null",
                set("auto_select_audio", "audio-01", "/preferences", Value::Null),
                "expected a sequence",
            ),
            (
                "a sidecar build number is a string",
                set("sidecar_name", "sidecar-01", "/number", json!("2")),
                "expected i64",
            ),
            (
                "an accessibility preference is null",
                set(
                    "auto_select_audio",
                    "audio-01",
                    "/accessibility",
                    json!({"captions": null}),
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "an order item's id is null",
                set(
                    "canonical_order",
                    &first("canonical_order"),
                    "/items/0/id",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a menu's selected is null",
                set(
                    "subtitle_menu",
                    &first("subtitle_menu"),
                    "/selected",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a canonicalise note is null",
                set("canonicalise", &first("canonicalise"), "/note", Value::Null),
                "is null, which the schema does not allow here",
            ),
            (
                "a presentation item's type is null",
                set(
                    "presentation_order",
                    &first("presentation_order"),
                    "/items/0/type",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a selection track's default is null",
                set(
                    "auto_select_audio",
                    "audio-01",
                    "/tracks/0/default",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a selection track's original is null",
                set(
                    "auto_select_subtitle",
                    "subs-01",
                    "/tracks/0/original",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "a label's role_names is null",
                set("label", "label-01", "/role_names", Value::Null),
                "expected a map",
            ),
            (
                "a sidecar parse number is a string",
                set("sidecar_name", "sidecar-07", "/expected/number", json!("3")),
                "expected u32",
            ),
            (
                "a write case's form is null",
                set("iso639_2_write", "write-01", "/expected/b", Value::Null),
                "expected a string",
            ),
            (
                "a label's roles holds a number",
                set("label", "label-01", "/roles", json!([1])),
                "expected a string",
            ),
            (
                "an expected order is null",
                set(
                    "track_order",
                    &first("track_order"),
                    "/expected",
                    Value::Null,
                ),
                "expected a sequence",
            ),
            (
                "a label description is null",
                set("label", "label-05", "/description", Value::Null),
                "is null, which the schema does not allow here",
            ),
            (
                "a match description is null",
                set("match", "match-01", "/description", Value::Null),
                "is null, which the schema does not allow here",
            ),
            (
                "a write description is null",
                set("iso639_2_write", "write-01", "/description", Value::Null),
                "is null, which the schema does not allow here",
            ),
            (
                "a sidecar build description is null",
                set("sidecar_name", "sidecar-01", "/description", Value::Null),
                "is null, which the schema does not allow here",
            ),
            (
                "a presentation item's original is null",
                set(
                    "presentation_order",
                    &first("presentation_order"),
                    "/items/0/original",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
            (
                "an order item's original is null",
                set(
                    "canonical_order",
                    &first("canonical_order"),
                    "/items/0/original",
                    Value::Null,
                ),
                "is null, which the schema does not allow here",
            ),
        ]);
    }

    // -----------------------------------------------------------------
    // A list where the schema wants an object, and the other way round
    // (stand-in review of revision 5)
    //
    // serde reads a struct from a JSON list as well as from an object, so
    // `accessibility: []` passed here as "no preferences", and
    // `expected: ["eng", "eng"]` as a write case's `{b, t}`. (The PHP
    // runner could not tell `{}` from `[]` at all; the reviewer's four
    // inputs are the first four rows.) Every struct is now read through
    // `object`, `objects` or `object_or_null`.
    // -----------------------------------------------------------------

    #[test]
    fn lists_where_objects_belong_and_objects_where_lists_belong_are_refused() {
        use serde_json::json;
        /// The real case file with the value at `pointer` inside case `id`
        /// replaced by `value`.
        fn replace(section: &str, id: &str, pointer: &str, value: Value) -> String {
            with_case(section, id, |c| {
                *c.pointer_mut(pointer)
                    .unwrap_or_else(|| panic!("no {pointer} in {section}/{id}")) = value;
            })
        }
        let not_an_object = "a JSON object belongs here, but this is a list";
        let first_case_as_a_list = with_top(|top| {
            let case = top["canonicalise"][0].clone();
            let values: Vec<Value> = case.as_object().unwrap().values().cloned().collect();
            top["canonicalise"][0] = Value::Array(values);
        });
        check_refusals(vec![
            (
                "accessibility is an empty list (the reviewer's input)",
                replace("auto_select_audio", "audio-01", "/accessibility", json!([])),
                not_an_object,
            ),
            (
                "display_names is an empty list (the reviewer's input)",
                replace(
                    "presentation_order",
                    "present-01",
                    "/display_names",
                    json!([]),
                ),
                "invalid type: sequence, expected a map",
            ),
            (
                "a selection track's roles is an empty object (the reviewer's input)",
                replace(
                    "auto_select_audio",
                    "audio-07",
                    "/tracks/0/roles",
                    json!({}),
                ),
                "invalid type: map, expected a sequence",
            ),
            (
                "rules is an empty object (the reviewer's input)",
                replace("canonicalise", "canon-01", "/rules", json!({})),
                "invalid type: map, expected a sequence",
            ),
            (
                "accessibility is a list of two values",
                replace(
                    "auto_select_subtitle",
                    "subs-01",
                    "/accessibility",
                    json!([true, false]),
                ),
                not_an_object,
            ),
            (
                "accessibility is a list in a presentation case",
                replace("subtitle_menu", "submenu-01", "/accessibility", json!([])),
                not_an_object,
            ),
            (
                "a write case's expected answer is a list",
                replace(
                    "iso639_2_write",
                    "write-01",
                    "/expected",
                    json!(["eng", "eng"]),
                ),
                not_an_object,
            ),
            (
                "a match case's expected answer is a list",
                replace("match", "match-01", "/expected", json!(["exact", 0])),
                not_an_object,
            ),
            (
                "a sidecar parse case's expected answer is a list",
                replace(
                    "sidecar_name",
                    "sidecar-07",
                    "/expected",
                    json!(["en", null, [], null, "srt"]),
                ),
                not_an_object,
            ),
            (
                "an order item is a list",
                replace("canonical_order", "order-01", "/items/0", json!(["en"])),
                not_an_object,
            ),
            (
                "a track is a list",
                replace(
                    "track_order",
                    "tracks-01",
                    "/tracks/0",
                    json!(["v1", "video", "und", []]),
                ),
                not_an_object,
            ),
            (
                "a presentation item is a list",
                replace(
                    "presentation_order",
                    "present-01",
                    "/items/0",
                    json!(["a", "en"]),
                ),
                not_an_object,
            ),
            (
                "a selection track is a list",
                replace(
                    "auto_select_subtitle",
                    "subs-01",
                    "/tracks/0",
                    json!(["s1", "en", []]),
                ),
                not_an_object,
            ),
            (
                "a whole case is a list of its values",
                first_case_as_a_list,
                not_an_object,
            ),
        ]);
    }
}
