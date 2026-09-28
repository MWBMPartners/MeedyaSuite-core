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

    /// BCP 47 language tag in canonical form (e.g., `"en"`, `"ja"`,
    /// `"zh-Hans"`) — see policy MWBM-MEDIA-LANG
    /// (`docs/standards/media-language-bcp47-policy.md`). `zh-Hans` is a
    /// BCP 47 tag (a language plus a script subtag), not an ISO 639 code
    /// on its own — ISO 639 only ever supplies the first part of a tag.
    ///
    /// Always a real tag, never free text, when it comes from this crate:
    /// [`Lyricsfile::from_ttml`] and [`Lyricsfile::parse`] both read the
    /// source's language through the LANG-002 reader
    /// (`meedya_lang::from_legacy_three_letter`). A recognised value is
    /// stored canonical (`EN-gb` → `en-GB`, `eng` → `en`). When the source
    /// gave a language this crate does not recognise (`xml:lang="zzz"`,
    /// `language: English` in a hand-written file), this is `und` —
    /// LANG-002: "The structured value is `und`, and the original text
    /// SHOULD be kept alongside" — and the text the source gave is kept in
    /// [`language_original`](Self::language_original), never guessed at
    /// (COMPAT-040). (Until Codex's review r7 the unrecognised text itself
    /// was stored here, so a reader of this field could not trust it to be
    /// a language; until the stand-in review of revision 5, `parse` still
    /// stored whatever text a file held.) `None` when the source gave no
    /// language at all. A caller who builds the struct by hand can put
    /// anything here; nothing checks it in memory, but
    /// [`Lyricsfile::to_yaml`] reads it the way `parse` does before writing
    /// it, so a file never says `language: English` — it says `und`, with
    /// `English` kept in `language_original` (from the stand-in review of
    /// revision 6; until then `to_yaml` wrote the text as it was).
    ///
    /// [`Lyricsfile::to_yaml`] always writes this value in quotes
    /// (`language: 'no'`), because a YAML 1.1 reader such as PyYAML reads
    /// an unquoted `no` — Norwegian — as the value false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// The language text exactly as the source gave it, when it was not
    /// recognised — in which case [`language`](Self::language) is `und`
    /// (LANG-002: the original text is kept alongside, so nothing is lost
    /// and a person can fix it).
    ///
    /// This paragraph describes what [`Lyricsfile::from_ttml`] gives: this
    /// field is set ONLY when `xml:lang` was not recognised, and is `None`
    /// whenever `language` is a recognised tag or absent. ([`Lyricsfile::from_lrc`]
    /// never sets either field: LRC has no language.)
    ///
    /// [`Lyricsfile::parse`] differs, because a file may already have this
    /// field: it is kept exactly as the file gives it, whatever `language`
    /// says. If `language` is then unrecognised too, `language` becomes
    /// `und` and its own text is not kept (the file already names its
    /// original text, and there is nowhere to keep a second one). If
    /// `language` is recognised, both are kept as they are — so after
    /// `parse`, unlike after `from_ttml`, a recognised `language` CAN come
    /// with a `language_original`. [`Lyricsfile::to_yaml`] applies the same
    /// rules as `parse` to what it writes, and writes this field in quotes,
    /// like `language`.
    ///
    /// **Not part of LRCGET's Lyricsfile 1.0 schema** — a MeedyaSuite
    /// addition, written to YAML only when present
    /// (`skip_serializing_if`), so a file with a recognised language (the
    /// usual case) has no such key.
    /// Extra keys are allowed: this module's forward-compatibility policy
    /// (top of this file) is that a reader ignores fields it does not
    /// know, the same reasoning `LyricsfileWord::syllables` already relies
    /// on, so a reader without this field still reads the file (it simply
    /// sees `language: und`). Proven for this crate's own reader by
    /// `forward_compat_unknown_field_does_not_fail`; LRCGET's reader is
    /// taken on that same stated policy, not re-tested here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_original: Option<String>,

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
                language_original: None,
                instrumental: false,
            },
            lines: Vec::new(),
            plain: None,
        }
    }

    /// Serialise to a YAML string. Always emits `version:
    /// "<LYRICSFILE_VERSION>"` as the first field.
    ///
    /// **The language is written as [`parse`](Self::parse) would read it.**
    /// Before writing, `to_yaml` reads `metadata.language` through the same
    /// LANG-002 reader `parse` uses, on its own copy (the struct is not
    /// changed): a recognised value is written canonical (`eng` → `en`), and
    /// one it does not recognise (`English`) is written as `und`, with the
    /// text in `language_original` unless that is already set. So a file
    /// never holds a language name or an old code in `language`, and
    /// `parse` reads back exactly what was written. (Until the stand-in
    /// review of revision 6 `to_yaml` wrote whatever the struct held — a
    /// caller who built it by hand could export `language: English`.)
    ///
    /// `metadata.language` and `metadata.language_original` are always
    /// written in quotes (`language: 'no'`). The YAML library this crate
    /// uses follows YAML 1.2, where `no` is just text, so it writes `no`
    /// bare; but YAML 1.1 readers — PyYAML, and older Ruby and JavaScript
    /// libraries — read a bare `no`, `yes`, `on`, `off`, `y` or `n` as true
    /// or false, so Norwegian (`no`) would come back as the value false.
    /// Quoted, every reader reads the text. (Found by the stand-in review
    /// of revision 5.) Other fields are written as the library writes them.
    /// A caller who serialises the struct with `serde_yaml` directly gets
    /// no quoting; use this method.
    pub fn to_yaml(&self) -> Result<String> {
        // Each of the two values is replaced by a stand-in word that the
        // YAML library always writes bare, on one line; each stand-in is
        // then replaced by its value, quoted. A stand-in is used only when
        // it occurs exactly once in the output (lyrics could, in theory,
        // contain the same letters) — otherwise the next number is tried,
        // so a stand-in is never mistaken for, or replaced inside, text.
        // What `parse` would make of the language — on a copy of the
        // metadata, so `self` is not changed.
        let mut metadata = self.metadata.clone();
        metadata.read_language();
        let values = [
            metadata.language.as_deref(),
            metadata.language_original.as_deref(),
        ];
        for attempt in 0..1000u32 {
            let stand_ins = [
                format!("meedya-lyricsfile-language-{attempt}"),
                format!("meedya-lyricsfile-language-original-{attempt}"),
            ];
            let mut copy = self.clone();
            copy.metadata.language = values[0].map(|_| stand_ins[0].clone());
            copy.metadata.language_original = values[1].map(|_| stand_ins[1].clone());
            let mut yaml =
                serde_yaml::to_string(&copy).map_err(|e| Error::LyricsfileYaml(e.to_string()))?;
            // Neither stand-in may appear anywhere but in its own place —
            // not elsewhere in the output, and not inside either value
            // (which would put a second copy in once the first is
            // replaced).
            let unique = stand_ins.iter().zip(values).all(|(stand_in, value)| {
                yaml.matches(stand_in.as_str()).count() == usize::from(value.is_some())
            }) && !values
                .iter()
                .flatten()
                .any(|value| stand_ins.iter().any(|w| value.contains(w.as_str())));
            if !unique {
                continue;
            }
            for (stand_in, value) in stand_ins.iter().zip(values) {
                if let Some(value) = value {
                    yaml = yaml.replacen(stand_in.as_str(), &yaml_quoted(value), 1);
                }
            }
            return Ok(yaml);
        }
        Err(Error::LyricsfileYaml(
            "could not find a stand-in word for the language that the lyrics do not already \
             contain"
                .to_string(),
        ))
    }

    /// Parse a YAML string. Unknown fields are silently ignored
    /// (forward-compat).
    ///
    /// **The language is read through the LANG-002 reader**
    /// (`meedya_lang::from_legacy_three_letter`), as it is for TTML, so
    /// `metadata.language` is a real tag or `None` whatever the file says
    /// (policy MWBM-MEDIA-LANG; the stand-in review of revision 5 found
    /// `parse` storing the file's text as it was):
    ///
    /// - absent (or `null`): stays `None`;
    /// - recognised: stored canonical (`EN-gb` → `en-GB`, `eng` → `en`);
    /// - not recognised (`English`, `zzz`, an empty value): stored as
    ///   `und`, and the file's text is moved to
    ///   `metadata.language_original` — unless the file already has a
    ///   `language_original`, which is then kept as it is (the
    ///   unrecognised text is not kept a second time).
    ///
    /// A `language_original` in the file is never changed or removed.
    pub fn parse(yaml: &str) -> Result<Self> {
        let mut lyricsfile: Self =
            serde_yaml::from_str(yaml).map_err(|e| Error::LyricsfileYaml(e.to_string()))?;
        lyricsfile.metadata.read_language();
        Ok(lyricsfile)
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

impl LyricsfileMetadata {
    /// Reads `language` through the LANG-002 reader, in place — the rules
    /// [`Lyricsfile::parse`] documents.
    fn read_language(&mut self) {
        let Some(text) = self.language.take() else {
            return;
        };
        match meedya_lang::from_legacy_three_letter(&text) {
            Some(tag) => self.language = Some(tag.tag),
            None => {
                self.language = Some("und".to_string());
                if self.language_original.is_none() {
                    self.language_original = Some(text);
                }
            }
        }
    }
}

/// `value` as a quoted YAML scalar that every YAML reader, 1.1 or 1.2,
/// reads back as exactly `value`: single quotes (a `'` inside doubled)
/// when every character may appear there as it is, otherwise double
/// quotes with each other character escaped (`\uXXXX` and so on) — for a
/// line break, a control character, or one YAML does not allow in a file
/// at all.
fn yaml_quoted(value: &str) -> String {
    // YAML's printable characters (YAML 1.2 section 5.1, the same set as
    // 1.1), less the line breaks 1.1 recognises (U+0085, U+2028, U+2029),
    // the byte-order mark and the tab (kept simple: escaped).
    fn as_is(c: char) -> bool {
        matches!(c,
            '\u{20}'..='\u{7E}'
            | '\u{A0}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}')
            && !matches!(c, '\u{2028}' | '\u{2029}' | '\u{FEFF}')
    }
    if value.chars().all(as_is) {
        return format!("'{}'", value.replace('\'', "''"));
    }
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if as_is(c) => out.push(c),
            c if u32::from(c) <= 0xFF => out.push_str(&format!("\\x{:02X}", u32::from(c))),
            c if u32::from(c) <= 0xFFFF => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push_str(&format!("\\U{:08X}", u32::from(c))),
        }
    }
    out.push('"');
    out
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
                language_original: None,
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
                language_original: None,
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

    // ------------------------------------------------------------
    // The language, read and written (policy MWBM-MEDIA-LANG; the
    // stand-in review of revision 5)
    // ------------------------------------------------------------

    /// The `(language, language_original)` `parse` gives for a document
    /// whose metadata carries `fields` (YAML lines, already indented).
    fn parsed_language(fields: &str) -> (Option<String>, Option<String>) {
        let yaml = format!(
            "version: \"1.0\"\nmetadata:\n  title: T\n  artist: A\n  instrumental: false\n{fields}"
        );
        let lf = Lyricsfile::parse(&yaml).expect("parse");
        (lf.metadata.language, lf.metadata.language_original)
    }

    #[test]
    fn parse_reads_the_language_through_the_policy_reader() {
        let some = |s: &str| Some(s.to_string());
        // Recognised: stored canonical; nothing else kept.
        assert_eq!(
            parsed_language("  language: EN-gb\n"),
            (some("en-GB"), None)
        );
        assert_eq!(parsed_language("  language: eng\n"), (some("en"), None));
        // Not recognised: `und`, the text moved to `language_original`.
        assert_eq!(
            parsed_language("  language: English\n"),
            (some("und"), some("English"))
        );
        assert_eq!(parsed_language("  language: ''\n"), (some("und"), some("")));
        // Absent, or null: stays absent.
        assert_eq!(parsed_language(""), (None, None));
        assert_eq!(parsed_language("  language: null\n"), (None, None));
        // A `language_original` the file already has is kept as it is —
        // with an unrecognised language (which becomes `und`, its own text
        // not kept a second time) and with a recognised one.
        assert_eq!(
            parsed_language("  language: zzz\n  language_original: Zed\n"),
            (some("und"), some("Zed"))
        );
        assert_eq!(
            parsed_language("  language: EN\n  language_original: English\n"),
            (some("en"), some("English"))
        );
    }

    #[test]
    fn to_yaml_writes_the_language_as_parse_reads_it() {
        // Found by the stand-in review of revision 6: a struct built by
        // hand with a language NAME or an old code was exported as it was
        // (`language: 'English'`), a value `parse` then turned into
        // something else. Now the file says what `parse` makes of it.
        let some = |s: &str| Some(s.to_string());
        let cases = [
            // (language, language_original) in the struct → in the file
            ((some("English"), None), ("'und'", some("'English'"))),
            ((some("eng"), None), ("'en'", None)),
            ((some("EN-gb"), None), ("'en-GB'", None)),
            // An original already set is kept; the unrecognised language's
            // own text is not kept a second time (as in `parse`).
            ((some("zzz"), some("Zed")), ("'und'", some("'Zed'"))),
        ];
        for ((language, original), (written, written_original)) in cases {
            let mut lf = Lyricsfile::new("T", "A");
            lf.metadata.language = language.clone();
            lf.metadata.language_original = original.clone();
            let yaml = lf.to_yaml().expect("to_yaml");
            assert!(
                yaml.contains(&format!("  language: {written}\n")),
                "{language:?}: {yaml}"
            );
            match &written_original {
                Some(text) => assert!(
                    yaml.contains(&format!("  language_original: {text}\n")),
                    "{language:?}: {yaml}"
                ),
                None => assert!(!yaml.contains("language_original"), "{language:?}: {yaml}"),
            }
            // The struct itself is not changed…
            assert_eq!(lf.metadata.language, language);
            assert_eq!(lf.metadata.language_original, original);
            // …and what `parse` reads back is written again unchanged.
            let back = Lyricsfile::parse(&yaml).expect("parse");
            assert_eq!(back.to_yaml().expect("to_yaml"), yaml, "{language:?}");
        }
    }

    #[test]
    fn yaml_1_1_boolean_words_are_quoted_and_round_trip() {
        // A YAML 1.1 reader (PyYAML) reads a bare `no` — Norwegian — as
        // false, and the same for the other five words. Recognised
        // languages among them go in `language`; the rest are not
        // languages, so they are kept in `language_original` beside `und`.
        let cases = [
            ("no", true),
            ("on", true),
            ("yes", true),
            ("off", false),
            ("y", false),
            ("n", false),
        ];
        for (word, recognised) in cases {
            let mut lf = Lyricsfile::new("T", "A");
            if recognised {
                lf.metadata.language = Some(word.into());
            } else {
                lf.metadata.language = Some("und".into());
                lf.metadata.language_original = Some(word.into());
            }
            let yaml = lf.to_yaml().expect("to_yaml");
            let line = if recognised {
                format!("  language: '{word}'\n")
            } else {
                format!("  language_original: '{word}'\n")
            };
            assert!(yaml.contains(&line), "{word}: {yaml}");
            assert!(yaml.contains("  language: '"), "{word}: {yaml}");
            assert_eq!(Lyricsfile::parse(&yaml).expect("parse"), lf, "{word}");
        }
    }

    #[test]
    fn a_language_value_of_any_text_round_trips_quoted() {
        // An apostrophe (doubled inside single quotes), and characters
        // single quotes cannot hold as they are (a line break, a tab, a
        // control character, U+0085 and U+2028, which YAML 1.1 counts as
        // line breaks) — those take double quotes with escapes.
        for text in [
            "it's",
            "two\nlines",
            "tab\there",
            "bell\u{7}",
            "next\u{85}line",
            "sep\u{2028}arator",
            "back\\slash \"quoted\"",
            "Français — 日本語",
            "meedya-lyricsfile-language-0",
        ] {
            let mut lf = Lyricsfile::new("T", "A");
            lf.metadata.language = Some("und".into());
            lf.metadata.language_original = Some(text.into());
            let yaml = lf.to_yaml().expect("to_yaml");
            assert_eq!(
                Lyricsfile::parse(&yaml).expect("parse"),
                lf,
                "{text:?}: {yaml}"
            );
        }
        // The stand-in word the quoting uses, also in the lyrics: the
        // language is still written in its own place.
        let mut lf = Lyricsfile::new("T", "A");
        lf.metadata.language = Some("fr".into());
        lf.plain = Some("meedya-lyricsfile-language-0 meedya-lyricsfile-language-1".into());
        let yaml = lf.to_yaml().expect("to_yaml");
        assert!(yaml.contains("  language: 'fr'\n"), "{yaml}");
        assert_eq!(Lyricsfile::parse(&yaml).expect("parse"), lf);
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
