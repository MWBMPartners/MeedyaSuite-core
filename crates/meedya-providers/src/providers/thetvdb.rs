// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// TheTVDB provider.
// Ported from MeedyaManager crates/mm-providers/src/video/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::PROVIDER_ID;
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{CoverArtInfo, ProviderResult, SearchQuery};

fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

/// Searches TheTVDB for TV shows and episodes.
///
/// Endpoint: `https://api4.thetvdb.com/v4/search`
/// Auth:     API key (Bearer token obtained via `/login`)
/// Limits:   30 RPM
pub struct TheTvdbProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl TheTvdbProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(api_key, "https://api4.thetvdb.com")
    }

    pub fn with_base_url(api_key: Option<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            api_key,
        }
    }

    fn configured(&self) -> bool {
        self.api_key.is_some()
    }

    fn parse_search(provider_name: &str, body: &str) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        struct TvdbResponse {
            data: Option<Vec<TvdbResult>>,
        }
        #[derive(Deserialize)]
        struct TvdbResult {
            id: Option<String>,
            name: Option<String>,
            first_air_time: Option<String>,
            image_url: Option<String>,
            overview: Option<String>,
            #[serde(rename = "type")]
            media_type: Option<String>,
        }

        let resp: TvdbResponse =
            serde_json::from_str(body).map_err(|e| parse_err("TheTVDB response", e))?;

        let data = resp.data.unwrap_or_default();
        let results = data
            .into_iter()
            .map(|r| {
                let year = r
                    .first_air_time
                    .as_deref()
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());
                let cover_art = r
                    .image_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .map(|u| {
                        vec![CoverArtInfo {
                            url: u.to_owned(),
                            width: None,
                            height: None,
                            mime_type: Some("image/jpeg".into()),
                        }]
                    })
                    .unwrap_or_default();

                let mut result = ProviderResult::new(provider_name);
                result.title = r.name;
                result.year = year;
                result.cover_art = cover_art;

                if let Some(id) = r.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id));
                }
                if let Some(o) = r.overview {
                    result.metadata.insert("overview".into(), Value::String(o));
                }
                if let Some(t) = r.media_type {
                    result
                        .metadata
                        .insert("media_type".into(), Value::String(t));
                }

                result
            })
            .collect();
        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for TheTvdbProvider {
    fn id(&self) -> &str {
        "thetvdb"
    }

    fn display_name(&self) -> &str {
        "TheTVDB"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            music_search: false,
            video_search: true,
            podcast_search: false,
            cover_art: true,
            lyrics: false,
            fingerprint_lookup: false,
            identifier_lookup: false,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
        if !self.configured() {
            return Err(ProviderError::NotConfigured("thetvdb".into()));
        }
        let key = self.api_key.as_deref().unwrap();
        let fallback = format!(
            "{} {}",
            query.title.as_deref().unwrap_or(""),
            query.artist.as_deref().unwrap_or("")
        );
        let q: &str = query.title.as_deref().unwrap_or(fallback.trim());

        debug!(provider = "thetvdb", query = q, "Sending search request");

        let limit = query.max_results.unwrap_or(10).to_string();
        let url = format!("{}/v4/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(key)
            .query(&[("query", q), ("limit", &limit)])
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            return Err(ProviderError::NetworkError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body = response.text().await.map_err(net_err)?;
        Self::parse_search("thetvdb", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tvdb_name() {
        let p = TheTvdbProvider::new(Some("key".into()));
        assert_eq!(p.id(), "thetvdb");
    }

    #[test]
    fn tvdb_capabilities_video_type() {
        let caps = TheTvdbProvider::new(None).capabilities();
        assert!(caps.video_search);
    }

    #[test]
    fn tvdb_parse_search_valid_json() {
        let json = r#"{
            "data": [{
                "id": "series-1396",
                "name": "Breaking Bad",
                "first_air_time": "2008-01-20",
                "image_url": "https://artworks.thetvdb.com/banners/bb.jpg",
                "overview": "A high school chemistry teacher...",
                "type": "series"
            }]
        }"#;
        let results = TheTvdbProvider::parse_search("thetvdb", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Breaking Bad"));
        assert_eq!(results[0].year, Some(2008));
        assert!(!results[0].cover_art.is_empty());
        assert_eq!(
            results[0]
                .metadata
                .get("media_type")
                .and_then(serde_json::Value::as_str),
            Some("series")
        );
    }

    #[test]
    fn tvdb_parse_null_data_returns_empty() {
        let json = r#"{"data": null}"#;
        let results = TheTvdbProvider::parse_search("thetvdb", json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn tvdb_parse_invalid_json_returns_err() {
        assert!(matches!(
            TheTvdbProvider::parse_search("thetvdb", "garbage"),
            Err(ProviderError::Other(_))
        ));
    }
}
