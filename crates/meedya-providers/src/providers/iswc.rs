// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// ISWC identifier provider (MusicBrainz works backend).
// Ported from MeedyaManager crates/mm-providers/src/identifiers/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{ISWC, PROVIDER_ID};
use crate::lucene::phrase_clause;
use crate::rate_limiter::{default_limiter_for, ProviderRateLimiter};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{ProviderResult, SearchQuery};

// net_err lives in providers::mod (see MeedyaSuite-core#80): centralised
// so every provider redacts a reqwest error's query string uniformly.
use super::net_err;

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

/// Normalise an ISWC to its compact, separator-free, uppercase form
/// (`T` + 10 digits), e.g. `t-034.524.680-1` -> `T0345246801`.
///
/// This is the canonical internal representation; it is **not** the form
/// MusicBrainz indexes — see [`format_iswc_dotted`].
pub fn normalise_iswc(iswc: &str) -> String {
    iswc.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Render an ISWC in MusicBrainz's stored display form,
/// `T-DDD.DDD.DDD-C`, e.g. `T0345246801` -> `T-034.524.680-1`.
///
/// Returns `None` when `iswc` does not normalise to a well-formed ISWC
/// (`T` followed by exactly 10 digits), so callers can fall back rather
/// than emitting a malformed query.
fn format_iswc_dotted(iswc: &str) -> Option<String> {
    let n = normalise_iswc(iswc);
    let digits = n.strip_prefix('T')?;
    if digits.len() != 10 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!(
        "T-{}.{}.{}-{}",
        &digits[0..3],
        &digits[3..6],
        &digits[6..9],
        &digits[9..10]
    ))
}

/// Build the Lucene query string for an ISWC works lookup.
///
/// # Why the dotted display form
///
/// ISRC and ISWC are deliberately handled differently, because MusicBrainz
/// indexes them differently. This is **live-verified** against
/// `musicbrainz.org/ws/2/`, not inferred from documentation (MusicBrainz
/// does not document the indexed form for either field):
///
/// | Field | Query form            | Live result |
/// |-------|-----------------------|-------------|
/// | ISRC  | `isrc:GBAYE0601498`   | matches     |
/// | ISRC  | `isrc:GB-AYE-06-01498`| 0 results   |
/// | ISWC  | `iswc:"T-304.031.869-8"` | matches  |
/// | ISWC  | `iswc:T3040318698`    | 0 results   |
/// | ISWC  | `iswc:"T-304031869-8"`| parse error |
///
/// So ISRCs are queried compact and ISWCs are queried in the punctuated
/// display form MusicBrainz stores. Emitting the compact form here — or
/// passing the caller's separators through unchanged — silently returns
/// zero results.
///
/// The value is phrase-quoted so its `-` and `.` cannot be parsed as Lucene
/// operators. If the input is too malformed to reformat, it falls back to
/// phrase-quoting the uppercased input rather than emitting nothing.
///
/// Re-verify after the 2026-11-30 Solr 10 reindex (issue #69): no ticket
/// announces an analyzer change for identifier fields, but the stored-form
/// query is the safest bet either way and this is the one behaviour that
/// cannot be confirmed until the new stack is live.
fn build_iswc_query(iswc: &str) -> String {
    let value = format_iswc_dotted(iswc).unwrap_or_else(|| iswc.to_uppercase());
    phrase_clause("iswc", &value)
}

/// Validate ISWC format: `T-123456789-C` (T + 9 digits + check digit).
/// Accepts the format with or without hyphens.
pub fn validate_iswc(iswc: &str) -> bool {
    let normalised = normalise_iswc(iswc);
    // Must be exactly 11 chars: T + 9 digits + 1 check digit
    normalised.len() == 11
        && normalised.starts_with('T')
        && normalised[1..].chars().all(|c| c.is_ascii_digit())
}

/// Looks up ISWC identifiers via MusicBrainz works API.
///
/// Endpoint: `https://musicbrainz.org/ws/2/work/?query=iswc:<ISWC>`
/// Auth:     None (but User-Agent required)
/// Limits:   ~1 req/sec — MusicBrainz's documented anonymous average, held
///           as one `musicbrainz.org` budget shared with the sibling
///           MusicBrainz-backed providers, not a per-provider allowance
pub struct IswcProvider {
    client: Client,
    base_url: String,
    user_agent: String,
    limiter: Arc<ProviderRateLimiter>,
}

impl IswcProvider {
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self::with_base_url(user_agent, "https://musicbrainz.org")
    }

    pub fn with_base_url(user_agent: impl Into<String>, base_url: impl Into<String>) -> Self {
        let user_agent = user_agent.into();
        let client = Client::builder()
            .user_agent(if user_agent.is_empty() {
                "meedya-providers/0.1".to_string()
            } else {
                user_agent.clone()
            })
            // Without an explicit timeout reqwest waits indefinitely; a
            // stalled MusicBrainz connection would hang the caller's task.
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest ClientBuilder failed — TLS initialisation error");
        Self {
            client,
            base_url: base_url.into(),
            user_agent,
            limiter: default_limiter_for("iswc"),
        }
    }

    /// Replace the shared default rate limiter (see [`default_limiter_for`])
    /// with a caller-supplied one — an app-wide budget held in a
    /// [`crate::rate_limiter::RateLimiterRegistry`], a paid tier or a
    /// self-hosted mirror with no such limit, or a permissive limiter in tests.
    pub fn with_rate_limiter(mut self, limiter: Arc<ProviderRateLimiter>) -> Self {
        self.limiter = limiter;
        self
    }

    fn configured(&self) -> bool {
        !self.user_agent.is_empty()
    }

    fn parse_works(provider_name: &str, body: &str) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        struct MbWorksResponse {
            works: Vec<MbWork>,
        }
        #[derive(Deserialize)]
        struct MbWork {
            id: Option<String>,
            title: Option<String>,
            iswcs: Option<Vec<String>>,
            relations: Option<Vec<MbRelation>>,
        }
        #[derive(Deserialize)]
        struct MbRelation {
            #[serde(rename = "type")]
            rel_type: Option<String>,
            artist: Option<MbRelArtist>,
        }
        #[derive(Deserialize)]
        struct MbRelArtist {
            name: Option<String>,
        }

        let resp: MbWorksResponse =
            serde_json::from_str(body).map_err(|e| parse_err("ISWC/MusicBrainz response", e))?;

        let results = resp
            .works
            .into_iter()
            .map(|work| {
                // Find the composer from relations
                let composer = work.relations.as_deref().and_then(|rels| {
                    rels.iter()
                        .find(|r| r.rel_type.as_deref() == Some("composer"))
                        .and_then(|r| r.artist.as_ref()?.name.clone())
                });

                let mut result = ProviderResult::new(provider_name);
                result.title = work.title;
                result.artist = composer;

                if let Some(id) = work.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id));
                }
                if let Some(iswc) = work.iswcs.and_then(|v| v.into_iter().next()) {
                    result.metadata.insert(ISWC.into(), Value::String(iswc));
                }
                result
            })
            .collect();
        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for IswcProvider {
    fn id(&self) -> &str {
        "iswc"
    }

    fn display_name(&self) -> &str {
        "ISWC (via MusicBrainz)"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            music_search: true,
            video_search: false,
            podcast_search: false,
            cover_art: false,
            lyrics: false,
            fingerprint_lookup: false,
            identifier_lookup: true,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
        if !self.configured() {
            return Err(ProviderError::NotConfigured("iswc".into()));
        }

        let iswc = query.iswc.as_deref().ok_or_else(|| {
            ProviderError::NotSupported("iswc: ISWC query requires an ISWC code".into())
        })?;

        if !validate_iswc(iswc) {
            return Err(ProviderError::Other(format!(
                "parse error: Invalid ISWC format: {iswc}"
            )));
        }

        debug!(
            provider = "iswc",
            iswc = iswc,
            "Sending ISWC lookup request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let url = format!("{}/ws/2/work/", self.base_url);
        // Throttle: musicbrainz.org allows ~1 req/sec on average, one
        // budget shared with the sibling providers (MeedyaSuite-core#94).
        self.limiter.wait_until_ready().await;
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .query(&[
                ("query", &build_iswc_query(iswc)),
                ("limit", &limit),
                ("fmt", &"json".to_owned()),
            ])
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            let s = response.status();
            if s.as_u16() == 503 {
                return Err(ProviderError::RateLimited("iswc".into()));
            }
            return Err(ProviderError::NetworkError(format!("HTTP {s}")));
        }

        let body = response.text().await.map_err(net_err)?;
        Self::parse_works("iswc", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_iswc_valid_standard() {
        assert!(validate_iswc("T0345246801")); // T + 10 digits
    }

    #[test]
    fn validate_iswc_valid_with_hyphens() {
        assert!(validate_iswc("T-034524680-1"));
    }

    #[test]
    fn validate_iswc_wrong_prefix() {
        assert!(!validate_iswc("X0345246801")); // Must start with T
    }

    #[test]
    fn validate_iswc_too_short() {
        assert!(!validate_iswc("T034524680")); // 10 chars (T + 9 digits) — need 11
    }

    #[test]
    fn iswc_provider_name() {
        assert_eq!(IswcProvider::new("App/1.0").id(), "iswc");
    }

    #[test]
    fn iswc_provider_capabilities() {
        let caps = IswcProvider::new("App/1.0").capabilities();
        assert!(caps.identifier_lookup);
        assert!(caps.music_search);
    }

    #[test]
    fn iswc_provider_parse_works_valid() {
        let json = r#"{
            "works": [{
                "id": "mb-work-1",
                "title": "Bohemian Rhapsody",
                "iswcs": ["T0345246801"],
                "relations": [{
                    "type": "composer",
                    "artist": {"name": "Freddie Mercury"}
                }]
            }]
        }"#;
        let results = IswcProvider::parse_works("iswc", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(results[0].artist.as_deref(), Some("Freddie Mercury"));
        assert_eq!(
            results[0]
                .metadata
                .get(ISWC)
                .and_then(serde_json::Value::as_str),
            Some("T0345246801")
        );
    }

    /// Forward-compat fixture: the NEW relation shape MusicBrainz uses
    /// going forward (`target-type` present, no `target` key). Our
    /// `MbRelation` struct never reads `target`, so this must extract the
    /// composer/title identically to the legacy shape below.
    #[test]
    fn iswc_provider_parse_works_new_relation_shape() {
        let json = r#"{
            "works": [{
                "id": "mb-work-1",
                "title": "Bohemian Rhapsody",
                "iswcs": ["T0345246801"],
                "relations": [{
                    "type": "composer",
                    "target-type": "artist",
                    "artist": {"name": "Freddie Mercury"}
                }]
            }]
        }"#;
        let results = IswcProvider::parse_works("iswc", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(results[0].artist.as_deref(), Some("Freddie Mercury"));
    }

    /// Forward-compat fixture: the LEGACY relation shape (`target` present,
    /// no `target-type`) — proving we never depended on `target` either,
    /// before or after the announced Solr 10 relationship-shape changes.
    #[test]
    fn iswc_provider_parse_works_legacy_relation_shape() {
        let json = r#"{
            "works": [{
                "id": "mb-work-1",
                "title": "Bohemian Rhapsody",
                "iswcs": ["T0345246801"],
                "relations": [{
                    "type": "composer",
                    "target": "artist-mbid-1234",
                    "artist": {"name": "Freddie Mercury"}
                }]
            }]
        }"#;
        let results = IswcProvider::parse_works("iswc", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(results[0].artist.as_deref(), Some("Freddie Mercury"));
    }

    #[test]
    fn normalise_iswc_strips_separators_and_uppercases() {
        assert_eq!(normalise_iswc("t-034.524.680-1"), "T0345246801");
        assert_eq!(normalise_iswc("T-034524680-1"), "T0345246801");
        assert_eq!(normalise_iswc("T0345246801"), "T0345246801");
    }

    #[test]
    fn format_iswc_dotted_from_every_accepted_input_form() {
        // Idempotent across compact, hyphen-only and already-dotted input.
        for input in [
            "T0345246801",
            "T-034524680-1",
            "T-034.524.680-1",
            "t0345246801",
        ] {
            assert_eq!(
                format_iswc_dotted(input).as_deref(),
                Some("T-034.524.680-1"),
                "input: {input}"
            );
        }
    }

    #[test]
    fn format_iswc_dotted_rejects_malformed_input() {
        assert_eq!(format_iswc_dotted("X0345246801"), None); // wrong prefix
        assert_eq!(format_iswc_dotted("T034524680"), None); // 9 digits
        assert_eq!(format_iswc_dotted("T03452468012"), None); // 11 digits
        assert_eq!(format_iswc_dotted("TABCDEFGHIJ"), None); // non-digits
        assert_eq!(format_iswc_dotted(""), None);
    }

    #[test]
    fn build_iswc_query_falls_back_to_quoted_uppercase_when_unformattable() {
        // Malformed input still produces syntactically valid Lucene rather
        // than nothing; validate_iswc gates this in practice.
        assert_eq!(build_iswc_query("not-an-iswc"), r#"iswc:"NOT-AN-ISWC""#);
    }

    #[test]
    fn build_iswc_query_is_idempotent_across_input_forms() {
        let expected = r#"iswc:"T-034.524.680-1""#;
        for input in ["T0345246801", "T-034524680-1", "T-034.524.680-1"] {
            assert_eq!(build_iswc_query(input), expected, "input: {input}");
        }
    }

    #[test]
    fn build_iswc_query_emits_the_dotted_display_form() {
        // Live-verified: MusicBrainz indexes ISWCs in the dotted display
        // form. Compact and hyphen-only forms return 0 results / a parse
        // error respectively.
        assert_eq!(
            build_iswc_query("T-034524680-1"),
            r#"iswc:"T-034.524.680-1""#
        );
    }

    #[test]
    fn iswc_provider_parse_invalid_json_returns_err() {
        assert!(matches!(
            IswcProvider::parse_works("iswc", "bad"),
            Err(ProviderError::Other(_))
        ));
    }

    #[tokio::test]
    async fn iswc_provider_search_without_iswc_returns_not_supported() {
        let p = IswcProvider::new("App/1.0");
        let q = SearchQuery {
            max_results: Some(5),
            ..Default::default()
        };
        assert!(matches!(
            p.search(&q).await,
            Err(ProviderError::NotSupported(_))
        ));
    }
}
