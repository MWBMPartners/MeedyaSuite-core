// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Standard, non-proprietary tag read/write.
//
// Covers the cross-format tag fields that have widespread player support:
// BPM, initial key, comment. Mixed In Key writes only these — so Phase 1
// reads MIK output natively without a dedicated MIK module.
//
// Format mapping (handled by lofty's ItemKey layer):
//
//   BPM (integer):
//     ID3v2  → TBPM
//     MP4    → tmpo
//     Vorbis → BPM
//     RIFF   → IBPM (where supported)
//
//   Initial Key:
//     ID3v2  → TKEY
//     MP4    → ----:com.apple.iTunes:initialkey
//     Vorbis → INITIALKEY
//
//   Comment:
//     ID3v2  → COMM (description-less)
//     MP4    → ©cmt
//     Vorbis → COMMENT

use lofty::tag::{ItemKey, Tag};

use crate::model::MusicalKey;

/// Read BPM from a tag. Accepts both integer (`IntegerBpm`) and float
/// (`Bpm`) keys; float wins when both are present.
pub fn read_bpm(tag: &Tag) -> Option<f64> {
    if let Some(s) = tag.get_string(&ItemKey::Bpm) {
        if let Ok(v) = s.parse::<f64>() {
            return Some(v);
        }
    }
    if let Some(s) = tag.get_string(&ItemKey::IntegerBpm) {
        if let Ok(v) = s.parse::<f64>() {
            return Some(v);
        }
    }
    None
}

/// Write BPM. Stores integer form (rounded) into `IntegerBpm` for maximum
/// player compatibility (TBPM and tmpo are integer-only) and the float
/// form into `Bpm` so MP4 files retain precision via the
/// `----:com.apple.iTunes:BPM` freeform atom.
pub fn write_bpm(tag: &mut Tag, bpm: f64) {
    let rounded = bpm.round() as i64;
    tag.insert_text(ItemKey::IntegerBpm, rounded.to_string());
    tag.insert_text(ItemKey::Bpm, format_bpm(bpm));
}

pub fn clear_bpm(tag: &mut Tag) {
    tag.remove_key(&ItemKey::Bpm);
    tag.remove_key(&ItemKey::IntegerBpm);
}

/// Read the initial key from a tag, parsing any of the common notations.
pub fn read_key(tag: &Tag) -> Option<MusicalKey> {
    let s = tag.get_string(&ItemKey::InitialKey)?;
    MusicalKey::parse(s)
}

/// Read the raw key string as written, without normalising. Useful when
/// the source string contains DJ-specific extensions we don't want to
/// silently lose (e.g., Camelot vs traditional preference).
pub fn read_key_raw(tag: &Tag) -> Option<String> {
    tag.get_string(&ItemKey::InitialKey).map(str::to_owned)
}

/// Write the initial key in traditional notation ("Am", "C", "F#m") —
/// the only notation Apple Music's Music app, foobar2000, and most
/// non-DJ players display correctly.
pub fn write_key(tag: &mut Tag, key: MusicalKey) {
    tag.insert_text(ItemKey::InitialKey, key.traditional());
}

/// Write a raw key string verbatim. Use when the caller wants to preserve
/// a Camelot or other non-traditional format the source app expects.
pub fn write_key_raw(tag: &mut Tag, value: String) {
    tag.insert_text(ItemKey::InitialKey, value);
}

pub fn clear_key(tag: &mut Tag) {
    tag.remove_key(&ItemKey::InitialKey);
}

pub fn read_comment(tag: &Tag) -> Option<String> {
    tag.get_string(&ItemKey::Comment).map(str::to_owned)
}

pub fn write_comment(tag: &mut Tag, value: String) {
    tag.insert_text(ItemKey::Comment, value);
}

pub fn clear_comment(tag: &mut Tag) {
    tag.remove_key(&ItemKey::Comment);
}

/// Format a BPM value for the float-precision `Bpm` key.
/// Trims trailing zeros so `128.0` → `"128"` and `127.5` → `"127.5"`.
fn format_bpm(bpm: f64) -> String {
    if bpm.fract() == 0.0 {
        format!("{:.0}", bpm)
    } else {
        let s = format!("{:.4}", bpm);
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KeyMode, Note};
    use lofty::tag::{Tag, TagType};

    fn fresh_tag(tag_type: TagType) -> Tag {
        Tag::new(tag_type)
    }

    #[test]
    fn format_bpm_integer() {
        assert_eq!(format_bpm(128.0), "128");
    }

    #[test]
    fn format_bpm_fractional() {
        assert_eq!(format_bpm(127.5), "127.5");
        assert_eq!(format_bpm(123.4567), "123.4567");
    }

    #[test]
    fn format_bpm_trailing_zeros_trimmed() {
        assert_eq!(format_bpm(120.10), "120.1");
    }

    #[test]
    fn write_then_read_bpm_id3v2() {
        let mut tag = fresh_tag(TagType::Id3v2);
        write_bpm(&mut tag, 128.0);
        assert_eq!(read_bpm(&tag), Some(128.0));
    }

    #[test]
    fn write_then_read_bpm_mp4() {
        let mut tag = fresh_tag(TagType::Mp4Ilst);
        write_bpm(&mut tag, 127.5);
        // MP4 has both Bpm (float) and IntegerBpm; float should win on read.
        assert_eq!(read_bpm(&tag), Some(127.5));
    }

    #[test]
    fn read_bpm_falls_back_to_integer_when_float_absent() {
        let mut tag = fresh_tag(TagType::Id3v2);
        // ID3v2 only supports integer BPM; write should set IntegerBpm.
        write_bpm(&mut tag, 130.0);
        assert_eq!(read_bpm(&tag), Some(130.0));
    }

    #[test]
    fn clear_bpm_removes_both_keys() {
        let mut tag = fresh_tag(TagType::Mp4Ilst);
        write_bpm(&mut tag, 128.0);
        clear_bpm(&mut tag);
        assert_eq!(read_bpm(&tag), None);
    }

    #[test]
    fn write_then_read_key_id3v2() {
        let mut tag = fresh_tag(TagType::Id3v2);
        let key = MusicalKey {
            tonic: Note::A,
            mode: KeyMode::Minor,
        };
        write_key(&mut tag, key);
        assert_eq!(read_key(&tag), Some(key));
        assert_eq!(read_key_raw(&tag).as_deref(), Some("Am"));
    }

    #[test]
    fn write_then_read_key_mp4() {
        let mut tag = fresh_tag(TagType::Mp4Ilst);
        let key = MusicalKey {
            tonic: Note::FSharp,
            mode: KeyMode::Minor,
        };
        write_key(&mut tag, key);
        assert_eq!(read_key(&tag), Some(key));
    }

    #[test]
    fn read_key_parses_camelot_input() {
        let mut tag = fresh_tag(TagType::Id3v2);
        write_key_raw(&mut tag, "8A".to_owned());
        let key = read_key(&tag).expect("parses Camelot");
        assert_eq!(key.traditional(), "Am");
    }

    #[test]
    fn write_key_raw_preserves_format() {
        let mut tag = fresh_tag(TagType::Id3v2);
        write_key_raw(&mut tag, "8A".to_owned());
        assert_eq!(read_key_raw(&tag).as_deref(), Some("8A"));
    }

    #[test]
    fn clear_key_removes_field() {
        let mut tag = fresh_tag(TagType::Id3v2);
        write_key(
            &mut tag,
            MusicalKey {
                tonic: Note::C,
                mode: KeyMode::Major,
            },
        );
        clear_key(&mut tag);
        assert_eq!(read_key(&tag), None);
    }

    #[test]
    fn comment_round_trip() {
        let mut tag = fresh_tag(TagType::Id3v2);
        write_comment(&mut tag, "Energy 7 - peak time".to_owned());
        assert_eq!(read_comment(&tag).as_deref(), Some("Energy 7 - peak time"));
        clear_comment(&mut tag);
        assert_eq!(read_comment(&tag), None);
    }

    #[test]
    fn read_returns_none_for_missing_fields() {
        let tag = fresh_tag(TagType::Id3v2);
        assert_eq!(read_bpm(&tag), None);
        assert_eq!(read_key(&tag), None);
        assert_eq!(read_comment(&tag), None);
    }
}
