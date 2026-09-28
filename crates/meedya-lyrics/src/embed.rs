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
//!     // Use `id3_language` if you know the lyrics' language; otherwise
//!     // `DEFAULT_LANGUAGE` says, honestly, that it is not known.
//!     let lang = meedya_lyrics::embed::id3_language("en-GB");
//!     let _ = meedya_lyrics::embed_synced(&path, &lyrics, lang);
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

/// The ID3v2 SYLT frame header's three-letter language field, for a
/// caller with no known language for the lyrics it is embedding.
///
/// **This constant's value changed.** It used to be `*b"eng"` — silently
/// claiming the lyrics were in English whenever the caller had not
/// bothered to say otherwise. Policy MWBM-MEDIA-LANG's LANG-003 forbids
/// that: an unknown language MUST be written as `und`, or the format's own
/// "unknown" marker where one exists, and MUST NOT be filled in with a
/// guess. ID3 defines exactly that marker — `XXX` — so this constant is
/// now `*b"XXX"` (TRACK-070 explicitly allows ID3 to use it). The name is
/// unchanged so existing callers still compile; only its meaning changed,
/// from "assume English" to "say, honestly, that the language is not
/// known."
///
/// A caller that DOES know the language should not reach for this
/// constant at all — see [`id3_language`], which turns any BCP 47 tag or
/// old three-letter code into the right three bytes for this same frame
/// field, falling back to this same `XXX` marker only when the value it
/// was given does not resolve to a real language.
pub const DEFAULT_LANGUAGE: [u8; 3] = *b"XXX";

/// Turns a language value — a BCP 47 tag (`en-GB`), an old three-letter
/// code (`eng`, `fre`), or ID3's own `XXX` "not known" marker — into the
/// three-letter ISO 639-2 **terminology** form the ID3v2 SYLT frame's
/// language field wants (policy MWBM-MEDIA-LANG, TRACK-070's ID3 row).
///
/// `value` is read with the LANG-002 reader
/// ([`meedya_lang::from_legacy_three_letter`]), which also accepts a
/// value that is already a full BCP 47 tag, so passing `"en-GB"`,
/// `"eng"`, or `"XXX"` all work. When the reader cannot make sense of
/// `value` at all (empty, malformed, a language with no ISO 639-2 code),
/// or the recognised language has no ISO 639-2 code of its own, this
/// returns [`DEFAULT_LANGUAGE`] (`XXX`) — never a guess (LANG-003).
///
/// Use this instead of hand-building the three bytes [`embed_synced`]
/// wants: it is the one place in this crate that turns "whatever language
/// value the caller happens to have" into "the exact bytes ID3 needs",
/// so every caller agrees on how an unknown or unusual language is
/// represented.
pub fn id3_language(value: &str) -> [u8; 3] {
    let Some(tag) = meedya_lang::from_legacy_three_letter(value) else {
        return DEFAULT_LANGUAGE;
    };
    let code = meedya_lang::iso639_2_code(&tag, meedya_lang::Iso639Form::Terminology);
    if code == "und" {
        return DEFAULT_LANGUAGE;
    }
    // `iso639_2_code` always returns exactly three ASCII letters for a
    // recognised language (either a genuine ISO 639-2 code or an echoed
    // `qaa`-`qtz` local-use code — see that function's doc comment), so
    // this conversion cannot fail in practice. House style forbids
    // unwrap/expect regardless: fall back to the same "not known" marker
    // rather than assume the invariant holds forever.
    code.into_bytes().try_into().unwrap_or(DEFAULT_LANGUAGE)
}

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
/// `lang` is the ISO-639-2 three-letter language code the SYLT frame
/// header wants. Build it with [`id3_language`] from whatever language
/// value you actually have (a BCP 47 tag or an old three-letter code);
/// pass [`DEFAULT_LANGUAGE`] (`b"XXX"`, ID3's own "language not known"
/// marker) only when the language genuinely is not known — never a
/// guessed language (policy MWBM-MEDIA-LANG, LANG-003). Encoding is
/// UTF-16 with BOM for cross-player compatibility with non-ASCII text.
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

    // #79 — `supports_tag_type` is too permissive here: lofty reports
    // read-only Id3v2 support for FLAC/APE/MPC (see lofty's own
    // `#[tag(supported_formats(... read_only(Ape, Flac, Mpc)))]` on
    // `Id3v2Tag`), which is enough for `supports_tag_type` and `insert_tag`
    // to both succeed structurally on those containers. The old guard let
    // them through, and the operation only died later at `save_to` with an
    // unhelpful lofty write error. `primary_tag_type()` instead reflects
    // what the container can actually be *written* as (derived from the
    // FILE type, not from what insert happens to accept), so this rejects
    // FLAC/APE/MPC up front with a clear, actionable error.
    if tagged.primary_tag_type() != TagType::Id3v2 {
        return Err(Error::UnsupportedForSync {
            tag_type: format!("{:?}", tagged.primary_tag_type()),
        });
    }

    // Step 1 of the two `tag_io` steps that keep a file's languages whole
    // through a save, taken straight after reading and before anything is
    // changed: a file that ALREADY holds one `TLAN` frame per language
    // (split by an older save, or by `meedya-tags-extended`'s
    // `TagFile::save`) was read by lofty as its last frame only, and this
    // save used to delete the rest (found by the stand-in review of
    // revision 6). This reads such frames from the file and puts every
    // language back; when they cannot be read it refuses, and nothing is
    // saved. (It used to run just before the save, inside one combined
    // helper; Codex's review of revisions 5–7 found that order undoes a
    // caller's own language change, so the helper was split in two.)
    tag_io::recover_languages_after_reading(&mut tagged, media)?;

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
            // Unreachable after the primary_tag_type() guard above: for an
            // ID3v2-primary container, insert_tag() always succeeds (see
            // that guard's comment), so tag_mut() always finds it here.
            // House style forbids unwrap/expect regardless — surface the
            // same UnsupportedFormat error tag_io.rs uses for this class of
            // failure rather than assume it can't happen.
            tagged.tag_mut(TagType::Id3v2).ok_or_else(|| {
                Error::Metadata(meedya_metadata::MetadataError::UnsupportedFormat(
                    "cannot create an Id3v2 tag in this container".to_string(),
                ))
            })?
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

    // Step 2, just before the save: a file listing several languages holds
    // them in ONE `TLAN` frame, but the conversion just above hands them
    // back as one item per language, and saving would write each as a
    // frame of its own — a reader keeps only the last, so adding lyrics
    // used to cut three languages down to one (found by the stand-in
    // review of revision 5). This joins them into one item again, as every
    // save in `tag_io` does. It does not read the file.
    tag_io::gather_languages_before_saving(&mut tagged);

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

    // ------------------------------------------------------------------
    // #79 — minimal, valid, UNTAGGED FLAC fixture, generated at test time
    // rather than committed as a binary. Mirrors the identical helper in
    // meedya-metadata's tag_io.rs tests (kept duplicated on purpose: it's
    // test-only, single-crate-local, and pulling in a shared test-utils
    // crate for four lines of bytes isn't worth it). See that module's
    // comment for why this is enough for lofty to accept the file.
    // ------------------------------------------------------------------
    // The trailing PADDING block (rather than ending on STREAMINFO alone) is
    // required, not decorative: it sidesteps a real lofty write-path bug
    // where a FLAC file whose only metadata block is STREAMINFO panics with
    // an out-of-bounds index on write (lofty's own TODO cites
    // lofty-rs/issues/445). See the identical helper in meedya-metadata's
    // tag_io.rs tests for the full explanation.
    fn minimal_untagged_flac() -> Vec<u8> {
        fn flac_block(last: bool, block_type: u8, content: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(4 + content.len());
            out.push((u8::from(last) << 7) | (block_type & 0x7F));
            out.extend_from_slice(&(content.len() as u32).to_be_bytes()[1..]);
            out.extend_from_slice(content);
            out
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"fLaC");
        out.extend_from_slice(&flac_block(false, 0, &[0u8; 34])); // STREAMINFO
        out.extend_from_slice(&flac_block(true, 1, &[0u8; 16])); // PADDING (last)
        out
    }

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
    fn embed_synced_rejects_non_id3v2_container() {
        // #79 regression: the old `supports_tag_type(Id3v2)` guard let FLAC
        // through (lofty reports read-only Id3v2 support for it), and the
        // call only failed later at save with an unhelpful lofty error —
        // never panicking, but never surfacing UnsupportedForSync either.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("untagged.flac");
        std::fs::write(&path, minimal_untagged_flac()).expect("write fixture");

        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![SyncedLine {
                at: Duration::from_millis(0),
                text: "hi".into(),
            }]),
        };

        let err = embed_synced(&path, &lyrics, DEFAULT_LANGUAGE).unwrap_err();
        assert!(
            matches!(err, Error::UnsupportedForSync { .. }),
            "expected UnsupportedForSync, got {err:?}"
        );
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

    /// A minimal untagged MP3: three silent MPEG-1 Layer III frames
    /// (header `FF FB 90 00`, 417 bytes each). The same fixture as
    /// meedya-metadata's tag_io.rs tests, duplicated for the same reason
    /// as the FLAC one above.
    fn minimal_untagged_mp3() -> Vec<u8> {
        const FRAME_LEN: usize = 417;
        let mut out = Vec::with_capacity(3 * FRAME_LEN);
        for _ in 0..3 {
            let mut frame = vec![0u8; FRAME_LEN];
            frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            out.extend_from_slice(&frame);
        }
        out
    }

    #[test]
    fn embed_synced_keeps_every_language_in_one_tlan_frame() {
        // Found by the stand-in review of revision 5: an MP3 listing three
        // languages in one TLAN frame came out of `embed_synced` with three
        // frames, of which a reader keeps only the last. The file is saved
        // here with `tag_io::gather_languages_before_saving`, like every
        // save in meedya-metadata.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("song.mp3");
        std::fs::write(&path, minimal_untagged_mp3()).expect("write fixture");
        tag_io::write_tags(
            &path,
            &[(CommonTag::Language, "pt-BR\0ger\0zh-Hant".into())],
        )
        .expect("three languages");

        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![SyncedLine {
                at: Duration::from_millis(500),
                text: "hello".into(),
            }]),
        };
        embed_synced(&path, &lyrics, id3_language("pt-BR")).expect("embed_synced");

        let read_back = tag_io::read_tags(&path).expect("read_tags");
        assert_eq!(
            read_back.get(&CommonTag::Language).map(Vec::as_slice),
            Some(["por", "deu", "zho"].map(String::from).as_slice())
        );
        let bytes = std::fs::read(&path).expect("read file");
        let tlan_frames = bytes.windows(4).filter(|w| *w == b"TLAN").count();
        assert_eq!(tlan_frames, 1, "exactly one TLAN frame");
        assert_eq!(bytes.windows(4).filter(|w| *w == b"SYLT").count(), 1);
    }

    #[test]
    fn embed_synced_keeps_languages_already_split_into_several_tlan_frames() {
        // Found by the stand-in review of revision 6: a file ALREADY
        // holding one TLAN frame per language (as `meedya-tags-extended`'s
        // `TagFile::save` leaves it — reproduced here with lofty directly,
        // which splits the same way) came out of `embed_synced` with only
        // the last language, because lofty reads only the last frame.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("split.mp3");
        std::fs::write(&path, minimal_untagged_mp3()).expect("write fixture");
        tag_io::write_tags(
            &path,
            &[(CommonTag::Language, "pt-BR\0ger\0zh-Hant".into())],
        )
        .expect("three languages");
        let split = lofty::read_from_path(&path).expect("read");
        split
            .save_to_path(&path, WriteOptions::default())
            .expect("split save");
        let tlan_frames = |path: &Path| {
            let bytes = std::fs::read(path).expect("read file");
            bytes.windows(4).filter(|w| *w == b"TLAN").count()
        };
        assert_eq!(tlan_frames(&path), 3, "split into three frames first");

        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![SyncedLine {
                at: Duration::from_millis(500),
                text: "hello".into(),
            }]),
        };
        embed_synced(&path, &lyrics, id3_language("pt-BR")).expect("embed_synced");

        let read_back = tag_io::read_tags(&path).expect("read_tags");
        assert_eq!(
            read_back.get(&CommonTag::Language).map(Vec::as_slice),
            Some(["por", "deu", "zho"].map(String::from).as_slice())
        );
        assert_eq!(tlan_frames(&path), 1, "merged into one TLAN frame");
    }

    #[test]
    fn embed_synced_accepts_default_language_xxx() {
        // DEFAULT_LANGUAGE is now `XXX` (ID3's "language not known"
        // marker, LANG-003 / TRACK-070) rather than the old `eng` guess.
        // It must pass the same three-ASCII-letters validation any other
        // language code does. Using a nonexistent file so the failure
        // comes from the later file-read step, not language validation —
        // proving `XXX` cleared that check.
        let lyrics = Lyrics {
            plain: None,
            synced: Some(vec![SyncedLine {
                at: Duration::from_millis(0),
                text: "hi".into(),
            }]),
        };
        let err = embed_synced(
            Path::new("/nonexistent/file.mp3"),
            &lyrics,
            DEFAULT_LANGUAGE,
        )
        .unwrap_err();
        assert!(
            matches!(err, Error::Metadata(_)),
            "expected the file-read error, got {err:?} — DEFAULT_LANGUAGE (XXX) should have \
             passed language validation"
        );
    }

    // ------------------------------------------------------------
    // id3_language (policy MWBM-MEDIA-LANG, TRACK-070's ID3 row)
    // ------------------------------------------------------------

    #[test]
    fn id3_language_recognised_bcp47_tag_writes_terminology_code() {
        assert_eq!(&id3_language("en-GB"), b"eng");
    }

    #[test]
    fn id3_language_bare_language_writes_terminology_not_bibliographic() {
        // German's bibliographic code is `ger`; TRACK-070 wants the
        // terminology form (`deu`) for ID3.
        assert_eq!(&id3_language("de"), b"deu");
    }

    #[test]
    fn id3_language_old_bibliographic_three_letter_code_reads_and_rewrites() {
        // `fre` is French's bibliographic ISO 639-2 code (LANG-002 reads
        // it as `fr`); TRACK-070 wants the terminology form back out.
        assert_eq!(&id3_language("fre"), b"fra");
    }

    #[test]
    fn id3_language_full_tag_with_region_writes_primary_languages_code() {
        assert_eq!(&id3_language("pt-BR"), b"por");
    }

    #[test]
    fn id3_language_tag_with_script_writes_primary_languages_code() {
        assert_eq!(&id3_language("zh-Hant"), b"zho");
    }

    #[test]
    fn id3_language_local_use_code_is_written_as_itself() {
        // `qaa`-`qtz` are reserved for local use; nobody but the two
        // parties using one knows what a registered replacement would
        // even mean, so TRACK-070 says write it as itself.
        assert_eq!(&id3_language("qaa"), b"qaa");
    }

    #[test]
    fn id3_language_und_is_xxx() {
        assert_eq!(id3_language("und"), DEFAULT_LANGUAGE);
    }

    #[test]
    fn id3_language_empty_is_xxx() {
        assert_eq!(id3_language(""), DEFAULT_LANGUAGE);
    }

    #[test]
    fn id3_language_unrecognised_three_letters_is_xxx() {
        // `zzz` is not a legacy ISO 639-2 code, not a registered ISO
        // 639-3 subtag, and not in the qaa-qtz local-use range — LANG-002
        // step 5 calls this unrecognised, and LANG-003 forbids guessing.
        assert_eq!(id3_language("zzz"), DEFAULT_LANGUAGE);
    }

    #[test]
    fn id3_language_private_use_only_tag_is_xxx() {
        // A tag that is nothing but private-use subtags has no primary
        // language to look an ISO 639-2 code up under.
        assert_eq!(id3_language("x-private"), DEFAULT_LANGUAGE);
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
