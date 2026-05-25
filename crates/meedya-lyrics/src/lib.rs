//! Lyrics fetching, LRC I/O, write targets, and the Lyricsfile YAML
//! format for the MeedyaSuite apps.
//!
//! Modeled on the LRCLIB client approach used by [lrcget], packaged as a
//! reusable library so MeedyaDL, MeedyaConverter, and MeedyaManager can share
//! the same lookup, parse, and write code paths.
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
//! - [`Lyricsfile::from_ttml`] — Apple Music TTML (line-level or
//!   `itunes:timing="Word"`) → Lyricsfile, preserving word-level timing
//!   within 1ms.
//! - [`Lyricsfile::from_lrc`] — Standard LRC or Enhanced LRC → Lyricsfile.
//! - [`Lyricsfile::parse`] / [`Lyricsfile::to_yaml`] — bidirectional YAML I/O.
//! - [`Lyricsfile::to_lrc`], [`Lyricsfile::to_enhanced_lrc`],
//!   [`Lyricsfile::to_srt`], [`Lyricsfile::to_webvtt`],
//!   [`Lyricsfile::to_ass`] — five player-compatible export targets.
//!
//! The format is experimental (LRCGET's own release notes warn of breaking
//! changes); the implementation here ships at spec version
//! [`LYRICSFILE_VERSION`] and tolerates unknown fields on parse for
//! forward-compatibility.
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
pub mod provider;
pub mod sidecar;

pub use embed::{embed, embed_synced, DEFAULT_LANGUAGE};
pub use error::{Error, Result};
pub use lyrics::{Lyrics, SyncedLine};
pub use lyricsfile::{
    Lyricsfile, LyricsfileLine, LyricsfileMetadata, LyricsfileWord, INSTRUMENTAL_MARKER,
    LYRICSFILE_VERSION,
};
pub use provider::lrclib::LrclibProvider;
pub use provider::{LyricsProvider, TrackQuery};
