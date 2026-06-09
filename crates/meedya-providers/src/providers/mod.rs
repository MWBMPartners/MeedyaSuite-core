// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Concrete provider implementations, each gated by its own feature flag.
//
// Each provider lives in its own file (`<name>.rs`) and is conditionally
// compiled via `#[cfg(feature = "provider-<name>")]`. Apps opt in only to
// the providers they need; downstream binary size and dependency surface
// scales with the chosen feature set.
//
// Ported from MeedyaManager `crates/mm-providers/src/{music,video,
// identifiers,podcasts}/mod.rs` under MeedyaSuite-core#12 / MeedyaManager#136.

#[cfg(feature = "provider-musicbrainz")]
pub mod musicbrainz;
#[cfg(feature = "provider-musicbrainz")]
pub use musicbrainz::MusicBrainzProvider;

#[cfg(feature = "provider-spotify")]
pub mod spotify;
#[cfg(feature = "provider-spotify")]
pub use spotify::SpotifyProvider;

#[cfg(feature = "provider-apple-music")]
pub mod apple_music;
#[cfg(feature = "provider-apple-music")]
pub use apple_music::AppleMusicProvider;

#[cfg(feature = "provider-deezer")]
pub mod deezer;
#[cfg(feature = "provider-deezer")]
pub use deezer::DeezerProvider;

#[cfg(feature = "provider-tmdb")]
pub mod tmdb;
#[cfg(feature = "provider-tmdb")]
pub use tmdb::TmdbProvider;

#[cfg(feature = "provider-thetvdb")]
pub mod thetvdb;
#[cfg(feature = "provider-thetvdb")]
pub use thetvdb::TheTvdbProvider;

#[cfg(feature = "provider-omdb")]
pub mod omdb;
#[cfg(feature = "provider-omdb")]
pub use omdb::OmdbProvider;

#[cfg(feature = "provider-apple-tv")]
pub mod apple_tv;
#[cfg(feature = "provider-apple-tv")]
pub use apple_tv::AppleTvProvider;

#[cfg(feature = "provider-itunes-store")]
pub mod itunes_store;
#[cfg(feature = "provider-itunes-store")]
pub use itunes_store::ItunesStoreProvider;

#[cfg(feature = "provider-apple-podcasts")]
pub mod apple_podcasts;
#[cfg(feature = "provider-apple-podcasts")]
pub use apple_podcasts::ApplePodcastsProvider;

#[cfg(feature = "provider-isrc")]
pub mod isrc;
#[cfg(feature = "provider-isrc")]
pub use isrc::IsrcProvider;

#[cfg(feature = "provider-eidr")]
pub mod eidr;
#[cfg(feature = "provider-eidr")]
pub use eidr::EidrProvider;

#[cfg(feature = "provider-iswc")]
pub mod iswc;
#[cfg(feature = "provider-iswc")]
pub use iswc::IswcProvider;
