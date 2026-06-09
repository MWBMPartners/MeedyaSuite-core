// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// MusicBrainz metadata provider.
// Ported from MeedyaManager crates/mm-providers/src/music/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{DURATION_SECS, PROVIDER_ID};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{ProviderResult, SearchQuery};

/// Build a `ProviderError::NetworkError` from a `reqwest::Error`.
fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

/// Build a parse-style `ProviderError::Other`.
fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

/// Resolve a free-text search term from `SearchQuery` (title + artist).
fn search_term(query: &SearchQuery) -> String {
    let combined = format!(
        "{} {}",
        query.title.as_deref().unwrap_or(""),
        query.artist.as_deref().unwrap_or("")
    );
    combined.trim().to_owned()
}

/// Insert duration (seconds) into result metadata using the conventional key.
fn insert_duration(result: &mut ProviderResult, secs: f64) {
    if let Some(num) = serde_json::Number::from_f64(secs) {
        result
            .metadata
            .insert(DURATION_SECS.into(), Value::Number(num));
    }
}

/// Searches the MusicBrainz open database.
///
/// Endpoint: `https://musicbrainz.org/ws/2/recording/`
/// Auth:     None required (but a User-Agent string is required)
/// Limits:   50 RPM (free tier)
pub struct MusicBrainzProvider {
    client: Client,
    base_url: String,
    /// Required by MusicBrainz API: identifies the application making requests.
    #[allow(dead_code)]
    user_agent: String,
}

impl MusicBrainzProvider {
    /// Create a provider with the standard MusicBrainz endpoint.
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self::with_base_url(user_agent, "https://musicbrainz.org")
    }

    /// Create a provider with a custom base URL (useful for test mocking).
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

    /// True when a User-Agent string is configured. Required by MusicBrainz API.
    fn configured(&self) -> bool {
        !self.user_agent.is_empty()
    }

    /// Parse a MusicBrainz recording search response into `ProviderResult`s.
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
            artist_credit: Option<Vec<MbArtistCredit>>,
            releases: Option<Vec<MbRelease>>,
            isrcs: Option<Vec<String>>,
            length: Option<u64>,
            score: Option<u32>,
        }

        #[derive(Deserialize)]
        struct MbArtistCredit {
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
            #[serde(rename = "track-count")]
            #[allow(dead_code)]
            track_count: Option<u32>,
        }

        let resp: MbResponse =
            serde_json::from_str(body).map_err(|e| parse_err("MusicBrainz response", e))?;

        let results = resp
            .recordings
            .into_iter()
            .map(|rec| {
                // Combine artist-credit names
                let artist = rec.artist_credit.as_deref().map(|credits| {
                    credits
                        .iter()
                        .filter_map(|c| c.artist.as_ref()?.name.as_deref())
                        .collect::<Vec<_>>()
                        .join("; ")
                });

                // Use the first release for album/year info
                let first_release = rec.releases.as_deref().and_then(|r| r.first());
                let album = first_release.and_then(|r| r.title.clone());
                let year = first_release
                    .and_then(|r| r.date.as_deref())
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());

                // MusicBrainz score is 0–100; normalise to [0.0, 1.0]
                let score = f64::from(rec.score.unwrap_or(0)) / 100.0;

                let mut result = ProviderResult::new(provider_name);
                result.title = rec.title;
                result.artist = artist;
                result.album = album;
                result.year = year;
                result.isrc = rec.isrcs.and_then(|v| v.into_iter().next());
                result.score = score;

                if let Some(id) = rec.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id));
                }
                if let Some(ms) = rec.length {
                    insert_duration(&mut result, ms as f64 / 1000.0);
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for MusicBrainzProvider {
    fn id(&self) -> &str {
        "musicbrainz"
    }

    fn display_name(&self) -> &str {
        "MusicBrainz"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            music_search: true,
            video_search: false,
            podcast_search: false,
            // Cover art comes via the Cover Art Archive (a separate provider).
            cover_art: false,
            lyrics: false,
            fingerprint_lookup: false,
            identifier_lookup: false,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
        if !self.configured() {
            return Err(ProviderError::NotConfigured("musicbrainz".into()));
        }

        // Build query string: ISRC takes priority over free-text
        let lucene_query = if let Some(isrc) = &query.isrc {
            format!("isrc:{isrc}")
        } else {
            let mut parts = Vec::new();
            if let Some(title) = &query.title {
                parts.push(format!("recording:{}", title.replace('"', "")));
            }
            if let Some(artist) = &query.artist {
                parts.push(format!("artistname:{}", artist.replace('"', "")));
            }
            if parts.is_empty() {
                search_term(query)
            } else {
                parts.join(" AND ")
            }
        };

        let url = format!("{}/ws/2/recording/", self.base_url);
        debug!(
            provider = "musicbrainz",
            query = &lucene_query,
            "Sending search request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .query(&[
                ("query", &lucene_query as &str),
                ("limit", &limit),
                ("fmt", "json"),
            ])
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 503 {
                return Err(ProviderError::RateLimited("musicbrainz".into()));
            }
            return Err(ProviderError::NetworkError(format!("HTTP {status}")));
        }

        let body = response.text().await.map_err(net_err)?;
        Self::parse_recordings("musicbrainz", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mb_name() {
        let p = MusicBrainzProvider::new("TestApp/1.0");
        assert_eq!(p.id(), "musicbrainz");
    }

    #[test]
    fn mb_capabilities_music_type() {
        let p = MusicBrainzProvider::new("TestApp/1.0");
        assert!(p.capabilities().music_search);
        assert!(!p.capabilities().video_search);
    }

    #[test]
    fn mb_capabilities_no_cover_art() {
        let p = MusicBrainzProvider::new("TestApp/1.0");
        // MusicBrainz exposes cover art via the Cover Art Archive (a separate provider).
        assert!(!p.capabilities().cover_art);
    }

    #[test]
    fn mb_parse_recordings_valid_json() {
        let json = r#"{
            "recordings": [{
                "id": "abc123",
                "title": "Comfortably Numb",
                "artist-credit": [{"artist": {"name": "Pink Floyd"}}],
                "releases": [{"title": "The Wall", "date": "1979-11-30"}],
                "isrcs": ["GBAYE7900498"],
                "length": 382000,
                "score": 100
            }]
        }"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Comfortably Numb"));
        assert_eq!(results[0].artist.as_deref(), Some("Pink Floyd"));
        assert_eq!(results[0].album.as_deref(), Some("The Wall"));
        assert_eq!(results[0].year, Some(1979));
        assert_eq!(results[0].isrc.as_deref(), Some("GBAYE7900498"));
        assert!((results[0].score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mb_parse_recordings_empty_list() {
        let json = r#"{"recordings": []}"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn mb_parse_recordings_invalid_json_returns_err() {
        let result = MusicBrainzProvider::parse_recordings("musicbrainz", "not json");
        assert!(matches!(result, Err(ProviderError::Other(_))));
    }

    #[test]
    fn mb_parse_duration_conversion_ms_to_secs() {
        let json = r#"{"recordings": [{"id": "x", "length": 240000, "score": 50}]}"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        let duration = results[0]
            .metadata
            .get(DURATION_SECS)
            .and_then(serde_json::Value::as_f64)
            .unwrap();
        assert!((duration - 240.0).abs() < 1e-3);
    }
}
