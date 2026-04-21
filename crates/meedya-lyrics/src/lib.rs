//! Lyrics fetching, LRC I/O, and write targets for the MeedyaSuite apps.
//!
//! Modeled on the LRCLIB client approach used by [lrcget], packaged as a
//! reusable library so MeedyaDL, MeedyaConverter, and MeedyaManager can share
//! the same lookup, parse, and write code paths.
//!
//! Tag-embed writes (USLT / Vorbis `LYRICS` / MP4 `©lyr`) will live in a
//! future `embed` module once `meedya-metadata` lands; for now only sidecar
//! writes are supported.
//!
//! [lrcget]: https://github.com/tranxuanthang/lrcget

pub mod error;
pub mod lrc;
pub mod lyrics;
pub mod provider;
pub mod sidecar;

pub use error::{Error, Result};
pub use lyrics::{Lyrics, SyncedLine};
pub use provider::lrclib::LrclibProvider;
pub use provider::{LyricsProvider, TrackQuery};
