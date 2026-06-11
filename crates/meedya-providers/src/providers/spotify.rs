// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Spotify provider (Client Credentials OAuth2).
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

/// Searches the Spotify Web API using Client Credentials OAuth2.
///
/// Endpoint: `https://api.spotify.com/v1/search`
/// Auth:     OAuth2 client-credentials (`client_id` + `client_secret`)
/// Limits:   100 RPM (standard tier)
pub struct SpotifyProvider {
    client: Client,
    base_url: String,
    client_id: Option<String>,
    client_secret: Option<String>,
}

impl SpotifyProvider {
    /// Create a Spotify provider. `client_id` and `client_secret` are optional;
    /// the provider is disabled if either is `None`.
    pub fn new(client_id: Option<String>, client_secret: Option<String>) -> Self {
        Self::with_base_url(client_id, client_secret, "https://api.spotify.com")
    }

    /// Create a Spotify provider with a custom base URL (for test mocking).
    pub fn with_base_url(
        client_id: Option<String>,
        client_secret: Option<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            client_id,
            client_secret,
        }
    }

    fn configured(&self) -> bool {
        self.client_id.is_some() && self.client_secret.is_some()
    }

    /// Obtain an access token using Client Credentials OAuth2.
    async fn get_access_token(&self) -> Result<String, ProviderError> {
        let id = self
            .client_id
            .as_deref()
            .ok_or_else(|| ProviderError::AuthenticationFailed {
                provider: "spotify".into(),
                reason: "No client_id".into(),
            })?;
        let secret =
            self.client_secret
                .as_deref()
                .ok_or_else(|| ProviderError::AuthenticationFailed {
                    provider: "spotify".into(),
                    reason: "No client_secret".into(),
                })?;

        let resp = self
            .client
            .post("https://accounts.spotify.com/api/token")
            .basic_auth(id, Some(secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await
            .map_err(net_err)?;

        if !resp.status().is_success() {
            return Err(ProviderError::AuthenticationFailed {
                provider: "spotify".into(),
                reason: format!("Token request failed: HTTP {}", resp.status()),
            });
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
        }
        let token: TokenResponse = resp
            .json()
            .await
            .map_err(|e| parse_err("Spotify token", e))?;
        Ok(token.access_token)
    }

    /// Parse a Spotify track search response into `ProviderResult`s.
    fn parse_tracks(provider_name: &str, body: &str) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        struct SpotifySearchResponse {
            tracks: Option<SpotifyTrackPage>,
        }
        #[derive(Deserialize)]
        struct SpotifyTrackPage {
            items: Vec<SpotifyTrack>,
        }
        #[derive(Deserialize)]
        struct SpotifyTrack {
            id: Option<String>,
            name: Option<String>,
            artists: Option<Vec<SpotifyArtist>>,
            album: Option<SpotifyAlbum>,
            duration_ms: Option<u64>,
            explicit: Option<bool>,
            external_ids: Option<SpotifyExternalIds>,
            popularity: Option<u32>,
        }
        #[derive(Deserialize)]
        struct SpotifyArtist {
            name: Option<String>,
        }
        #[derive(Deserialize)]
        struct SpotifyAlbum {
            name: Option<String>,
            release_date: Option<String>,
            images: Option<Vec<SpotifyImage>>,
        }
        #[derive(Deserialize)]
        struct SpotifyImage {
            url: String,
            width: Option<u32>,
            height: Option<u32>,
        }
        #[derive(Deserialize)]
        struct SpotifyExternalIds {
            isrc: Option<String>,
        }

        let resp: SpotifySearchResponse =
            serde_json::from_str(body).map_err(|e| parse_err("Spotify search", e))?;

        let tracks = resp.tracks.map(|p| p.items).unwrap_or_default();

        let results = tracks
            .into_iter()
            .map(|track| {
                let artist = track.artists.as_deref().map(|artists| {
                    artists
                        .iter()
                        .filter_map(|a| a.name.as_deref())
                        .collect::<Vec<_>>()
                        .join("; ")
                });
                let album_name = track.album.as_ref().and_then(|a| a.name.clone());
                let year = track
                    .album
                    .as_ref()
                    .and_then(|a| a.release_date.as_deref())
                    .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());
                let cover_art = track
                    .album
                    .as_ref()
                    .and_then(|a| a.images.as_deref())
                    .map(|imgs| {
                        imgs.iter()
                            .map(|img| CoverArtInfo {
                                url: img.url.clone(),
                                width: img.width,
                                height: img.height,
                                mime_type: Some("image/jpeg".into()),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let isrc = track.external_ids.and_then(|ids| ids.isrc);
                // Normalise Spotify popularity 0–100 to [0.0, 1.0]
                let score = f64::from(track.popularity.unwrap_or(0)) / 100.0;
                let content_advisory = if track.explicit.unwrap_or(false) {
                    "explicit"
                } else {
                    "clean"
                };

                let mut result = ProviderResult::new(provider_name);
                result.title = track.name;
                result.artist = artist;
                result.album = album_name;
                result.year = year;
                result.isrc = isrc;
                result.score = score;
                result.cover_art = cover_art;
                result.metadata.insert(
                    CONTENT_ADVISORY.into(),
                    Value::String(content_advisory.into()),
                );
                if let Some(id) = track.id {
                    result
                        .metadata
                        .insert(PROVIDER_ID.into(), Value::String(id));
                }
                if let Some(ms) = track.duration_ms {
                    insert_duration(&mut result, ms as f64 / 1000.0);
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl MetadataProvider for SpotifyProvider {
    fn id(&self) -> &str {
        "spotify"
    }

    fn display_name(&self) -> &str {
        "Spotify"
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
        if !self.configured() {
            return Err(ProviderError::NotConfigured("spotify".into()));
        }

        let token = self.get_access_token().await?;

        // Build Spotify search query
        let sp_query = if let Some(isrc) = &query.isrc {
            format!("isrc:{isrc}")
        } else {
            let mut parts = Vec::new();
            if let Some(title) = &query.title {
                parts.push(format!("track:{title}"));
            }
            if let Some(artist) = &query.artist {
                parts.push(format!("artist:{artist}"));
            }
            if parts.is_empty() {
                search_term(query)
            } else {
                parts.join(" ")
            }
        };

        let url = format!("{}/v1/search", self.base_url);
        debug!(
            provider = "spotify",
            query = &sp_query,
            "Sending search request"
        );

        let limit = query.max_results.unwrap_or(10).to_string();
        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[
                ("q", &sp_query),
                ("type", &"track".to_owned()),
                ("limit", &limit),
            ])
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 429 {
                return Err(ProviderError::RateLimited("spotify".into()));
            }
            return Err(ProviderError::NetworkError(format!("HTTP {status}")));
        }

        let body = response.text().await.map_err(net_err)?;
        Self::parse_tracks("spotify", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_name() {
        let p = SpotifyProvider::new(Some("id".into()), Some("secret".into()));
        assert_eq!(p.id(), "spotify");
    }

    #[test]
    fn spotify_capabilities_provides_cover_art() {
        let p = SpotifyProvider::new(None, None);
        assert!(p.capabilities().cover_art);
    }

    #[test]
    fn spotify_capabilities_music_search() {
        let p = SpotifyProvider::new(None, None);
        assert!(p.capabilities().music_search);
    }

    #[test]
    fn spotify_parse_tracks_valid_json() {
        let json = r#"{
            "tracks": {
                "items": [{
                    "id": "sp123",
                    "name": "Bohemian Rhapsody",
                    "artists": [{"name": "Queen"}],
                    "album": {
                        "name": "A Night at the Opera",
                        "release_date": "1975-11-21",
                        "images": [{"url": "https://img.spotify.com/big.jpg", "width": 640, "height": 640}]
                    },
                    "duration_ms": 354000,
                    "explicit": false,
                    "external_ids": {"isrc": "GBUM71505078"},
                    "popularity": 90
                }]
            }
        }"#;
        let results = SpotifyProvider::parse_tracks("spotify", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(results[0].artist.as_deref(), Some("Queen"));
        assert_eq!(results[0].album.as_deref(), Some("A Night at the Opera"));
        assert_eq!(results[0].year, Some(1975));
        assert_eq!(results[0].isrc.as_deref(), Some("GBUM71505078"));
        assert!((results[0].score - 0.9).abs() < 1e-9);
        assert!(!results[0].cover_art.is_empty());
    }

    #[test]
    fn spotify_parse_tracks_empty() {
        let json = r#"{"tracks": {"items": []}}"#;
        let results = SpotifyProvider::parse_tracks("spotify", json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn spotify_parse_tracks_invalid_json() {
        let result = SpotifyProvider::parse_tracks("spotify", "bad json");
        assert!(matches!(result, Err(ProviderError::Other(_))));
    }

    #[test]
    fn spotify_parse_explicit_track_flagged() {
        let json =
            r#"{"tracks": {"items": [{"id": "x","name": "T","explicit": true,"popularity": 0}]}}"#;
        let results = SpotifyProvider::parse_tracks("spotify", json).unwrap();
        assert_eq!(
            results[0]
                .metadata
                .get(CONTENT_ADVISORY)
                .and_then(serde_json::Value::as_str),
            Some("explicit")
        );
    }
}
