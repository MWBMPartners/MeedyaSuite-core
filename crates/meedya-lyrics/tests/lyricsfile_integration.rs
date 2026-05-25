// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Integration tests for the full Lyricsfile pipeline (#34).
//
// Per-module tests cover individual converters in isolation. These
// tests exercise the realistic end-to-end paths a consumer (MeedyaDL,
// MeedyaConverter) walks:
//
//   Apple TTML
//      ↓ from_ttml
//   Lyricsfile (in-memory)
//      ↓ to_yaml
//   YAML on disk
//      ↓ parse
//   Lyricsfile (back in memory)
//      ↓ to_enhanced_lrc / to_lrc / to_srt / to_webvtt / to_ass
//   Sidecar file(s) on disk
//
// They also pin the issue-#34 acceptance criteria: word-level timing
// preserved within 1ms via TTML, 10ms via LRC, plus instrumental
// round-trip integrity.

use meedya_lyrics::{Lyricsfile, LyricsfileMetadata, LYRICSFILE_VERSION};

// A representative Apple Music word-level TTML sample with two lines,
// one with word timing and one without (mixed-shape inputs are common).
const SAMPLE_TTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
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
      <p begin="00:00:04.000" end="00:00:06.500">I was wondering</p>
    </div>
  </body>
</tt>"#;

#[test]
fn end_to_end_ttml_through_yaml_back_to_struct_preserves_word_timing() {
    let lf = Lyricsfile::from_ttml(SAMPLE_TTML, "Hello", "Adele").expect("from_ttml");
    let yaml = lf.to_yaml().expect("to_yaml");
    let back = Lyricsfile::parse(&yaml).expect("parse");

    // Acceptance criterion: word-level timing preserved within 1ms.
    assert_eq!(back.lines.len(), 2);
    let first = &back.lines[0];
    assert_eq!(first.words.len(), 3);
    assert!((first.words[0].start_ms - 1000).abs() <= 1);
    assert!((first.words[1].start_ms - 1900).abs() <= 1);
    assert!((first.words[2].start_ms - 2500).abs() <= 1);
    assert_eq!(first.words[0].text, "Hello,");
    assert_eq!(first.words[2].text, "me");

    // Mixed-shape: second line has no word timing.
    let second = &back.lines[1];
    assert!(second.words.is_empty());
    assert_eq!(second.text, "I was wondering");
    assert_eq!(second.start_ms, 4000);
}

#[test]
fn ttml_to_enhanced_lrc_back_to_lyricsfile_preserves_words_within_10ms() {
    let original = Lyricsfile::from_ttml(SAMPLE_TTML, "Hello", "Adele").unwrap();
    let enh = original.to_enhanced_lrc();
    let recovered = Lyricsfile::from_lrc(&enh, "Hello", "Adele").unwrap();

    // The LRC format is centisecond-precision, so word starts can lose
    // up to ~10ms in the round-trip. Issue #34 acceptance criterion
    // is 10ms tolerance for LRC paths.
    assert_eq!(recovered.lines.len(), 2);
    let words = &recovered.lines[0].words;
    assert_eq!(words.len(), 3);
    assert!((words[0].start_ms - 1000).abs() <= 10);
    assert!((words[1].start_ms - 1900).abs() <= 10);
    assert!((words[2].start_ms - 2500).abs() <= 10);
}

#[test]
fn all_five_exports_produce_non_empty_output_for_sample() {
    let lf = Lyricsfile::from_ttml(SAMPLE_TTML, "Hello", "Adele").unwrap();
    let lrc = lf.to_lrc();
    let enh = lf.to_enhanced_lrc();
    let srt = lf.to_srt();
    let vtt = lf.to_webvtt();
    let ass = lf.to_ass();

    assert!(lrc.contains("[00:01.00]"), "LRC missing timestamp: {lrc}");
    assert!(enh.contains("<00:01.00>"), "Enhanced LRC missing word marker");
    assert!(srt.contains("00:00:01,000 -->"), "SRT missing start");
    assert!(vtt.starts_with("WEBVTT"), "VTT missing header");
    assert!(ass.contains("Dialogue: 0,0:00:01.00,"), "ASS missing dialogue");
}

#[test]
fn yaml_output_is_self_documenting_and_human_readable() {
    // Smoke test that the YAML output looks like the format we expect a
    // user to be willing to edit by hand. The LRCGET 2.0 release notes
    // pitch this as a core feature.
    let lf = Lyricsfile {
        version: LYRICSFILE_VERSION.into(),
        metadata: LyricsfileMetadata {
            title: "Hello".into(),
            artist: "Adele".into(),
            album: Some("25".into()),
            duration_ms: Some(295_000),
            offset_ms: None,
            language: Some("en".into()),
            instrumental: false,
        },
        lines: vec![],
        plain: Some("plain text fallback".into()),
    };
    let yaml = lf.to_yaml().unwrap();
    // Field names should be unquoted (YAML default), values readable.
    assert!(yaml.contains("title: Hello"));
    assert!(yaml.contains("artist: Adele"));
    assert!(yaml.contains("album: '25'") || yaml.contains("album: \"25\"") || yaml.contains("album: 25"));
    assert!(yaml.contains("duration_ms: 295000"));
    assert!(yaml.contains("language: en"));
    assert!(yaml.contains("instrumental: false"));
}

#[test]
fn instrumental_track_round_trips_through_yaml_and_lrc() {
    let mut lf = Lyricsfile::new("Silent Movie", "Some Composer");
    lf.mark_instrumental();
    lf.metadata.album = Some("Silent Films Vol 1".into());

    // YAML round-trip
    let yaml = lf.to_yaml().unwrap();
    let back = Lyricsfile::parse(&yaml).unwrap();
    assert!(back.metadata.instrumental);
    assert_eq!(back.metadata.album, Some("Silent Films Vol 1".into()));

    // LRC round-trip via the instrumental marker
    let lrc = lf.to_lrc();
    assert!(lrc.contains("[au: instrumental]"));
    let recovered = Lyricsfile::from_lrc(&lrc, "Silent Movie", "Some Composer").unwrap();
    assert!(recovered.metadata.instrumental);
    assert!(recovered.lines.is_empty());
}

#[test]
fn empty_ttml_does_not_panic_or_emit_invalid_output() {
    let empty = r#"<tt><body></body></tt>"#;
    let lf = Lyricsfile::from_ttml(empty, "T", "A").unwrap();
    let yaml = lf.to_yaml().unwrap();
    // Must round-trip cleanly even with no lines.
    let back = Lyricsfile::parse(&yaml).unwrap();
    assert!(back.lines.is_empty());
    assert!(!back.metadata.instrumental);
}

#[test]
fn forward_compat_unknown_fields_dont_break_export() {
    // A YAML document from a future Lyricsfile version with extra
    // fields should still parse and export cleanly through our code.
    let future_yaml = r#"
version: "1.1"
metadata:
  title: T
  artist: A
  instrumental: false
  future_metadata_field: "ignored"
lines:
  - text: "hi"
    start_ms: 1000
    end_ms: 2000
    speaker: "ignored-future-field"
"#;
    let lf = Lyricsfile::parse(future_yaml).unwrap();
    let lrc = lf.to_lrc();
    assert!(lrc.contains("[00:01.00]hi"));
}
