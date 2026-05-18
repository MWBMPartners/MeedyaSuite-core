// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// CUE sheet parser and import adapter.
//
// Parses standard CUE sheets (EAC, XLD, dBpoweramp, foobar2000, manual)
// into a rich `CueSheet` structure that downstream consumers can use for:
//
//   - Soft playback trim (this crate's `import()` adapter — narrow case)
//   - Chapter authoring (future MeedyaConverter integration — see notes)
//   - Tag enrichment (titles, performers, ISRC, CATALOG)
//
// The grammar implemented here is the subset documented at
// https://www.gnu.org/software/ccd2cue/manual/html_node/CUE-sheet-format.html
// covering the directives that real-world rippers emit. Non-standard
// directives are preserved verbatim as `RemEntry` records when they appear
// after `REM`; unknown top-level directives generate warnings.
//
// ## Time precision
//
// CUE indexes are in MM:SS:FF format where FF is CD frames at 75 fps
// (1 frame ≈ 13.333 ms). The `CueTime` type preserves frame-accurate
// values; `to_milliseconds()` returns rounded ms for trim-bound use.
// Chapter authoring should consume `CueTime` directly to keep frame
// alignment with the source.

use std::path::{Path, PathBuf};

use crate::{EntryLocator, ImportReport, LibraryEntry, SourceInfo};

pub const KIND: &str = "cuesheet";

// ============================================================
// Public Data Model
// ============================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CueSheet {
    /// Disc-level barcode (UPC/EAN), from `CATALOG`.
    pub catalog: Option<String>,
    /// CD-Text disc title.
    pub title: Option<String>,
    /// CD-Text disc performer (album artist).
    pub performer: Option<String>,
    pub songwriter: Option<String>,
    /// Disc-level `REM` directives (e.g., `REM GENRE Pop`, `REM DATE 2023`).
    pub rems: Vec<RemEntry>,
    pub files: Vec<CueFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueFile {
    /// Path as written in the CUE — may be relative to the CUE's directory.
    pub path: String,
    pub format: FileFormat,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileFormat {
    Wave,
    Aiff,
    Mp3,
    Flac,
    Binary,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u8,
    pub kind: TrackKind,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub songwriter: Option<String>,
    pub isrc: Option<String>,
    pub flags: Vec<String>,
    /// Synthetic pregap (silence inserted before the track's audio).
    pub pregap: Option<CueTime>,
    /// Synthetic postgap (silence inserted after the track's audio).
    pub postgap: Option<CueTime>,
    /// All `INDEX NN MM:SS:FF` entries. Index 01 is the track's audio start;
    /// Index 00 is the pregap start (when the pregap is inside the file).
    pub indexes: Vec<CueIndex>,
    pub rems: Vec<RemEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueIndex {
    pub number: u8,
    pub time: CueTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CueTime {
    pub minutes: u32,
    pub seconds: u8,
    pub frames: u8,
}

impl CueTime {
    pub const ZERO: CueTime = CueTime {
        minutes: 0,
        seconds: 0,
        frames: 0,
    };

    /// Convert to milliseconds, rounded to the nearest ms.
    /// 1 CD frame = 1/75 second; rounding bias of +37 frame-millis.
    pub fn to_milliseconds(self) -> u64 {
        let total_frames = u64::from(self.minutes) * 60 * 75
            + u64::from(self.seconds) * 75
            + u64::from(self.frames);
        (total_frames * 1000 + 37) / 75
    }
}

// ============================================================
// Public API
// ============================================================

/// Parse a CUE sheet from a string.
pub fn parse_str(input: &str) -> Result<CueSheet, String> {
    let mut parser = Parser::new(input);
    parser.parse()
}

/// Parse a CUE sheet from a file. Strips a UTF-8 BOM if present.
pub fn parse_file(path: &Path) -> Result<CueSheet, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to read CUE file {}: {}", path.display(), e))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| {
            format!(
                "CUE file {} is not valid UTF-8. Re-save as UTF-8 to import.",
                path.display()
            )
        })?
        .trim_start_matches('\u{feff}');
    parse_str(text)
}

/// Library-import adapter — emits `LibraryEntry` records for the narrow case
/// where CUE indexes encode soft trim semantics: per-track file rips where
/// `INDEX 01` is non-zero (pregap inside the audio file).
///
/// Single-file album rips (one FILE with multiple tracks) are intentionally
/// not flattened into LibraryEntries — they require virtual-split or chapter
/// authoring, not single-file trim. A warning is emitted in those cases.
pub fn import(path: &Path) -> Result<ImportReport, String> {
    let sheet = parse_file(path)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));

    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for file in &sheet.files {
        if file.tracks.len() > 1 {
            warnings.push(format!(
                "FILE \"{}\" has {} tracks — single-file album rip; \
                 use chapter authoring (not yet implemented) rather than trim.",
                file.path,
                file.tracks.len()
            ));
            continue;
        }

        let Some(track) = file.tracks.first() else {
            continue;
        };

        let Some(audio_start) = index_at(track, 1).or_else(|| index_at(track, 0)) else {
            continue;
        };

        let start_ms = audio_start.time.to_milliseconds();
        if start_ms == 0 {
            continue;
        }

        entries.push(LibraryEntry {
            locator: EntryLocator::Path(resolve_relative(parent, &file.path)),
            start_ms: Some(start_ms),
            stop_ms: None,
        });
    }

    Ok(ImportReport {
        source: SourceInfo {
            kind: KIND,
            path: path.to_path_buf(),
        },
        entries,
        warnings,
    })
}

fn index_at(track: &CueTrack, n: u8) -> Option<&CueIndex> {
    track.indexes.iter().find(|i| i.number == n)
}

fn resolve_relative(parent: &Path, file_path: &str) -> PathBuf {
    let p = Path::new(file_path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        parent.join(p)
    }
}

// ============================================================
// Parser (private)
// ============================================================

struct Parser<'a> {
    lines: std::str::Lines<'a>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lines: input.lines(),
        }
    }

    fn parse(&mut self) -> Result<CueSheet, String> {
        let mut sheet = CueSheet::default();

        while let Some(line) = self.lines.next() {
            let tokens = tokenize(line);
            if tokens.is_empty() {
                continue;
            }

            match tokens[0].to_ascii_uppercase().as_str() {
                "REM" => sheet.rems.push(parse_rem(&tokens)),
                "CATALOG" => sheet.catalog = tokens.get(1).cloned(),
                "TITLE" => sheet.title = tokens.get(1).cloned(),
                "PERFORMER" => sheet.performer = tokens.get(1).cloned(),
                "SONGWRITER" => sheet.songwriter = tokens.get(1).cloned(),
                "FILE" => {
                    let file = self.parse_file_block(&tokens)?;
                    sheet.files.push(file);
                }
                "CDTEXTFILE" => {} // ignored — external CD-Text binary
                _ => {}            // unknown disc-level directive — silently ignore
            }
        }

        Ok(sheet)
    }

    fn parse_file_block(&mut self, header: &[String]) -> Result<CueFile, String> {
        let path = header
            .get(1)
            .cloned()
            .ok_or_else(|| "FILE directive missing path".to_string())?;
        let format = header
            .get(2)
            .map(|s| FileFormat::from_token(s))
            .unwrap_or(FileFormat::Other(String::new()));

        let mut file = CueFile {
            path,
            format,
            tracks: Vec::new(),
        };

        while let Some(line) = self.peek_line() {
            let tokens = tokenize(line);
            if tokens.is_empty() {
                self.lines.next();
                continue;
            }

            let directive = tokens[0].to_ascii_uppercase();
            if directive == "FILE" {
                break;
            }
            self.lines.next();

            if directive == "TRACK" {
                let track = self.parse_track_block(&tokens)?;
                file.tracks.push(track);
            } else {
                // disc-level directives that appeared after FILE — tolerate
            }
        }

        Ok(file)
    }

    fn parse_track_block(&mut self, header: &[String]) -> Result<CueTrack, String> {
        let number: u8 = header
            .get(1)
            .ok_or_else(|| "TRACK directive missing number".to_string())?
            .parse()
            .map_err(|e| format!("TRACK number parse error: {e}"))?;
        let kind = match header.get(2).map(String::as_str) {
            Some("AUDIO") => TrackKind::Audio,
            Some(other) => TrackKind::Other(other.to_string()),
            None => TrackKind::Audio,
        };

        let mut track = CueTrack {
            number,
            kind,
            title: None,
            performer: None,
            songwriter: None,
            isrc: None,
            flags: Vec::new(),
            pregap: None,
            postgap: None,
            indexes: Vec::new(),
            rems: Vec::new(),
        };

        while let Some(line) = self.peek_line() {
            let tokens = tokenize(line);
            if tokens.is_empty() {
                self.lines.next();
                continue;
            }

            let directive = tokens[0].to_ascii_uppercase();
            if directive == "TRACK" || directive == "FILE" {
                break;
            }
            self.lines.next();

            match directive.as_str() {
                "TITLE" => track.title = tokens.get(1).cloned(),
                "PERFORMER" => track.performer = tokens.get(1).cloned(),
                "SONGWRITER" => track.songwriter = tokens.get(1).cloned(),
                "ISRC" => track.isrc = tokens.get(1).cloned(),
                "FLAGS" => track.flags = tokens[1..].to_vec(),
                "PREGAP" => track.pregap = tokens.get(1).and_then(|t| parse_time(t).ok()),
                "POSTGAP" => track.postgap = tokens.get(1).and_then(|t| parse_time(t).ok()),
                "INDEX" => {
                    if let (Some(num), Some(time)) = (tokens.get(1), tokens.get(2)) {
                        if let (Ok(n), Ok(t)) = (num.parse::<u8>(), parse_time(time)) {
                            track.indexes.push(CueIndex { number: n, time: t });
                        }
                    }
                }
                "REM" => track.rems.push(parse_rem(&tokens)),
                _ => {}
            }
        }

        Ok(track)
    }

    fn peek_line(&self) -> Option<&'a str> {
        self.lines.clone().next()
    }
}

impl FileFormat {
    fn from_token(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "WAVE" => FileFormat::Wave,
            "AIFF" => FileFormat::Aiff,
            "MP3" => FileFormat::Mp3,
            "FLAC" => FileFormat::Flac,
            "BINARY" | "MOTOROLA" => FileFormat::Binary,
            other => FileFormat::Other(other.to_string()),
        }
    }
}

// ============================================================
// Tokenizer + small parsers
// ============================================================

/// Split a CUE line into whitespace-separated tokens, with double-quoted
/// strings treated as a single token (quotes stripped). Standard CUE format
/// has no escape sequences inside quoted strings; embedded quotes are not
/// supported by the spec.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let mut s = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                s.push(c);
            }
            tokens.push(s);
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                chars.next();
            }
            tokens.push(s);
        }
    }
    tokens
}

fn parse_time(s: &str) -> Result<CueTime, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("invalid time {s}"));
    }
    let minutes: u32 = parts[0]
        .parse()
        .map_err(|_| format!("bad minutes in {s}"))?;
    let seconds: u8 = parts[1]
        .parse()
        .map_err(|_| format!("bad seconds in {s}"))?;
    let frames: u8 = parts[2].parse().map_err(|_| format!("bad frames in {s}"))?;
    if seconds > 59 || frames > 74 {
        return Err(format!("time component out of range: {s}"));
    }
    Ok(CueTime {
        minutes,
        seconds,
        frames,
    })
}

fn parse_rem(tokens: &[String]) -> RemEntry {
    if tokens.len() <= 1 {
        return RemEntry {
            key: String::new(),
            value: String::new(),
        };
    }
    let key = tokens[1].clone();
    let value = tokens[2..].join(" ");
    RemEntry { key, value }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_cue(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".cue")
            .tempfile()
            .expect("create tempfile");
        f.write_all(content.as_bytes()).expect("write cue");
        f.flush().expect("flush");
        f
    }

    #[test]
    fn time_to_ms_zero() {
        assert_eq!(CueTime::ZERO.to_milliseconds(), 0);
    }

    #[test]
    fn time_to_ms_one_second() {
        assert_eq!(
            CueTime {
                minutes: 0,
                seconds: 1,
                frames: 0
            }
            .to_milliseconds(),
            1000
        );
    }

    #[test]
    fn time_to_ms_one_minute() {
        assert_eq!(
            CueTime {
                minutes: 1,
                seconds: 0,
                frames: 0
            }
            .to_milliseconds(),
            60_000
        );
    }

    #[test]
    fn time_to_ms_one_frame_rounds_to_13() {
        assert_eq!(
            CueTime {
                minutes: 0,
                seconds: 0,
                frames: 1
            }
            .to_milliseconds(),
            13
        );
    }

    #[test]
    fn time_to_ms_max_frame_rounds_correctly() {
        // 74 frames * 1000 / 75 = 986.66... → rounds to 987
        assert_eq!(
            CueTime {
                minutes: 0,
                seconds: 0,
                frames: 74
            }
            .to_milliseconds(),
            987
        );
        // 37 frames * 1000 / 75 = 493.33... → rounds to 493
        assert_eq!(
            CueTime {
                minutes: 0,
                seconds: 0,
                frames: 37
            }
            .to_milliseconds(),
            493
        );
    }

    #[test]
    fn tokenize_simple() {
        assert_eq!(tokenize("TRACK 01 AUDIO"), vec!["TRACK", "01", "AUDIO"]);
    }

    #[test]
    fn tokenize_quoted_with_spaces() {
        assert_eq!(
            tokenize(r#"TITLE "Hello World""#),
            vec!["TITLE", "Hello World"]
        );
    }

    #[test]
    fn tokenize_mixed() {
        assert_eq!(
            tokenize(r#"FILE "my album.flac" WAVE"#),
            vec!["FILE", "my album.flac", "WAVE"]
        );
    }

    #[test]
    fn parse_time_valid() {
        let t = parse_time("01:23:45").unwrap();
        assert_eq!(t.minutes, 1);
        assert_eq!(t.seconds, 23);
        assert_eq!(t.frames, 45);
    }

    #[test]
    fn parse_time_unpadded() {
        let t = parse_time("1:2:3").unwrap();
        assert_eq!(t.minutes, 1);
        assert_eq!(t.seconds, 2);
        assert_eq!(t.frames, 3);
    }

    #[test]
    fn parse_time_rejects_out_of_range() {
        assert!(parse_time("00:60:00").is_err());
        assert!(parse_time("00:00:75").is_err());
    }

    #[test]
    fn parse_realistic_eac_cuesheet() {
        let cue = r#"REM GENRE "Pop"
REM DATE 2023
REM DISCID DEADBEEF
REM COMMENT "ExactAudioCopy v1.6"
PERFORMER "Some Artist"
TITLE "Some Album"
CATALOG 0123456789012
FILE "Some Artist - Some Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Some Artist"
    ISRC USAT12345678
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Some Artist"
    ISRC USAT12345679
    INDEX 00 03:14:50
    INDEX 01 03:15:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 01 06:42:30
"#;
        let sheet = parse_str(cue).unwrap();
        assert_eq!(sheet.title.as_deref(), Some("Some Album"));
        assert_eq!(sheet.performer.as_deref(), Some("Some Artist"));
        assert_eq!(sheet.catalog.as_deref(), Some("0123456789012"));
        assert_eq!(sheet.rems.len(), 4);
        assert_eq!(sheet.rems[0].key, "GENRE");
        assert_eq!(sheet.rems[0].value, "Pop");

        assert_eq!(sheet.files.len(), 1);
        let file = &sheet.files[0];
        assert_eq!(file.path, "Some Artist - Some Album.flac");
        assert_eq!(file.format, FileFormat::Wave);
        assert_eq!(file.tracks.len(), 3);

        let t2 = &file.tracks[1];
        assert_eq!(t2.number, 2);
        assert_eq!(t2.title.as_deref(), Some("Track Two"));
        assert_eq!(t2.isrc.as_deref(), Some("USAT12345679"));
        assert_eq!(t2.indexes.len(), 2);
        assert_eq!(t2.indexes[0].number, 0);
        assert_eq!(t2.indexes[1].number, 1);
    }

    #[test]
    fn import_single_file_album_warns_no_entries() {
        let cue = r#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 03:15:00
"#;
        let f = write_cue(cue);
        let report = import(f.path()).unwrap();
        assert!(report.entries.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("single-file album"));
    }

    #[test]
    fn import_per_track_with_pregap_emits_entry() {
        // Per-track file rip where INDEX 01 is non-zero (pregap inside the file).
        let cue = r#"FILE "track02.flac" WAVE
  TRACK 02 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:02:50
"#;
        let f = write_cue(cue);
        let report = import(f.path()).unwrap();
        assert_eq!(report.entries.len(), 1);
        let entry = &report.entries[0];
        // 2.50s → 2*1000 + 50 frames ≈ 2667ms
        assert_eq!(entry.start_ms, Some(2667));
        assert_eq!(entry.stop_ms, None);
        if let EntryLocator::Path(ref p) = entry.locator {
            assert!(p.ends_with("track02.flac"));
        } else {
            panic!("expected Path locator");
        }
    }

    #[test]
    fn import_per_track_with_zero_index_skipped() {
        let cue = r#"FILE "track01.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
        let f = write_cue(cue);
        let report = import(f.path()).unwrap();
        assert!(report.entries.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn parses_utf8_bom() {
        let cue = "\u{feff}TITLE \"Test\"\nFILE \"a.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n";
        let f = write_cue(cue);
        let sheet = parse_file(f.path()).unwrap();
        assert_eq!(sheet.title.as_deref(), Some("Test"));
    }

    #[test]
    fn relative_path_resolved_against_cue_directory() {
        let cue = r#"FILE "audio.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:01:00
"#;
        let f = write_cue(cue);
        let report = import(f.path()).unwrap();
        let entry = &report.entries[0];
        if let EntryLocator::Path(ref p) = entry.locator {
            assert_eq!(p.parent(), f.path().parent());
            assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("audio.flac"));
        } else {
            panic!("expected Path locator");
        }
    }
}
