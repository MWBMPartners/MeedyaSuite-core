// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// MeedyaDB API client.
// Extracted from MeedyaConverter MetadataProviders.swift (MeedyaDBClient).
//
// Base URL: https://api.meedya.tv/v1
// Auth: X-API-Key header

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::DbError;

/// MeedyaDB API base URL.
const DEFAULT_BASE_URL: &str = "https://api.meedya.tv/v1";

/// API search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: u64,
}

/// A single search result from the MeedyaDB API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub media_type: String,
    pub artist: Option<String>,
    pub year: Option<u16>,
    pub cover_art_url: Option<String>,
    pub confidence: Option<f64>,
}

/// Client for the MeedyaDB API.
pub struct MeedyaDbClient {
    base_url: String,
    api_key: String,
    http_client: reqwest::Client,
}

impl MeedyaDbClient {
    /// Create a new client with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Create a client with a custom base URL (for testing).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            http_client: reqwest::Client::builder()
                .user_agent("MeedyaSuite/1.0")
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Search for media by query string.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        media_type: Option<&str>,
    ) -> Result<SearchResponse, DbError> {
        let url = format!("{}/search", self.base_url);
        let mut params = vec![
            ("q", query.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(mt) = media_type {
            params.push(("type", mt.to_string()));
        }

        let resp = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&params)
            .send()
            .await
            .map_err(|e| DbError::NetworkError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(DbError::AuthError("Invalid API key".into()));
        }

        if !resp.status().is_success() {
            return Err(DbError::ApiError(format!("HTTP {}", resp.status())));
        }

        resp.json()
            .await
            .map_err(|e| DbError::SerializationError(e.to_string()))
    }

    /// Look up a specific media item by ID.
    pub async fn get_media(&self, media_id: &str) -> Result<serde_json::Value, DbError> {
        let url = format!("{}/media/{}", self.base_url, media_id);

        let resp = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| DbError::NetworkError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(DbError::NotFound(media_id.into()));
        }

        if !resp.status().is_success() {
            return Err(DbError::ApiError(format!("HTTP {}", resp.status())));
        }

        resp.json()
            .await
            .map_err(|e| DbError::SerializationError(e.to_string()))
    }

    /// Match media by filename (fuzzy matching).
    pub async fn match_by_filename(
        &self,
        filename: &str,
    ) -> Result<SearchResponse, DbError> {
        let url = format!("{}/match", self.base_url);

        let resp = self
            .http_client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&[("filename", filename)])
            .send()
            .await
            .map_err(|e| DbError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DbError::ApiError(format!("HTTP {}", resp.status())));
        }

        resp.json()
            .await
            .map_err(|e| DbError::SerializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_result_deserialization() {
        let json = r#"{
            "results": [
                {
                    "id": "m123",
                    "title": "Midnights",
                    "media_type": "album",
                    "artist": "Taylor Swift",
                    "year": 2022,
                    "cover_art_url": null,
                    "confidence": 0.95
                }
            ],
            "total": 1
        }"#;
        let response: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].title, "Midnights");
        assert_eq!(response.total, 1);
    }
}
