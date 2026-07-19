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
    Lyricsfile, LyricsfileLine, LyricsfileMetadata, LyricsfileSyllable, LyricsfileWord,
    LYRICSFILE_VERSION,
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
        // CRITICAL: trim_text(false) preserves whitespace-only text
        // nodes between sibling elements. We need that to distinguish
        // word-level TTML (timed spans separated by whitespace text
        // nodes) from syllable-level TTML (timed spans with no text
        // node between — direct tag adjacency). Inside spans we trim
        // the accumulated text manually on close.
        reader.config_mut().trim_text(false);

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
        // **Syllable grouping signal** (#60).
        //
        // `true` iff the immediately-preceding sibling event in the
        // current <p> was a `</span>` that closed a timed span AND no
        // whitespace text node has been observed between then and now.
        //
        // When the next `<span begin>` opens with this flag still
        // `true`, the new span is a SYLLABLE CONTINUATION of the
        // previous word — its content is appended to the previous
        // word's `syllables` vec rather than emitted as a new word.
        //
        // Lifecycle:
        // - Reset to `false` on entering `<p>` (first span starts a new word).
        // - Set to `true` on `</span>` close inside a `<p>`.
        // - Reset to `false` on any non-empty text event between spans
        //   (whitespace OR non-whitespace — both indicate the next
        //   span is not adjacent in document order).
        let mut pair_eligible_with_prev_span: bool = false;
        // **Apple `lyricOffset` extraction** (#61 — issue body documents
        // sign convention).
        //
        // Tracks whether we're currently inside `<head><metadata>
        // <iTunesMetadata>` so the `lyricOffset` read on `<audio>` is
        // scoped — we don't want a stray `lyricOffset` attribute on
        // some unrelated element to feed `metadata.offset_ms`.
        //
        // First valid `lyricOffset` value wins. Subsequent occurrences
        // (which shouldn't exist in well-formed TTML) are ignored.
        let mut in_itunes_metadata: bool = false;
        let mut lyric_offset_ms: Option<i64> = None;

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
                        b"iTunesMetadata" => {
                            in_itunes_metadata = true;
                        }
                        b"audio" if in_itunes_metadata && lyric_offset_ms.is_none() => {
                            // Open form `<audio lyricOffset="...">`.
                            // Real Apple TTML usually self-closes this
                            // element (handled in the Empty branch
                            // below), but we accept both shapes.
                            lyric_offset_ms = read_lyric_offset_attr(e);
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
                                // First span inside the <p> is always
                                // a new word, never a syllable
                                // continuation.
                                pair_eligible_with_prev_span = false;
                            }
                        }
                        b"span" => {
                            // Only treat <span> as a word if it has a
                            // `begin` attr; otherwise it's a styling
                            // wrapper and we let its text fall through
                            // into the plain-text buffer.
                            if let Some(start_ms) = read_time_attr(e, b"begin")? {
                                // Capture syllable-grouping decision
                                // BEFORE we reset the flag for the
                                // current span's lifetime.
                                let is_syllable_continuation = pair_eligible_with_prev_span;
                                current_word = Some(PendingWord {
                                    start_ms,
                                    end_ms: read_time_attr(e, b"end")?,
                                    text: String::new(),
                                    is_syllable_continuation,
                                });
                                // While inside a span, the flag has
                                // no meaning (the next span pair-joins
                                // with the CLOSING of this one, not
                                // with its opening).
                                pair_eligible_with_prev_span = false;
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
                        // Outside a span but inside a <p>. Any
                        // non-empty text between sibling spans —
                        // whitespace OR otherwise — terminates the
                        // pair-joining eligibility with the previous
                        // span, because the next span isn't adjacent
                        // in document order any more.
                        if !text.is_empty() {
                            pair_eligible_with_prev_span = false;
                        }
                        line_text_buf.push_str(&text);
                    }
                }
                Ok(Event::End(ref e)) => {
                    match local_name(e.name().as_ref()) {
                        b"iTunesMetadata" => {
                            in_itunes_metadata = false;
                        }
                        b"span" => {
                            if let Some(word) = current_word.take() {
                                if let Some(line) = current_line.as_mut() {
                                    let trimmed = word.text.trim().to_string();
                                    if !trimmed.is_empty() {
                                        if word.is_syllable_continuation && !line.words.is_empty() {
                                            // Append this span as a
                                            // syllable of the previous
                                            // word. If it's the FIRST
                                            // continuation (prev word's
                                            // syllables is empty),
                                            // first promote prev's
                                            // own text/timing into
                                            // syllables[0] so the
                                            // schema invariant
                                            // "concat(syllables[*].
                                            // text) == word.text"
                                            // holds.
                                            let prev = line
                                                .words
                                                .last_mut()
                                                .expect("non-empty checked above");
                                            if prev.syllables.is_empty() {
                                                prev.syllables.push(LyricsfileSyllable {
                                                    text: prev.text.clone(),
                                                    start_ms: prev.start_ms,
                                                    end_ms: prev.end_ms,
                                                });
                                            }
                                            prev.syllables.push(LyricsfileSyllable {
                                                text: trimmed.clone(),
                                                start_ms: word.start_ms,
                                                end_ms: word.end_ms,
                                            });
                                            // Rebuild the merged
                                            // word's surface fields
                                            // from the syllables vec.
                                            // start_ms stays at the
                                            // first syllable; end_ms
                                            // walks forward with each
                                            // appended syllable.
                                            prev.text = prev
                                                .syllables
                                                .iter()
                                                .map(|s| s.text.as_str())
                                                .collect::<String>();
                                            prev.end_ms = word.end_ms;
                                        } else {
                                            line.words.push(LyricsfileWord {
                                                text: trimmed,
                                                start_ms: word.start_ms,
                                                end_ms: word.end_ms,
                                                syllables: Vec::new(),
                                            });
                                        }
                                    }
                                }
                            }
                            // Closing a timed span — the next sibling
                            // span (if it arrives without whitespace
                            // between) is a syllable continuation.
                            if current_line.is_some() {
                                pair_eligible_with_prev_span = true;
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
                    let qualified_name = e.name();
                    let name = local_name(qualified_name.as_ref());
                    // Self-closing <p/> or <span/> — rare in Apple TTML
                    // but handle for spec compliance. A self-closing
                    // timed span has no inner text so it can't be a
                    // meaningful syllable continuation; we always emit
                    // it as a new (text-less) word. Set the
                    // pair-eligibility flag so a FOLLOWING text-bearing
                    // span can still pair-join (rare but defensive).
                    if name == b"span" {
                        if let (Some(start_ms), Some(line)) =
                            (read_time_attr(e, b"begin")?, current_line.as_mut())
                        {
                            line.words.push(LyricsfileWord {
                                text: String::new(),
                                start_ms,
                                end_ms: read_time_attr(e, b"end")?,
                                syllables: Vec::new(),
                            });
                            pair_eligible_with_prev_span = true;
                        }
                    } else if name == b"audio" && in_itunes_metadata && lyric_offset_ms.is_none() {
                        // Apple's canonical form:
                        //   <audio lyricOffset="-0.271" role="spatial"/>
                        // Self-closed and nested inside iTunesMetadata.
                        lyric_offset_ms = read_lyric_offset_attr(e);
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
        if lyric_offset_ms.is_some() {
            lf.metadata.offset_ms = lyric_offset_ms;
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
    /// Set on `<span begin>` Start when the immediately-preceding
    /// sibling was a `</span>` close with no whitespace text node
    /// between. On Close, this drives the merge-into-previous-word
    /// branch (syllable promotion).
    is_syllable_continuation: bool,
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

/// Read Apple's `lyricOffset` attribute off `<audio>` and convert from
/// seconds (float) to milliseconds (i64).
///
/// # Sign convention — **ASSUMPTION**, not verified
///
/// Apple's TTML emits `lyricOffset` as a signed float in seconds
/// (Closer fixture: `lyricOffset="-0.271"`). LRC's `[offset:NNN]` tag
/// stores a signed integer in milliseconds with the convention
/// "positive shifts lyrics LATER than the marked timestamp, negative
/// shifts EARLIER" (and that's what every major LRC consumer —
/// foobar2000, MusicBee, Plex, Synchronicity — honours).
///
/// **This function PRESERVES Apple's sign verbatim.** We assume
/// Apple's `lyricOffset` uses the same sign convention as LRC's
/// `[offset:]` tag, so `-0.271` seconds → `Some(-271)` ms. If that
/// assumption turns out to be inverted, the fix is a one-line
/// negation: change `(secs * 1000.0).round() as i64` to
/// `-(secs * 1000.0).round() as i64`. The fixture-based test
/// `extracts_lyric_offset_from_closer_fixture` pins our current
/// assumption explicitly so a future verifier knows exactly what
/// flipping the sign would change.
///
/// **How to verify** (tracked as MeedyaSuite-core #61):
/// 1. Pick ~5 songs across genres, download via MeedyaDL.
/// 2. A/B compare uncalibrated-LRC vs calibrated-LRC in a player
///    that honours `[offset:]` (foobar2000 + LRC plugin, MusicBee).
/// 3. If the calibrated output IMPROVES sync → sign is right; mark
///    #61 verified and close.
/// 4. If the calibrated output WORSENS sync by the same magnitude
///    in the opposite direction → flip the sign + update the test.
///
/// # `leadingSilence` deliberately ignored
///
/// Apple's `<iTunesMetadata leadingSilence="0.280">` is the AAC
/// encoder pre-roll silence and is handled by the audio decoder's
/// gapless-playback machinery (CoreAudio, libavcodec). Lifting it
/// into `metadata.offset_ms` would double-correct on any player
/// that already honours the AAC priming sample. Out of scope here.
fn read_lyric_offset_attr(elem: &BytesStart) -> Option<i64> {
    let raw = read_attr(elem, b"lyricOffset").ok().flatten()?;
    let secs: f64 = raw.trim().parse().ok()?;
    // f64 → i64 via round. Apple emits at most 3 fractional digits
    // (millisecond precision), so round() is exact; we never lose
    // sub-ms accuracy.
    Some((secs * 1000.0).round() as i64)
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
        assert!(
            lf.lines[0].words.is_empty(),
            "got words: {:?}",
            lf.lines[0].words
        );
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

    // ------------------------------------------------------------
    // Syllable grouping tests (#60)
    // ------------------------------------------------------------

    #[test]
    fn syllable_pair_groups_into_one_word_with_two_syllables() {
        // Two adjacent timed spans with NO whitespace text node
        // between them — the canonical Apple syllable encoding.
        // Mirrors `.examplefiles/.../Closer_PrettyPrint.ttml` line 20:
        // `<span begin="7.516" end="8.097">Clos</span><span
        // begin="8.097" end="8.904">er</span>`.
        let ttml = r#"<tt xmlns:itunes="http://music.apple.com/lyric-ttml-internal" itunes:timing="Word"><body><div><p begin="7.516" end="8.904"><span begin="7.516" end="8.097">Clos</span><span begin="8.097" end="8.904">er</span></p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "Closer", "Ne-Yo").unwrap();
        assert_eq!(lf.lines.len(), 1);
        let line = &lf.lines[0];
        // Expect a SINGLE merged word with two syllables, not two
        // separate words. This is the key #60 grouping behaviour.
        assert_eq!(
            line.words.len(),
            1,
            "expected 1 merged word, got {}: {:?}",
            line.words.len(),
            line.words
        );
        let word = &line.words[0];
        assert_eq!(word.text, "Closer");
        assert_eq!(word.start_ms, 7_516);
        assert_eq!(word.end_ms, Some(8_904));
        assert_eq!(word.syllables.len(), 2);
        assert_eq!(word.syllables[0].text, "Clos");
        assert_eq!(word.syllables[0].start_ms, 7_516);
        assert_eq!(word.syllables[0].end_ms, Some(8_097));
        assert_eq!(word.syllables[1].text, "er");
        assert_eq!(word.syllables[1].start_ms, 8_097);
        assert_eq!(word.syllables[1].end_ms, Some(8_904));
    }

    #[test]
    fn word_with_whitespace_between_does_not_group_into_syllables() {
        // Spaces between sibling timed spans — the canonical
        // word-level encoding. Each span stays a separate word, no
        // syllables vec populated. Pins the inverse of the syllable
        // pair test.
        let ttml = r#"<tt itunes:timing="Word"><body><div><p begin="0.0"><span begin="0.0">Hello</span> <span begin="0.5">world</span></p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.lines[0].words.len(), 2);
        assert_eq!(lf.lines[0].words[0].text, "Hello");
        assert!(lf.lines[0].words[0].syllables.is_empty());
        assert_eq!(lf.lines[0].words[1].text, "world");
        assert!(lf.lines[0].words[1].syllables.is_empty());
    }

    #[test]
    fn mixed_line_groups_syllable_pair_but_leaves_other_words_alone() {
        // Real-world shape from Closer L31: "Come <Clos><er>" — first
        // word is standalone, then a syllable pair. The parser must
        // produce 2 words for the line, where word[1] has 2
        // syllables and word[0] does not.
        let ttml = r#"<tt itunes:timing="Word"><body><div><p begin="1:53.630"><span begin="1:53.630" end="1:54.187">Come</span> <span begin="1:54.187" end="1:55.121">clos</span><span begin="1:55.121" end="1:56.818">er</span></p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        let line = &lf.lines[0];
        assert_eq!(line.words.len(), 2, "got: {:?}", line.words);
        assert_eq!(line.words[0].text, "Come");
        assert!(line.words[0].syllables.is_empty());
        assert_eq!(line.words[1].text, "closer");
        assert_eq!(line.words[1].syllables.len(), 2);
        assert_eq!(line.words[1].syllables[0].text, "clos");
        assert_eq!(line.words[1].syllables[1].text, "er");
        // 1:54.187 = 60_000 + 54_000 + 187 = 114_187ms
        assert_eq!(line.words[1].start_ms, 114_187);
        // 1:56.818 = 60_000 + 56_000 + 818 = 116_818ms
        assert_eq!(line.words[1].end_ms, Some(116_818));
    }

    #[test]
    fn three_syllable_word_promotes_first_span_then_appends_two_more() {
        // A word split into 3 syllables — exercises the
        // promote-prev-text-to-syllables[0] branch followed by two
        // continuation appends.
        let ttml = r#"<tt itunes:timing="Word"><body><div><p begin="0.0"><span begin="0.0" end="0.3">be</span><span begin="0.3" end="0.6">au</span><span begin="0.6" end="1.0">ti</span></p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        let line = &lf.lines[0];
        assert_eq!(line.words.len(), 1);
        assert_eq!(line.words[0].text, "beauti");
        assert_eq!(line.words[0].syllables.len(), 3);
        assert_eq!(line.words[0].syllables[0].text, "be");
        assert_eq!(line.words[0].syllables[1].text, "au");
        assert_eq!(line.words[0].syllables[2].text, "ti");
        assert_eq!(line.words[0].start_ms, 0);
        assert_eq!(line.words[0].end_ms, Some(1_000));
    }

    #[test]
    fn syllable_grouping_against_pretty_fixture_file() {
        // End-to-end test against the committed pretty-printed
        // fixture. Confirms the parser handles real Apple TTML
        // (indented, namespace-decorated, with multiple <div>s).
        let ttml = include_str!("../test-fixtures/closer-syllable-pretty.ttml");
        let lf = Lyricsfile::from_ttml(ttml, "Closer", "Ne-Yo").unwrap();

        // Line 1 ("Closer" — syllable pair) — first line in the file.
        let l1 = &lf.lines[0];
        assert_eq!(l1.words.len(), 1, "L1 should be one merged word");
        assert_eq!(l1.words[0].text, "Closer");
        assert_eq!(l1.words[0].syllables.len(), 2);

        // The Verse line ("Turn the lights off in this place") should
        // produce 7 distinct words, none with syllables.
        let verse_idx = lf
            .lines
            .iter()
            .position(|l| l.text.starts_with("Turn"))
            .expect("Turn line present in fixture");
        let verse = &lf.lines[verse_idx];
        assert_eq!(verse.words.len(), 7, "got: {:?}", verse.words);
        assert!(verse.words.iter().all(|w| w.syllables.is_empty()));

        // The PreChorus L31 line should produce "Come" + "closer"
        // (syllable-merged) at minimum. The x-bg wrapper's nested
        // spans may or may not contribute additional words depending
        // on whether the parser descends into them — for v1, the
        // primary expectation is that the main-vocal syllable pair
        // groups correctly.
        let pre_idx = lf
            .lines
            .iter()
            .position(|l| l.text.contains("Come"))
            .expect("Come line present");
        let pre = &lf.lines[pre_idx];
        let closer_word = pre
            .words
            .iter()
            .find(|w| w.text == "closer")
            .expect("syllable-merged closer present in PreChorus line");
        assert_eq!(closer_word.syllables.len(), 2);
    }

    // ------------------------------------------------------------
    // Apple `lyricOffset` extraction tests (#61 partial implementation)
    // ------------------------------------------------------------
    //
    // These tests pin the SIGN CONVENTION assumption made by
    // `read_lyric_offset_attr`. Apple's `lyricOffset="-0.271"`
    // currently maps to `metadata.offset_ms = Some(-271)` — sign
    // preserved verbatim. If a future verifier finds the convention
    // is inverted in real players, the fix is a single negation
    // inside the helper AND the assertions below need to flip too.

    #[test]
    fn extracts_lyric_offset_negative_from_self_closing_audio() {
        // Real Apple shape: <audio lyricOffset="-0.271" role="spatial"/>
        // — self-closing inside <iTunesMetadata>.
        let ttml = r#"<tt xmlns:itunes="http://music.apple.com/lyric-ttml-internal">
            <head><metadata>
                <iTunesMetadata xmlns="http://music.apple.com/lyric-ttml-internal" leadingSilence="0.280">
                    <audio lyricOffset="-0.271" role="spatial"/>
                </iTunesMetadata>
            </metadata></head>
            <body><div><p begin="0.0">hi</p></div></body>
        </tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, Some(-271));
    }

    #[test]
    fn extracts_lyric_offset_positive_value() {
        let ttml = r#"<tt><head><metadata>
            <iTunesMetadata>
                <audio lyricOffset="0.500"/>
            </iTunesMetadata>
        </metadata></head><body><div><p begin="0.0">hi</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, Some(500));
    }

    #[test]
    fn extracts_lyric_offset_zero_value() {
        // Apple sometimes emits 0.0 — should land as Some(0), not None,
        // so the downstream `to_lrc` no-op-skip logic correctly omits
        // the tag (rather than emitting `[offset:0]`).
        let ttml = r#"<tt><head><metadata>
            <iTunesMetadata>
                <audio lyricOffset="0.0"/>
            </iTunesMetadata>
        </metadata></head><body><div><p begin="0.0">hi</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, Some(0));
    }

    #[test]
    fn lyric_offset_absent_yields_none() {
        // No iTunesMetadata, no audio element — offset_ms stays None.
        let ttml = r#"<tt><body><div><p begin="0.0">hi</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, None);
    }

    #[test]
    fn lyric_offset_on_audio_outside_itunes_metadata_is_ignored() {
        // Defensive: a stray `<audio lyricOffset>` outside of
        // iTunesMetadata should NOT feed offset_ms. Only the
        // scoped-by-parent attribute counts.
        let ttml = r#"<tt><head>
            <audio lyricOffset="-0.500"/>
        </head><body><div><p begin="0.0">hi</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, None);
    }

    #[test]
    fn lyric_offset_first_value_wins_against_duplicates() {
        // Multi-tag conflict — first valid lyricOffset wins. Apple's
        // TTML shouldn't ever have two but defensive against future
        // format quirks.
        let ttml = r#"<tt><head><metadata>
            <iTunesMetadata>
                <audio lyricOffset="-0.271"/>
                <audio lyricOffset="0.500"/>
            </iTunesMetadata>
        </metadata></head><body><div><p begin="0.0">hi</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, Some(-271));
    }

    #[test]
    fn lyric_offset_malformed_value_silently_skipped() {
        // Recoverable: malformed value yields None rather than an
        // error. Lyrics still play without calibration.
        let ttml = r#"<tt><head><metadata>
            <iTunesMetadata>
                <audio lyricOffset="not-a-number"/>
            </iTunesMetadata>
        </metadata></head><body><div><p begin="0.0">hi</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        assert_eq!(lf.metadata.offset_ms, None);
    }

    #[test]
    fn extracts_lyric_offset_from_closer_fixture() {
        // Real-world fixture pin. Closer's `<audio lyricOffset="-0.271"
        // role="spatial"/>` → offset_ms = -271. THIS IS THE SIGN-
        // CONVENTION ASSUMPTION ANCHOR — see #61.
        let ttml = include_str!("../test-fixtures/closer-syllable-pretty.ttml");
        let lf = Lyricsfile::from_ttml(ttml, "Closer", "Ne-Yo").unwrap();
        assert_eq!(
            lf.metadata.offset_ms,
            Some(-271),
            "Apple's lyricOffset=-0.271 should map to offset_ms=-271 (sign preserved verbatim)"
        );
    }

    #[test]
    fn lyric_offset_round_trips_from_ttml_to_lrc_export() {
        // End-to-end: Apple TTML → Lyricsfile → LRC export. The
        // [offset:] header tag must appear in the exported LRC with
        // the same sign that came out of the TTML.
        let ttml = r#"<tt><head><metadata>
            <iTunesMetadata>
                <audio lyricOffset="-0.271"/>
            </iTunesMetadata>
        </metadata></head><body><div><p begin="00:00:01.000">Hello</p></div></body></tt>"#;
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        let lrc = lf.to_lrc();
        assert!(
            lrc.starts_with("[offset:-271]\n"),
            "expected Apple lyricOffset to flow through to LRC export, got: {lrc:?}"
        );
    }

    #[test]
    fn syllable_grouping_against_minified_word_fixture_does_not_false_positive() {
        // Adversarial-verifier amendment: minified word-only TTML
        // must NOT be misclassified as syllable-level. The fixture
        // has all whitespace between structural elements stripped,
        // BUT preserves the literal spaces between word-level
        // <span>s inside each <p>. The parser must still produce
        // separate words with no syllables vec populated.
        let ttml = include_str!("../test-fixtures/word-only-minified.ttml");
        let lf = Lyricsfile::from_ttml(ttml, "t", "a").unwrap();
        // Expect multiple words per line, none with syllables.
        for line in &lf.lines {
            assert!(
                !line.words.is_empty(),
                "minified word-only fixture should produce words"
            );
            for word in &line.words {
                assert!(
                    word.syllables.is_empty(),
                    "minified word-only fixture should not produce syllables: word={:?}",
                    word
                );
            }
        }
    }
}
