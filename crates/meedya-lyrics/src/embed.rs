//! Embed lyrics into a media file's tag container.
//!
//! Two write targets:
//!
//! - [`embed`] — plain-text via `meedya-metadata`'s `CommonTag::Lyrics`,
//!   which maps to the correct atom per format: USLT for ID3v2, `LYRICS`
//!   for Vorbis Comment, `©lyr` for MP4 ilst. Format detection delegated
//!   to lofty.
//! - [`embed_synced`] — ID3v2 SYLT (synchronised lyrics). Only works on
//!   ID3v2 containers (MP3, optionally WAV/AIFF with an ID3 chunk). For
//!   other formats (MP4 / Vorbis / FLAC), no widely-supported synchronised
//!   embed standard exists — this function returns an error and callers
//!   should fall back to `embed` for plain-text.
//!
//! ## Recommended pattern
//!
//! ```ignore
//! // Best-effort: write plain text everywhere, plus SYLT on ID3v2.
//! let _plain = meedya_lyrics::embed(&path, &lyrics)?;
//! if lyrics.synced.is_some() {
//!     // Only succeeds on ID3v2 containers; otherwise ignore the error.
//!     let _ = meedya_lyrics::embed_synced(&path, &lyrics, b"eng".to_owned());
//! }
//! ```

use std::borrow::Cow;
use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::TaggedFile;
use lofty::id3::v2::{
    BinaryFrame, Frame, FrameId, Id3v2Tag, SyncTextContentType, SynchronizedTextFrame,
    TimestampFormat,
};
use lofty::prelude::*;
use lofty::tag::TagType;
use lofty::TextEncoding;
use meedya_metadata::{tag_io, CommonTag};

use crate::{Error, Lyrics, Result};

/// ISO-639-2 language code, three lowercase ASCII letters. Used for SYLT
/// frame headers when the source lyrics don't carry a language tag.
pub const DEFAULT_LANGUAGE: [u8; 3] = *b"eng";

/// Embed the plain-text representation of `lyrics` into `media`'s tags.
///
/// Returns `true` if anything was written, `false` if `lyrics` has no
/// embeddable content. Synchronised timestamps, if present, are flattened
/// to plain text. Use [`embed_synced`] alongside this for ID3v2 SYLT.
pub fn embed(media: &Path, lyrics: &Lyrics) -> Result<bool> {
    let Some(text) = plain_text(lyrics) else {
        return Ok(false);
    };
    tag_io::write_tags(media, &[(CommonTag::Lyrics, text)])?;
    Ok(true)
}

/// Embed synchronised lyrics (ID3v2 SYLT frame) into `media`.
///
/// `lang` is the ISO-639-2 three-letter language code; pass [`DEFAULT_LANGUAGE`]
/// (`b"eng"`) if unknown. Encoding is UTF-16 with BOM for cross-player
/// compatibility with non-ASCII text.
///
/// Errors if the file is not an ID3v2 container or if `lyrics.synced` is
/// `None` / empty. Replaces any existing SYLT frame.
pub fn embed_synced(media: &Path, lyrics: &Lyrics, lang: [u8; 3]) -> Result<()> {
    let synced = lyrics
        .synced
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(Error::NoSyncedLyrics)?;

    if !lang.iter().all(u8::is_ascii_alphabetic) {
        return Err(Error::InvalidLanguageCode);
    }

    let mut tagged: TaggedFile = lofty::read_from_path(media)
        .map_err(|e| Error::Metadata(meedya_metadata::MetadataError::ReadError(e.to_string())))?;

    if !tagged.supports_tag_type(TagType::Id3v2) {
        return Err(Error::UnsupportedForSync {
            tag_type: format!("{:?}", tagged.primary_tag_type()),
        });
    }

    // Serialize a SynchronizedTextFrame and insert it as a SYLT binary frame.
    // Lofty doesn't expose SYLT as a Frame enum variant in 0.22, so we go
    // via bytes — this is the documented escape hatch for less-common frames.
    let entries: Vec<(u32, String)> = synced
        .iter()
        .map(|line| (millis(line), line.text.clone()))
        .collect();

    let sylt = SynchronizedTextFrame::new(
        TextEncoding::UTF16,
        lang,
        TimestampFormat::MS,
        SyncTextContentType::Lyrics,
        None,
        entries,
    );
    let bytes = sylt
        .as_bytes()
        .map_err(|e| Error::Metadata(meedya_metadata::MetadataError::WriteError(e.to_string())))?;
    let frame_id = FrameId::Valid(Cow::Borrowed("SYLT"));
    let sylt_frame = Frame::Binary(BinaryFrame::new(frame_id.clone(), bytes));

    let id3v2 = match tagged.tag_mut(TagType::Id3v2) {
        Some(tag) => tag,
        None => {
            tagged.insert_tag(lofty::tag::Tag::new(TagType::Id3v2));
            tagged
                .tag_mut(TagType::Id3v2)
                .expect("ID3v2 tag was just inserted")
        }
    };

    // Get the underlying Id3v2Tag if possible to use the typed insert; otherwise
    // fall back to the generic Tag API by removing-and-reinserting via a workaround.
    // lofty's Tag wraps Id3v2Tag transparently when the tag_type matches, but the
    // typed API requires us to round-trip through Id3v2Tag::from(&Tag) and back.
    let mut id3v2_typed = Id3v2Tag::from(std::mem::replace(
        id3v2,
        lofty::tag::Tag::new(TagType::Id3v2),
    ));
    id3v2_typed.remove(&frame_id).for_each(drop);
    id3v2_typed.insert(sylt_frame);
    *id3v2 = lofty::tag::Tag::from(id3v2_typed);

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(media)
        .map_err(|e| Error::Metadata(meedya_metadata::MetadataError::WriteError(e.to_string())))?;
    let mut file = file;
    tagged
        .save_to(&mut file, WriteOptions::default())
        .map_err(|e| Error::Metadata(meedya_metadata::MetadataError::WriteError(e.to_string())))?;

    Ok(())
}

fn millis(line: &crate::lyrics::SyncedLine) -> u32 {
    let ms = line.at.as_millis();
    u32::try_from(ms).unwrap_or(u32::MAX)
}

fn plain_text(lyrics: &Lyrics) -> Option<String> {
    if let Some(plain) = lyrics.plain.as_deref() {
        if !plain.is_empty() {
            return Some(plain.to_string());
        }
    }
    let synced = lyrics.synced.as_deref()?;
    if synced.is_empty() {
        return None;
    }
    let flat = synced
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if flat.is_empty() {
        None
    } else {
        Some(flat)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::lyrics::SyncedLine;

    #[test]
    fn plain_text_prefers_plain() {
        let lyrics = Lyrics {
            plain: Some("full text".into()),
            synced: Some(vec![SyncedLine {
                at: Duration::from_millis(0),
                text: "ignored".into(),
            }]),
        };
        assert_eq!(plain_text(&lyrics).as_deref(), Some("full text"));
    }

    #[test]
    fn plain_text_flattens_synced_when_plain_missing() {
        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![
                SyncedLine {
                    at: Duration::from_millis(0),
                    text: "one".into(),
                },
                SyncedLine {
                    at: Duration::from_millis(1000),
                    text: "two".into(),
                },
            ]),
        };
        assert_eq!(plain_text(&lyrics).as_deref(), Some("one\ntwo"));
    }

    #[test]
    fn plain_text_none_when_empty() {
        assert_eq!(plain_text(&Lyrics::default()), None);
        assert_eq!(
            plain_text(&Lyrics {
                plain: Some(String::new()),
                synced: Some(vec![]),
            }),
            None
        );
    }

    #[test]
    fn embed_missing_file_propagates_metadata_error() {
        let lyrics = Lyrics {
            plain: Some("x".into()),
            synced: None,
        };
        let err = embed(Path::new("/nonexistent/file.mp3"), &lyrics).unwrap_err();
        assert!(matches!(err, crate::Error::Metadata(_)));
    }

    #[test]
    fn embed_empty_lyrics_is_noop() {
        let ok = embed(Path::new("/nonexistent/file.mp3"), &Lyrics::default()).unwrap();
        assert!(!ok);
    }

    #[test]
    fn embed_synced_rejects_unsynced() {
        let lyrics = Lyrics {
            plain: Some("only plain".into()),
            synced: None,
        };
        let err = embed_synced(
            Path::new("/nonexistent/file.mp3"),
            &lyrics,
            DEFAULT_LANGUAGE,
        )
        .unwrap_err();
        assert!(matches!(err, Error::NoSyncedLyrics));
    }

    #[test]
    fn embed_synced_rejects_empty_synced() {
        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![]),
        };
        let err = embed_synced(
            Path::new("/nonexistent/file.mp3"),
            &lyrics,
            DEFAULT_LANGUAGE,
        )
        .unwrap_err();
        assert!(matches!(err, Error::NoSyncedLyrics));
    }

    #[test]
    fn embed_synced_rejects_bad_language_code() {
        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![SyncedLine {
                at: Duration::from_millis(0),
                text: "hi".into(),
            }]),
        };
        let err = embed_synced(Path::new("/nonexistent/file.mp3"), &lyrics, *b"1ng").unwrap_err();
        assert!(matches!(err, Error::InvalidLanguageCode));
    }

    #[test]
    fn millis_clamps_huge_durations() {
        let line = SyncedLine {
            at: Duration::from_secs(10_000_000_000), // > u32::MAX ms
            text: "huge".into(),
        };
        assert_eq!(millis(&line), u32::MAX);
    }

    #[test]
    fn millis_preserves_small_durations() {
        let line = SyncedLine {
            at: Duration::from_millis(12_345),
            text: "small".into(),
        };
        assert_eq!(millis(&line), 12_345);
    }
}
