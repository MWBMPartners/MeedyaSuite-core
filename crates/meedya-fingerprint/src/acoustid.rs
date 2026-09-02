// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// AcoustID fingerprinting and lookup.
// Extracted from MeedyaDL acoustid_service.rs.
//
// Generates Chromaprint audio fingerprints and looks up AcoustID
// identifiers via the acoustid.org API. Enables music identification
// compatible with MusicBrainz Picard and other AcoustID ecosystem tools.
//
// NOTE: The actual fingerprint generation requires `rusty-chromaprint`
// and `symphonia` crates. These are heavy dependencies, so this module
// defines the types and API client, while the PCM-level fingerprinting
// is gated behind an optional feature flag in consuming apps.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::FingerprintError;

/// Formats a `reqwest::Error`, stripping the query string from any URL it
/// carries first.
///
/// `reqwest::Error`'s `Display` impl appends `" for url ({url})"` —
/// including the full query string — so any endpoint that carries
/// secrets there would leak them into logs and returned error text.
/// `Error::url_mut` exists precisely for this (its own docs name "remove
/// sensitive information from the URL … but do not want to remove the
/// URL entirely" as the intended use); clearing only the query keeps the
/// host for diagnostics.
///
/// As of #87 the AcoustID `client` key travels in the POST body, not the
/// URL, so `lookup`'s own request no longer has anything for this to
/// strip — the key simply isn't in the URL at all, which is strictly
/// better for the #80 leak class than redacting it after the fact. This
/// stays in place as defense in depth (a future query parameter, or a
/// redirect that echoes one, would still be caught) and is kept for any
/// other request built against `ACOUSTID_API_URL` in this module.
///
/// This is a deliberate duplicate of `meedya_providers::providers::net_err`
/// rather than a shared dependency: `meedya-fingerprint` is a leaf crate
/// in the workspace dependency graph and cannot depend on
/// `meedya-providers` (see MeedyaSuite-core#80).
fn sanitized(mut e: reqwest::Error) -> String {
    if let Some(url) = e.url_mut() {
        url.set_query(None);
    }
    e.to_string()
}

/// AcoustID API endpoint.
const ACOUSTID_API_URL: &str = "https://api.acoustid.org/v2/lookup";

/// Delay between API requests (~3 req/sec rate limit).
const API_RATE_LIMIT_DELAY: Duration = Duration::from_millis(334);

/// Result of an AcoustID fingerprint lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcoustIdResult {
    /// The AcoustID UUID.
    pub acoustid: String,
    /// Confidence score (0.0 to 1.0).
    pub score: f64,
    /// MusicBrainz recording IDs (if returned by the API).
    pub recording_ids: Vec<String>,
    /// The compressed Chromaprint fingerprint (base64-encoded).
    pub fingerprint: String,
    /// Audio duration in seconds.
    pub duration_secs: u32,
}

/// Client for the AcoustID lookup API.
pub struct AcoustIdClient {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
}

impl AcoustIdClient {
    /// Create a new AcoustID client with the given API key.
    pub fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, ACOUSTID_API_URL)
    }

    /// Create a client pointed at a caller-supplied lookup endpoint instead
    /// of the real AcoustID API — useful for test mocking (e.g. against a
    /// `wiremock` server). Mirrors the `with_base_url` convention used by
    /// the providers in `meedya-providers`.
    pub fn with_base_url(api_key: String, base_url: impl Into<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.into(),
            http_client: reqwest::Client::builder()
                .user_agent("MeedyaSuite/1.0")
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Look up a fingerprint against the AcoustID API.
    ///
    /// `fingerprint` is the compressed Chromaprint (URL-safe base64).
    /// `duration_secs` is the audio duration in seconds.
    ///
    /// Returns the best match, or `FingerprintError::NoMatch` if none found.
    pub async fn lookup(
        &self,
        fingerprint: &str,
        duration_secs: u32,
    ) -> Result<AcoustIdResult, FingerprintError> {
        let duration_str = duration_secs.to_string();
        let params = [
            ("client", self.api_key.as_str()),
            ("meta", "recordings"),
            ("fingerprint", fingerprint),
            ("duration", duration_str.as_str()),
        ];

        // POST with a form-encoded body rather than GET with a query
        // string (#87). Chromaprint fingerprints scale with track
        // duration — DJ mixes and continuous albums, core MeedyaSuite
        // content, produce base64 blobs that reach several KB — and
        // proxies/CDNs commonly cap URLs around 8KB. AcoustID's own docs
        // direct clients to POST for fingerprint lookups. This also moves
        // the API key out of the URL entirely (it now travels in the
        // body, not the `client` query parameter), which is strictly
        // better for the #80 leak class than a URL that gets logged,
        // cached, or captured by an intermediary.
        //
        // `.form()` needs no extra reqwest feature/build flag: unlike
        // `.json()` (behind the `json` feature), form encoding uses
        // `serde_urlencoded`, an unconditional (non-optional) reqwest
        // dependency — confirmed against this workspace's Cargo.lock
        // rather than assumed. It sets `Content-Type:
        // application/x-www-form-urlencoded` automatically.
        let response = self
            .http_client
            .post(&self.base_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| FingerprintError::NetworkError(sanitized(e)))?;

        if !response.status().is_success() {
            return Err(FingerprintError::AcoustIdApiError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| FingerprintError::AcoustIdApiError(sanitized(e)))?;

        // Check API-level status
        let status = body["status"].as_str().unwrap_or("error");
        if status != "ok" {
            let message = body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown API error");
            return Err(FingerprintError::AcoustIdApiError(message.into()));
        }

        // Extract best result
        let results = body["results"]
            .as_array()
            .ok_or(FingerprintError::NoMatch)?;

        let best = results.first().ok_or(FingerprintError::NoMatch)?;

        let acoustid = best["id"]
            .as_str()
            .ok_or(FingerprintError::NoMatch)?
            .to_string();

        let score = best["score"].as_f64().unwrap_or(0.0);

        // Extract MusicBrainz recording IDs
        let recording_ids = best["recordings"]
            .as_array()
            .map(|recordings| {
                recordings
                    .iter()
                    .filter_map(|r| r["id"].as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();

        Ok(AcoustIdResult {
            acoustid,
            score,
            recording_ids,
            fingerprint: fingerprint.to_string(),
            duration_secs,
        })
    }

    /// Enforce rate limiting between API calls.
    pub async fn rate_limit_delay() {
        tokio::time::sleep(API_RATE_LIMIT_DELAY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Method;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn acoustid_result_serialization() {
        let result = AcoustIdResult {
            acoustid: "abc-123".into(),
            score: 0.95,
            recording_ids: vec!["mb-001".into()],
            fingerprint: "AQAA".into(),
            duration_secs: 240,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: AcoustIdResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.acoustid, "abc-123");
        assert_eq!(back.score, 0.95);
    }

    // Canary for MeedyaSuite-core#80: reqwest::Error's Display appends
    // " for url ({url})" including the query string, and AcoustID lookups
    // pass the API key as the `client` query parameter. This forces a
    // real reqwest error against a URL carrying a fake API key and
    // asserts `sanitized`'s output contains neither the secret nor the
    // query string, while still naming the host for diagnostics.
    #[tokio::test]
    async fn sanitized_strips_api_key_from_query_string() {
        // 127.0.0.1 on a closed port refuses the connection immediately
        // (no listener, no DNS, no real network) so `.send()` fails fast
        // and deterministically — hermetic and quick.
        let client = reqwest::Client::new();
        let result = client
            .get("http://127.0.0.1:1/v2/lookup?client=SUPERSECRET")
            .send()
            .await;

        let err = result.expect_err("connection to a closed port must fail");
        let message = sanitized(err);

        assert!(
            !message.contains("SUPERSECRET"),
            "sanitized leaked the API key into the error text: {message}"
        );
        assert!(
            !message.contains("client="),
            "sanitized leaked the query string into the error text: {message}"
        );
        assert!(
            message.contains("127.0.0.1"),
            "sanitized should still name the host for diagnostics: {message}"
        );
    }

    // Regression for MeedyaSuite-core#87: a Chromaprint fingerprint scales
    // with track duration, and DJ mixes / continuous albums (core
    // MeedyaSuite content) can produce a base64 blob of several KB — well
    // past the ~8KB URL length that proxies and CDNs commonly cap. This
    // builds a fingerprint deliberately >= 8KB and asserts the request
    // wiremock actually received is a POST carrying the fingerprint (and
    // the API key) in the body, with neither in the URL.
    #[tokio::test]
    async fn lookup_sends_large_fingerprint_in_post_body_not_url() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v2/lookup"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "results": [],
            })))
            .mount(&mock_server)
            .await;

        // Well over the 8KB threshold the issue cites. Plain ASCII so
        // form-urlencoding leaves it unchanged, making the body/URL
        // substring checks below unambiguous.
        let big_fingerprint = "A".repeat(8 * 1024 + 1);
        let api_key = "SUPERSECRET_CLIENT_KEY";

        let client = AcoustIdClient::with_base_url(
            api_key.to_string(),
            format!("{}/v2/lookup", mock_server.uri()),
        );

        // NoMatch is expected (empty `results`) — this test is about
        // transport, not the match outcome.
        let result = client.lookup(&big_fingerprint, 5400).await;
        assert!(matches!(result, Err(FingerprintError::NoMatch)));

        let requests = mock_server
            .received_requests()
            .await
            .expect("request recording is on by default");
        assert_eq!(requests.len(), 1, "expected exactly one lookup request");
        let request = &requests[0];

        assert_eq!(request.method, Method::POST);

        let url = request.url.to_string();
        assert!(
            request.url.query().is_none(),
            "URL carried a query string, but the fingerprint must travel in \
             the body: {url}"
        );
        assert!(
            !url.contains(&big_fingerprint),
            "fingerprint leaked into the request URL"
        );
        assert!(
            !url.contains(api_key),
            "API key leaked into the request URL"
        );

        let body = String::from_utf8_lossy(&request.body);
        assert!(
            body.contains(&big_fingerprint),
            "fingerprint did not arrive in the POST body"
        );
        assert!(
            body.contains(api_key),
            "API key did not arrive in the POST body"
        );
    }

    // Guards against the GET -> POST transport change (#87) altering
    // response handling: a normal successful lookup must still parse to
    // the same `AcoustIdResult` it did over GET.
    #[tokio::test]
    async fn lookup_parses_successful_response_identically_over_post() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v2/lookup"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "results": [
                    {
                        "id": "abc-123",
                        "score": 0.93,
                        "recordings": [
                            {"id": "mb-001"},
                            {"id": "mb-002"},
                        ],
                    }
                ],
            })))
            .mount(&mock_server)
            .await;

        let client = AcoustIdClient::with_base_url(
            "test-key".to_string(),
            format!("{}/v2/lookup", mock_server.uri()),
        );

        let result = client
            .lookup("AQAAsome-fingerprint", 240)
            .await
            .expect("mock server returned a match");

        assert_eq!(result.acoustid, "abc-123");
        assert_eq!(result.score, 0.93);
        assert_eq!(result.recording_ids, vec!["mb-001", "mb-002"]);
        assert_eq!(result.fingerprint, "AQAAsome-fingerprint");
        assert_eq!(result.duration_secs, 240);
    }
}
