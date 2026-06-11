// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Deezer metadata provider.
// Ported from MeedyaManager crates/mm-providers/src/music/mod.rs
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

fn search_term(query: &SearchQuery) -> String {
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

/// Searches the Deezer public API (no auth required).
///
/// Endpoint: `https://api.deezer.com/search`
/// Auth:     None
/// Limits:   50 RPM
pub struct DeezerProvider {
    client: Client,
    base_url: String,
    enabled: bool,
}

impl DeezerProvider {
    pub fn new() -> Self {
        Self::with_base_url("https://api.deezer.com")
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            enabled: true,
        }
    }

    fn parse_deezer(provider_name: &str, body: &str) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        struct DeezerResponse {
            data: Vec<DeezerTrack>,
        }
        #[derive(Deserialize)]
        struct DeezerTrack {
            id: Option<u64>,
            title: Option<String>,
            artist: Option<DeezerArtist>,
            album: Option<DeezerAlbum>,
            duration: Option<u64>,
            isrc: Option<String>,
            explicit_lyrics: Option<bool>,
            rank: Option<u64>,
        }
        #[derive(Deserialize)]
        struct DeezerArtist {
            name: Option<String>,
        }
        #[derive(Deserialize)]
        struct DeezerAlbum {
            title: Option<String>,
            cover_xl: Option<String>,
            cover_medium: Option<String>,
        }

        let resp: DeezerResponse =
            serde_json::from_str(body).map_err(|e| parse_err("Deezer response", e))?;

        let results = resp
            .data
            .into_iter()
            .map(|t| {
                let mut cover_art = Vec::new();
                if let Some(xl) = t.album.as_ref().and_then(|a| a.cover_xl.as_deref()) {
                    cover_art.push(CoverArtInfo {
                        url: xl.to_owned(),
                        width: Some(1000),
                        height: Some(1000),
                        mime_type: Some("image/jpeg".into()),
                    });
                }
                if let Some(med) = t.album.as_ref().and_then(|a| a.cover_medium.as_deref()) {
                    cover_art.push(CoverArtInfo {
                        url: med.to_owned(),
                        width: Some(250),
                        height: Some(250),
                        mime_type: Some("image/jpeg".into()),
                    });
                }

                // Deezer rank is up to ~100_000; normalise to [0.0, 1.0]
                let score = t
                    .rank
                    .map_or(0.5, |r| (r as f64 / 100_000.0).clamp(0.0, 1.0));

                let content_advisory = if t.explicit_lyrics.unwrap_or(false) {
                    "explicit"
                } else {
                    "clean"
                };

                let mut result = ProviderResult::new(provider_name);
                result.title = t.title;
                result.artist = t.artist.and_then(|a| a.name);
                result.album = t.album.and_then(|a| a.title);
                result.isrc = t.isrc;
                result.score = score;
                result.cover_art = cover_art;
                result.metadata.insert(
                    CONTENT_ADVISORY.into(),
                    Value::String(content_advisory.into()),
                );
                if let Some(id) = t.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id.to_string()));
                }
                if let Some(secs) = t.duration {
                    insert_duration(&mut result, secs as f64);
                }

                result
            })
            .collect();

        Ok(results)
    }
}

impl Default for DeezerProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for DeezerProvider {
    fn id(&self) -> &str {
        "deezer"
    }

    fn display_name(&self) -> &str {
        "Deezer"
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
            return Err(ProviderError::NotConfigured("deezer".into()));
        }

        // Deezer supports ISRC lookup via `/track/isrc:<isrc>`
        let url = if let Some(isrc) = &query.isrc {
            format!("{}/track/isrc:{isrc}", self.base_url)
        } else {
            format!("{}/search", self.base_url)
        };

        let q = if query.isrc.is_some() {
            None
        } else {
            let term = if let (Some(t), Some(a)) = (&query.title, &query.artist) {
                format!("{t} {a}")
            } else {
                search_term(query)
            };
            Some(term)
        };

        debug!(provider = "deezer", query = ?q, "Sending search request");

        let mut req = self.client.get(&url);
        if let Some(q) = &q {
            let limit = query.max_results.unwrap_or(10).to_string();
            req = req.query(&[("q", q.as_str()), ("limit", &limit)]);
        }

        let response = req.send().await.map_err(net_err)?;

        if !response.status().is_success() {
            return Err(ProviderError::NetworkError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body = response.text().await.map_err(net_err)?;

        // ISRC lookup returns a single track object; wrap it
        if query.isrc.is_some() {
            let wrapped = format!("{{\"data\":[{body}]}}");
            Self::parse_deezer("deezer", &wrapped)
        } else {
            Self::parse_deezer("deezer", &body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deezer_name() {
        let p = DeezerProvider::new();
        assert_eq!(p.id(), "deezer");
    }

    #[test]
    fn deezer_capabilities_music_search() {
        let p = DeezerProvider::new();
        assert!(p.capabilities().music_search);
    }

    #[test]
    fn deezer_parse_valid_json() {
        let json = r#"{
            "data": [{
                "id": 9876,
                "title": "Get Lucky",
                "artist": {"name": "Daft Punk"},
                "album": {
                    "title": "Random Access Memories",
                    "cover_xl": "https://cdn.deezer.com/xl.jpg",
                    "cover_medium": "https://cdn.deezer.com/med.jpg"
                },
                "duration": 248,
                "isrc": "GBUM71300400",
                "explicit_lyrics": false,
                "rank": 850000
            }]
        }"#;
        let results = DeezerProvider::parse_deezer("deezer", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Get Lucky"));
        assert_eq!(results[0].artist.as_deref(), Some("Daft Punk"));
        assert_eq!(results[0].isrc.as_deref(), Some("GBUM71300400"));
        let duration = results[0]
            .metadata
            .get(DURATION_SECS)
            .and_then(serde_json::Value::as_f64)
            .unwrap();
        assert!((duration - 248.0).abs() < 1e-3);
        assert_eq!(results[0].cover_art.len(), 2);
    }

    #[test]
    fn deezer_parse_empty_data() {
        let json = r#"{"data": []}"#;
        let results = DeezerProvider::parse_deezer("deezer", json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn deezer_parse_invalid_json_returns_err() {
        let result = DeezerProvider::parse_deezer("deezer", "bad");
        assert!(matches!(result, Err(ProviderError::Other(_))));
    }
}
