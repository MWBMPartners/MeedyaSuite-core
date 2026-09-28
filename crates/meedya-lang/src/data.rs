// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::data — the compiled-in copy of the MWBM-MEDIA-LANG reference
// data (docs/standards/data/bcp47-language-data-v1.json).
//
// WHY a copy lives here at all: the policy (docs/standards/media-language-
// bcp47-policy.md, section 8.2) says every implementation reads this data,
// "or an exact copy", rather than keeping its own hand-typed list, so the
// Rust, PHP and Swift implementations can never quietly disagree about,
// say, which three-letter code means German. `data/bcp47-language-data-
// v1.json` in this crate (loaded below with `include_str!`, so it becomes
// part of the compiled binary — no file to go missing at runtime) is that
// copy. The test at the bottom of this file is the tripwire that stops it
// drifting from the master copy in `docs/standards/`.
//
// This module only keeps the parts of that JSON file the rest of this
// crate actually reads. It does not load the `extlangs` table itself
// (which lists each extlang's valid prefix language(s) — this policy does
// not check that an extlang follows the right prefix, only that it is
// registered at all) but it does load `preferred.extlang`, used as a
// membership test: LANG-001 step 5 only folds an extlang into the primary
// language when the registry actually lists it (`zh-abc` keeps its
// unregistered `abc` rather than promoting it).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Deserialize;

/// The raw JSON text compiled into the binary at build time. Read only by
/// [`data()`]'s one-time parse and by the `embedded_data_matches_docs_copy`
/// test below — everything else in this crate goes through [`data()`].
const RAW: &str = include_str!("../data/bcp47-language-data-v1.json");

/// Parses [`RAW`] the first time anything in this crate needs it, and
/// keeps the result for the rest of the process. The data never changes
/// while the program is running, so parsing it once (rather than on every
/// call) is both correct and the only sensible choice — this file is
/// large (the IANA registry has thousands of entries).
static PARSED: LazyLock<Data> = LazyLock::new(|| {
    let raw: RawData = serde_json::from_str(RAW).expect(
        "crates/meedya-lang/data/bcp47-language-data-v1.json failed to parse as the \
         reference-data schema — it should be a byte-for-byte copy of \
         docs/standards/data/bcp47-language-data-v1.json; see \
         embedded_data_matches_docs_copy for how to refresh it",
    );
    Data::from_raw(raw)
});

/// Returns the parsed reference data. Cheap after the first call in the
/// life of the process.
pub(crate) fn data() -> &'static Data {
    &PARSED
}

/// The reference-data version this build was compiled against — the
/// embedded JSON file's own `data_version` field (see
/// `docs/standards/data/bcp47-language-data-v1.schema.json`). A caller
/// that needs to confirm which registry snapshot is behind an answer
/// (for example, when comparing this crate's behaviour against another
/// implementation) reads this rather than assuming a version.
pub fn embedded_data_version() -> &'static str {
    &PARSED.data_version
}

/// True when `value` falls within one of the given inclusive, ASCII
/// `[first, last]` ranges (compared as plain ASCII text, exactly as the
/// policy's reference data itself is compared — see the `ranges` schema
/// entry in `bcp47-language-data-v1.schema.json`). Used for the
/// registry's local-use ranges: `qaa`–`qtz` (languages), `Qaaa`–`Qabx`
/// (scripts), `QM`–`QZ` and `XA`–`XZ` (regions).
///
/// `value` must also be the same length as the range's two ends. Plain
/// text comparison alone is not enough: `qb` sorts between `qaa` and
/// `qtz`, but a two-letter subtag is not one of the three-letter
/// local-use codes. Before policy revision 4 this check had no length
/// test, so `qb` counted as local use — `iso639_2_code` then wrote `qb`
/// into a three-letter field, and `canonicalise` did not report `qb` as
/// unregistered.
pub(crate) fn in_ranges(value: &str, ranges: &[(String, String)]) -> bool {
    ranges.iter().any(|(first, last)| {
        value.len() == first.len()
            && value.len() == last.len()
            && first.as_str() <= value
            && value <= last.as_str()
    })
}

/// The bibliographic and terminology ISO 639-2 codes for one BCP 47
/// primary language subtag (TRACK-070). The two forms differ for twenty
/// languages — `de` is `ger`/`deu`, `bo` is `tib`/`bod` — which is why
/// both are kept rather than picking one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Iso639Codes {
    /// Bibliographic form — Matroska's old `Language` element.
    pub b: String,
    /// Terminology form — the MP4/MOV media header and ID3.
    pub t: String,
}

/// One entry from the registry's grandfathered-tag table (LANG-001 step
/// 2): the tag exactly as the registry spells it, and its replacement if
/// the registry gives one (`None` when the tag has no replacement and is
/// kept as-is, such as `i-default`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct GrandfatheredEntry {
    pub tag: String,
    pub preferred: Option<String>,
}

/// The shape of `preferred`/`deprecated_without_replacement` in the JSON
/// file: one list or map per subtag kind. Deserialised once, then folded
/// into [`Data`]'s flatter, HashSet-backed fields for fast lookups.
#[derive(Deserialize)]
struct RawPreferred {
    language: HashMap<String, String>,
    extlang: HashMap<String, String>,
    script: HashMap<String, String>,
    region: HashMap<String, String>,
    variant: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawDeprecatedWithoutReplacement {
    language: Vec<String>,
    region: Vec<String>,
    script: Vec<String>,
    variant: Vec<String>,
}

/// Mirrors the top level of `bcp47-language-data-v1.json`. Only the
/// fields this crate reads are declared — serde ignores the rest (the
/// schema's `sources`, `extlangs`, `special_languages`, and so on) rather
/// than erroring on them, so a future registry refresh that adds a field
/// this crate doesn't yet use won't break the build.
#[derive(Deserialize)]
struct RawData {
    data_version: String,
    languages: Vec<String>,
    language_ranges: Vec<(String, String)>,
    scripts: Vec<String>,
    script_ranges: Vec<(String, String)>,
    regions: Vec<String>,
    region_ranges: Vec<(String, String)>,
    variants: Vec<String>,
    preferred: RawPreferred,
    deprecated_without_replacement: RawDeprecatedWithoutReplacement,
    grandfathered: HashMap<String, GrandfatheredEntry>,
    redundant_preferred: HashMap<String, String>,
    iso639_2: HashMap<String, String>,
    iso639_2_for_language: HashMap<String, Iso639Codes>,
    /// `["qaa", "qtz"]` — the ISO 639-2 local-use range. A primary
    /// language in this range is written as itself (TRACK-070), because
    /// nobody but the two parties using a local-use code knows what a
    /// registered replacement would even mean.
    iso639_2_local_use: (String, String),
}

/// The parsed reference data, in the shape this crate's algorithms
/// actually want: set membership for "is this subtag registered?"
/// questions, maps for "what does this subtag get replaced with?"
/// questions.
pub(crate) struct Data {
    pub data_version: String,
    pub languages: HashSet<String>,
    pub language_ranges: Vec<(String, String)>,
    pub scripts: HashSet<String>,
    pub script_ranges: Vec<(String, String)>,
    pub regions: HashSet<String>,
    pub region_ranges: Vec<(String, String)>,
    pub variants: HashSet<String>,
    pub preferred_language: HashMap<String, String>,
    pub preferred_extlang: HashMap<String, String>,
    pub preferred_script: HashMap<String, String>,
    pub preferred_region: HashMap<String, String>,
    pub preferred_variant: HashMap<String, String>,
    pub deprecated_language: HashSet<String>,
    pub deprecated_region: HashSet<String>,
    pub deprecated_script: HashSet<String>,
    pub deprecated_variant: HashSet<String>,
    pub grandfathered: HashMap<String, GrandfatheredEntry>,
    pub redundant_preferred: HashMap<String, String>,
    pub iso639_2: HashMap<String, String>,
    pub iso639_2_for_language: HashMap<String, Iso639Codes>,
    pub iso639_2_local_use: (String, String),
}

impl Data {
    /// True when `language` (already lower case) is one of the codes
    /// TRACK-070 says are written as themselves — the ISO 639-2 range
    /// reserved for local use, `qaa`–`qtz`. Three ASCII letters are
    /// required: `qb` sorts inside the range as text but is not a
    /// local-use code (see [`in_ranges`] for the fault this closes).
    pub(crate) fn is_iso639_2_local_use(&self, language: &str) -> bool {
        let (first, last) = (&self.iso639_2_local_use.0, &self.iso639_2_local_use.1);
        language.len() == 3
            && language.bytes().all(|b| b.is_ascii_lowercase())
            && first.as_str() <= language
            && language <= last.as_str()
    }
}

impl Data {
    fn from_raw(raw: RawData) -> Self {
        Data {
            data_version: raw.data_version,
            languages: raw.languages.into_iter().collect(),
            language_ranges: raw.language_ranges,
            scripts: raw.scripts.into_iter().collect(),
            script_ranges: raw.script_ranges,
            regions: raw.regions.into_iter().collect(),
            region_ranges: raw.region_ranges,
            variants: raw.variants.into_iter().collect(),
            preferred_language: raw.preferred.language,
            preferred_extlang: raw.preferred.extlang,
            preferred_script: raw.preferred.script,
            preferred_region: raw.preferred.region,
            preferred_variant: raw.preferred.variant,
            deprecated_language: raw
                .deprecated_without_replacement
                .language
                .into_iter()
                .collect(),
            deprecated_region: raw
                .deprecated_without_replacement
                .region
                .into_iter()
                .collect(),
            deprecated_script: raw
                .deprecated_without_replacement
                .script
                .into_iter()
                .collect(),
            deprecated_variant: raw
                .deprecated_without_replacement
                .variant
                .into_iter()
                .collect(),
            grandfathered: raw.grandfathered,
            redundant_preferred: raw.redundant_preferred,
            iso639_2: raw.iso639_2,
            iso639_2_for_language: raw.iso639_2_for_language,
            iso639_2_local_use: raw.iso639_2_local_use,
        }
    }
}

#[cfg(test)]
mod tests {
    /// The load-bearing test for this whole module: the copy of the
    /// reference data compiled into this crate MUST be byte-for-byte
    /// identical to the master copy under `docs/standards/`. If this
    /// fails, someone edited one copy and not the other — refresh with:
    ///
    ///   cp docs/standards/data/bcp47-language-data-v1.json \
    ///      crates/meedya-lang/data/bcp47-language-data-v1.json
    ///
    /// (run from the repository root), then re-run the tests.
    #[test]
    fn embedded_data_matches_docs_copy() {
        let embedded = super::RAW;
        let docs_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/standards/data/bcp47-language-data-v1.json"
        );
        let docs = std::fs::read_to_string(docs_path)
            .unwrap_or_else(|e| panic!("could not read the master copy at {docs_path}: {e}"));
        assert_eq!(
            embedded, docs,
            "crates/meedya-lang/data/bcp47-language-data-v1.json has drifted from \
             docs/standards/data/bcp47-language-data-v1.json — refresh it with:\n\n  \
             cp docs/standards/data/bcp47-language-data-v1.json \
             crates/meedya-lang/data/bcp47-language-data-v1.json\n\n(run from the \
             repository root)"
        );
    }

    #[test]
    fn parses_without_panicking_and_reports_its_version() {
        // Forces the LazyLock to actually run its parser (a bad copy of
        // the data file would panic here, in a test, rather than the
        // first time some unrelated app calls into this crate).
        assert_eq!(super::embedded_data_version(), "1.0.0");
    }
}
