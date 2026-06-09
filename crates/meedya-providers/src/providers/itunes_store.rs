// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// iTunes Store metadata provider (iTunes Search API — tvShow entity).
// Ported from MeedyaManager crates/mm-providers/src/video/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.
//
// Uses the same iTunes Search API as AppleTvProvider but targets the `tvShow`
// media type / `tvSeason` entity. Re-implements the parse helper inline so each
// provider feature can be toggled independently without cross-module deps.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{CONTENT_ADVISORY, DURATION_SECS, PROVIDER_ID};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{CoverArtInfo, ProviderResult, SearchQuery};

fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

fn insert_duration(result: &mut ProviderResult, secs: f64) {
    if let Some(num) = serde_json::Number::from_f64(secs) {
        result
            .metadata
            .insert(DURATION_SECS.into(), Value::Number(num));
    }
}

/// Searches the iTunes Store for purchased/available movies and TV shows.
///
/// Auth: None; Limits: 20 RPM
pub struct ItunesStoreProvider {
    client: Client,
    base_url: String,
    enabled: bool,
    country: String,
}

impl ItunesStoreProvider {
    pub fn new(country: impl Into<String>) -> Self {
        Self::with_base_url(country, "https://itunes.apple.com")
    }

    pub fn with_base_url(country: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            enabled: true,
            country: country.into(),
        }
    }

    pub(crate) fn parse(
        provider_name: &str,
        body: &str,
    ) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesVideoResponse {
            results: Vec<ItunesVideoResult>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesVideoResult {
            track_id: Option<u64>,
            track_name: Option<String>,
            artist_name: Option<String>,
            collection_name: Option<String>,
            artwork_url100: Option<String>,
            release_date: Option<String>,
            track_time_millis: Option<u64>,
            primary_genre_name: Option<String>,
            content_advisory_rating: Option<String>,
        }

        let resp: ItunesVideoResponse =
            serde_json::from_str(body).map_err(|e| parse_err("iTunes Store response", e))?;

        let results = resp
            .results
            .into_iter()
            .map(|r| {
                let year = r
                    .release_date
                    .as_deref()
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());
                let cover_art = r
                    .artwork_url100
                    .as_deref()
                    .map(|url| {
                        let hires = url.replace("100x100", "600x600");
                        vec![
                            CoverArtInfo {
                                url: hires,
                                width: Some(600),
                                height: Some(600),
                                mime_type: Some("image/jpeg".into()),
                            },
                            CoverArtInfo {
                                url: url.to_owned(),
                                width: Some(100),
                                height: Some(100),
                                mime_type: Some("image/jpeg".into()),
                            },
                        ]
                    })
                    .unwrap_or_default();

                let mut result = ProviderResult::new(provider_name);
                result.title = r.track_name;
                result.artist = r.artist_name;
                result.album = r.collection_name;
                result.year = year;
                result.genre = r.primary_genre_name;
                result.cover_art = cover_art;

                if let Some(id) = r.track_id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id.to_string()));
                }
                if let Some(ms) = r.track_time_millis {
                    insert_duration(&mut result, ms as f64 / 1000.0);
                }
                if let Some(advisory) = r.content_advisory_rating {
                    result
                        .metadata
                        .insert(CONTENT_ADVISORY.into(), Value::String(advisory));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

impl Default for ItunesStoreProvider {
    fn default() -> Self {
        Self::new("US")
    }
}

#[async_trait]
impl MetadataProvider for ItunesStoreProvider {
    fn id(&self) -> &str {
        "itunes_store"
    }

    fn display_name(&self) -> &str {
        "iTunes Store"
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
        if !self.enabled {
            return Err(ProviderError::NotConfigured("itunes_store".into()));
        }
        let fallback = format!(
            "{} {}",
            query.title.as_deref().unwrap_or(""),
            query.artist.as_deref().unwrap_or("")
        );
        let term: &str = query.title.as_deref().unwrap_or(fallback.trim());
        debug!(
            provider = "itunes_store",
            term = term,
            "Sending iTunes TV search request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let url = format!("{}/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[
                ("term", term),
                ("media", "tvShow"),
                ("entity", "tvSeason"),
                ("country", &self.country),
                ("limit", &limit),
            ])
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
        Self::parse("itunes_store", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn itunes_store_name() {
        assert_eq!(ItunesStoreProvider::new("US").id(), "itunes_store");
    }

    #[test]
    fn itunes_store_capabilities_video_type() {
        assert!(ItunesStoreProvider::default().capabilities().video_search);
    }

    #[test]
    fn itunes_store_parse_valid_json() {
        let json =
            r#"{"results": [{"trackId": 9999, "trackName": "Breaking Bad", "artistName": "AMC"}]}"#;
        let results = ItunesStoreProvider::parse("itunes_store", json).unwrap();
        assert_eq!(results[0].title.as_deref(), Some("Breaking Bad"));
    }
}
