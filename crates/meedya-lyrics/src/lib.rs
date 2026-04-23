//! Lyrics fetching, LRC I/O, and write targets for the MeedyaSuite apps.
//!
//! Modeled on the LRCLIB client approach used by [lrcget], packaged as a
//! reusable library so MeedyaDL, MeedyaConverter, and MeedyaManager can share
//! the same lookup, parse, and write code paths.
//!
//! Two write targets are supported:
//! - [`sidecar::write`] — `.lrc` next to the media file (synced lyrics only).
//! - [`embed::embed`] — plain-text tag via `meedya-metadata` (USLT for ID3v2,
//!   `LYRICS` for Vorbis, `©lyr` for MP4). Synchronized ID3v2 SYLT is not
//!   yet supported.
//!
//! [lrcget]: https://github.com/tranxuanthang/lrcget

pub mod embed;
pub mod error;
pub mod lrc;
pub mod lyrics;
pub mod provider;
pub mod sidecar;

pub use error::{Error, Result};
pub use lyrics::{Lyrics, SyncedLine};
pub use provider::lrclib::LrclibProvider;
pub use provider::{LyricsProvider, TrackQuery};
