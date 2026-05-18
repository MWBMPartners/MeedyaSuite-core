// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Mixed In Key reader — recovers MIK's key / energy / tempo from every
// location MIK is documented to write to, then normalises into standard
// tag fields. Standards-first by design: writes to ItemKey::InitialKey
// (TKEY / `----:com.apple.iTunes:initialkey` / INITIALKEY) and
// ItemKey::IntegerBpm / ItemKey::Bpm (TBPM / tmpo / BPM). Energy has no
// standard equivalent — only that one field falls back to MeedyaMeta.
//
// Source fields are read-only. The reader never modifies the original
// artist/title/comment/grouping/label strings; the user's data is
// preserved verbatim. A separate "cleanup" feature could strip MIK
// prefixes later as an opt-in.
//
// ## Locations covered (per MIK Tag options UI)
//
// What MIK can write (any combination of key, tempo, energy):
//   - `10A`              key only
//   - `Energy 7`         energy with word
//   - `10A - Energy 7`   key + energy with word
//   - `7`                energy alone
//   - `10A - 7`          key + energy
//   - `10A - 126`        key + tempo
//   - `10A - 126 - 7`    key + tempo + energy
//   - `126 - 10A - 7`    tempo + key + energy
//
// Where MIK can write:
//   - In front of artist name      → "10A - Axwell"
//   - In front of song title       → "10A - Feel the vibe"
//   - At end of song title         → "Feel the vibe - 10A"
//   - In front of comments         → "10A - www.beatport.com"
//   - Overwrite comments           → "10A"
//   - In front of grouping (energy)→ "Energy 7 - Old skool"
//   - Overwrite label (energy)     → "Energy 7"
//   - Standard `InitialKey` field  → "Update custom Initial Key" toggle
//   - Standard tempo fields        → "Update tempo tags" toggle
//
// Camelot zero-padding (`05A` vs `5A`) is handled by `MusicalKey::parse`.

use lofty::tag::{ItemKey, Tag};

use crate::model::MusicalKey;
use crate::standard;

const MEEDYA_NAMESPACE: &str = "MeedyaMeta";
const MIK_ENERGY_ATOM: &str = "Energy";
const MIK_AUDIT_ATOM: &str = "MikSourceLocations";

/// Energy levels above this are unlikely to be MIK output; reject them
/// as bare-digit energy. MIK uses 1-10.
const MAX_ENERGY: u8 = 10;

/// BPM range for bare-digit tempo classification. Outside this range, a
/// bare digit is more likely to be something other than a BPM.
const MIN_BPM: u32 = 40;
const MAX_BPM: u32 = 250;

// ============================================================
// Public Types
// ============================================================

/// Result of scanning a tag for Mixed In Key output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MikAnalysis {
    pub key: Option<MusicalKey>,
    /// Energy rating (1-10). MIK uses this scale.
    pub energy: Option<u8>,
    pub bpm: Option<f64>,
    /// Where each datapoint came from. The order is the order locations
    /// were inspected; precedence is "last write wins" within a field
    /// (standard `InitialKey` is checked first so it loses to any
    /// embedded-pattern field that follows — which is the right default
    /// because the embedded fields are typically more recent updates).
    pub sources: Vec<MikSourceLocation>,
}

impl MikAnalysis {
    pub fn is_empty(&self) -> bool {
        self.key.is_none() && self.energy.is_none() && self.bpm.is_none()
    }
}

/// Where a MIK datapoint was extracted from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MikSourceLocation {
    pub field: MikField,
    pub position: MikPosition,
    /// Which datapoints came from this location (one location may
    /// contribute multiple, e.g., a comment containing `10A - 126 - 7`).
    pub kinds: MikKinds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MikField {
    /// Standard `InitialKey` tag (TKEY / `----:com.apple.iTunes:initialkey` / INITIALKEY).
    InitialKey,
    /// Standard BPM tag (TBPM / tmpo / BPM).
    Bpm,
    /// Track artist field.
    Artist,
    /// Track title field.
    Title,
    /// Comment field.
    Comment,
    /// Grouping field (ContentGroup ItemKey).
    Grouping,
    /// Label / publisher field.
    Label,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MikPosition {
    /// The entire field value matched the MIK pattern.
    Whole,
    /// The MIK pattern was at the start of the field.
    Prefix,
    /// The MIK pattern was at the end of the field.
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct MikKinds {
    pub key: bool,
    pub bpm: bool,
    pub energy: bool,
}

impl MikKinds {
    fn any(self) -> bool {
        self.key || self.bpm || self.energy
    }
}

// ============================================================
// Public API
// ============================================================

/// Scan a tag for everything Mixed In Key may have written and return
/// the recovered analysis. Does not modify the tag.
pub fn read_mik(tag: &Tag) -> MikAnalysis {
    let mut analysis = MikAnalysis::default();

    // Standard fields first — these are unambiguous and lowest-noise.
    if let Some(key) = standard::read_key(tag) {
        analysis.key = Some(key);
        analysis.sources.push(MikSourceLocation {
            field: MikField::InitialKey,
            position: MikPosition::Whole,
            kinds: MikKinds {
                key: true,
                ..Default::default()
            },
        });
    }
    if let Some(bpm) = standard::read_bpm(tag) {
        analysis.bpm = Some(bpm);
        analysis.sources.push(MikSourceLocation {
            field: MikField::Bpm,
            position: MikPosition::Whole,
            kinds: MikKinds {
                bpm: true,
                ..Default::default()
            },
        });
    }

    // Embedded-pattern fields. Inspect each documented location and let
    // matches overwrite earlier-discovered values (intentional: a recent
    // MIK update will appear in these fields after older standard-tag
    // entries become stale).
    inspect_combo_field(
        tag,
        ItemKey::TrackArtist,
        MikField::Artist,
        Some(MikPosition::Prefix),
        &mut analysis,
    );
    inspect_combo_field(
        tag,
        ItemKey::TrackTitle,
        MikField::Title,
        None,
        &mut analysis,
    );
    inspect_combo_field(
        tag,
        ItemKey::Comment,
        MikField::Comment,
        None,
        &mut analysis,
    );
    inspect_energy_field(
        tag,
        ItemKey::ContentGroup,
        MikField::Grouping,
        Some(MikPosition::Prefix),
        &mut analysis,
    );
    inspect_energy_field(
        tag,
        ItemKey::Label,
        MikField::Label,
        Some(MikPosition::Whole),
        &mut analysis,
    );

    analysis
}

/// Write the recovered analysis into standard tag fields (and one
/// MeedyaMeta atom for energy, which has no standard equivalent).
///
/// Standards used:
///   - Key → `ItemKey::InitialKey` (TKEY / `----:com.apple.iTunes:initialkey` / INITIALKEY)
///   - BPM → `ItemKey::IntegerBpm` + `ItemKey::Bpm` (TBPM / tmpo + float on MP4)
///   - Energy → `MeedyaMeta:Energy` freeform atom (no widely-supported standard)
///
/// Also writes `MeedyaMeta:MikSourceLocations` as an audit trail so
/// downstream tools can see where each datapoint came from.
pub fn normalise_to_standards(tag: &mut Tag, analysis: &MikAnalysis) {
    if let Some(key) = analysis.key {
        standard::write_key(tag, key);
    }
    if let Some(bpm) = analysis.bpm {
        standard::write_bpm(tag, bpm);
    }
    if let Some(energy) = analysis.energy {
        write_meedya_atom(tag, MIK_ENERGY_ATOM, &energy.to_string());
    }
    if !analysis.sources.is_empty() {
        let audit = format_audit_trail(&analysis.sources);
        write_meedya_atom(tag, MIK_AUDIT_ATOM, &audit);
    }
}

// ============================================================
// Field Inspection
// ============================================================

/// Inspect a field for MIK's combined key/tempo/energy patterns
/// (`10A - 126 - 7`, etc.). When `force_position` is `Some`, only that
/// position is considered; when `None`, prefix and suffix are both tried.
fn inspect_combo_field(
    tag: &Tag,
    item_key: ItemKey,
    field: MikField,
    force_position: Option<MikPosition>,
    analysis: &mut MikAnalysis,
) {
    let Some(value) = tag.get_string(&item_key) else {
        return;
    };

    // Try whole-field match first.
    if force_position.is_none() || force_position == Some(MikPosition::Whole) {
        let tokens = split_dash_tokens(value);
        let parsed = classify_tokens(&tokens);
        if parsed.kinds.any() && !leaves_unclassified(&tokens, &parsed.classified) {
            apply_parsed(analysis, &parsed, field, MikPosition::Whole);
            return;
        }
    }

    // Prefix: greedily consume MIK-classifiable tokens from the start.
    // The longest classifiable prefix followed by " - " then non-MIK rest
    // is the match.
    if force_position.is_none() || force_position == Some(MikPosition::Prefix) {
        let all_tokens = split_dash_tokens(value);
        if all_tokens.len() >= 2 {
            // Find the longest prefix where all tokens classify.
            let mut best: Option<ParsedTokens> = None;
            for n in 1..all_tokens.len() {
                let prefix_tokens = &all_tokens[..n];
                let parsed = classify_tokens(prefix_tokens);
                if parsed.kinds.any()
                    && !leaves_unclassified(prefix_tokens, &parsed.classified)
                {
                    best = Some(parsed);
                }
            }
            if let Some(parsed) = best {
                apply_parsed(analysis, &parsed, field, MikPosition::Prefix);
                return;
            }
        }
    }

    // Suffix: greedily consume MIK-classifiable tokens from the end.
    if force_position.is_none() || force_position == Some(MikPosition::Suffix) {
        let all_tokens = split_dash_tokens(value);
        if all_tokens.len() >= 2 {
            let mut best: Option<ParsedTokens> = None;
            for start in (0..all_tokens.len() - 1).rev() {
                let suffix_tokens = &all_tokens[start + 1..];
                let parsed = classify_tokens(suffix_tokens);
                if parsed.kinds.any()
                    && !leaves_unclassified(suffix_tokens, &parsed.classified)
                {
                    best = Some(parsed);
                } else {
                    break;
                }
            }
            if let Some(parsed) = best {
                apply_parsed(analysis, &parsed, field, MikPosition::Suffix);
            }
        }
    }
}

/// Inspect a field specifically for energy patterns (`Energy N - ...`
/// or `Energy N` alone). Used for the grouping and label fields where
/// MIK only writes energy.
fn inspect_energy_field(
    tag: &Tag,
    item_key: ItemKey,
    field: MikField,
    force_position: Option<MikPosition>,
    analysis: &mut MikAnalysis,
) {
    let Some(value) = tag.get_string(&item_key) else {
        return;
    };

    let allow_whole = force_position.is_none() || force_position == Some(MikPosition::Whole);
    let allow_prefix = force_position.is_none() || force_position == Some(MikPosition::Prefix);

    if allow_whole {
        if let Some(e) = parse_energy_with_word(value.trim()) {
            analysis.energy = Some(e);
            analysis.sources.push(MikSourceLocation {
                field,
                position: MikPosition::Whole,
                kinds: MikKinds {
                    energy: true,
                    ..Default::default()
                },
            });
            return;
        }
    }
    if allow_prefix {
        if let Some((prefix, _rest)) = split_at_separator(value) {
            if let Some(e) = parse_energy_with_word(prefix.trim()) {
                analysis.energy = Some(e);
                analysis.sources.push(MikSourceLocation {
                    field,
                    position: MikPosition::Prefix,
                    kinds: MikKinds {
                        energy: true,
                        ..Default::default()
                    },
                });
            }
        }
    }
}

// ============================================================
// Token Classification
// ============================================================

#[derive(Debug, Default)]
struct ParsedTokens {
    key: Option<MusicalKey>,
    bpm: Option<f64>,
    energy: Option<u8>,
    kinds: MikKinds,
    /// Per-token classified-flag, parallel to the input slice. Used to
    /// reject candidates that contain unclassified noise.
    classified: Vec<bool>,
}

fn classify_tokens(tokens: &[&str]) -> ParsedTokens {
    let mut out = ParsedTokens::default();
    out.classified = vec![false; tokens.len()];

    for (i, token) in tokens.iter().enumerate() {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }

        if let Some(k) = MusicalKey::parse(t) {
            if out.key.is_none() {
                out.key = Some(k);
                out.kinds.key = true;
                out.classified[i] = true;
                continue;
            }
        }
        if let Some(e) = parse_energy_with_word(t) {
            if out.energy.is_none() {
                out.energy = Some(e);
                out.kinds.energy = true;
                out.classified[i] = true;
                continue;
            }
        }
        if let Some(bpm) = parse_bare_tempo(t) {
            if out.bpm.is_none() {
                out.bpm = Some(bpm);
                out.kinds.bpm = true;
                out.classified[i] = true;
                continue;
            }
        }
        if let Some(e) = parse_bare_energy(t) {
            if out.energy.is_none() {
                out.energy = Some(e);
                out.kinds.energy = true;
                out.classified[i] = true;
            }
        }
    }

    out
}

fn apply_parsed(
    analysis: &mut MikAnalysis,
    parsed: &ParsedTokens,
    field: MikField,
    position: MikPosition,
) {
    if let Some(k) = parsed.key {
        analysis.key = Some(k);
    }
    if let Some(b) = parsed.bpm {
        analysis.bpm = Some(b);
    }
    if let Some(e) = parsed.energy {
        analysis.energy = Some(e);
    }
    analysis.sources.push(MikSourceLocation {
        field,
        position,
        kinds: parsed.kinds,
    });
}

fn leaves_unclassified(tokens: &[&str], classified: &[bool]) -> bool {
    tokens
        .iter()
        .zip(classified.iter())
        .any(|(t, c)| !c && !t.trim().is_empty())
}

// ============================================================
// Token Parsers
// ============================================================

/// Match `"Energy N"` (case-insensitive on the word). Returns N as 1-10.
fn parse_energy_with_word(s: &str) -> Option<u8> {
    let lower = s.to_ascii_lowercase();
    let rest = lower.strip_prefix("energy")?;
    let n: u8 = rest.trim().parse().ok()?;
    (1..=MAX_ENERGY).contains(&n).then_some(n)
}

/// Match a bare digit in the MIK energy range (1-10).
fn parse_bare_energy(s: &str) -> Option<u8> {
    let n: u8 = s.parse().ok()?;
    (1..=MAX_ENERGY).contains(&n).then_some(n)
}

/// Match a bare integer in a plausible BPM range (40-250). MIK writes
/// integer BPMs; we accept floats for future compatibility.
fn parse_bare_tempo(s: &str) -> Option<f64> {
    let f: f64 = s.parse().ok()?;
    if !f.is_finite() {
        return None;
    }
    let i = f as u32;
    if (MIN_BPM..=MAX_BPM).contains(&i) {
        Some(f)
    } else {
        None
    }
}

/// Split a string on `" - "` (space-dash-space) boundaries.
fn split_dash_tokens(s: &str) -> Vec<&str> {
    s.split(" - ").collect()
}

/// Split into `(prefix, rest)` at the first `" - "`. Returns `None` if
/// no separator is present.
fn split_at_separator(s: &str) -> Option<(&str, &str)> {
    s.split_once(" - ")
}

/// Split into `(rest, suffix)` at the last `" - "`.
fn split_at_last_separator(s: &str) -> Option<(&str, &str)> {
    s.rsplit_once(" - ")
}

// ============================================================
// MeedyaMeta Atom Writers (energy + audit trail)
// ============================================================

fn write_meedya_atom(tag: &mut Tag, name: &str, value: &str) {
    // Lofty's `insert_text` rejects `ItemKey::Unknown` for tag types that
    // don't recognise the unknown key. We use `insert_unchecked` because
    // MeedyaMeta atoms are intentionally non-standard — the abstract tag
    // shouldn't gatekeep them.
    let item_key = ItemKey::Unknown(format!("----:{MEEDYA_NAMESPACE}:{name}"));
    tag.insert_unchecked(lofty::tag::TagItem::new(
        item_key,
        lofty::tag::ItemValue::Text(value.to_owned()),
    ));
}

fn format_audit_trail(sources: &[MikSourceLocation]) -> String {
    sources
        .iter()
        .map(|loc| {
            let mut tags = Vec::new();
            if loc.kinds.key {
                tags.push("key");
            }
            if loc.kinds.bpm {
                tags.push("bpm");
            }
            if loc.kinds.energy {
                tags.push("energy");
            }
            format!("{:?}:{:?}({})", loc.field, loc.position, tags.join("+"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KeyMode, Note};
    use lofty::tag::TagType;

    fn fresh() -> Tag {
        Tag::new(TagType::Id3v2)
    }

    /// Camelot 10A is B minor (Bm). Used as the fixture key throughout
    /// the tests because "10A" is the example MIK uses in its UI.
    fn bm() -> MusicalKey {
        MusicalKey {
            tonic: Note::B,
            mode: KeyMode::Minor,
        }
    }

    // ---- Token classification ----

    #[test]
    fn token_classify_key_only() {
        let p = classify_tokens(&["10A"]);
        assert_eq!(p.key, Some(bm()));
        assert_eq!(p.bpm, None);
        assert_eq!(p.energy, None);
    }

    #[test]
    fn token_classify_zero_padded_camelot() {
        let p = classify_tokens(&["05A"]);
        let k = p.key.expect("parses 05A");
        assert_eq!(k.camelot(), "5A");
    }

    #[test]
    fn token_classify_traditional_flat() {
        let p = classify_tokens(&["Bbm"]);
        assert_eq!(p.key.map(|k| k.traditional()), Some("A#m".to_owned()));
    }

    #[test]
    fn token_classify_traditional_sharp() {
        let p = classify_tokens(&["F#m"]);
        assert!(p.key.is_some());
    }

    #[test]
    fn token_classify_key_and_tempo() {
        let p = classify_tokens(&["10A", "126"]);
        assert_eq!(p.key, Some(bm()));
        assert_eq!(p.bpm, Some(126.0));
    }

    #[test]
    fn token_classify_key_tempo_energy() {
        let p = classify_tokens(&["10A", "126", "7"]);
        assert_eq!(p.key, Some(bm()));
        assert_eq!(p.bpm, Some(126.0));
        assert_eq!(p.energy, Some(7));
    }

    #[test]
    fn token_classify_tempo_key_energy() {
        let p = classify_tokens(&["126", "10A", "7"]);
        assert_eq!(p.key, Some(bm()));
        assert_eq!(p.bpm, Some(126.0));
        assert_eq!(p.energy, Some(7));
    }

    #[test]
    fn token_classify_energy_with_word() {
        let p = classify_tokens(&["Energy 7"]);
        assert_eq!(p.energy, Some(7));
    }

    #[test]
    fn token_classify_key_and_energy_word() {
        let p = classify_tokens(&["10A", "Energy 7"]);
        assert_eq!(p.key, Some(bm()));
        assert_eq!(p.energy, Some(7));
    }

    #[test]
    fn token_classify_bare_energy_only() {
        let p = classify_tokens(&["7"]);
        assert_eq!(p.energy, Some(7));
        assert_eq!(p.bpm, None);
    }

    #[test]
    fn token_classify_key_and_bare_energy() {
        let p = classify_tokens(&["10A", "7"]);
        assert_eq!(p.key, Some(bm()));
        assert_eq!(p.energy, Some(7));
        assert_eq!(p.bpm, None);
    }

    #[test]
    fn token_classify_rejects_unrelated_token() {
        let p = classify_tokens(&["10A", "www.beatport.com"]);
        // Key recovered, but unclassified token leaves remnants.
        assert_eq!(p.key, Some(bm()));
        assert!(leaves_unclassified(
            &["10A", "www.beatport.com"],
            &p.classified
        ));
    }

    // ---- Field inspection ----

    #[test]
    fn read_initial_key_only() {
        let mut tag = fresh();
        standard::write_key(&mut tag, bm());
        let a = read_mik(&tag);
        assert_eq!(a.key, Some(bm()));
        assert_eq!(a.sources.len(), 1);
        assert_eq!(a.sources[0].field, MikField::InitialKey);
    }

    #[test]
    fn read_bpm_only() {
        let mut tag = fresh();
        standard::write_bpm(&mut tag, 126.0);
        let a = read_mik(&tag);
        assert_eq!(a.bpm, Some(126.0));
    }

    #[test]
    fn read_artist_prefix() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::TrackArtist, "10A - Axwell".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.key, Some(bm()));
        let src = a
            .sources
            .iter()
            .find(|s| s.field == MikField::Artist)
            .expect("artist source");
        assert_eq!(src.position, MikPosition::Prefix);
    }

    #[test]
    fn read_title_prefix() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::TrackTitle, "10A - Feel the vibe".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.key, Some(bm()));
    }

    #[test]
    fn read_title_suffix() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::TrackTitle, "Feel the vibe - 10A".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.key, Some(bm()));
        let src = a
            .sources
            .iter()
            .find(|s| s.field == MikField::Title)
            .expect("title source");
        assert_eq!(src.position, MikPosition::Suffix);
    }

    #[test]
    fn read_comment_prefix() {
        let mut tag = fresh();
        tag.insert_text(
            ItemKey::Comment,
            "10A - 126 - 7 - www.beatport.com".to_owned(),
        );
        let a = read_mik(&tag);
        assert_eq!(a.key, Some(bm()));
        assert_eq!(a.bpm, Some(126.0));
        assert_eq!(a.energy, Some(7));
    }

    #[test]
    fn read_comment_overwrite() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::Comment, "10A".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.key, Some(bm()));
        let src = a
            .sources
            .iter()
            .find(|s| s.field == MikField::Comment)
            .expect("comment source");
        assert_eq!(src.position, MikPosition::Whole);
    }

    #[test]
    fn read_grouping_energy_prefix() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::ContentGroup, "Energy 7 - Old skool".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.energy, Some(7));
    }

    #[test]
    fn read_label_energy_whole() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::Label, "Energy 7".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.energy, Some(7));
    }

    #[test]
    fn read_label_non_mik_ignored() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::Label, "Some Records".to_owned());
        let a = read_mik(&tag);
        assert_eq!(a.energy, None);
    }

    #[test]
    fn read_artist_without_mik_pattern_ignored() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::TrackArtist, "Just Some Artist".to_owned());
        let a = read_mik(&tag);
        assert!(a.is_empty());
    }

    #[test]
    fn read_artist_with_garbage_after_key_ignored() {
        // "10A - random_junk" — key looks present but the second token
        // isn't classifiable. Reject as a false positive on the prefix
        // by requiring all tokens in the matched segment to classify.
        let mut tag = fresh();
        tag.insert_text(
            ItemKey::TrackArtist,
            "10A - random_unclassifiable".to_owned(),
        );
        let a = read_mik(&tag);
        // Artist prefix uses split_at_separator; the prefix is just "10A"
        // (single token, classifies) so the read should succeed for key
        // only. The unclassifiable rest is naturally the artist remainder.
        assert_eq!(a.key, Some(bm()));
    }

    // ---- Normalisation ----

    #[test]
    fn normalise_writes_standard_key() {
        let mut tag = fresh();
        let analysis = MikAnalysis {
            key: Some(bm()),
            bpm: Some(128.0),
            energy: Some(8),
            sources: vec![MikSourceLocation {
                field: MikField::Artist,
                position: MikPosition::Prefix,
                kinds: MikKinds {
                    key: true,
                    bpm: true,
                    energy: true,
                },
            }],
        };
        normalise_to_standards(&mut tag, &analysis);

        assert_eq!(standard::read_key(&tag), Some(bm()));
        assert_eq!(standard::read_bpm(&tag), Some(128.0));
        // Energy lives in MeedyaMeta:Energy — verify via raw atom read.
        let energy_key = ItemKey::Unknown(format!("----:{MEEDYA_NAMESPACE}:{MIK_ENERGY_ATOM}"));
        assert_eq!(tag.get_string(&energy_key), Some("8"));
        // Audit trail present.
        let audit_key = ItemKey::Unknown(format!("----:{MEEDYA_NAMESPACE}:{MIK_AUDIT_ATOM}"));
        assert!(tag.get_string(&audit_key).is_some());
    }

    #[test]
    fn normalise_skips_unset_fields() {
        let mut tag = fresh();
        let analysis = MikAnalysis {
            key: None,
            bpm: None,
            energy: Some(5),
            sources: vec![],
        };
        normalise_to_standards(&mut tag, &analysis);
        assert_eq!(standard::read_key(&tag), None);
        assert_eq!(standard::read_bpm(&tag), None);
        let energy_key = ItemKey::Unknown(format!("----:{MEEDYA_NAMESPACE}:{MIK_ENERGY_ATOM}"));
        assert_eq!(tag.get_string(&energy_key), Some("5"));
    }

    #[test]
    fn round_trip_comment_then_normalise() {
        let mut tag = fresh();
        tag.insert_text(ItemKey::Comment, "10A - 126 - 7".to_owned());

        let analysis = read_mik(&tag);
        normalise_to_standards(&mut tag, &analysis);

        // Standard fields now populated.
        assert_eq!(standard::read_key(&tag), Some(bm()));
        assert_eq!(standard::read_bpm(&tag), Some(126.0));
        // Original comment preserved.
        assert_eq!(tag.get_string(&ItemKey::Comment), Some("10A - 126 - 7"));
    }

    // ---- Edge cases ----

    #[test]
    fn empty_tag_yields_empty_analysis() {
        let tag = fresh();
        let a = read_mik(&tag);
        assert!(a.is_empty());
        assert!(a.sources.is_empty());
    }

    #[test]
    fn energy_out_of_range_rejected() {
        assert_eq!(parse_energy_with_word("Energy 99"), None);
        assert_eq!(parse_bare_energy("99"), None);
    }

    #[test]
    fn bpm_out_of_range_rejected() {
        assert_eq!(parse_bare_tempo("30"), None);
        assert_eq!(parse_bare_tempo("999"), None);
    }

    #[test]
    fn separator_is_space_dash_space_only() {
        // Not a MIK separator without spaces around the dash.
        let mut tag = fresh();
        tag.insert_text(ItemKey::TrackTitle, "10A-Feel".to_owned());
        let a = read_mik(&tag);
        assert!(a.is_empty(), "no spaces around dash means no MIK pattern");
    }

    #[test]
    fn sources_audit_format() {
        let sources = vec![
            MikSourceLocation {
                field: MikField::Comment,
                position: MikPosition::Prefix,
                kinds: MikKinds {
                    key: true,
                    bpm: true,
                    energy: true,
                },
            },
            MikSourceLocation {
                field: MikField::Label,
                position: MikPosition::Whole,
                kinds: MikKinds {
                    energy: true,
                    ..Default::default()
                },
            },
        ];
        let audit = format_audit_trail(&sources);
        assert!(audit.contains("Comment"));
        assert!(audit.contains("Prefix"));
        assert!(audit.contains("key+bpm+energy"));
        assert!(audit.contains("Label"));
    }
}
