use serde::Deserialize;
use tracing::debug;

use crate::{AcoustIdMatch, AcoustIdRecording, Fingerprint};

const ACOUSTID_API_URL: &str = "https://api.acoustid.org/v2/lookup";

/// AcoustID API client for fingerprint-based music identification.
pub struct AcoustIdClient {
    api_key: String,
    client: reqwest::Client,
}

impl AcoustIdClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            api_key: api_key.into(),
            client,
        }
    }

    /// Look up a fingerprint against the AcoustID database.
    pub async fn lookup(&self, fingerprint: &Fingerprint) -> Result<Vec<AcoustIdMatch>, String> {
        let response = self
            .client
            .get(ACOUSTID_API_URL)
            .query(&[
                ("client", self.api_key.as_str()),
                ("duration", &fingerprint.duration.to_string()),
                ("fingerprint", &fingerprint.fingerprint),
                ("meta", "recordings"),
            ])
            .send()
            .await
            .map_err(|e| format!("AcoustID request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("AcoustID API returned status {}", response.status()));
        }

        let body: AcoustIdResponse = response
            .json()
            .await
            .map_err(|e| format!("failed to parse AcoustID response: {e}"))?;

        if body.status != "ok" {
            return Err(format!("AcoustID error: {:?}", body.error));
        }

        let matches = body
            .results
            .unwrap_or_default()
            .into_iter()
            .map(|r| AcoustIdMatch {
                id: r.id,
                score: r.score,
                recordings: r
                    .recordings
                    .unwrap_or_default()
                    .into_iter()
                    .map(|rec| AcoustIdRecording {
                        id: rec.id,
                        title: rec.title,
                        artists: rec
                            .artists
                            .unwrap_or_default()
                            .into_iter()
                            .map(|a| a.name)
                            .collect(),
                        duration: rec.duration,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        debug!("AcoustID returned {} matches", matches.len());
        Ok(matches)
    }
}

// AcoustID API response structures

#[derive(Debug, Deserialize)]
struct AcoustIdResponse {
    status: String,
    error: Option<AcoustIdError>,
    results: Option<Vec<AcoustIdResult>>,
}

#[derive(Debug, Deserialize)]
struct AcoustIdError {
    #[allow(dead_code)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct AcoustIdResult {
    id: String,
    score: f64,
    recordings: Option<Vec<AcoustIdApiRecording>>,
}

#[derive(Debug, Deserialize)]
struct AcoustIdApiRecording {
    id: String,
    title: Option<String>,
    artists: Option<Vec<AcoustIdArtist>>,
    duration: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AcoustIdArtist {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation() {
        let client = AcoustIdClient::new("test-key");
        assert_eq!(client.api_key, "test-key");
    }
}
