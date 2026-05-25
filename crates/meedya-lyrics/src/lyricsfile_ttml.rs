// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// TTML → Lyricsfile converter (#34)
// =================================
//
// Converts Apple Music's TTML lyric documents into the Lyricsfile YAML
// format defined in `lyricsfile.rs`. Handles both:
//
// 1. **Line-level TTML** (default for Apple Music): `<p begin="..." end="..."`>`
//    elements with plain text content. The whole `<p>` becomes one
//    `LyricsfileLine` with no `words`.
//
// 2. **Word-level TTML** (`itunes:timing="Word"`): `<p>` elements
//    containing one `<span begin="..." end="..."`>`word</span>` per word.
//    Each `<span>` becomes a `LyricsfileWord` inside the line.
//
// ## Time format
//
// TTML timestamps follow `HH:MM:SS.mmm` (3-digit fractional seconds).
// Older Apple files sometimes use 2-digit centiseconds — we tolerate
// both. The converter normalises to integer milliseconds for
// Lyricsfile storage (matching LRCGET's reference parser).
//
// ## XML namespace handling
//
// `quick-xml` doesn't auto-resolve XML namespace prefixes when iterating
// events. We match on local-name suffix (e.g., `b"p"`, `b"span"`,
// `b"tt"`) so the converter works regardless of the prefix the producer
// chose (`tt:p` vs `p`, `itunes:timing` vs `it:timing`). This matches
// how Apple's own player parses its TTML.
//
// ## What we don't preserve
//
// - **Styling**: `<span style="...">` attributes (bold, italic, colour).
//   The Lyricsfile spec has no styling primitives; players that need
//   bold/italic should consume the original TTML, not the Lyricsfile.
// - **Speaker / agent labels**: TTML's `ttm:agent` is dropped. (When
//   the spec stabilises an agent field, we revisit.)
// - **Multiple `<div>` blocks**: All `<p>` elements are flattened in
//   document order regardless of which `<div>` they came from.

use std::str;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::{Error, Result};
use crate::lyricsfile::{
    Lyricsfile, LyricsfileLine, LyricsfileMetadata, LyricsfileWord, LYRICSFILE_VERSION,
};

impl Lyricsfile {
    /// Convert a TTML document into a Lyricsfile.
    ///
    /// `title`, `artist`, and `album` come from the caller (typically
    /// from track metadata fetched alongside the TTML — Apple's TTML
    /// `<head><metadata>` block is unreliable and often empty).
    /// `duration_ms` is set to `None` here; callers with track duration
    /// should populate it on the returned struct.
    ///
    /// If the TTML is empty or contains no `<p>` elements, returns an
    /// otherwise-valid Lyricsfile with `instrumental: false` and an
    /// empty `lines` vector. Callers that interpret no-content TTML as
    /// instrumental should `mark_instrumental()` on the result.
    pub fn from_ttml(
        ttml: &str,
        title: impl Into<String>,
        artist: impl Into<String>,
    ) -> Result<Self> {
        let mut lf = Self {
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
        };

        let mut reader = Reader::from_str(ttml);
        reader.config_mut().trim_text(true);

        // Tracks whether we're inside a <p> that we're currently
        // accumulating into. When word-level <span> elements appear
        // inside, each becomes a `LyricsfileWord`; otherwise the <p>'s
        // text content becomes the line text.
        let mut current_line: Option<PendingLine> = None;
        // For word-level <span> mode: the span we're currently inside.
        let mut current_word: Option<PendingWord> = None;
        // Plain text buffer for line-level <p> content (when no <span>
        // children appear). Apple's TTML may include nested formatting
        // spans without `begin`/`end` attrs — we coalesce those into
        // plain text rather than treating them as words.
        let mut line_text_buf = String::new();
        // Document-level language hint from `xml:lang` on <tt>.
        let mut document_language: Option<String> = None;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    match local_name(e.name().as_ref()) {
                        b"tt" => {
                            if document_language.is_none() {
                                document_language = read_attr(e, b"xml:lang")?.or_else(|| {
                                    // Apple sometimes uses `lang` (no
                                    // namespace prefix) — accept that
                                    // too.
                                    read_attr(e, b"lang").ok().flatten()
                                });
                            }
                        }
                        b"p" => {
                            let begin = read_time_attr(e, b"begin")?;
                            let end = read_time_attr(e, b"end")?;
                            if let Some(start_ms) = begin {
                                current_line = Some(PendingLine {
                                    start_ms,
                                    end_ms: end,
                                    words: Vec::new(),
                                });
                                line_text_buf.clear();
                            }
                        }
                        b"span" => {
                            // Only treat <span> as a word if it has a
                            // `begin` attr; otherwise it's a styling
                            // wrapper and we let its text fall through
                            // into the plain-text buffer.
                            if let Some(start_ms) = read_time_attr(e, b"begin")? {
                                current_word = Some(PendingWord {
                                    start_ms,
                                    end_ms: read_time_attr(e, b"end")?,
                                    text: String::new(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(t)) => {
                    let text = t
                        .unescape()
                        .map_err(|e| Error::Ttml(format!("text unescape failed: {e}")))?
                        .into_owned();
                    if let Some(word) = current_word.as_mut() {
                        word.text.push_str(&text);
                    } else if current_line.is_some() {
                        line_text_buf.push_str(&text);
                    }
                }
                Ok(Event::End(ref e)) => {
                    match local_name(e.name().as_ref()) {
                        b"span" => {
                            if let Some(word) = current_word.take() {
                                if let Some(line) = current_line.as_mut() {
                                    let trimmed = word.text.trim().to_string();
                                    if !trimmed.is_empty() {
                                        line.words.push(LyricsfileWord {
                                            text: trimmed,
                                            start_ms: word.start_ms,
                                            end_ms: word.end_ms,
                                        });
                                    }
                                }
                            }
                        }
                        b"p" => {
                            if let Some(mut line) = current_line.take() {
                                // Reconstruct the line text from words
                                // (preferred — preserves spacing
                                // explicitly) or fall back to the
                                // plain-text buffer.
                                let text = if line.words.is_empty() {
                                    line_text_buf.trim().to_string()
                                } else {
                                    line.words
                                        .iter()
                                        .map(|w| w.text.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                };
                                line_text_buf.clear();
                                if !text.is_empty() || !line.words.is_empty() {
                                    lf.lines.push(LyricsfileLine {
                                        text,
                                        start_ms: line.start_ms,
                                        end_ms: line.end_ms,
                                        words: std::mem::take(&mut line.words),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    // Self-closing <p/> or <span/> — rare in Apple TTML
                    // but handle for spec compliance.
                    if local_name(e.name().as_ref()) == b"span" {
                        if let (Some(start_ms), Some(line)) =
                            (read_time_attr(e, b"begin")?, current_line.as_mut())
                        {
                            line.words.push(LyricsfileWord {
                                text: String::new(),
                                start_ms,
                                end_ms: read_time_attr(e, b"end")?,
                            });
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(Error::Ttml(format!(
                        "XML parse error at position {}: {e}",
                        reader.buffer_position()
                    )))
                }
                _ => {}
            }
            buf.clear();
        }

        if document_language.is_some() {
            lf.metadata.language = document_language;
        }
        Ok(lf)
    }
}

// ============================================================
// Internal helpers
// ============================================================

struct PendingLine {
    start_ms: i64,
    end_ms: Option<i64>,
    words: Vec<LyricsfileWord>,
}

struct PendingWord {
    start_ms: i64,
    end_ms: Option<i64>,
    text: String,
}

/// Strip an XML namespace prefix (`tt:p` → `p`, `itunes:timing` →
/// `timing`) so we match on the local name regardless of producer.
fn local_name(qualified: &[u8]) -> &[u8] {
    match qualified.iter().rposition(|&b| b == b':') {
        Some(idx) => &qualified[idx + 1..],
        None => qualified,
    }
}

/// Read an attribute by exact qualified name (e.g., `b"xml:lang"`,
/// `b"begin"`). Returns `Ok(None)` when absent.
fn read_attr(elem: &BytesStart, name: &[u8]) -> Result<Option<String>> {
    for attr in elem.attributes() {
        let attr = attr.map_err(|e| Error::Ttml(format!("attribute parse failed: {e}")))?;
        if attr.key.as_ref() == name {
            let value = attr
                .unescape_value()
                .map_err(|e| Error::Ttml(format!("attribute unescape failed: {e}")))?
                .into_owned();
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Read a TTML time attribute and convert to milliseconds.
fn read_time_attr(elem: &BytesStart, name: &[u8]) -> Result<Option<i64>> {
    match read_attr(elem, name)? {
        Some(value) => parse_ttml_time(&value).map(Some),
        None => Ok(None),
    }
}

/// Parse a TTML time expression to milliseconds.
///
/// Supports:
/// - `HH:MM:SS.mmm` (Apple Music canonical, 3-digit fractional)
/// - `HH:MM:SS.cc` (older 2-digit centiseconds)
/// - `MM:SS.mmm` (no hours)
/// - `SS.mmm` (seconds only)
/// - `<number>s` clock-time (e.g., `12.5s`) — TTML 1.0 spec form
///
/// Returns the time in integer milliseconds (LRCGET storage unit).
fn parse_ttml_time(raw: &str) -> Result<i64> {
    let s = raw.trim();
    if let Some(stripped) = s.strip_suffix('s') {
        let secs: f64 = stripped
            .parse()
            .map_err(|_| Error::Ttml(format!("invalid clock-time seconds: {raw}")))?;
        return Ok((secs * 1000.0).round() as i64);
    }

    // HH:MM:SS or MM:SS or SS form
    let parts: Vec<&str> = s.split(':').collect();
    let (hours, minutes, seconds_field) = match parts.as_slice() {
        [h, m, s] => (parse_uint(h)?, parse_uint(m)?, *s),
        [m, s] => (0u64, parse_uint(m)?, *s),
        [s] => (0u64, 0u64, *s),
        _ => return Err(Error::Ttml(format!("unrecognised time format: {raw}"))),
    };

    let (secs_str, frac_str) = match seconds_field.split_once('.') {
        Some((s, f)) => (s, f),
        None => (seconds_field, ""),
    };
    let secs: u64 = parse_uint(secs_str)?;
    let frac_ms: u64 = if frac_str.is_empty() {
        0
    } else {
        let n: u64 = parse_uint(frac_str)?;
        // Normalise the fractional digits: 2 digits → centiseconds,
        // 3 digits → milliseconds, longer → truncate by integer
        // division. Matches the LRC parser convention in lrc.rs.
        match frac_str.len() {
            1 => n * 100,
            2 => n * 10,
            3 => n,
            len => n / 10u64.pow((len - 3) as u32),
        }
    };

    Ok(((hours * 3_600_000) + (minutes * 60_000) + (secs * 1_000) + frac_ms) as i64)
}

fn parse_uint(s: &str) -> Result<u64> {
    s.parse::<u64>()
        .map_err(|_| Error::Ttml(format!("invalid integer: {s}")))
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricsfile::LYRICSFILE_VERSION;

    #[test]
    fn parses_line_level_ttml() {
        let ttml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xml:lang="en">
  <body>
    <div>
      <p begin="00:00:01.000" end="00:00:03.500">Hello, it's me</p>
      <p begin="00:00:04.000" end="00:00:06.500">I was wondering</p>
    </div>
  </body>
</tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "Hello", "Adele").unwrap();
        assert_eq!(lf.version, LYRICSFILE_VERSION);
        assert_eq!(lf.metadata.language, Some("en".into()));
        assert_eq!(lf.lines.len(), 2);
        assert_eq!(lf.lines[0].text, "Hello, it's me");
        assert_eq!(lf.lines[0].start_ms, 1000);
        assert_eq!(lf.lines[0].end_ms, Some(3500));
        assert!(lf.lines[0].words.is_empty());
        assert_eq!(lf.lines[1].start_ms, 4000);
    }

    #[test]
    fn parses_word_level_ttml_apple_style() {
        let ttml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml"
    xmlns:itunes="http://music.apple.com/lyric-ttml-internal"
    itunes:timing="Word" xml:lang="en">
  <body>
    <div>
      <p begin="00:00:01.000" end="00:00:03.500">
        <span begin="00:00:01.000" end="00:00:01.800">Hello,</span>
        <span begin="00:00:01.900" end="00:00:02.400">it's</span>
        <span begin="00:00:02.500" end="00:00:03.500">me</span>
      </p>
    </div>
  </body>
</tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "Hello", "Adele").unwrap();
        assert_eq!(lf.lines.len(), 1);
        let line = &lf.lines[0];
        assert_eq!(line.start_ms, 1000);
        assert_eq!(line.end_ms, Some(3500));
        assert_eq!(line.words.len(), 3);
        assert_eq!(line.words[0].text, "Hello,");
        assert_eq!(line.words[0].start_ms, 1000);
        assert_eq!(line.words[0].end_ms, Some(1800));
        assert_eq!(line.words[1].text, "it's");
        assert_eq!(line.words[1].start_ms, 1900);
        assert_eq!(line.words[2].text, "me");
        assert_eq!(line.words[2].start_ms, 2500);
        // Text reconstructed from words joined by spaces.
        assert_eq!(line.text, "Hello, it's me");
    }

    #[test]
    fn preserves_word_timing_within_1ms() {
        let ttml = r#"<tt><body><div>
            <p begin="00:00:00.123" end="00:00:00.456">
                <span begin="00:00:00.123" end="00:00:00.234">a</span>
                <span begin="00:00:00.234" end="00:00:00.456">b</span>
            </p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines[0].words[0].start_ms, 123);
        assert_eq!(lf.lines[0].words[0].end_ms, Some(234));
        assert_eq!(lf.lines[0].words[1].start_ms, 234);
        assert_eq!(lf.lines[0].words[1].end_ms, Some(456));
    }

    #[test]
    fn tolerates_two_digit_centiseconds() {
        // Older Apple TTML sometimes uses `.cc` instead of `.mmm`.
        let ttml = r#"<tt><body><div>
            <p begin="00:00:01.50" end="00:00:03.25">hi</p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines[0].start_ms, 1_500);
        assert_eq!(lf.lines[0].end_ms, Some(3_250));
    }

    #[test]
    fn tolerates_clock_time_seconds_form() {
        let ttml = r#"<tt><body><div>
            <p begin="12.5s" end="15s">hi</p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines[0].start_ms, 12_500);
        assert_eq!(lf.lines[0].end_ms, Some(15_000));
    }

    #[test]
    fn handles_namespaced_element_names() {
        // Producer uses `tt:p` / `tt:span` prefixes.
        let ttml = r#"<tt:tt xmlns:tt="http://www.w3.org/ns/ttml">
            <tt:body><tt:div>
                <tt:p begin="00:00:01.000" end="00:00:02.000">hello</tt:p>
            </tt:div></tt:body>
        </tt:tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines.len(), 1);
        assert_eq!(lf.lines[0].text, "hello");
        assert_eq!(lf.lines[0].start_ms, 1_000);
    }

    #[test]
    fn empty_ttml_returns_empty_lines() {
        let ttml = r#"<tt><body></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert!(lf.lines.is_empty());
        assert!(!lf.metadata.instrumental); // caller decides
    }

    #[test]
    fn span_without_begin_is_treated_as_styling_not_word() {
        // <span> with no `begin` attr is a styling wrapper, not a
        // timed word — its text content should fall through to the
        // line text.
        let ttml = r#"<tt><body><div>
            <p begin="00:00:01.000" end="00:00:02.000">Hello <span>world</span></p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines.len(), 1);
        assert!(lf.lines[0].words.is_empty(), "got words: {:?}", lf.lines[0].words);
        assert!(
            lf.lines[0].text.contains("Hello") && lf.lines[0].text.contains("world"),
            "got text: {:?}",
            lf.lines[0].text
        );
    }

    #[test]
    fn missing_p_begin_attr_is_silently_skipped() {
        // Defensive: a `<p>` with no `begin` attr can't be timed, so
        // we skip it rather than emitting an unsynced line.
        let ttml = r#"<tt><body><div>
            <p>no timing</p>
            <p begin="00:00:05.000">timed</p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines.len(), 1);
        assert_eq!(lf.lines[0].text, "timed");
    }

    #[test]
    fn multiple_div_blocks_are_flattened_in_document_order() {
        let ttml = r#"<tt><body>
            <div><p begin="00:00:01.000">a</p></div>
            <div><p begin="00:00:02.000">b</p></div>
            <div><p begin="00:00:03.000">c</p></div>
        </body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines.len(), 3);
        assert_eq!(
            lf.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn handles_xml_entities_in_lyric_text() {
        let ttml = r#"<tt><body><div>
            <p begin="00:00:01.000">don&apos;t &amp; can&apos;t</p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines[0].text, "don't & can't");
    }

    #[test]
    fn malformed_xml_returns_ttml_error() {
        let ttml = "<tt><body><p begin=";
        let err = Lyricsfile::from_ttml(ttml, "t", "a").unwrap_err();
        assert!(matches!(err, Error::Ttml(_)), "got: {err:?}");
    }

    #[test]
    fn invalid_time_format_returns_ttml_error() {
        let ttml = r#"<tt><body><div>
            <p begin="not a time">hi</p>
        </div></body></tt>"#;
        let err = Lyricsfile::from_ttml(ttml, "t", "a").unwrap_err();
        assert!(matches!(err, Error::Ttml(_)));
    }

    #[test]
    fn end_ms_is_optional() {
        let ttml = r#"<tt><body><div>
            <p begin="00:00:01.000">no end</p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines[0].start_ms, 1000);
        assert_eq!(lf.lines[0].end_ms, None);
    }

    #[test]
    fn fallback_to_no_prefix_lang_attribute() {
        // Some TTML producers emit `lang="en"` rather than `xml:lang="en"`.
        let ttml = r#"<tt lang="ja"><body><div>
            <p begin="00:00:01.000">こんにちは</p>
        </div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.language, Some("ja".into()));
    }
}
