// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Unified data model for extended (DJ-flavoured) tag metadata.
//
// Each datapoint may originate from multiple sources (Serato, Rekordbox,
// Traktor, Virtual DJ, Mixed In Key, MeedyaMeta). Readers populate fields
// from whichever sources they parsed; consumers should consult the `source`
// field (where present) to disambiguate when multiple origins exist.

use std::fmt;

/// Aggregated extended tag data for a single audio file.
///
/// Fields default to `None` / empty when the underlying source did not
/// supply that datapoint. A reader that fails to parse a particular source
/// leaves its corresponding contributions out without erroring the rest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtendedTags {
    /// Beats per minute. Float to allow non-integer values from Serato/MIK.
    pub bpm: Option<f64>,
    /// Musical key, normalized across Camelot / Open Key / traditional notations.
    pub key: Option<MusicalKey>,
    /// Energy rating (typically 1-10). No widely standardised storage; readers
    /// extract from source-specific fields where supported.
    pub energy: Option<u8>,
    /// Cue points (memory cues, hot cues), aggregated from all sources.
    pub cue_points: Vec<CuePoint>,
    /// Loop regions.
    pub loops: Vec<LoopPoint>,
    /// Beat grid, if any source provided one.
    pub beat_grid: Option<BeatGrid>,
    /// Free-text comment from the standard `COMM` / `©cmt` / `comment` field.
    pub comment: Option<String>,
}

/// Origin of a particular tag value, used when aggregating across sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// MeedyaSuite-native tags (`MeedyaMeta:*`).
    MeedyaMeta,
    /// Standard, non-proprietary tags (TBPM/TKEY/tmpo/initialkey/INITIALKEY).
    Standard,
    Serato,
    Rekordbox,
    Traktor,
    VirtualDj,
    /// Mixed In Key only writes standard tags; this variant is reserved for
    /// future use if MIK ever stamps an identifying marker we can detect.
    MixedInKey,
    Unknown,
}

// ============================================================
// Cue Points
// ============================================================

/// A single cue point. Hot cues are numbered (0-7 typically); memory cues
/// have `hot_cue_index = None`.
#[derive(Debug, Clone, PartialEq)]
pub struct CuePoint {
    pub position_ms: u64,
    pub label: Option<String>,
    pub color: Option<Rgb>,
    /// Hot cue slot, when applicable (DJ apps usually expose 0-7 or 0-15).
    pub hot_cue_index: Option<u8>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoopPoint {
    pub start_ms: u64,
    pub end_ms: u64,
    pub label: Option<String>,
    pub color: Option<Rgb>,
    pub hot_cue_index: Option<u8>,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

// ============================================================
// Beat Grid
// ============================================================

/// A beat grid: anchor points marking downbeats with the BPM in effect.
/// Most rippers/analysers store one anchor + a constant BPM; tracks with
/// tempo changes carry multiple anchors.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatGrid {
    pub markers: Vec<BeatGridMarker>,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatGridMarker {
    pub position_ms: u64,
    pub bpm: f64,
}

// ============================================================
// Musical Key
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MusicalKey {
    pub tonic: Note,
    pub mode: KeyMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Note {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyMode {
    Major,
    Minor,
}

impl MusicalKey {
    /// Parse a key string in any of the common DJ notations:
    /// - Camelot: `8A`, `12B`, `1A`, `1B`
    /// - Open Key: `8d`, `12m`, `1d`, `1m` (d = major, m = minor)
    /// - Traditional: `Am`, `C`, `F#m`, `Db`, `Bbm` (case-insensitive)
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return None;
        }
        Self::parse_camelot(trimmed)
            .or_else(|| Self::parse_open_key(trimmed))
            .or_else(|| Self::parse_traditional(trimmed))
    }

    fn parse_camelot(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if !(2..=3).contains(&bytes.len()) {
            return None;
        }
        let last = bytes[bytes.len() - 1];
        let mode = match last {
            b'A' | b'a' => KeyMode::Minor,
            b'B' | b'b' => KeyMode::Major,
            _ => return None,
        };
        let num: u8 = s[..s.len() - 1].parse().ok()?;
        camelot_to_key(num, mode)
    }

    fn parse_open_key(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if !(2..=3).contains(&bytes.len()) {
            return None;
        }
        let last = bytes[bytes.len() - 1];
        let mode = match last {
            b'd' | b'D' => KeyMode::Major,
            b'm' | b'M' => KeyMode::Minor,
            _ => return None,
        };
        let num: u8 = s[..s.len() - 1].parse().ok()?;
        // Open Key uses the same numbering as Camelot but swaps letter semantics.
        camelot_to_key(num, mode)
    }

    fn parse_traditional(s: &str) -> Option<Self> {
        let normalized = s.replace('♯', "#").replace('♭', "b");
        let mut chars = normalized.chars();
        let first = chars.next()?.to_ascii_uppercase();
        let mut tonic_str = String::from(first);
        let mut rest: String = chars.collect();
        if rest.starts_with('#') || rest.starts_with('b') || rest.starts_with('B') {
            let accidental = rest.remove(0);
            // Lowercase 'b' is flat; uppercase 'B' is the note B (only valid
            // immediately after another note letter for traditional notation
            // like "Eb", "Ab" — never "BB"). Treat uppercase B after a letter
            // the same as lowercase b.
            let acc = if accidental == 'b' || accidental == 'B' {
                'b'
            } else {
                accidental
            };
            tonic_str.push(acc);
        }
        let tonic = Note::parse(&tonic_str)?;

        let trimmed = rest.trim_start();
        let mode = if trimmed.is_empty() {
            KeyMode::Major
        } else if trimmed.eq_ignore_ascii_case("m")
            || trimmed.eq_ignore_ascii_case("min")
            || trimmed.eq_ignore_ascii_case("minor")
        {
            KeyMode::Minor
        } else if trimmed.eq_ignore_ascii_case("maj") || trimmed.eq_ignore_ascii_case("major") {
            KeyMode::Major
        } else {
            return None;
        };

        Some(MusicalKey { tonic, mode })
    }

    /// Camelot wheel position, e.g., "8A" (A minor), "8B" (C major).
    pub fn camelot(&self) -> String {
        let (num, _) = key_to_camelot_pair(*self);
        let letter = match self.mode {
            KeyMode::Minor => 'A',
            KeyMode::Major => 'B',
        };
        format!("{num}{letter}")
    }

    /// Open Key notation, e.g., "8m" (A minor), "8d" (C major).
    pub fn open_key(&self) -> String {
        let (num, _) = key_to_camelot_pair(*self);
        let letter = match self.mode {
            KeyMode::Major => 'd',
            KeyMode::Minor => 'm',
        };
        format!("{num}{letter}")
    }

    /// Traditional notation, e.g., "Am" (A minor), "C" (C major).
    pub fn traditional(&self) -> String {
        let mut s = self.tonic.symbol().to_string();
        if matches!(self.mode, KeyMode::Minor) {
            s.push('m');
        }
        s
    }
}

impl fmt::Display for MusicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.traditional())
    }
}

impl Note {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "C" => Some(Note::C),
            "C#" | "Db" => Some(Note::CSharp),
            "D" => Some(Note::D),
            "D#" | "Eb" => Some(Note::DSharp),
            "E" => Some(Note::E),
            "F" => Some(Note::F),
            "F#" | "Gb" => Some(Note::FSharp),
            "G" => Some(Note::G),
            "G#" | "Ab" => Some(Note::GSharp),
            "A" => Some(Note::A),
            "A#" | "Bb" => Some(Note::ASharp),
            "B" => Some(Note::B),
            _ => None,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Note::C => "C",
            Note::CSharp => "C#",
            Note::D => "D",
            Note::DSharp => "D#",
            Note::E => "E",
            Note::F => "F",
            Note::FSharp => "F#",
            Note::G => "G",
            Note::GSharp => "G#",
            Note::A => "A",
            Note::ASharp => "A#",
            Note::B => "B",
        }
    }
}

// ============================================================
// Camelot wheel mapping
// ============================================================
//
// Standard Camelot wheel:
//   1A=Abm  1B=B    2A=Ebm  2B=F#   3A=Bbm  3B=Db
//   4A=Fm   4B=Ab   5A=Cm   5B=Eb   6A=Gm   6B=Bb
//   7A=Dm   7B=F    8A=Am   8B=C    9A=Em   9B=G
//   10A=Bm  10B=D   11A=F#m 11B=A   12A=C#m 12B=E
//
// Open Key uses the same numbering with d/m swapped onto major/minor.

fn camelot_to_key(num: u8, mode: KeyMode) -> Option<MusicalKey> {
    if !(1..=12).contains(&num) {
        return None;
    }
    let tonic = match (num, mode) {
        (1, KeyMode::Minor) => Note::GSharp,  // Abm
        (1, KeyMode::Major) => Note::B,       // B
        (2, KeyMode::Minor) => Note::DSharp,  // Ebm
        (2, KeyMode::Major) => Note::FSharp,  // F#
        (3, KeyMode::Minor) => Note::ASharp,  // Bbm
        (3, KeyMode::Major) => Note::CSharp,  // Db
        (4, KeyMode::Minor) => Note::F,       // Fm
        (4, KeyMode::Major) => Note::GSharp,  // Ab
        (5, KeyMode::Minor) => Note::C,       // Cm
        (5, KeyMode::Major) => Note::DSharp,  // Eb
        (6, KeyMode::Minor) => Note::G,       // Gm
        (6, KeyMode::Major) => Note::ASharp,  // Bb
        (7, KeyMode::Minor) => Note::D,       // Dm
        (7, KeyMode::Major) => Note::F,       // F
        (8, KeyMode::Minor) => Note::A,       // Am
        (8, KeyMode::Major) => Note::C,       // C
        (9, KeyMode::Minor) => Note::E,       // Em
        (9, KeyMode::Major) => Note::G,       // G
        (10, KeyMode::Minor) => Note::B,      // Bm
        (10, KeyMode::Major) => Note::D,      // D
        (11, KeyMode::Minor) => Note::FSharp, // F#m
        (11, KeyMode::Major) => Note::A,      // A
        (12, KeyMode::Minor) => Note::CSharp, // C#m
        (12, KeyMode::Major) => Note::E,      // E
        _ => return None,
    };
    Some(MusicalKey { tonic, mode })
}

fn key_to_camelot_pair(key: MusicalKey) -> (u8, KeyMode) {
    let num = match (key.tonic, key.mode) {
        (Note::GSharp, KeyMode::Minor) | (Note::B, KeyMode::Major) => 1,
        (Note::DSharp, KeyMode::Minor) | (Note::FSharp, KeyMode::Major) => 2,
        (Note::ASharp, KeyMode::Minor) | (Note::CSharp, KeyMode::Major) => 3,
        (Note::F, KeyMode::Minor) | (Note::GSharp, KeyMode::Major) => 4,
        (Note::C, KeyMode::Minor) | (Note::DSharp, KeyMode::Major) => 5,
        (Note::G, KeyMode::Minor) | (Note::ASharp, KeyMode::Major) => 6,
        (Note::D, KeyMode::Minor) | (Note::F, KeyMode::Major) => 7,
        (Note::A, KeyMode::Minor) | (Note::C, KeyMode::Major) => 8,
        (Note::E, KeyMode::Minor) | (Note::G, KeyMode::Major) => 9,
        (Note::B, KeyMode::Minor) | (Note::D, KeyMode::Major) => 10,
        (Note::FSharp, KeyMode::Minor) | (Note::A, KeyMode::Major) => 11,
        (Note::CSharp, KeyMode::Minor) | (Note::E, KeyMode::Major) => 12,
    };
    (num, key.mode)
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_camelot_a_minor() {
        let k = MusicalKey::parse("8A").unwrap();
        assert_eq!(k.tonic, Note::A);
        assert_eq!(k.mode, KeyMode::Minor);
    }

    #[test]
    fn parse_camelot_c_major() {
        let k = MusicalKey::parse("8B").unwrap();
        assert_eq!(k.tonic, Note::C);
        assert_eq!(k.mode, KeyMode::Major);
    }

    #[test]
    fn parse_camelot_low_high_bounds() {
        assert_eq!(MusicalKey::parse("1A").unwrap().tonic, Note::GSharp);
        assert_eq!(MusicalKey::parse("12B").unwrap().tonic, Note::E);
    }

    #[test]
    fn parse_camelot_invalid_number() {
        assert!(MusicalKey::parse("0A").is_none());
        assert!(MusicalKey::parse("13A").is_none());
    }

    #[test]
    fn parse_open_key_major() {
        let k = MusicalKey::parse("8d").unwrap();
        assert_eq!(k.tonic, Note::C);
        assert_eq!(k.mode, KeyMode::Major);
    }

    #[test]
    fn parse_open_key_minor() {
        let k = MusicalKey::parse("8m").unwrap();
        assert_eq!(k.tonic, Note::A);
        assert_eq!(k.mode, KeyMode::Minor);
    }

    #[test]
    fn parse_traditional_major() {
        assert_eq!(
            MusicalKey::parse("C").unwrap(),
            MusicalKey {
                tonic: Note::C,
                mode: KeyMode::Major
            }
        );
    }

    #[test]
    fn parse_traditional_minor() {
        assert_eq!(
            MusicalKey::parse("Am").unwrap(),
            MusicalKey {
                tonic: Note::A,
                mode: KeyMode::Minor
            }
        );
    }

    #[test]
    fn parse_traditional_sharp() {
        assert_eq!(
            MusicalKey::parse("F#m").unwrap(),
            MusicalKey {
                tonic: Note::FSharp,
                mode: KeyMode::Minor
            }
        );
    }

    #[test]
    fn parse_traditional_flat() {
        assert_eq!(
            MusicalKey::parse("Bbm").unwrap(),
            MusicalKey {
                tonic: Note::ASharp,
                mode: KeyMode::Minor
            }
        );
        assert_eq!(
            MusicalKey::parse("Db").unwrap(),
            MusicalKey {
                tonic: Note::CSharp,
                mode: KeyMode::Major
            }
        );
    }

    #[test]
    fn parse_traditional_minor_long_form() {
        assert_eq!(MusicalKey::parse("A min").unwrap().mode, KeyMode::Minor);
        assert_eq!(MusicalKey::parse("A minor").unwrap().mode, KeyMode::Minor);
        assert_eq!(MusicalKey::parse("C maj").unwrap().mode, KeyMode::Major);
    }

    #[test]
    fn round_trip_camelot() {
        for num in 1..=12 {
            for mode in [KeyMode::Major, KeyMode::Minor] {
                let original = camelot_to_key(num, mode).unwrap();
                let s = original.camelot();
                let parsed = MusicalKey::parse(&s).unwrap();
                assert_eq!(original, parsed, "round-trip failed for {s}");
            }
        }
    }

    #[test]
    fn camelot_format() {
        let am = MusicalKey {
            tonic: Note::A,
            mode: KeyMode::Minor,
        };
        assert_eq!(am.camelot(), "8A");
        assert_eq!(am.open_key(), "8m");
        assert_eq!(am.traditional(), "Am");
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(MusicalKey::parse("").is_none());
        assert!(MusicalKey::parse("   ").is_none());
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(MusicalKey::parse("xyz").is_none());
        assert!(MusicalKey::parse("99Z").is_none());
    }
}
