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

/// Extracts a leading four-digit year from a provider date string.
///
/// Provider APIs return dates in inconsistent shapes — full ISO dates
/// (`"1979-11-30"`), bare years (`"1979"`), year ranges (`"1979–1983"`),
/// and occasionally malformed junk. The naive approach — a fixed
/// 4-byte-wide slice clamped only to the string's byte length — panics
/// whenever byte offset 4 falls inside a multi-byte UTF-8 character
/// (e.g. `"20€25"`, where `'€'` is a 3-byte character spanning bytes
/// 2..5) rather than on a char boundary. Provider JSON is untrusted
/// input, so this must never panic on it.
///
/// This walks `char`s instead of bytes, collecting up to 4 leading ASCII
/// digits, and returns `Some(year)` only when exactly 4 were collected —
/// anything else (fewer digits, or a non-digit encountered first) is
/// treated as "no year present". Because it never indexes into the byte
/// string, it cannot straddle a char boundary and cannot panic.
///
/// Behaviour:
///
/// - `"1979-11-30"` -> `Some(1979)` (leading digits before the separator)
/// - `"1979"`        -> `Some(1979)` (bare year)
/// - `"1979–1983"`   -> `Some(1979)` (range: first year wins)
/// - `"20245"`       -> `Some(2024)` (extra trailing digit is ignored)
/// - `"79"`          -> `None` (fewer than 4 digits)
/// - `""`            -> `None` (no digits)
/// - `"Nov 1979"`    -> `None` (non-digit before any digit is collected)
///
/// Note that `"79"` -> `None` is a deliberate tightening versus the old
/// byte-slice code, which would have produced the nonsense year `79` —
/// two-digit years were never a supported provider format.
#[cfg(any(
    feature = "provider-musicbrainz",
    feature = "provider-isrc",
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-itunes-store",
    feature = "provider-apple-tv",
    feature = "provider-apple-podcasts",
    feature = "provider-eidr",
    feature = "provider-thetvdb",
    feature = "provider-tmdb",
    feature = "provider-omdb",
))]
pub(crate) fn leading_year(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).take(4).collect();
    if digits.len() == 4 {
        digits.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
#[cfg(any(
    feature = "provider-musicbrainz",
    feature = "provider-isrc",
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-itunes-store",
    feature = "provider-apple-tv",
    feature = "provider-apple-podcasts",
    feature = "provider-eidr",
    feature = "provider-thetvdb",
    feature = "provider-tmdb",
    feature = "provider-omdb",
))]
mod tests {
    use super::leading_year;

    #[test]
    fn leading_year_full_date_returns_year() {
        assert_eq!(leading_year("1979-11-30"), Some(1979));
    }

    #[test]
    fn leading_year_bare_year_returns_year() {
        assert_eq!(leading_year("1979"), Some(1979));
    }

    #[test]
    fn leading_year_range_returns_first_year() {
        assert_eq!(leading_year("1979–1983"), Some(1979));
    }

    #[test]
    fn leading_year_extra_trailing_digit_is_ignored() {
        assert_eq!(leading_year("20245"), Some(2024));
    }

    #[test]
    fn leading_year_two_digits_returns_none() {
        // Deliberate tightening: the old byte-slice code would have
        // produced the nonsense year 79 here. Two-digit years were never
        // a supported provider date format.
        assert_eq!(leading_year("79"), None);
    }

    #[test]
    fn leading_year_empty_returns_none() {
        assert_eq!(leading_year(""), None);
    }

    #[test]
    fn leading_year_non_digit_prefix_returns_none() {
        assert_eq!(leading_year("Nov 1979"), None);
    }

    #[test]
    fn leading_year_multibyte_date_does_not_panic() {
        // Regression: byte-slicing the old date string at a fixed 4-byte
        // offset panicked with "byte index 4 is not a char boundary"
        // because '€' is a 3-byte character spanning bytes 2..5 of
        // "20€25". Character-based iteration never slices by byte
        // offset, so this returns None instead of panicking. Same bug
        // class as the fix in
        // `providers::isrc::validate_isrc` (see
        // `validate_isrc_non_ascii_alphanumeric_does_not_panic`).
        assert_eq!(leading_year("20€25"), None);
    }

    #[test]
    fn leading_year_fullwidth_digits_does_not_panic() {
        // Regression: fullwidth digits (e.g. U+FF11 '1') are multi-byte
        // and are not `is_ascii_digit`, so the digit run ends immediately
        // rather than a byte offset landing mid-character.
        assert_eq!(leading_year("１９７９"), None);
    }
}
