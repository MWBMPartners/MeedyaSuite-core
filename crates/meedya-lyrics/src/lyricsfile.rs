// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Lyricsfile (.lyrics) — YAML lyrics format with word-level synchronisation
// =========================================================================
//
// `Lyricsfile` is the open, extensible lyrics format introduced by LRCGET
// v2.0.0 and co-endorsed by LRCLIB. It is YAML-based, supports word-level
// timing, overlapping vocal lines, plain + synced sections in one document,
// and explicit instrumental marking.
//
// This module implements the **canonical schema** (mirroring LRCGET's
// reference Rust implementation byte-for-byte) plus parse / serialise.
// Format converters (TTML → Lyricsfile, LRC → Lyricsfile, and the five
// reverse exports — LRC / Enhanced LRC / SRT / WebVTT / ASS) live in
// sibling modules under `lyricsfile/`.
//
// ## Why we mirror LRCGET's struct layout exactly
//
// The format is *experimental* (the LRCGET 2.0.0 release notes warn:
// *"This is a new format; expect breaking changes in future versions as
// the specification is refined"*). The safest way to stay compatible is to
// mirror the reference parser's struct shape exactly, so any file we
// produce parses identically in LRCGET. When the spec churns, we update
// the constants in one place and the consumers cascade.
//
// ## Forward-compatibility policy
//
// Every optional field carries `#[serde(skip_serializing_if = "Option::is_none")]`
// on the write side and `#[serde(default)]` on the read side. Unknown
// fields on a future Lyricsfile version are silently ignored at parse
// time (default `serde_yaml` behaviour) so a v1.1-produced file still
// parses through a v1.0 reader.
//
// ## References
//
// - LRCGET v2.0.0 release: <https://github.com/tranxuanthang/lrcget/releases/tag/2.0.0>
// - LRCGET reference implementation: <https://github.com/tranxuanthang/lrcget/blob/main/src-tauri/src/lyricsfile.rs>
// - LRCLIB co-endorsement: <https://lrclib.net/>

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Spec version this module is built against. Embedded in the YAML
/// `version:` header so consumers can detect drift across releases.
pub const LYRICSFILE_VERSION: &str = "1.0";

/// Sentinel string LRCGET uses inside its LRC export when a track is
/// marked instrumental. Round-trip consumers (LRC → Lyricsfile →
/// instrumental marking) recognise this string and lift it back into
/// `metadata.instrumental = true`.
pub const INSTRUMENTAL_MARKER: &str = "[au: instrumental]";

// ============================================================
// Public types — mirror LRCGET's reference parser
// ============================================================

/// A single Lyricsfile document.
///
/// Lifecycle: `from_ttml` / `from_lrc` / `parse` → owned `Lyricsfile` →
/// `to_yaml` / `to_lrc` / `to_enhanced_lrc` / `to_srt` / `to_webvtt` /
/// `to_ass`. The struct is `Clone` so callers can fan out into multiple
/// export formats without re-parsing the source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lyricsfile {
    /// Spec version (e.g., `"1.0"`). Always set to [`LYRICSFILE_VERSION`]
    /// on write; the read path tolerates other values for forward-compat.
    pub version: String,

    /// Track-level metadata: title, artist, album, duration, etc.
    pub metadata: LyricsfileMetadata,

    /// Synced lyrics. Empty when the track is instrumental or
    /// plain-text-only. Multiple entries with overlapping start_ms ranges
    /// are valid and represent simultaneous vocal lines (duets,
    /// call-and-response).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<LyricsfileLine>,

    /// Plain-text lyrics. Used when the source has plain text without
    /// timing data, or when synced and plain are intentionally different
    /// (e.g., synced version omits `[verse]` / `[chorus]` labels).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain: Option<String>,
}

/// Track-level metadata header.
///
/// All optional fields except `title`, `artist`, and `instrumental`. The
/// LRCGET reference parser treats missing-but-required fields as YAML
/// errors; we do the same to stay drop-in compatible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricsfileMetadata {
    pub title: String,
    pub artist: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,

    /// Total track duration in milliseconds. Used by players for
    /// playhead alignment and by LRCLIB for de-duplication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,

    /// Global timing offset in milliseconds. Positive values shift the
    /// lyrics later relative to the audio; negative shifts earlier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_ms: Option<i64>,

    /// ISO-639 language code (e.g., `"en"`, `"ja"`, `"zh-Hans"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Set `true` when the track has no vocals. When `true`, `lines` is
    /// expected to be empty and the file is the lyric-format equivalent
    /// of a "no lyrics for this track" marker.
    #[serde(default)]
    pub instrumental: bool,
}

/// A single synced line. Always has a start time; end time is optional
/// (when omitted, the line ends at the next line's start or at the
/// track's end). The optional `words` vector carries karaoke-style
/// word-level timing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricsfileLine {
    pub text: String,
    pub start_ms: i64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,

    /// Word-level breakdown. When present, players highlight word-by-word
    /// as the line plays. The concatenation of `words[*].text` (with
    /// spaces) is expected to equal `text` modulo whitespace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<LyricsfileWord>,
}

/// A single timed word inside a line.
///
/// The optional `syllables` vector carries sub-word atomic timing
/// fragments — used by Apple Music's `/syllable-lyrics` endpoint where
/// a single word can be split into multiple `<span begin>` elements
/// (e.g. "Closer" → "Clos" + "er", each with its own millisecond
/// timestamp) for karaoke-style highlighting.
///
/// When `syllables` is non-empty:
/// - `text` is the concatenation of `syllables[*].text` (no separator —
///   the absence of inter-syllable whitespace is the signal that they
///   compose a single word rather than two adjacent words).
/// - `start_ms` equals `syllables.first().start_ms`.
/// - `end_ms` equals `syllables.last().end_ms` (when set).
///
/// When `syllables` is empty (the v1.0 default), the word has line- or
/// word-level granularity only; renderers should fall through to
/// `text` + `start_ms` + `end_ms` as before.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricsfileWord {
    pub text: String,
    pub start_ms: i64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,

    /// Sub-word atomic timing fragments. Empty for word-level files;
    /// non-empty when the source carried syllable-level timing (e.g.
    /// Apple Music's `/syllable-lyrics?extend=ttmlLocalizations`).
    ///
    /// `#[serde(default, skip_serializing_if = "Vec::is_empty")]` keeps
    /// the wire format byte-identical for word-only files: word-level
    /// YAML emitted today is unchanged going forward (no `syllables:`
    /// key when the vec is empty), and v1.0 readers ignore the field
    /// on a v1.0+syllables file per the existing forward-compat policy
    /// (`forward_compat_unknown_field_in_line_does_not_fail`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub syllables: Vec<LyricsfileSyllable>,
}

/// A single timed syllable inside a word. Same shape as
/// [`LyricsfileWord`] but represents one phonetic/visual fragment of a
/// word rather than a whole word.
///
/// Syllables are emitted by Apple Music's `/syllable-lyrics` endpoint
/// for tracks where word-level highlighting would feel coarse (long
/// drawn-out vowels in choruses, hold-note ornaments, multi-syllable
/// words in slow ballads). Concatenating `syllables[*].text` inside a
/// word reconstructs the word text byte-for-byte with no inter-syllable
/// separator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricsfileSyllable {
    pub text: String,
    pub start_ms: i64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
}

// ============================================================
// Construction + YAML I/O
// ============================================================

impl Lyricsfile {
    /// Build an empty Lyricsfile shell with the given track metadata.
    /// Callers populate `lines` / `plain` separately (or use one of the
    /// `from_*` converters).
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            version: LYRICSFILE_VERSION.to_string(),
            metadata: LyricsfileMetadata {
                title: title.into(),
                artist: artist.into(),
                album: None,
                duration_ms: None,
                offset_ms: None,
                language: None,
                instrumental: false,
            },
            lines: Vec::new(),
            plain: None,
        }
    }

    /// Serialise to a YAML string. Always emits `version:
    /// "<LYRICSFILE_VERSION>"` as the first field.
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self).map_err(|e| Error::LyricsfileYaml(e.to_string()))
    }

    /// Parse a YAML string. Unknown fields are silently ignored
    /// (forward-compat).
    pub fn parse(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| Error::LyricsfileYaml(e.to_string()))
    }

    /// Mark this Lyricsfile as instrumental. Clears `lines` and `plain`
    /// since neither is meaningful when the track has no vocals.
    pub fn mark_instrumental(&mut self) {
        self.metadata.instrumental = true;
        self.lines.clear();
        self.plain = None;
    }

    /// `true` when this Lyricsfile has at least one line with non-empty
    /// `words`. Used by consumers to decide whether word-level exports
    /// (Enhanced LRC, karaoke-style WebVTT) are meaningful.
    pub fn has_word_level_timing(&self) -> bool {
        self.lines.iter().any(|l| !l.words.is_empty())
    }

    /// `true` when this Lyricsfile has at least one word with non-empty
    /// `syllables`. Strict superset of [`has_word_level_timing`] — a
    /// syllable-level file is by definition also word-level. Consumers
    /// pick the richer export (e.g. syllable Enhanced LRC) when this
    /// returns `true`.
    pub fn has_syllable_level_timing(&self) -> bool {
        self.lines
            .iter()
            .any(|l| l.words.iter().any(|w| !w.syllables.is_empty()))
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Lyricsfile {
        Lyricsfile {
            version: LYRICSFILE_VERSION.to_string(),
            metadata: LyricsfileMetadata {
                title: "Hello".into(),
                artist: "Adele".into(),
                album: Some("25".into()),
                duration_ms: Some(295_000),
                offset_ms: None,
                language: Some("en".into()),
                instrumental: false,
            },
            lines: vec![
                LyricsfileLine {
                    text: "Hello, it's me".into(),
                    start_ms: 1000,
                    end_ms: Some(3500),
                    words: vec![
                        LyricsfileWord {
                            text: "Hello,".into(),
                            start_ms: 1000,
                            end_ms: Some(1800),
                            syllables: Vec::new(),
                        },
                        LyricsfileWord {
                            text: "it's".into(),
                            start_ms: 1900,
                            end_ms: Some(2400),
                            syllables: Vec::new(),
                        },
                        LyricsfileWord {
                            text: "me".into(),
                            start_ms: 2500,
                            end_ms: Some(3500),
                            syllables: Vec::new(),
                        },
                    ],
                },
                LyricsfileLine {
                    text: "I was wondering".into(),
                    start_ms: 4000,
                    end_ms: None,
                    words: Vec::new(),
                },
            ],
            plain: Some("Hello, it's me\nI was wondering".into()),
        }
    }

    #[test]
    fn roundtrip_through_yaml() {
        let input = sample();
        let yaml = input.to_yaml().expect("serialise");
        let parsed = Lyricsfile::parse(&yaml).expect("parse");
        assert_eq!(input, parsed);
    }

    #[test]
    fn version_is_always_emitted_first_field() {
        let yaml = sample().to_yaml().unwrap();
        // First non-empty line should be `version: ...`. serde_yaml
        // emits struct fields in declaration order, so this is stable.
        let first = yaml.lines().find(|l| !l.is_empty()).unwrap();
        assert!(
            first.starts_with("version:"),
            "expected version first, got: {first}"
        );
    }

    #[test]
    fn optional_metadata_fields_are_omitted_when_none() {
        let mut lf = Lyricsfile::new("Title", "Artist");
        lf.metadata.album = None;
        let yaml = lf.to_yaml().unwrap();
        assert!(!yaml.contains("album:"), "got: {yaml}");
        assert!(!yaml.contains("duration_ms:"), "got: {yaml}");
        assert!(!yaml.contains("language:"), "got: {yaml}");
    }

    #[test]
    fn empty_lines_vec_is_omitted_from_output() {
        let lf = Lyricsfile::new("T", "A");
        let yaml = lf.to_yaml().unwrap();
        assert!(!yaml.contains("lines:"), "got: {yaml}");
    }

    #[test]
    fn empty_words_vec_is_omitted_from_output() {
        let mut lf = Lyricsfile::new("T", "A");
        lf.lines.push(LyricsfileLine {
            text: "no word breakdown".into(),
            start_ms: 0,
            end_ms: None,
            words: Vec::new(),
        });
        let yaml = lf.to_yaml().unwrap();
        assert!(!yaml.contains("words:"), "got: {yaml}");
    }

    #[test]
    fn forward_compat_unknown_field_does_not_fail() {
        let yaml = r#"
version: "2.0"
metadata:
  title: T
  artist: A
  instrumental: false
  future_field_we_dont_understand: yes
lines: []
"#;
        let lf = Lyricsfile::parse(yaml).expect("forward-compat parse");
        assert_eq!(lf.metadata.title, "T");
        assert_eq!(lf.version, "2.0");
    }

    #[test]
    fn forward_compat_unknown_field_in_line_does_not_fail() {
        let yaml = r#"
version: "1.0"
metadata: { title: T, artist: A, instrumental: false }
lines:
  - text: "hi"
    start_ms: 0
    unknown_per_line_field: 42
"#;
        let lf = Lyricsfile::parse(yaml).expect("forward-compat parse");
        assert_eq!(lf.lines.len(), 1);
        assert_eq!(lf.lines[0].text, "hi");
    }

    #[test]
    fn mark_instrumental_clears_lines_and_plain() {
        let mut lf = sample();
        lf.mark_instrumental();
        assert!(lf.metadata.instrumental);
        assert!(lf.lines.is_empty());
        assert!(lf.plain.is_none());
    }

    #[test]
    fn has_word_level_timing_detects_words() {
        assert!(sample().has_word_level_timing());

        let mut no_words = sample();
        for line in &mut no_words.lines {
            line.words.clear();
        }
        assert!(!no_words.has_word_level_timing());
    }

    // ------------------------------------------------------------
    // Syllable schema tests (#60)
    // ------------------------------------------------------------

    fn syllable_sample() -> Lyricsfile {
        // "Closer" split into "Clos" + "er" syllables, mirroring the
        // actual Apple Music TTML at .examplefiles/.../Closer_PrettyPrint.ttml
        // line 20: `<span begin="7.516" end="8.097">Clos</span><span
        // begin="8.097" end="8.904">er</span>`.
        Lyricsfile {
            version: LYRICSFILE_VERSION.to_string(),
            metadata: LyricsfileMetadata {
                title: "Closer".into(),
                artist: "Ne-Yo".into(),
                album: None,
                duration_ms: None,
                offset_ms: None,
                language: Some("en".into()),
                instrumental: false,
            },
            lines: vec![LyricsfileLine {
                text: "Closer".into(),
                start_ms: 7_516,
                end_ms: Some(8_904),
                words: vec![LyricsfileWord {
                    text: "Closer".into(),
                    start_ms: 7_516,
                    end_ms: Some(8_904),
                    syllables: vec![
                        LyricsfileSyllable {
                            text: "Clos".into(),
                            start_ms: 7_516,
                            end_ms: Some(8_097),
                        },
                        LyricsfileSyllable {
                            text: "er".into(),
                            start_ms: 8_097,
                            end_ms: Some(8_904),
                        },
                    ],
                }],
            }],
            plain: None,
        }
    }

    #[test]
    fn empty_syllables_vec_is_omitted_from_output() {
        // Pin the wire compat property: a word-level Lyricsfile (no
        // syllables) must serialise byte-identical pre/post #60.
        let mut lf = Lyricsfile::new("T", "A");
        lf.lines.push(LyricsfileLine {
            text: "hi".into(),
            start_ms: 0,
            end_ms: None,
            words: vec![LyricsfileWord {
                text: "hi".into(),
                start_ms: 0,
                end_ms: None,
                syllables: Vec::new(),
            }],
        });
        let yaml = lf.to_yaml().unwrap();
        assert!(
            !yaml.contains("syllables:"),
            "empty syllables vec must not emit the key: {yaml}"
        );
    }

    #[test]
    fn has_syllable_level_timing_detects_syllables() {
        assert!(syllable_sample().has_syllable_level_timing());
        // Word-only file must NOT report syllable-level.
        assert!(!sample().has_syllable_level_timing());
        // Line-only file must NOT report syllable-level.
        let mut line_only = sample();
        for line in &mut line_only.lines {
            line.words.clear();
        }
        assert!(!line_only.has_syllable_level_timing());
    }

    #[test]
    fn syllable_roundtrip_through_yaml() {
        let input = syllable_sample();
        let yaml = input.to_yaml().expect("serialise");
        // The syllables key must appear when syllables exist.
        assert!(yaml.contains("syllables:"), "missing syllables key: {yaml}");
        let parsed = Lyricsfile::parse(&yaml).expect("parse");
        assert_eq!(input, parsed);
    }

    #[test]
    fn syllable_schema_is_forward_compat_with_word_only_readers() {
        // A v1.0 reader that knows about words but not syllables must
        // still successfully parse a v1.0+syllables document. This pins
        // the additive-field policy that justifies keeping
        // LYRICSFILE_VERSION at 1.0.
        let yaml = r#"
version: "1.0"
metadata: { title: T, artist: A, instrumental: false }
lines:
  - text: "hi"
    start_ms: 0
    words:
      - text: "hi"
        start_ms: 0
        syllables:
          - text: "h"
            start_ms: 0
          - text: "i"
            start_ms: 100
"#;
        let lf = Lyricsfile::parse(yaml).expect("forward-compat parse");
        assert_eq!(lf.lines[0].words[0].syllables.len(), 2);
        assert_eq!(lf.lines[0].words[0].syllables[1].text, "i");
        assert_eq!(lf.lines[0].words[0].syllables[1].start_ms, 100);
    }

    #[test]
    fn new_builds_minimal_valid_document() {
        let lf = Lyricsfile::new("Track", "Artist");
        assert_eq!(lf.version, LYRICSFILE_VERSION);
        assert_eq!(lf.metadata.title, "Track");
        assert_eq!(lf.metadata.artist, "Artist");
        assert!(!lf.metadata.instrumental);
        assert!(lf.lines.is_empty());
    }

    #[test]
    fn instrumental_marker_constant_matches_lrcget() {
        // Pin the magic string against LRCGET v2.0.0's reference.
        // If LRCGET ever changes this we need to update + run a migration.
        assert_eq!(INSTRUMENTAL_MARKER, "[au: instrumental]");
    }

    #[test]
    fn malformed_yaml_returns_lyricsfile_yaml_error() {
        let err = Lyricsfile::parse("not yaml at all: :\n  :").unwrap_err();
        assert!(matches!(err, Error::LyricsfileYaml(_)), "got: {err:?}");
    }

    #[test]
    fn missing_required_metadata_field_errors() {
        // title is required; omitting it should fail at parse time.
        let yaml = r#"
version: "1.0"
metadata:
  artist: A
  instrumental: false
"#;
        assert!(matches!(
            Lyricsfile::parse(yaml).unwrap_err(),
            Error::LyricsfileYaml(_)
        ));
    }
}
