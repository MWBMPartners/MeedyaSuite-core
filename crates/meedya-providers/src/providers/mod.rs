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

/// Feature gate shared by [`build_client`] and its tests: every provider
/// module that builds its `reqwest::Client` through the shared constructor
/// rather than its own bespoke `Client::builder()` chain. musicbrainz,
/// isrc and iswc are deliberately absent — they already carry their own
/// timeout (see MeedyaSuite-core#76) and their own MusicBrainz-specific
/// User-Agent handling, so folding them into a shared helper would be
/// pure churn with no behaviour change.
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
use std::time::Duration;

/// Total time budget for one request — DNS + connect + TLS + send + wait +
/// receive, end to end. Without an explicit timeout reqwest waits
/// indefinitely on a stalled or black-holed connection, hanging the
/// caller's async task forever with no error and no recovery path (see
/// MeedyaSuite-core#76). Matches the existing convention in
/// `musicbrainz.rs` (and `isrc.rs` / `iswc.rs`).
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Time budget for establishing the TCP/TLS connection specifically, kept
/// shorter than [`REQUEST_TIMEOUT`] so a dead or unreachable host fails
/// fast instead of burning the entire request budget just to discover
/// nothing is listening.
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Crate-wide fallback User-Agent, used whenever a caller has no
/// meaningful one of its own to supply. Mirrors the fallback already used
/// by `MusicBrainzProvider::with_base_url` (and the ISRC/ISWC providers).
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
const DEFAULT_USER_AGENT: &str = "meedya-providers/0.1";

/// Shared `reqwest::Client` constructor for the providers that have no
/// bespoke client configuration of their own (see MeedyaSuite-core#76).
/// Centralising construction here — rather than each provider calling
/// `Client::new()` — means the *next* provider added to this workspace
/// inherits a request timeout by construction, not by a reviewer
/// remembering to add one.
///
/// `user_agent` follows the same empty-string fallback established by
/// `MusicBrainzProvider::with_base_url`: pass `""` for a provider that has
/// no user-agent concept of its own (all ten current callers do) and this
/// substitutes [`DEFAULT_USER_AGENT`].
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
pub(crate) fn build_client(user_agent: &str) -> reqwest::Client {
    build_client_with_timeouts(user_agent, REQUEST_TIMEOUT, CONNECT_TIMEOUT)
}

/// [`build_client`]'s timeout-parameterised sibling. Exists so tests can
/// exercise the *behaviour* of the timeout (reqwest doesn't expose a
/// configured timeout for inspection) without waiting out the real 30s
/// production budget — construct with millisecond-scale durations against
/// a deliberate black hole instead.
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
fn build_client_with_timeouts(
    user_agent: &str,
    request_timeout: Duration,
    connect_timeout: Duration,
) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(if user_agent.is_empty() {
            DEFAULT_USER_AGENT
        } else {
            user_agent
        })
        .timeout(request_timeout)
        .connect_timeout(connect_timeout)
        .build()
        .expect("reqwest ClientBuilder failed — TLS initialisation error")
}

#[cfg(test)]
#[cfg(any(
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-eidr",
    feature = "provider-apple-podcasts",
))]
mod build_client_tests {
    use super::{build_client, build_client_with_timeouts, Duration};
    use tokio::net::TcpListener;

    #[test]
    fn build_client_falls_back_to_default_user_agent() {
        // Smoke test that the empty-string fallback doesn't panic and
        // produces a usable client; the interesting behaviour (the
        // timeout actually firing) is covered below since reqwest gives
        // no way to inspect a configured User-Agent either.
        let _ = build_client("");
        let _ = build_client("some-provider/1.0");
    }

    /// Points a client at a listener that accepts the TCP connection and
    /// then goes silent forever — a deterministic, hermetic black hole
    /// with no real network involved — and asserts the call fails with a
    /// timeout well within the test's patience rather than hanging.
    /// Uses `build_client_with_timeouts` with millisecond-scale durations
    /// so the test stays fast; production code always goes through
    /// `build_client`, which applies the real 30s/10s budget.
    #[tokio::test]
    async fn build_client_request_times_out_on_a_stalled_connection() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("binding an ephemeral port must succeed");
        let addr = listener.local_addr().expect("bound listener has an addr");

        // Accept the connection but never write a response — the request
        // times out waiting on a reply, not on the connect step.
        tokio::spawn(async move {
            if let Ok((_socket, _peer)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });

        let client = build_client_with_timeouts(
            "build-client-test/1.0",
            Duration::from_millis(200),
            Duration::from_millis(200),
        );

        let result = client.get(format!("http://{addr}/")).send().await;

        let err = result.expect_err("a stalled connection must time out, not hang");
        assert!(
            err.is_timeout(),
            "expected a timeout error specifically, got: {err}"
        );
    }
}

/// Feature gate shared by [`net_err`] and its test: every provider module
/// that captures a `reqwest::Error` into a `ProviderError::NetworkError`.
/// (`provider-apple-podcasts` is deliberately absent — that provider maps
/// its network errors inline rather than through this helper.)
#[cfg(any(
    feature = "provider-musicbrainz",
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-isrc",
    feature = "provider-eidr",
    feature = "provider-iswc",
    feature = "provider-apple-podcasts",
))]
use crate::traits::ProviderError;

/// Builds a `ProviderError::NetworkError` from a `reqwest::Error`, first
/// stripping the query string from any URL the error carries.
///
/// `reqwest::Error`'s `Display` impl appends `" for url ({url})"` —
/// including the full query string — so an error from a request that
/// carried an API key as a query parameter (e.g. TMDb's `api_key=...`,
/// OMDb's `apikey=...`) would otherwise leak that key into logs and
/// returned error text. `Error::url_mut` exists precisely for this: its
/// own docs name "remove sensitive information from the URL (e.g. an API
/// key in the query), but do not want to remove the URL entirely" as the
/// intended use. Clearing only the query (not the whole URL via
/// `without_url()`) keeps scheme/host/path for diagnostics — no provider
/// in this workspace puts a secret in the path.
///
/// Shared by every provider that authenticates via a query parameter
/// *and* every provider that doesn't (musicbrainz, apple-music, etc. use
/// header auth or no auth at all) as deliberate defence-in-depth: a
/// future provider that adds query-string auth is safe by default
/// instead of depending on its author remembering to redact it here.
/// See MeedyaSuite-core#80.
#[cfg(any(
    feature = "provider-musicbrainz",
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-isrc",
    feature = "provider-eidr",
    feature = "provider-iswc",
    feature = "provider-apple-podcasts",
))]
pub(crate) fn net_err(mut e: reqwest::Error) -> ProviderError {
    if let Some(url) = e.url_mut() {
        url.set_query(None);
    }
    ProviderError::NetworkError(e.to_string())
}

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

// Canary for MeedyaSuite-core#80: reqwest::Error's Display appends
// " for url ({url})" including the query string, so an unredacted error
// from a query-authenticated request (TMDb, OMDb) would leak the API key
// into logs and returned error text. This forces a real reqwest error
// against a URL carrying a fake API key in the query string and asserts
// `net_err`'s output contains neither the secret nor the query string at
// all, while still naming the host for diagnostics.
#[cfg(test)]
#[cfg(any(
    feature = "provider-musicbrainz",
    feature = "provider-spotify",
    feature = "provider-apple-music",
    feature = "provider-deezer",
    feature = "provider-tmdb",
    feature = "provider-thetvdb",
    feature = "provider-omdb",
    feature = "provider-apple-tv",
    feature = "provider-itunes-store",
    feature = "provider-isrc",
    feature = "provider-eidr",
    feature = "provider-iswc",
    feature = "provider-apple-podcasts",
))]
mod net_err_tests {
    use super::net_err;

    #[tokio::test]
    async fn net_err_strips_api_key_from_query_string() {
        // 127.0.0.1 on a closed port refuses the connection immediately
        // (no listener, no DNS, no real network) so `.send()` fails fast
        // and deterministically — hermetic and quick.
        let client = reqwest::Client::new();
        let result = client
            .get("http://127.0.0.1:1/lookup?api_key=SUPERSECRET")
            .send()
            .await;

        let err = result.expect_err("connection to a closed port must fail");
        let message = net_err(err).to_string();

        assert!(
            !message.contains("SUPERSECRET"),
            "net_err leaked the API key into the error text: {message}"
        );
        assert!(
            !message.contains("api_key"),
            "net_err leaked the query string into the error text: {message}"
        );
        assert!(
            message.contains("127.0.0.1"),
            "net_err should still name the host for diagnostics: {message}"
        );
    }
}
