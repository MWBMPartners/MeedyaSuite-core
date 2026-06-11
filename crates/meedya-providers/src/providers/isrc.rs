// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// ISRC identifier provider (MusicBrainz recordings backend).
// Ported from MeedyaManager crates/mm-providers/src/identifiers/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{DURATION_SECS, PROVIDER_ID};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{ProviderResult, SearchQuery};

fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

/// Validate ISRC format: 2 country + 3 registrant + 2 year + 5 designation = 12 chars.
/// Accepts hyphens as separators (e.g. `GB-AYE-06-01498`).
pub fn validate_isrc(isrc: &str) -> bool {
    let normalised: String = isrc.chars().filter(|c| c.is_alphanumeric()).collect();
    normalised.len() == 12
        && normalised[..2].chars().all(|c| c.is_ascii_alphabetic())
        && normalised[2..5].chars().all(|c| c.is_ascii_alphanumeric())
        && normalised[5..7].chars().all(|c| c.is_ascii_digit())
        && normalised[7..12].chars().all(|c| c.is_ascii_digit())
}

/// Looks up ISRC identifiers via MusicBrainz recording search.
///
/// Endpoint: `https://musicbrainz.org/ws/2/recording/?query=isrc:<ISRC>`
/// Auth:     None (but User-Agent required)
/// Limits:   30 RPM
pub struct IsrcProvider {
    client: Client,
    base_url: String,
    user_agent: String,
}

impl IsrcProvider {
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
            .build()
            .expect("reqwest ClientBuilder failed — TLS initialisation error");
        Self {
            client,
            base_url: base_url.into(),
            user_agent,
        }
    }

    /// True if a User-Agent string is configured. Required by MusicBrainz API.
    fn configured(&self) -> bool {
        !self.user_agent.is_empty()
    }

    fn parse_recordings(
        provider_name: &str,
        body: &str,
    ) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        struct MbResponse {
            recordings: Vec<MbRecording>,
        }
        #[derive(Deserialize)]
        struct MbRecording {
            id: Option<String>,
            title: Option<String>,
            #[serde(rename = "artist-credit")]
            artist_credit: Option<Vec<MbCredit>>,
            releases: Option<Vec<MbRelease>>,
            isrcs: Option<Vec<String>>,
            length: Option<u64>,
        }
        #[derive(Deserialize)]
        struct MbCredit {
            artist: Option<MbArtist>,
        }
        #[derive(Deserialize)]
        struct MbArtist {
            name: Option<String>,
        }
        #[derive(Deserialize)]
        struct MbRelease {
            title: Option<String>,
            date: Option<String>,
        }

        let resp: MbResponse =
            serde_json::from_str(body).map_err(|e| parse_err("ISRC/MusicBrainz response", e))?;

        let results = resp
            .recordings
            .into_iter()
            .map(|rec| {
                let artist = rec.artist_credit.as_deref().map(|credits| {
                    credits
                        .iter()
                        .filter_map(|c| c.artist.as_ref()?.name.as_deref())
                        .collect::<Vec<_>>()
                        .join("; ")
                });
                let first_release = rec.releases.as_deref().and_then(|r| r.first());
                let album = first_release.and_then(|r| r.title.clone());
                let year = first_release
                    .and_then(|r| r.date.as_deref())
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());

                let mut result = ProviderResult::new(provider_name);
                result.title = rec.title;
                result.artist = artist;
                result.album = album;
                result.year = year;
                result.isrc = rec.isrcs.and_then(|v| v.into_iter().next());

                if let Some(id) = rec.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id));
                }
                if let Some(length_ms) = rec.length {
                    let secs = length_ms as f64 / 1000.0;
                    if let Some(num) = serde_json::Number::from_f64(secs) {
                        result
                            .metadata
                            .insert(DURATION_SECS.into(), Value::Number(num));
                    }
                }

                result
            })
            .collect();
        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for IsrcProvider {
    fn id(&self) -> &str {
        "isrc"
    }

    fn display_name(&self) -> &str {
        "ISRC (via MusicBrainz)"
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
            return Err(ProviderError::NotConfigured("isrc".into()));
        }

        let isrc = query.isrc.as_deref().ok_or_else(|| {
            ProviderError::NotSupported("isrc: ISRC query requires an ISRC code".into())
        })?;

        if !validate_isrc(isrc) {
            return Err(ProviderError::Other(format!(
                "parse error: Invalid ISRC format: {isrc}"
            )));
        }

        debug!(
            provider = "isrc",
            isrc = isrc,
            "Sending ISRC lookup request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let url = format!("{}/ws/2/recording/", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .query(&[
                ("query", &format!("isrc:{isrc}")),
                ("limit", &limit),
                ("fmt", &"json".to_owned()),
            ])
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            let s = response.status();
            if s.as_u16() == 503 {
                return Err(ProviderError::RateLimited("isrc".into()));
            }
            return Err(ProviderError::NetworkError(format!("HTTP {s}")));
        }

        let body = response.text().await.map_err(net_err)?;
        Self::parse_recordings("isrc", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_isrc_valid_standard() {
        assert!(validate_isrc("GBAYE0601498")); // 12 chars, no hyphens
    }

    #[test]
    fn validate_isrc_valid_with_hyphens() {
        assert!(validate_isrc("GB-AYE-06-01498"));
    }

    #[test]
    fn validate_isrc_too_short() {
        assert!(!validate_isrc("GBAYE060149")); // 11 chars
    }

    #[test]
    fn validate_isrc_too_long() {
        assert!(!validate_isrc("GBAYE06014980")); // 13 chars
    }

    #[test]
    fn validate_isrc_invalid_country_code() {
        // Country must be 2 letters; digits in first 2 positions → invalid
        assert!(!validate_isrc("12AYE0601498"));
    }

    #[test]
    fn isrc_provider_name() {
        assert_eq!(IsrcProvider::new("App/1.0").id(), "isrc");
    }

    #[test]
    fn isrc_provider_capabilities() {
        let caps = IsrcProvider::new("App/1.0").capabilities();
        assert!(caps.identifier_lookup);
        assert!(caps.music_search);
        assert!(!caps.cover_art);
    }

    #[test]
    fn isrc_provider_parse_recordings_valid() {
        let json = r#"{
            "recordings": [{
                "id": "mb-rec-1",
                "title": "Comfortably Numb",
                "artist-credit": [{"artist": {"name": "Pink Floyd"}}],
                "releases": [{"title": "The Wall", "date": "1979-11-30"}],
                "isrcs": ["GBAYE7900498"],
                "length": 382000
            }]
        }"#;
        let results = IsrcProvider::parse_recordings("isrc", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Comfortably Numb"));
        assert_eq!(results[0].isrc.as_deref(), Some("GBAYE7900498"));
        assert_eq!(results[0].artist.as_deref(), Some("Pink Floyd"));
    }

    #[test]
    fn isrc_provider_parse_invalid_json_returns_err() {
        assert!(matches!(
            IsrcProvider::parse_recordings("isrc", "bad"),
            Err(ProviderError::Other(_))
        ));
    }

    #[tokio::test]
    async fn isrc_provider_search_without_isrc_returns_not_supported() {
        let p = IsrcProvider::new("App/1.0");
        let q = SearchQuery {
            max_results: Some(5),
            ..Default::default()
        };
        assert!(matches!(
            p.search(&q).await,
            Err(ProviderError::NotSupported(_))
        ));
    }

    #[tokio::test]
    async fn isrc_provider_search_invalid_isrc_returns_parse_err() {
        let p = IsrcProvider::new("App/1.0");
        let q = SearchQuery {
            isrc: Some("BAD".into()),
            max_results: Some(5),
            ..Default::default()
        };
        assert!(matches!(p.search(&q).await, Err(ProviderError::Other(_))));
    }
}
