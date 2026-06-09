// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Apple Music metadata provider (iTunes Search API).
// Ported from MeedyaManager crates/mm-providers/src/music/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{CONTENT_ADVISORY, DURATION_SECS, PROVIDER_ID, TRACK_TOTAL};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{CoverArtInfo, ProviderResult, SearchQuery};

fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

fn search_term_fallback(query: &SearchQuery) -> String {
    let combined = format!(
        "{} {}",
        query.title.as_deref().unwrap_or(""),
        query.artist.as_deref().unwrap_or("")
    );
    combined.trim().to_owned()
}

fn insert_duration(result: &mut ProviderResult, secs: f64) {
    if let Some(num) = serde_json::Number::from_f64(secs) {
        result
            .metadata
            .insert(DURATION_SECS.into(), Value::Number(num));
    }
}

/// Searches via the iTunes Search API (no auth required for basic track search).
///
/// Endpoint: `https://itunes.apple.com/search`
/// Auth:     None (JWT for full Apple Music API path remains stubbed)
/// Limits:   20 RPM (conservative; Apple does not publish limits)
pub struct AppleMusicProvider {
    client: Client,
    base_url: String,
    enabled: bool,
    country: String,
}

impl AppleMusicProvider {
    /// Create an Apple Music provider. The iTunes Search API is always available (no auth).
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

    fn parse_itunes(provider_name: &str, body: &str) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesResponse {
            results: Vec<ItunesTrack>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesTrack {
            track_id: Option<u64>,
            track_name: Option<String>,
            artist_name: Option<String>,
            collection_name: Option<String>,
            artwork_url100: Option<String>,
            release_date: Option<String>,
            track_number: Option<u32>,
            track_count: Option<u32>,
            disc_number: Option<u32>,
            primary_genre_name: Option<String>,
            track_time_millis: Option<u64>,
            explicit_ness: Option<String>,
        }

        let resp: ItunesResponse =
            serde_json::from_str(body).map_err(|e| parse_err("iTunes response", e))?;

        let results = resp
            .results
            .into_iter()
            .map(|t| {
                let cover_art = t
                    .artwork_url100
                    .as_deref()
                    .map(|url| {
                        // Replace 100x100 with higher-res variant
                        let hires = url.replace("100x100", "3000x3000");
                        vec![
                            CoverArtInfo {
                                url: hires,
                                width: Some(3000),
                                height: Some(3000),
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

                let year = t
                    .release_date
                    .as_deref()
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());

                let content_advisory = t.explicit_ness.as_deref().map(|e| {
                    if e.to_lowercase() == "explicit" {
                        "explicit"
                    } else {
                        "clean"
                    }
                    .to_owned()
                });

                let mut result = ProviderResult::new(provider_name);
                result.title = t.track_name;
                result.artist = t.artist_name;
                result.album = t.collection_name;
                result.year = year;
                result.track_number = t.track_number;
                result.disc_number = t.disc_number;
                result.genre = t.primary_genre_name;
                result.cover_art = cover_art;

                if let Some(id) = t.track_id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id.to_string()));
                }
                if let Some(total) = t.track_count {
                    result
                        .metadata
                        .insert(TRACK_TOTAL.into(), Value::Number(total.into()));
                }
                if let Some(ms) = t.track_time_millis {
                    insert_duration(&mut result, ms as f64 / 1000.0);
                }
                if let Some(advisory) = content_advisory {
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

#[async_trait]
impl MetadataProvider for AppleMusicProvider {
    fn id(&self) -> &str {
        "apple_music"
    }

    fn display_name(&self) -> &str {
        "Apple Music"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            music_search: true,
            video_search: false,
            podcast_search: false,
            cover_art: true,
            lyrics: false,
            fingerprint_lookup: false,
            identifier_lookup: false,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
        if !self.enabled {
            return Err(ProviderError::NotConfigured("apple_music".into()));
        }

        let search_term = if let Some(title) = &query.title {
            if let Some(artist) = &query.artist {
                format!("{title} {artist}")
            } else {
                title.clone()
            }
        } else {
            search_term_fallback(query)
        };

        let url = format!("{}/search", self.base_url);
        debug!(
            provider = "apple_music",
            term = &search_term,
            "Sending iTunes search request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let response = self
            .client
            .get(&url)
            .query(&[
                ("term", &search_term),
                ("media", &"music".to_owned()),
                ("entity", &"song".to_owned()),
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
        Self::parse_itunes("apple_music", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_music_name() {
        let p = AppleMusicProvider::new("US");
        assert_eq!(p.id(), "apple_music");
    }

    #[test]
    fn apple_music_capabilities_provides_cover_art() {
        let p = AppleMusicProvider::new("US");
        assert!(p.capabilities().cover_art);
    }

    #[test]
    fn apple_music_parse_itunes_valid_json() {
        let json = r#"{
            "results": [{
                "trackId": 123456,
                "trackName": "Yesterday",
                "artistName": "The Beatles",
                "collectionName": "Help!",
                "artworkUrl100": "https://is1.mzstatic.com/100x100.jpg",
                "releaseDate": "1965-08-06T00:00:00Z",
                "trackNumber": 10,
                "trackCount": 14,
                "discNumber": 1,
                "primaryGenreName": "Rock",
                "trackTimeMillis": 125000
            }]
        }"#;
        let results = AppleMusicProvider::parse_itunes("apple_music", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Yesterday"));
        assert_eq!(results[0].artist.as_deref(), Some("The Beatles"));
        assert_eq!(results[0].year, Some(1965));
        assert_eq!(results[0].genre.as_deref(), Some("Rock"));
        assert_eq!(results[0].track_number, Some(10));
        // Track total now in metadata
        assert_eq!(
            results[0]
                .metadata
                .get(TRACK_TOTAL)
                .and_then(serde_json::Value::as_u64),
            Some(14)
        );
        // Cover art: hi-res + thumbnail
        assert_eq!(results[0].cover_art.len(), 2);
    }

    #[test]
    fn apple_music_parse_hi_res_url_generated() {
        let json = r#"{
            "results": [{"artworkUrl100": "https://x.com/100x100.jpg"}]
        }"#;
        let results = AppleMusicProvider::parse_itunes("apple_music", json).unwrap();
        let largest = results[0]
            .cover_art
            .iter()
            .max_by_key(|a| u64::from(a.width.unwrap_or(0)) * u64::from(a.height.unwrap_or(0)));
        assert!(largest.unwrap().url.contains("3000x3000"));
    }

    #[test]
    fn apple_music_parse_empty_results() {
        let json = r#"{"results": []}"#;
        let results = AppleMusicProvider::parse_itunes("apple_music", json).unwrap();
        assert!(results.is_empty());
    }
}
