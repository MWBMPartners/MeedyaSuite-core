// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// LRC → Lyricsfile converter (#34)
// =================================
//
// Converts standard `[mm:ss.xx]` LRC (and Enhanced LRC) text into the
// Lyricsfile YAML format defined in `lyricsfile.rs`.
//
// ## Source formats
//
// 1. **Plain LRC** — one timestamp per line:
//    `[00:12.34]Hello, it's me`
//    Each timed line becomes a `LyricsfileLine` with `words = []`.
//
// 2. **Multi-timestamp lines** — same text at multiple times:
//    `[00:12.34][00:24.68]repeat`
//    Expanded into one `LyricsfileLine` per timestamp (matches the
//    existing `lrc::parse` policy in this crate).
//
// 3. **Enhanced LRC** — inline word-level timestamps:
//    `[00:12.34]<00:12.34>Hello, <00:13.10>it's <00:13.50>me`
//    Each `<mm:ss.xx>` marker becomes a `LyricsfileWord`.
//
// 4. **Instrumental marker** — `[au: instrumental]` anywhere in the
//    file flips `metadata.instrumental = true` and clears `lines`.
//
// ## What we don't preserve
//
// - **LRC metadata tags** (`[ti:...]`, `[ar:...]`, `[al:...]`) — title,
//   artist, and album come from the caller. The LRC metadata block is
//   often missing or stale; callers know what they wanted to tag.
// - **`[offset:N]` tag** — could be lifted into `metadata.offset_ms`,
//   but in practice the offset is applied at playback time, not at
//   storage. Leave for a follow-up if it becomes user-visible.

use crate::error::Result;
use crate::lyricsfile::LyricsfileMetadata;
use crate::lyricsfile::{
    Lyricsfile, LyricsfileLine, LyricsfileWord, INSTRUMENTAL_MARKER, LYRICSFILE_VERSION,
};

impl Lyricsfile {
    /// Convert a standard LRC (or Enhanced LRC) document into a
    /// Lyricsfile. `title`, `artist`, and `album` are caller-supplied;
    /// LRC metadata tags in the source are ignored.
    ///
    /// Empty LRC (no timed lines, no instrumental marker) returns a
    /// valid Lyricsfile with an empty `lines` vector.
    pub fn from_lrc(
        lrc: &str,
        title: impl Into<String>,
        artist: impl Into<String>,
    ) -> Result<Self> {
        // Capture caller's metadata strings once — both the
        // instrumental short-circuit and the synced-lines branch need
        // them.
        let title: String = title.into();
        let artist: String = artist.into();

        // Instrumental detection — case-insensitive prefix check across
        // the whole document (LRCGET's reference behaviour).
        if is_instrumental_lrc(lrc) {
            let mut lf = Lyricsfile::new(title, artist);
            lf.metadata.instrumental = true;
            return Ok(lf);
        }

        let mut lines: Vec<LyricsfileLine> = Vec::new();
        for raw in lrc.lines() {
            for entry in parse_lrc_line(raw) {
                lines.push(entry);
            }
        }
        lines.sort_by_key(|l| l.start_ms);

        // Backfill `end_ms` from the next line's `start_ms` (where
        // unset) so consumers that need a closed timing window have
        // one. Matches LRCGET's reference behaviour.
        for idx in 0..lines.len() {
            if lines[idx].end_ms.is_none() {
                if let Some(next) = lines.get(idx + 1) {
                    lines[idx].end_ms = Some(next.start_ms);
                }
            }
        }

        Ok(Lyricsfile {
            version: LYRICSFILE_VERSION.to_string(),
            metadata: LyricsfileMetadata {
                title,
                artist,
                album: None,
                duration_ms: None,
                offset_ms: None,
                language: None,
                instrumental: false,
            },
            lines,
            plain: None,
        })
    }
}

/// `true` when `[au: instrumental]` (case-insensitive) appears anywhere
/// in the input. Matches LRCGET's reference policy.
fn is_instrumental_lrc(input: &str) -> bool {
    let lowered = input.to_lowercase();
    lowered.contains("[au:") && lowered.contains("instrumental")
}

/// Parse a single LRC line into one or more [`LyricsfileLine`] entries.
///
/// Handles:
/// - Plain text after `[mm:ss.xx]`
/// - Multi-timestamp lines (`[00:01.00][00:05.00]repeat`)
/// - Enhanced LRC word-level inline timestamps (`<mm:ss.xx>word`)
/// - LRC metadata tags (`[ti:...]`, `[ar:...]`) — silently skipped
fn parse_lrc_line(raw: &str) -> Vec<LyricsfileLine> {
    let mut rest = raw;
    let mut stamps: Vec<i64> = Vec::new();
    while let Some(stripped) = rest.strip_prefix('[') {
        let Some(end) = stripped.find(']') else { break };
        let tag = &stripped[..end];
        match parse_lrc_timestamp(tag) {
            Some(ms) => {
                stamps.push(ms);
                rest = &stripped[end + 1..];
            }
            None => break, // metadata tag (`[ti:...]`) — skip the whole line
        }
    }
    if stamps.is_empty() {
        return Vec::new();
    }

    let body = rest;
    let (text, words) = extract_enhanced_words(body);

    stamps
        .into_iter()
        .map(|start_ms| LyricsfileLine {
            text: text.clone(),
            start_ms,
            end_ms: None,
            words: words.clone(),
        })
        .collect()
}

/// Parse `mm:ss[.xx]` (or `mm:ss[.xxx]`) into milliseconds. Returns
/// `None` for non-timestamp tags (LRC metadata like `ti:`/`ar:`/`au:`).
fn parse_lrc_timestamp(tag: &str) -> Option<i64> {
    let (mm, rest) = tag.split_once(':')?;
    let mins: u64 = mm.parse().ok()?;
    let (ss, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let secs: u64 = ss.parse().ok()?;
    if secs >= 60 {
        return None;
    }
    let frac_ms: u64 = if frac.is_empty() {
        0
    } else {
        let n: u64 = frac.parse().ok()?;
        match frac.len() {
            1 => n * 100,
            2 => n * 10,
            3 => n,
            len => n / 10u64.pow((len - 3) as u32),
        }
    };
    Some((mins * 60_000 + secs * 1_000 + frac_ms) as i64)
}

/// Split an LRC line body into plain text and (optional) word-level
/// timings. If no `<mm:ss.xx>` markers are present, returns the trimmed
/// body as `text` and an empty `words` vec.
fn extract_enhanced_words(body: &str) -> (String, Vec<LyricsfileWord>) {
    if !body.contains('<') {
        return (body.trim().to_string(), Vec::new());
    }

    // Walk through `<mm:ss.xx>text<mm:ss.xx>text...` segments.
    let mut words: Vec<LyricsfileWord> = Vec::new();
    let mut text_buf = String::new();
    let mut rest = body;
    let mut leading_text = String::new();

    // Collect any plain text before the first `<` (rare but valid).
    if let Some(idx) = rest.find('<') {
        leading_text = rest[..idx].trim().to_string();
        rest = &rest[idx..];
    }

    while let Some(stripped) = rest.strip_prefix('<') {
        let Some(end) = stripped.find('>') else { break };
        let tag = &stripped[..end];
        let Some(start_ms) = parse_lrc_timestamp(tag) else {
            // Malformed `<...>` — treat as literal text and bail to the
            // plain-text path.
            return (body.trim().to_string(), Vec::new());
        };
        let after = &stripped[end + 1..];
        // Word text runs until the next `<` (or end of line).
        let (word_text, next_rest) = match after.find('<') {
            Some(idx) => (&after[..idx], &after[idx..]),
            None => (after, ""),
        };
        let trimmed = word_text.trim();
        if !trimmed.is_empty() {
            if !text_buf.is_empty() {
                text_buf.push(' ');
            }
            text_buf.push_str(trimmed);
            words.push(LyricsfileWord {
                text: trimmed.to_string(),
                start_ms,
                end_ms: None,
            });
        }
        rest = next_rest;
    }

    // Backfill word end times from the next word's start (where
    // available). Matches LRCGET reference behaviour.
    for idx in 0..words.len() {
        if words[idx].end_ms.is_none() {
            if let Some(next) = words.get(idx + 1) {
                words[idx].end_ms = Some(next.start_ms);
            }
        }
    }

    let text = if leading_text.is_empty() {
        text_buf
    } else if text_buf.is_empty() {
        leading_text
    } else {
        format!("{leading_text} {text_buf}")
    };

    (text, words)
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc() {
        let lrc = "[00:12.34]Hello, it's me\n[00:24.68]I was wondering\n";
        let lf = Lyricsfile::from_lrc(lrc, "Hello", "Adele").unwrap();
        assert_eq!(lf.lines.len(), 2);
        assert_eq!(lf.lines[0].start_ms, 12_340);
        assert_eq!(lf.lines[0].text, "Hello, it's me");
        assert!(lf.lines[0].words.is_empty());
        assert_eq!(lf.lines[1].start_ms, 24_680);
    }

    #[test]
    fn backfills_end_ms_from_next_line() {
        let lrc = "[00:01.00]a\n[00:03.00]b\n[00:05.00]c\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        assert_eq!(lf.lines[0].end_ms, Some(3_000));
        assert_eq!(lf.lines[1].end_ms, Some(5_000));
        // Last line keeps end_ms unset.
        assert_eq!(lf.lines[2].end_ms, None);
    }

    #[test]
    fn expands_multi_timestamp_lines_into_separate_entries() {
        let lrc = "[00:01.00][00:05.00]chorus\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        assert_eq!(lf.lines.len(), 2);
        assert_eq!(lf.lines[0].start_ms, 1_000);
        assert_eq!(lf.lines[1].start_ms, 5_000);
        assert_eq!(lf.lines[0].text, lf.lines[1].text);
        assert_eq!(lf.lines[0].text, "chorus");
    }

    #[test]
    fn skips_lrc_metadata_tags() {
        let lrc = "[ti:Hello]\n[ar:Adele]\n[al:25]\n[00:12.34]synced\n";
        let lf = Lyricsfile::from_lrc(lrc, "Hello", "Adele").unwrap();
        assert_eq!(lf.lines.len(), 1);
        assert_eq!(lf.lines[0].text, "synced");
        assert_eq!(lf.metadata.title, "Hello"); // from caller, not [ti:]
    }

    #[test]
    fn instrumental_marker_flips_metadata_and_clears_lines() {
        let lrc = "[au: instrumental]\n[00:01.00]should be discarded\n";
        let lf = Lyricsfile::from_lrc(lrc, "Track", "Artist").unwrap();
        assert!(lf.metadata.instrumental);
        assert!(lf.lines.is_empty());
    }

    #[test]
    fn instrumental_marker_is_case_insensitive() {
        let lrc = "[AU: INSTRUMENTAL]\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        assert!(lf.metadata.instrumental);
    }

    #[test]
    fn parses_enhanced_lrc_word_level() {
        let lrc = "[00:01.00]<00:01.00>Hello, <00:01.80>it's <00:02.50>me\n";
        let lf = Lyricsfile::from_lrc(lrc, "Hello", "Adele").unwrap();
        assert_eq!(lf.lines.len(), 1);
        let line = &lf.lines[0];
        assert_eq!(line.start_ms, 1_000);
        assert_eq!(line.words.len(), 3);
        assert_eq!(line.words[0].text, "Hello,");
        assert_eq!(line.words[0].start_ms, 1_000);
        assert_eq!(line.words[1].text, "it's");
        assert_eq!(line.words[1].start_ms, 1_800);
        assert_eq!(line.words[2].text, "me");
        assert_eq!(line.words[2].start_ms, 2_500);
        assert_eq!(line.text, "Hello, it's me");
    }

    #[test]
    fn backfills_word_end_ms_from_next_word() {
        let lrc = "[00:01.00]<00:01.00>a <00:02.00>b <00:03.00>c\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        let words = &lf.lines[0].words;
        assert_eq!(words[0].end_ms, Some(2_000));
        assert_eq!(words[1].end_ms, Some(3_000));
        assert_eq!(words[2].end_ms, None); // last word unset
    }

    #[test]
    fn empty_lrc_returns_empty_lines_no_error() {
        let lf = Lyricsfile::from_lrc("", "t", "a").unwrap();
        assert!(lf.lines.is_empty());
        assert!(!lf.metadata.instrumental);
    }

    #[test]
    fn three_digit_milliseconds_parse() {
        let lrc = "[00:00.123]x\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        assert_eq!(lf.lines[0].start_ms, 123);
    }

    #[test]
    fn invalid_seconds_field_skips_line_gracefully() {
        // 60 seconds is invalid; line is silently dropped.
        let lrc = "[00:60.00]bad\n[00:05.00]good\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        assert_eq!(lf.lines.len(), 1);
        assert_eq!(lf.lines[0].text, "good");
    }

    #[test]
    fn caller_metadata_overrides_lrc_tags() {
        // Even when the LRC has its own [ti:] [ar:], the caller wins —
        // mirrors the from_ttml signature contract.
        let lrc = "[ti:WrongTitle]\n[ar:WrongArtist]\n[00:01.00]line\n";
        let lf = Lyricsfile::from_lrc(lrc, "RightTitle", "RightArtist").unwrap();
        assert_eq!(lf.metadata.title, "RightTitle");
        assert_eq!(lf.metadata.artist, "RightArtist");
    }

    #[test]
    fn enhanced_lrc_with_leading_text_before_first_word_marker() {
        let lrc = "[00:01.00]intro <00:02.00>word\n";
        let lf = Lyricsfile::from_lrc(lrc, "t", "a").unwrap();
        // Leading text is preserved and prepended to the joined word text.
        assert!(lf.lines[0].text.contains("intro"));
        assert!(lf.lines[0].text.contains("word"));
    }
}
