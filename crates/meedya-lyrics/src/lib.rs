//! Lyrics fetching, LRC I/O, write targets, syllable-aware TTML parsing,
//! and the Lyricsfile YAML format for the MeedyaSuite apps.
//!
//! Modeled on the LRCLIB client approach used by [lrcget], packaged as a
//! reusable library so MeedyaDL, MeedyaConverter, MeedyaManager, and
//! iLyricsDB can share the same lookup, parse, and write code paths.
//!
//! Three write targets are supported:
//! - [`sidecar::write`] — `.lrc` next to the media file (synced lyrics only).
//! - [`embed::embed`] — plain-text tag via `meedya-metadata` (USLT for ID3v2,
//!   `LYRICS` for Vorbis, `©lyr` for MP4).
//! - [`embed::embed_synced`] — ID3v2 SYLT (synchronised lyrics frame). ID3v2
//!   containers only; other formats return an error and callers should fall
//!   back to [`embed::embed`].
//!
//! ## Lyricsfile (`.lyrics`) format
//!
//! The [`Lyricsfile`] struct implements the YAML lyrics format introduced
//! by [LRCGET v2.0.0](https://github.com/tranxuanthang/lrcget/releases/tag/2.0.0)
//! and co-endorsed by LRCLIB. Conversion paths:
//!
//! - [`Lyricsfile::from_ttml`] — Apple Music TTML (line / word / syllable
//!   level) → Lyricsfile, preserving word-level timing within 1ms and
//!   grouping adjacent timed `<span>` siblings into syllables when no
//!   whitespace text node separates them.
//! - [`Lyricsfile::from_lrc`] — Standard LRC or Enhanced LRC → Lyricsfile.
//!   Reads the `[offset:NNN]` calibration tag when present.
//! - [`Lyricsfile::parse`] / [`Lyricsfile::to_yaml`] — bidirectional YAML I/O.
//! - [`Lyricsfile::to_lrc`], [`Lyricsfile::to_enhanced_lrc`],
//!   [`Lyricsfile::to_srt`], [`Lyricsfile::to_webvtt`],
//!   [`Lyricsfile::to_ass`] — five player-compatible export targets.
//!   The LRC family emit an `[offset:NNN]` header when
//!   [`LyricsfileMetadata::offset_ms`] is set, so calibration values
//!   from Apple's `lyricOffset` flow through the conversion chain.
//!
//! The format is experimental (LRCGET's own release notes warn of breaking
//! changes); the implementation here ships at spec version
//! [`LYRICSFILE_VERSION`] and tolerates unknown fields on parse for
//! forward-compatibility.
//!
//! ## TTML granularity classifier
//!
//! [`classify_ttml_granularity`] returns one of [`TtmlGranularity`]
//! `{Syllable, Word, Line, Unknown}` without parsing the full document.
//! Used by downstream consumers (notably MeedyaDL's Step 1b enrichment)
//! to decide whether a freshly downloaded TTML is rich enough to keep,
//! or whether a richer source should be fetched. The detection is
//! namespace-aware (resolves both Apple iTunes TTML namespaces) and
//! falls through to a structural-pair check when the
//! `itunes:timing` attribute is missing or ambiguous.
//!
//! ## Apple Music TTML specification reference
//!
//! The format Apple ships from `/syllable-lyrics` is undocumented by
//! Apple. The [`APPLE_MUSIC_TTML_SPEC.md`](../docs/APPLE_MUSIC_TTML_SPEC.md)
//! document next to this crate is a thorough reverse-engineered
//! specification — endpoint contract, namespaces, granularity tiers,
//! full element/attribute schema, background-vocal handling, cross-format
//! conversion table, and open questions. Update that document FIRST
//! when you discover a quirk that contradicts current behaviour, then
//! amend the implementation here. The spec is the source of truth.
//!
//! [lrcget]: https://github.com/tranxuanthang/lrcget

pub mod embed;
pub mod error;
pub mod lrc;
pub mod lyrics;
pub mod lyricsfile;
pub mod lyricsfile_export;
pub mod lyricsfile_lrc;
pub mod lyricsfile_ttml;
pub mod lyricsfile_ttml_classify;
pub mod provider;
pub mod sidecar;

pub use embed::{embed, embed_synced, DEFAULT_LANGUAGE};
pub use error::{Error, Result};
pub use lyrics::{Lyrics, SyncedLine};
pub use lyricsfile::{
    Lyricsfile, LyricsfileLine, LyricsfileMetadata, LyricsfileSyllable, LyricsfileWord,
    INSTRUMENTAL_MARKER, LYRICSFILE_VERSION,
};
pub use lyricsfile_ttml_classify::{classify_ttml_granularity, TtmlGranularity};
pub use provider::lrclib::LrclibProvider;
pub use provider::{LyricsProvider, TrackQuery};
