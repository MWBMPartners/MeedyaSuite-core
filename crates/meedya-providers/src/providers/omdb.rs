// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// OMDb (Open Movie Database) provider.
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

/// Searches the OMDb API, which provides IMDb-sourced film/TV metadata.
///
/// Endpoint: `https://www.omdbapi.com/`
/// Auth:     API key (`apikey` query param; free tier = 1000 req/day)
/// Limits:   10 RPM (free tier)
pub struct OmdbProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl OmdbProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(api_key, "https://www.omdbapi.com")
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
        #[serde(rename_all = "PascalCase")]
        struct OmdbSearchResponse {
            search: Option<Vec<OmdbSearchResult>>,
            #[serde(rename = "Error")]
            error: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct OmdbSearchResult {
            #[serde(rename = "imdbID")]
            imdb_id: Option<String>,
            title: Option<String>,
            year: Option<String>,
            poster: Option<String>,
            #[serde(rename = "Type")]
            media_type: Option<String>,
        }

        let resp: OmdbSearchResponse =
            serde_json::from_str(body).map_err(|e| parse_err("OMDb response", e))?;

        if let Some(err) = resp.error {
            return Err(ProviderError::Other(format!(
                "parse error: OMDb error: {err}"
            )));
        }

        let items = resp.search.unwrap_or_default();
        let results = items
            .into_iter()
            .map(|r| {
                let year = r
                    .year
                    .as_deref()
                    .and_then(|y| y[..4.min(y.len())].parse::<u32>().ok());
                let cover_art = r
                    .poster
                    .as_deref()
                    .filter(|p| *p != "N/A" && !p.is_empty())
                    .map(|p| {
                        vec![CoverArtInfo {
                            url: p.to_owned(),
                            width: None,
                            height: None,
                            mime_type: Some("image/jpeg".into()),
                        }]
                    })
                    .unwrap_or_default();

                let mut result = ProviderResult::new(provider_name);
                result.title = r.title;
                result.year = year;
                result.cover_art = cover_art;

                if let Some(id) = r.imdb_id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id));
                }
                if let Some(t) = &r.media_type {
                    result
                        .metadata
                        .insert("media_type".into(), Value::String(t.clone()));
                }

                result
            })
            .collect();
        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for OmdbProvider {
    fn id(&self) -> &str {
        "omdb"
    }

    fn display_name(&self) -> &str {
        "OMDb / IMDb"
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
            return Err(ProviderError::NotConfigured("omdb".into()));
        }
        let key = self.api_key.as_deref().unwrap();
        let fallback = format!(
            "{} {}",
            query.title.as_deref().unwrap_or(""),
            query.artist.as_deref().unwrap_or("")
        );
        let title: &str = query.title.as_deref().unwrap_or(fallback.trim());

        debug!(provider = "omdb", title = title, "Sending search request");

        let mut params = vec![
            ("s", title.to_owned()),
            ("apikey", key.to_owned()),
            ("type", "movie".to_owned()), // Default to movies; could be configurable
        ];
        if let Some(y) = query.year {
            params.push(("y", y.to_string()));
        }

        let response = self
            .client
            .get(&self.base_url)
            .query(&params)
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
        Self::parse_search("omdb", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omdb_name() {
        assert_eq!(OmdbProvider::new(Some("key".into())).id(), "omdb");
    }

    #[test]
    fn omdb_parse_search_valid_json() {
        let json = r#"{
            "Search": [{
                "Title": "Interstellar",
                "Year": "2014",
                "imdbID": "tt0816692",
                "Type": "movie",
                "Poster": "https://m.media-amazon.com/images/M/poster.jpg"
            }],
            "totalResults": "1",
            "Response": "True"
        }"#;
        let results = OmdbProvider::parse_search("omdb", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Interstellar"));
        assert_eq!(results[0].year, Some(2014));
        assert_eq!(
            results[0]
                .metadata
                .get(PROVIDER_ID)
                .and_then(serde_json::Value::as_str),
            Some("tt0816692")
        );
        assert!(!results[0].cover_art.is_empty());
    }

    #[test]
    fn omdb_parse_error_response_returns_err() {
        let json = r#"{"Response": "False", "Error": "Movie not found!"}"#;
        let result = OmdbProvider::parse_search("omdb", json);
        assert!(matches!(result, Err(ProviderError::Other(_))));
    }

    #[test]
    fn omdb_parse_na_poster_produces_no_cover_art() {
        let json = r#"{
            "Search": [{"Title": "X", "Year": "2020", "imdbID": "tt0", "Type": "movie", "Poster": "N/A"}]
        }"#;
        let results = OmdbProvider::parse_search("omdb", json).unwrap();
        assert!(results[0].cover_art.is_empty());
    }

    #[test]
    fn omdb_parse_invalid_json_returns_err() {
        assert!(matches!(
            OmdbProvider::parse_search("omdb", "bad"),
            Err(ProviderError::Other(_))
        ));
    }
}
