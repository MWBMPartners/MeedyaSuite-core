// Copyright (c) 2026 MWBMPartners
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
    http_client: reqwest::Client,
}

impl AcoustIdClient {
    /// Create a new AcoustID client with the given API key.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
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
        let params = [
            ("client", self.api_key.as_str()),
            ("meta", "recordings"),
            ("fingerprint", fingerprint),
            ("duration", &duration_secs.to_string()),
        ];

        let response = self
            .http_client
            .get(ACOUSTID_API_URL)
            .query(&params)
            .send()
            .await
            .map_err(|e| FingerprintError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(FingerprintError::AcoustIdApiError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| FingerprintError::AcoustIdApiError(e.to_string()))?;

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
}
