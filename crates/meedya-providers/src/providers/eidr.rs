// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// EIDR identifier provider.
// Ported from MeedyaManager crates/mm-providers/src/identifiers/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{EIDR, PROVIDER_ID};
use crate::traits::{MetadataProvider, ProviderCapabilities, ProviderError};
use crate::types::{ProviderResult, SearchQuery};

fn net_err(e: reqwest::Error) -> ProviderError {
    ProviderError::NetworkError(e.to_string())
}

fn parse_err(context: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Other(format!("parse error: {context}: {e}"))
}

/// Validate EIDR format: `10.5240/XXXX-XXXX-XXXX-XXXX-XXXX-C` (DOI-based).
pub fn validate_eidr(eidr: &str) -> bool {
    // Must start with the EIDR DOI prefix
    eidr.starts_with("10.5240/") && eidr.len() > 10
}

/// Looks up EIDR (Entertainment Identifier Registry) titles for video content.
///
/// Endpoint: `https://id.eidr.org/EIDR/object/<DOI>`
/// Auth:     Basic auth (EIDR registry account required)
/// Limits:   10 RPM (paid API)
pub struct EidrProvider {
    client: Client,
    base_url: String,
    username: Option<String>,
    password: Option<String>,
}

impl EidrProvider {
    pub fn new(username: Option<String>, password: Option<String>) -> Self {
        Self::with_base_url(username, password, "https://id.eidr.org")
    }

    pub fn with_base_url(
        username: Option<String>,
        password: Option<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            username,
            password,
        }
    }

    /// True if both username and password are present.
    fn configured(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }

    /// Parse an EIDR JSON response into a single `ProviderResult`.
    fn parse_eidr_json(
        provider_name: &str,
        body: &str,
    ) -> Result<Vec<ProviderResult>, ProviderError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct EidrRecord {
            #[serde(rename = "ID")]
            id: Option<String>,
            #[serde(rename = "ResourceName")]
            resource_name: Option<EidrLocalizedName>,
            #[serde(rename = "ReleaseDate")]
            release_date: Option<String>,
            #[serde(rename = "ExtraObjectMetadata")]
            extra: Option<EidrExtra>,
        }

        #[derive(Deserialize)]
        struct EidrLocalizedName {
            value: Option<String>,
        }

        #[derive(Deserialize)]
        struct EidrExtra {
            movie: Option<EidrMovie>,
        }

        #[derive(Deserialize)]
        struct EidrMovie {
            directors: Option<Vec<String>>,
        }

        let record: EidrRecord =
            serde_json::from_str(body).map_err(|e| parse_err("EIDR response", e))?;

        let year = record
            .release_date
            .as_deref()
            .and_then(|d| d[..4.min(d.len())].parse::<u32>().ok());

        let director = record
            .extra
            .as_ref()
            .and_then(|e| e.movie.as_ref())
            .and_then(|m| m.directors.as_deref())
            .and_then(|d| d.first())
            .cloned();

        let mut result = ProviderResult::new(provider_name);
        result.title = record.resource_name.and_then(|n| n.value);
        result.artist = director; // Director for film
        result.year = year;

        if let Some(id) = record.id {
            result
                .metadata
                .insert(PROVIDER_ID.into(), Value::String(id.clone()));
            result.metadata.insert(EIDR.into(), Value::String(id));
        }

        Ok(vec![result])
    }
}

#[async_trait]
impl MetadataProvider for EidrProvider {
    fn id(&self) -> &str {
        "eidr"
    }

    fn display_name(&self) -> &str {
        "EIDR"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            music_search: false,
            video_search: true,
            podcast_search: false,
            cover_art: false,
            lyrics: false,
            fingerprint_lookup: false,
            identifier_lookup: true,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
        if !self.configured() {
            return Err(ProviderError::NotConfigured("eidr".into()));
        }

        let eidr = query.eidr.as_deref().ok_or_else(|| {
            ProviderError::NotSupported("eidr: EIDR query requires an EIDR DOI".into())
        })?;

        if !validate_eidr(eidr) {
            return Err(ProviderError::Other(format!(
                "parse error: Invalid EIDR format: {eidr}"
            )));
        }

        debug!(
            provider = "eidr",
            eidr = eidr,
            "Sending EIDR lookup request"
        );

        let url = format!("{}/EIDR/object/{}", self.base_url, eidr);
        let response = self
            .client
            .get(&url)
            .basic_auth(self.username.as_deref().unwrap(), self.password.as_deref())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(net_err)?;

        if !response.status().is_success() {
            let s = response.status();
            if s.as_u16() == 401 {
                return Err(ProviderError::AuthenticationFailed {
                    provider: "eidr".into(),
                    reason: "Invalid EIDR credentials".into(),
                });
            }
            return Err(ProviderError::NetworkError(format!("HTTP {s}")));
        }

        let body = response.text().await.map_err(net_err)?;
        Self::parse_eidr_json("eidr", &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_eidr_valid() {
        assert!(validate_eidr("10.5240/AEBE-0317-CE0D-4943-5916-E"));
    }

    #[test]
    fn validate_eidr_wrong_prefix() {
        assert!(!validate_eidr("10.1000/AEBE-0317-CE0D-4943-5916-E"));
    }

    #[test]
    fn validate_eidr_too_short() {
        assert!(!validate_eidr("10.5240/"));
    }

    #[test]
    fn eidr_provider_name() {
        assert_eq!(EidrProvider::new(None, None).id(), "eidr");
    }

    #[test]
    fn eidr_provider_capabilities() {
        let caps = EidrProvider::new(None, None).capabilities();
        assert!(caps.identifier_lookup);
        assert!(caps.video_search);
    }

    #[test]
    fn eidr_provider_parse_json_valid() {
        let json = r#"{
            "ID": "10.5240/AEBE-0317-CE0D-4943-5916-E",
            "ResourceName": {"value": "Inception"},
            "ReleaseDate": "2010-07-16",
            "ExtraObjectMetadata": {
                "movie": {"directors": ["Christopher Nolan"]}
            }
        }"#;
        let results = EidrProvider::parse_eidr_json("eidr", json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Inception"));
        assert_eq!(results[0].year, Some(2010));
        assert_eq!(results[0].artist.as_deref(), Some("Christopher Nolan"));
        // EIDR is now stored in metadata
        assert_eq!(
            results[0]
                .metadata
                .get(EIDR)
                .and_then(serde_json::Value::as_str),
            Some("10.5240/AEBE-0317-CE0D-4943-5916-E")
        );
    }
}
