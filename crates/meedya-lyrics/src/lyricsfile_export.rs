// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Lyricsfile → other-format exporters (#34)
// ==========================================
//
// Five complementary export paths for [`Lyricsfile`]:
//
// 1. [`Lyricsfile::to_lrc`] — line-level `[mm:ss.xx]text` LRC. Drops
//    word-level timing (collapses to the line `text`). Instrumental
//    Lyricsfiles emit the `[au: instrumental]` marker.
// 2. [`Lyricsfile::to_enhanced_lrc`] — Enhanced LRC with inline
//    `<mm:ss.xx>` word markers. Falls back to plain LRC for lines
//    without word-level timing.
// 3. [`Lyricsfile::to_srt`] — standard SubRip subtitle format with
//    `HH:MM:SS,mmm --> HH:MM:SS,mmm` ranges. Uses the line's `end_ms`
//    if present, otherwise the next line's `start_ms`, otherwise
//    `start_ms + 3000` as a safety default.
// 4. [`Lyricsfile::to_webvtt`] — WebVTT (`.vtt`) format. Same timing
//    cascade as SRT; uses `HH:MM:SS.mmm` (dot, not comma) and the
//    `WEBVTT` header.
// 5. [`Lyricsfile::to_ass`] — Advanced SubStation Alpha (`.ass`) with
//    a single `Default` style. Useful for video players that consume
//    `.ass` directly (e.g., MPV, MPC-HC).
//
// All five exporters are pure functions (no I/O, no allocation
// surprises). Empty input yields a minimal valid output document
// for SRT/VTT/ASS, and an empty string for LRC.

use std::fmt::Write as _;

use crate::lyricsfile::{Lyricsfile, LyricsfileLine, LyricsfileWord, INSTRUMENTAL_MARKER};

/// Default per-line duration (in ms) when neither `end_ms` nor a
/// following line is available. Three seconds is the LRCGET reference
/// fallback and feels right for one-shot last-line cases.
const DEFAULT_TRAILING_DURATION_MS: i64 = 3_000;

impl Lyricsfile {
    /// Export to standard LRC. Word-level timing is collapsed to line
    /// level. Instrumental Lyricsfiles emit `[au: instrumental]`.
    pub fn to_lrc(&self) -> String {
        if self.metadata.instrumental {
            return format!("{INSTRUMENTAL_MARKER}\n");
        }

        let mut out = String::new();
        for line in &self.lines {
            let _ = writeln!(out, "{}{}", format_lrc_timestamp(line.start_ms), line.text);
        }
        out
    }

    /// Export to Enhanced LRC. Lines with `words` get inline word
    /// timestamps; lines without fall back to plain LRC.
    pub fn to_enhanced_lrc(&self) -> String {
        if self.metadata.instrumental {
            return format!("{INSTRUMENTAL_MARKER}\n");
        }

        let mut out = String::new();
        for line in &self.lines {
            let stamp = format_lrc_timestamp(line.start_ms);
            if line.words.is_empty() {
                let _ = writeln!(out, "{stamp}{}", line.text);
            } else {
                let mut buf = String::new();
                for word in &line.words {
                    let _ = write!(
                        buf,
                        "<{}>{} ",
                        format_lrc_timestamp_bare(word.start_ms),
                        word.text
                    );
                }
                let _ = writeln!(out, "{stamp}{}", buf.trim_end());
            }
        }
        out
    }

    /// Export to SubRip (.srt) format.
    pub fn to_srt(&self) -> String {
        let mut out = String::new();
        let lines = &self.lines;
        for (idx, line) in lines.iter().enumerate() {
            let start = line.start_ms;
            let end = resolve_line_end(line, lines.get(idx + 1));
            let _ = writeln!(out, "{}", idx + 1);
            let _ = writeln!(
                out,
                "{} --> {}",
                format_srt_timestamp(start),
                format_srt_timestamp(end)
            );
            let _ = writeln!(out, "{}", line.text);
            let _ = writeln!(out);
        }
        out
    }

    /// Export to WebVTT (.vtt) format.
    pub fn to_webvtt(&self) -> String {
        let mut out = String::from("WEBVTT\n\n");
        let lines = &self.lines;
        for (idx, line) in lines.iter().enumerate() {
            let start = line.start_ms;
            let end = resolve_line_end(line, lines.get(idx + 1));
            let _ = writeln!(
                out,
                "{} --> {}",
                format_vtt_timestamp(start),
                format_vtt_timestamp(end)
            );
            let _ = writeln!(out, "{}", line.text);
            let _ = writeln!(out);
        }
        out
    }

    /// Export to Advanced SubStation Alpha (.ass) with a single Default
    /// style. Players that consume `.ass` natively will render each
    /// line as a centred subtitle.
    pub fn to_ass(&self) -> String {
        let title = ass_escape(&self.metadata.title);
        let mut out = String::new();
        let _ = writeln!(out, "[Script Info]");
        let _ = writeln!(out, "Title: {title}");
        let _ = writeln!(out, "ScriptType: v4.00+");
        let _ = writeln!(out, "PlayResX: 1920");
        let _ = writeln!(out, "PlayResY: 1080");
        let _ = writeln!(out, "WrapStyle: 0");
        let _ = writeln!(out);
        let _ = writeln!(out, "[V4+ Styles]");
        let _ = writeln!(
            out,
            "Format: Name, Fontname, Fontsize, PrimaryColour, OutlineColour, BackColour, Bold, Italic, Alignment, BorderStyle, Outline, Shadow, MarginL, MarginR, MarginV, Encoding"
        );
        let _ = writeln!(
            out,
            "Style: Default,Arial,72,&H00FFFFFF,&H000000FF,&H00000000,0,0,2,1,3,2,40,40,40,1"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "[Events]");
        let _ = writeln!(
            out,
            "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
        );
        let lines = &self.lines;
        for (idx, line) in lines.iter().enumerate() {
            let start = line.start_ms;
            let end = resolve_line_end(line, lines.get(idx + 1));
            let _ = writeln!(
                out,
                "Dialogue: 0,{},{},Default,,0,0,0,,{}",
                format_ass_timestamp(start),
                format_ass_timestamp(end),
                ass_escape(&line.text)
            );
        }
        out
    }
}

// ============================================================
// Timing-cascade helper
// ============================================================

fn resolve_line_end(line: &LyricsfileLine, next: Option<&LyricsfileLine>) -> i64 {
    if let Some(end) = line.end_ms {
        return end;
    }
    if let Some(next) = next {
        return next.start_ms;
    }
    line.start_ms + DEFAULT_TRAILING_DURATION_MS
}

// ============================================================
// Timestamp formatters
// ============================================================

/// Format `ms` as `[mm:ss.xx]` for LRC line markers (2-digit
/// centiseconds — the LRC convention).
fn format_lrc_timestamp(ms: i64) -> String {
    let total = ms.max(0) as u64;
    let mins = total / 60_000;
    let secs = (total / 1_000) % 60;
    let cs = (total % 1_000) / 10;
    format!("[{:02}:{:02}.{:02}]", mins, secs, cs)
}

/// Bare `mm:ss.xx` without the surrounding brackets (Enhanced LRC
/// `<...>` word markers use this).
fn format_lrc_timestamp_bare(ms: i64) -> String {
    let total = ms.max(0) as u64;
    let mins = total / 60_000;
    let secs = (total / 1_000) % 60;
    let cs = (total % 1_000) / 10;
    format!("{:02}:{:02}.{:02}", mins, secs, cs)
}

/// Format `ms` as `HH:MM:SS,mmm` (SRT convention — comma decimal).
fn format_srt_timestamp(ms: i64) -> String {
    let total = ms.max(0) as u64;
    let hours = total / 3_600_000;
    let mins = (total / 60_000) % 60;
    let secs = (total / 1_000) % 60;
    let frac = total % 1_000;
    format!("{:02}:{:02}:{:02},{:03}", hours, mins, secs, frac)
}

/// Format `ms` as `HH:MM:SS.mmm` (WebVTT convention — dot decimal).
fn format_vtt_timestamp(ms: i64) -> String {
    let total = ms.max(0) as u64;
    let hours = total / 3_600_000;
    let mins = (total / 60_000) % 60;
    let secs = (total / 1_000) % 60;
    let frac = total % 1_000;
    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs, frac)
}

/// Format `ms` as `H:MM:SS.cc` (ASS convention — single-digit hours,
/// 2-digit centiseconds).
fn format_ass_timestamp(ms: i64) -> String {
    let total = ms.max(0) as u64;
    let hours = total / 3_600_000;
    let mins = (total / 60_000) % 60;
    let secs = (total / 1_000) % 60;
    let cs = (total % 1_000) / 10;
    format!("{}:{:02}:{:02}.{:02}", hours, mins, secs, cs)
}

/// Escape `\` and `\n` and `,` and `{}` for ASS Dialogue text (the
/// rest of ASS's escaping is for tags we don't emit).
fn ass_escape(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('\n', "\\N")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

// `LyricsfileWord` is currently only used inside the Enhanced LRC path;
// keep the import so future word-level VTT / ASS exporters (NotePoint
// for #34 follow-up) can lift it without re-importing.
#[allow(dead_code)]
const _USE_WORD: fn(&LyricsfileWord) -> i64 = |w| w.start_ms;

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricsfile::{LyricsfileLine, LyricsfileMetadata, LyricsfileWord};

    fn three_lines() -> Lyricsfile {
        Lyricsfile {
            version: "1.0".into(),
            metadata: LyricsfileMetadata {
                title: "Song".into(),
                artist: "Artist".into(),
                album: None,
                duration_ms: None,
                offset_ms: None,
                language: None,
                instrumental: false,
            },
            lines: vec![
                LyricsfileLine {
                    text: "First line".into(),
                    start_ms: 1_000,
                    end_ms: Some(3_000),
                    words: vec![
                        LyricsfileWord {
                            text: "First".into(),
                            start_ms: 1_000,
                            end_ms: Some(2_000),
                        },
                        LyricsfileWord {
                            text: "line".into(),
                            start_ms: 2_100,
                            end_ms: Some(3_000),
                        },
                    ],
                },
                LyricsfileLine {
                    text: "Second line".into(),
                    start_ms: 4_000,
                    end_ms: None,
                    words: Vec::new(),
                },
                LyricsfileLine {
                    text: "Third line".into(),
                    start_ms: 7_000,
                    end_ms: None,
                    words: Vec::new(),
                },
            ],
            plain: None,
        }
    }

    fn instrumental() -> Lyricsfile {
        let mut lf = Lyricsfile::new("Quiet", "Some Composer");
        lf.mark_instrumental();
        lf
    }

    // ----------------------------------------------------------
    // LRC export
    // ----------------------------------------------------------

    #[test]
    fn to_lrc_emits_one_line_per_entry() {
        let out = three_lines().to_lrc();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "[00:01.00]First line");
        assert_eq!(lines[1], "[00:04.00]Second line");
        assert_eq!(lines[2], "[00:07.00]Third line");
    }

    #[test]
    fn to_lrc_for_instrumental_emits_marker() {
        assert_eq!(instrumental().to_lrc().trim(), INSTRUMENTAL_MARKER);
    }

    #[test]
    fn to_lrc_drops_word_level_timing() {
        // First line in three_lines() has words; LRC output should not
        // contain `<` from Enhanced LRC syntax.
        let out = three_lines().to_lrc();
        assert!(!out.contains('<'), "got: {out}");
    }

    // ----------------------------------------------------------
    // Enhanced LRC export
    // ----------------------------------------------------------

    #[test]
    fn to_enhanced_lrc_emits_word_markers() {
        let out = three_lines().to_enhanced_lrc();
        let first = out.lines().next().unwrap();
        assert!(first.starts_with("[00:01.00]"));
        assert!(first.contains("<00:01.00>First"));
        assert!(first.contains("<00:02.10>line"));
    }

    #[test]
    fn to_enhanced_lrc_falls_back_to_plain_for_unworded_lines() {
        let out = three_lines().to_enhanced_lrc();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[1], "[00:04.00]Second line");
    }

    #[test]
    fn to_enhanced_lrc_for_instrumental_emits_marker() {
        assert_eq!(instrumental().to_enhanced_lrc().trim(), INSTRUMENTAL_MARKER);
    }

    // ----------------------------------------------------------
    // SRT export
    // ----------------------------------------------------------

    #[test]
    fn to_srt_emits_numbered_cues_with_comma_decimal() {
        let out = three_lines().to_srt();
        // Cue 1 — line has end_ms=3000
        assert!(out.contains("1\n00:00:01,000 --> 00:00:03,000\nFirst line"));
        // Cue 2 — line.end_ms=None, next.start_ms=7000
        assert!(out.contains("2\n00:00:04,000 --> 00:00:07,000\nSecond line"));
        // Cue 3 — final line, no next; falls back to start + 3000ms
        assert!(out.contains("3\n00:00:07,000 --> 00:00:10,000\nThird line"));
    }

    #[test]
    fn to_srt_terminates_each_cue_with_blank_line() {
        let out = three_lines().to_srt();
        // Each cue ends with a blank line; final cue still has one
        // (SRT conformance — most players require it).
        let blank_count = out.matches("\n\n").count();
        assert!(blank_count >= 3, "got: {out}");
    }

    // ----------------------------------------------------------
    // WebVTT export
    // ----------------------------------------------------------

    #[test]
    fn to_webvtt_starts_with_header() {
        let out = three_lines().to_webvtt();
        assert!(out.starts_with("WEBVTT\n\n"));
    }

    #[test]
    fn to_webvtt_uses_dot_decimal_not_comma() {
        let out = three_lines().to_webvtt();
        assert!(out.contains("00:00:01.000 --> 00:00:03.000"));
        assert!(!out.contains(",000"));
    }

    // ----------------------------------------------------------
    // ASS export
    // ----------------------------------------------------------

    #[test]
    fn to_ass_has_required_sections() {
        let out = three_lines().to_ass();
        assert!(out.contains("[Script Info]"));
        assert!(out.contains("[V4+ Styles]"));
        assert!(out.contains("[Events]"));
        assert!(out.contains("Style: Default,"));
    }

    #[test]
    fn to_ass_emits_one_dialogue_per_line() {
        let out = three_lines().to_ass();
        let dialogues: Vec<&str> = out.lines().filter(|l| l.starts_with("Dialogue:")).collect();
        assert_eq!(dialogues.len(), 3);
        assert!(dialogues[0].contains("0:00:01.00,0:00:03.00"));
        assert!(dialogues[0].ends_with("First line"));
    }

    #[test]
    fn to_ass_escapes_braces_and_newlines_in_text() {
        let mut lf = Lyricsfile::new("T", "A");
        lf.lines.push(LyricsfileLine {
            text: "line with {curly} and\nnewline".into(),
            start_ms: 0,
            end_ms: Some(2000),
            words: Vec::new(),
        });
        let out = lf.to_ass();
        assert!(out.contains("\\{curly\\}"));
        assert!(out.contains("\\N"));
    }

    // ----------------------------------------------------------
    // Timestamp formatters
    // ----------------------------------------------------------

    #[test]
    fn lrc_timestamp_format_pads_to_two_digits() {
        assert_eq!(format_lrc_timestamp(0), "[00:00.00]");
        assert_eq!(format_lrc_timestamp(61_500), "[01:01.50]");
        assert_eq!(format_lrc_timestamp(123_456), "[02:03.45]");
    }

    #[test]
    fn srt_timestamp_format_includes_hours_and_comma() {
        assert_eq!(format_srt_timestamp(0), "00:00:00,000");
        assert_eq!(format_srt_timestamp(3_661_234), "01:01:01,234");
    }

    #[test]
    fn vtt_timestamp_format_uses_dot_decimal() {
        assert_eq!(format_vtt_timestamp(3_661_234), "01:01:01.234");
    }

    #[test]
    fn ass_timestamp_uses_centiseconds() {
        assert_eq!(format_ass_timestamp(3_661_234), "1:01:01.23");
    }

    // ----------------------------------------------------------
    // Round-trip
    // ----------------------------------------------------------

    #[test]
    fn lrc_round_trip_preserves_line_timing_and_text() {
        let original = three_lines();
        let lrc_out = original.to_lrc();
        let back = Lyricsfile::from_lrc(&lrc_out, "Song", "Artist").unwrap();
        assert_eq!(back.lines.len(), 3);
        // Line-level timing preserved to the centisecond.
        assert_eq!(back.lines[0].start_ms, 1_000);
        assert_eq!(back.lines[1].start_ms, 4_000);
        assert_eq!(back.lines[0].text, "First line");
    }

    #[test]
    fn enhanced_lrc_round_trip_preserves_word_timing_within_10ms() {
        let original = three_lines();
        let enh = original.to_enhanced_lrc();
        let back = Lyricsfile::from_lrc(&enh, "Song", "Artist").unwrap();
        let words = &back.lines[0].words;
        assert_eq!(words.len(), 2);
        // Word start_ms preserved to centisecond precision (LRC format
        // limit). Acceptance criterion in #34 is 10 ms.
        assert!((words[0].start_ms - 1_000).abs() < 10);
        assert!((words[1].start_ms - 2_100).abs() < 10);
        assert_eq!(words[0].text, "First");
    }

    #[test]
    fn instrumental_lyricsfile_round_trips_through_lrc() {
        let original = instrumental();
        let lrc = original.to_lrc();
        let back = Lyricsfile::from_lrc(&lrc, "Quiet", "Some Composer").unwrap();
        assert!(back.metadata.instrumental);
        assert!(back.lines.is_empty());
    }
}
