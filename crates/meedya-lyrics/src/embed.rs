//! Embed lyrics into a media file's tag container.
//!
//! Writes plain (unsynced) lyrics via `meedya-metadata`'s `CommonTag::Lyrics`,
//! which maps to the correct atom per format: USLT for ID3v2, `LYRICS` for
//! Vorbis Comment, `©lyr` for MP4 ilst, etc. Container detection is delegated
//! to lofty.
//!
//! Synchronized embed (ID3v2 SYLT) is not yet supported — when `lyrics.synced`
//! is present but `lyrics.plain` is not, the synced lines are flattened to
//! plain text (one line per stamp, timestamps dropped).

use std::path::Path;

use meedya_metadata::{tag_io, CommonTag};

use crate::{Lyrics, Result};

/// Embed the plain-text representation of `lyrics` into `media`'s tags.
///
/// Returns `true` if anything was written, `false` if `lyrics` has no
/// embeddable content.
pub fn embed(media: &Path, lyrics: &Lyrics) -> Result<bool> {
    let Some(text) = plain_text(lyrics) else {
        return Ok(false);
    };
    tag_io::write_tags(media, &[(CommonTag::Lyrics, text)])?;
    Ok(true)
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
}
