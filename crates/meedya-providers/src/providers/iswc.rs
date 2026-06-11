// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// ISWC identifier provider (MusicBrainz works backend).
// Ported from MeedyaManager crates/mm-providers/src/identifiers/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{ISWC, PROVIDER_ID};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{ProviderResult, SearchQuery};

fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

/// Validate ISWC format: `T-123456789-C` (T + 9 digits + check digit).
/// Accepts the format with or without hyphens.
pub fn validate_iswc(iswc: &str) -> bool {
    let normalised: String = iswc
        .to_uppercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    // Must be exactly 11 chars: T + 9 digits + 1 check digit
    normalised.len() == 11
        && normalised.starts_with('T')
        && normalised[1..].chars().all(|c| c.is_ascii_digit())
}

/// Looks up ISWC identifiers via MusicBrainz works API.
///
/// Endpoint: `https://musicbrainz.org/ws/2/work/?query=iswc:<ISWC>`
/// Auth:     None (but User-Agent required)
/// Limits:   50 RPM
pub struct IswcProvider {
    client: Client,
    base_url: String,
    user_agent: String,
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
            .build()
            .expect("reqwest ClientBuilder failed — TLS initialisation error");
        Self {
            client,
            base_url: base_url.into(),
            user_agent,
        }
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
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .query(&[
                ("query", &format!("iswc:{iswc}")),
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
