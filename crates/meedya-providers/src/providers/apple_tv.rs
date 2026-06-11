// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Apple TV metadata provider (iTunes Search API — movie entity).
// Ported from MeedyaManager crates/mm-providers/src/video/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

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

/// Searches Apple TV via the iTunes Search API for films and TV episodes.
///
/// Endpoint: `https://itunes.apple.com/search?media=movie`
/// Auth:     None (public API)
/// Limits:   20 RPM (conservative)
pub struct AppleTvProvider {
    client: Client,
    base_url: String,
    enabled: bool,
    country: String,
}

impl AppleTvProvider {
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

    pub(crate) fn parse_itunes_video(
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
            artist_name: Option<String>, // Director for movies
            collection_name: Option<String>,
            artwork_url100: Option<String>,
            release_date: Option<String>,
            track_time_millis: Option<u64>,
            primary_genre_name: Option<String>,
            content_advisory_rating: Option<String>,
        }

        let resp: ItunesVideoResponse =
            serde_json::from_str(body).map_err(|e| parse_err("Apple TV response", e))?;

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
                result.artist = r.artist_name; // Director for films
                result.album = r.collection_name; // Series name for TV episodes
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

impl Default for AppleTvProvider {
    fn default() -> Self {
        Self::new("US")
    }
}

#[async_trait]
impl MetadataProvider for AppleTvProvider {
    fn id(&self) -> &str {
        "apple_tv"
    }

    fn display_name(&self) -> &str {
        "Apple TV"
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
            return Err(ProviderError::NotConfigured("apple_tv".into()));
        }
        let fallback = format!(
            "{} {}",
            query.title.as_deref().unwrap_or(""),
            query.artist.as_deref().unwrap_or("")
        );
        let term: &str = query.title.as_deref().unwrap_or(fallback.trim());
        debug!(
            provider = "apple_tv",
            term = term,
            "Sending iTunes video search request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let url = format!("{}/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[
                ("term", term),
                ("media", "movie"),
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
        Self::parse_itunes_video("apple_tv", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_tv_name() {
        assert_eq!(AppleTvProvider::new("US").id(), "apple_tv");
    }

    #[test]
    fn apple_tv_capabilities_video_type() {
        assert!(AppleTvProvider::default().capabilities().video_search);
    }

    #[test]
    fn apple_tv_parse_itunes_video_valid_json() {
        let json = r#"{
            "results": [{
                "trackId": 1234,
                "trackName": "Interstellar",
                "artistName": "Christopher Nolan",
                "collectionName": null,
                "artworkUrl100": "https://is1.mzstatic.com/100x100.jpg",
                "releaseDate": "2014-11-07T00:00:00Z",
                "trackTimeMillis": 9720000,
                "primaryGenreName": "Sci-Fi",
                "contentAdvisoryRating": "PG-13"
            }]
        }"#;
        let results = AppleTvProvider::parse_itunes_video("apple_tv", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Interstellar"));
        assert_eq!(results[0].artist.as_deref(), Some("Christopher Nolan")); // Director
        assert_eq!(results[0].year, Some(2014));
        assert_eq!(results[0].genre.as_deref(), Some("Sci-Fi"));
        assert_eq!(
            results[0]
                .metadata
                .get(CONTENT_ADVISORY)
                .and_then(serde_json::Value::as_str),
            Some("PG-13")
        );
        assert_eq!(results[0].cover_art.len(), 2);
    }

    #[test]
    fn apple_tv_parse_empty_results() {
        let json = r#"{"results": []}"#;
        let results = AppleTvProvider::parse_itunes_video("apple_tv", json).unwrap();
        assert!(results.is_empty());
    }
}
