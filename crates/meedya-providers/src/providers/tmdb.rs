// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// TMDb (The Movie Database) provider.
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

/// Searches The Movie Database (TMDb) for films and TV shows.
///
/// Endpoint: `https://api.themoviedb.org/3/search/multi`
/// Auth:     API key (`api_key` query param or Bearer token)
/// Limits:   40 RPM
pub struct TmdbProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl TmdbProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(api_key, "https://api.themoviedb.org")
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

    fn parse_multi_search(
        provider_name: &str,
        body: &str,
    ) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        struct TmdbResponse {
            results: Vec<TmdbResult>,
        }

        #[derive(Deserialize)]
        struct TmdbResult {
            id: Option<u64>,
            media_type: Option<String>,
            title: Option<String>, // movies
            name: Option<String>,  // TV shows
            overview: Option<String>,
            release_date: Option<String>,   // movies
            first_air_date: Option<String>, // TV shows
            poster_path: Option<String>,
            vote_average: Option<f64>,
            #[allow(dead_code)]
            genre_ids: Option<Vec<u32>>,
        }

        let resp: TmdbResponse =
            serde_json::from_str(body).map_err(|e| parse_err("TMDb response", e))?;

        const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";

        let results = resp
            .results
            .into_iter()
            .map(|r| {
                let title = r.title.or(r.name);
                let year = r
                    .release_date
                    .as_deref()
                    .or(r.first_air_date.as_deref())
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());
                let cover_art = r
                    .poster_path
                    .as_deref()
                    .map(|p| {
                        vec![
                            CoverArtInfo {
                                url: format!("{IMAGE_BASE}/original{p}"),
                                width: None,
                                height: None,
                                mime_type: Some("image/jpeg".into()),
                            },
                            CoverArtInfo {
                                url: format!("{IMAGE_BASE}/w500{p}"),
                                width: Some(500),
                                height: Some(750),
                                mime_type: Some("image/jpeg".into()),
                            },
                        ]
                    })
                    .unwrap_or_default();
                let score = r.vote_average.map_or(0.5, |v| (v / 10.0).clamp(0.0, 1.0));

                let mut result = ProviderResult::new(provider_name);
                result.title = title;
                result.year = year;
                result.cover_art = cover_art;
                result.score = score;

                if let Some(id) = r.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id.to_string()));
                }
                if let Some(overview) = r.overview {
                    if !overview.is_empty() {
                        result
                            .metadata
                            .insert("overview".into(), Value::String(overview));
                    }
                }
                if let Some(mt) = &r.media_type {
                    result
                        .metadata
                        .insert("media_type".into(), Value::String(mt.clone()));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for TmdbProvider {
    fn id(&self) -> &str {
        "tmdb"
    }

    fn display_name(&self) -> &str {
        "The Movie Database (TMDb)"
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
            return Err(ProviderError::NotConfigured("tmdb".into()));
        }
        let key = self.api_key.as_deref().unwrap();
        let title_fallback = format!(
            "{} {}",
            query.title.as_deref().unwrap_or(""),
            query.artist.as_deref().unwrap_or("")
        );
        let search_query: String = query
            .title
            .clone()
            .unwrap_or_else(|| title_fallback.trim().to_owned());

        debug!(
            provider = "tmdb",
            query = %search_query,
            "Sending search request"
        );

        let mut params = vec![
            ("api_key", key.to_owned()),
            ("query", search_query),
            ("page", "1".to_owned()),
        ];
        if let Some(year) = query.year {
            params.push(("year", year.to_string()));
        }

        let url = format!("{}/3/search/multi", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            let s = response.status();
            if s.as_u16() == 429 {
                return Err(ProviderError::RateLimited("tmdb".into()));
            }
            return Err(ProviderError::NetworkError(format!("HTTP {s}")));
        }

        let body = response.text().await.map_err(net_err)?;
        let mut results = Self::parse_multi_search("tmdb", &body)?;
        results.truncate(query.max_results.unwrap_or(10));
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_name() {
        let p = TmdbProvider::new(Some("key".into()));
        assert_eq!(p.id(), "tmdb");
    }

    #[test]
    fn tmdb_capabilities_video_type() {
        let caps = TmdbProvider::new(None).capabilities();
        assert!(caps.video_search);
        assert!(!caps.music_search);
        assert!(caps.cover_art);
    }

    #[test]
    fn tmdb_parse_multi_search_valid_json() {
        let json = r#"{
            "results": [{
                "id": 27205,
                "media_type": "movie",
                "title": "Inception",
                "overview": "A thief who steals corporate secrets...",
                "release_date": "2010-07-16",
                "poster_path": "/9gk7adHYeDvHkCSEqAvQNLV5Uge.jpg",
                "vote_average": 8.4
            }]
        }"#;
        let results = TmdbProvider::parse_multi_search("tmdb", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Inception"));
        assert_eq!(results[0].year, Some(2010));
        assert!((results[0].score - 0.84).abs() < 1e-9);
        assert!(!results[0].cover_art.is_empty());
        // Original image URL should be in cover art
        assert!(results[0]
            .cover_art
            .iter()
            .any(|a| a.url.contains("original")));
    }

    #[test]
    fn tmdb_parse_tv_show_uses_name_field() {
        let json = r#"{
            "results": [{
                "id": 1396,
                "media_type": "tv",
                "name": "Breaking Bad",
                "first_air_date": "2008-01-20",
                "vote_average": 9.5
            }]
        }"#;
        let results = TmdbProvider::parse_multi_search("tmdb", json).unwrap();
        assert_eq!(results[0].title.as_deref(), Some("Breaking Bad"));
        assert_eq!(results[0].year, Some(2008));
    }

    #[test]
    fn tmdb_parse_empty_results() {
        let json = r#"{"results": []}"#;
        let results = TmdbProvider::parse_multi_search("tmdb", json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn tmdb_parse_invalid_json_returns_err() {
        let result = TmdbProvider::parse_multi_search("tmdb", "bad json");
        assert!(matches!(result, Err(ProviderError::Other(_))));
    }

    #[test]
    fn tmdb_parse_overview_stored_in_metadata() {
        let json =
            r#"{"results": [{"id": 1, "media_type": "movie", "overview": "Description here"}]}"#;
        let results = TmdbProvider::parse_multi_search("tmdb", json).unwrap();
        assert_eq!(
            results[0]
                .metadata
                .get("overview")
                .and_then(serde_json::Value::as_str),
            Some("Description here")
        );
    }

    #[test]
    fn tmdb_parse_score_normalised_from_vote_average() {
        // vote_average = 7.5 → score = 0.75
        let json = r#"{"results": [{"id": 1, "media_type": "movie", "vote_average": 7.5}]}"#;
        let results = TmdbProvider::parse_multi_search("tmdb", json).unwrap();
        assert!((results[0].score - 0.75).abs() < 1e-9);
    }
}
