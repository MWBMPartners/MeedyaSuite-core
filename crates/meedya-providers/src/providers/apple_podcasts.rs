// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Apple Podcasts metadata provider (iTunes Search API — podcast entity).
// Ported from MeedyaManager crates/mm-providers/src/podcasts/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::PROVIDER_ID;
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{CoverArtInfo, ProviderResult, SearchQuery};

/// Searches Apple Podcasts via the iTunes Search API.
///
/// Endpoint: `https://itunes.apple.com/search?media=podcast`
/// Auth:     None (public API)
/// Limits:   20 RPM (conservative)
pub struct ApplePodcastsProvider {
    client: Client,
    base_url: String,
    /// Whether the provider should respond to queries (test hook).
    pub(crate) enabled: bool,
    country: String,
}

impl ApplePodcastsProvider {
    /// Create a new Apple Podcasts provider for the given country code (ISO 3166-1 alpha-2).
    pub fn new(country: impl Into<String>) -> Self {
        Self::with_base_url(country, "https://itunes.apple.com")
    }

    /// Create a provider with a custom base URL (for test mocking).
    pub fn with_base_url(country: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            enabled: true,
            country: country.into(),
        }
    }

    /// Parse an iTunes podcast search response into `ProviderResult`s.
    pub(crate) fn parse_podcasts(
        provider_name: &str,
        body: &str,
    ) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesPodcastResponse {
            results: Vec<ItunesPodcastResult>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesPodcastResult {
            collection_id: Option<u64>,
            collection_name: Option<String>,
            artist_name: Option<String>,
            artwork_url600: Option<String>,
            artwork_url100: Option<String>,
            release_date: Option<String>,
            primary_genre_name: Option<String>,
            track_count: Option<u32>,
            feed_url: Option<String>,
            collection_view_url: Option<String>,
        }

        let resp: ItunesPodcastResponse = serde_json::from_str(body).map_err(|e| {
            ProviderError::Other(format!("parse error: Apple Podcasts response: {e}"))
        })?;

        let results = resp
            .results
            .into_iter()
            .map(|r| {
                // Prefer 600px cover, fall back to 100px
                let mut cover_art = Vec::new();
                if let Some(url) = &r.artwork_url600 {
                    cover_art.push(CoverArtInfo {
                        url: url.clone(),
                        width: Some(600),
                        height: Some(600),
                        mime_type: Some("image/jpeg".into()),
                    });
                }
                if let Some(url) = &r.artwork_url100 {
                    cover_art.push(CoverArtInfo {
                        url: url.clone(),
                        width: Some(100),
                        height: Some(100),
                        mime_type: Some("image/jpeg".into()),
                    });
                }

                let year = r
                    .release_date
                    .as_deref()
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());

                let mut result = ProviderResult::new(provider_name);
                result.title = r.collection_name; // Podcast name
                result.artist = r.artist_name; // Podcast author / network
                result.genre = r.primary_genre_name;
                result.year = year;
                result.cover_art = cover_art;

                // Provider-specific identifiers go into metadata
                if let Some(id) = r.collection_id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id.to_string()));
                }
                if let Some(feed) = &r.feed_url {
                    result
                        .metadata
                        .insert("feed_url".into(), Value::String(feed.clone()));
                }
                if let Some(view_url) = &r.collection_view_url {
                    result
                        .metadata
                        .insert("podcast_url".into(), Value::String(view_url.clone()));
                }
                if let Some(count) = r.track_count {
                    result
                        .metadata
                        .insert("episode_count".into(), Value::Number(count.into()));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

impl Default for ApplePodcastsProvider {
    fn default() -> Self {
        Self::new("US")
    }
}

#[async_trait]
impl MetadataProvider for ApplePodcastsProvider {
    fn id(&self) -> &str {
        "apple_podcasts"
    }

    fn display_name(&self) -> &str {
        "Apple Podcasts"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            music_search: false,
            video_search: false,
            podcast_search: true,
            cover_art: true,
            lyrics: false,
            fingerprint_lookup: false,
            identifier_lookup: false,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
        if !self.enabled {
            return Err(ProviderError::NotConfigured("apple_podcasts".into()));
        }

        // Build a free-text term from title or artist (no upstream `query` field).
        let fallback = format!(
            "{} {}",
            query.title.as_deref().unwrap_or(""),
            query.artist.as_deref().unwrap_or("")
        );
        let fallback_trimmed = fallback.trim();
        let term: &str = query
            .title
            .as_deref()
            .or(query.artist.as_deref())
            .unwrap_or(fallback_trimmed);

        debug!(
            provider = "apple_podcasts",
            term = term,
            "Sending iTunes podcast search request"
        );

        let limit = query.max_results.unwrap_or(20).to_string();
        let url = format!("{}/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[
                ("term", term),
                ("media", "podcast"),
                ("entity", "podcast"),
                ("country", &self.country),
                ("limit", &limit),
            ])
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::NetworkError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;
        Self::parse_podcasts("apple_podcasts", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_podcasts_name() {
        assert_eq!(ApplePodcastsProvider::new("US").id(), "apple_podcasts");
    }

    #[test]
    fn apple_podcasts_display_name() {
        assert_eq!(
            ApplePodcastsProvider::new("US").display_name(),
            "Apple Podcasts"
        );
    }

    #[test]
    fn apple_podcasts_capabilities_podcast() {
        let caps = ApplePodcastsProvider::new("US").capabilities();
        assert!(caps.podcast_search);
        assert!(!caps.music_search);
        assert!(caps.cover_art);
    }

    #[test]
    fn apple_podcasts_parse_valid_json() {
        let json = r#"{
            "results": [{
                "collectionId": 12345678,
                "collectionName": "The Daily",
                "artistName": "The New York Times",
                "artworkUrl600": "https://is1.mzstatic.com/600x600.jpg",
                "artworkUrl100": "https://is1.mzstatic.com/100x100.jpg",
                "releaseDate": "2024-01-15T00:00:00Z",
                "primaryGenreName": "News",
                "trackCount": 2500,
                "feedUrl": "https://feeds.nytimes.com/thedaily",
                "collectionViewUrl": "https://podcasts.apple.com/us/podcast/the-daily/id1200361736"
            }]
        }"#;
        let results = ApplePodcastsProvider::parse_podcasts("apple_podcasts", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("The Daily"));
        assert_eq!(results[0].artist.as_deref(), Some("The New York Times"));
        assert_eq!(results[0].year, Some(2024));
        assert_eq!(results[0].genre.as_deref(), Some("News"));
        assert_eq!(
            results[0]
                .metadata
                .get(PROVIDER_ID)
                .and_then(|v| v.as_str()),
            Some("12345678")
        );
        // Both cover art sizes present
        assert_eq!(results[0].cover_art.len(), 2);
        // Extra fields stored in metadata
        assert!(results[0].metadata.contains_key("feed_url"));
        assert!(results[0].metadata.contains_key("episode_count"));
        assert_eq!(
            results[0]
                .metadata
                .get("episode_count")
                .and_then(serde_json::Value::as_u64),
            Some(2500)
        );
    }

    #[test]
    fn apple_podcasts_parse_empty_results() {
        let json = r#"{"results": []}"#;
        let results = ApplePodcastsProvider::parse_podcasts("apple_podcasts", json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn apple_podcasts_parse_invalid_json_returns_err() {
        let result = ApplePodcastsProvider::parse_podcasts("apple_podcasts", "bad json");
        assert!(matches!(result, Err(ProviderError::Other(_))));
    }

    #[test]
    fn apple_podcasts_parse_600px_art_preferred() {
        let json = r#"{
            "results": [{
                "artworkUrl600": "https://x.com/big.jpg",
                "artworkUrl100": "https://x.com/small.jpg"
            }]
        }"#;
        let results = ApplePodcastsProvider::parse_podcasts("apple_podcasts", json).unwrap();
        let largest = results[0]
            .cover_art
            .iter()
            .max_by_key(|a| u64::from(a.width.unwrap_or(0)) * u64::from(a.height.unwrap_or(0)))
            .unwrap();
        assert_eq!(largest.width, Some(600));
    }

    #[test]
    fn apple_podcasts_parse_missing_artwork_produces_no_cover_art() {
        let json = r#"{"results": [{"collectionName": "My Podcast"}]}"#;
        let results = ApplePodcastsProvider::parse_podcasts("apple_podcasts", json).unwrap();
        assert!(results[0].cover_art.is_empty());
    }

    #[test]
    fn apple_podcasts_parse_no_feed_url_skips_extra() {
        let json = r#"{"results": [{"collectionName": "Podcast"}]}"#;
        let results = ApplePodcastsProvider::parse_podcasts("apple_podcasts", json).unwrap();
        assert!(!results[0].metadata.contains_key("feed_url"));
    }

    #[tokio::test]
    async fn apple_podcasts_search_disabled_returns_err() {
        let mut p = ApplePodcastsProvider::new("US");
        p.enabled = false;
        let q = SearchQuery {
            title: Some("Test".into()),
            max_results: Some(5),
            ..Default::default()
        };
        assert!(matches!(
            p.search(&q).await,
            Err(ProviderError::NotConfigured(_))
        ));
    }
}
