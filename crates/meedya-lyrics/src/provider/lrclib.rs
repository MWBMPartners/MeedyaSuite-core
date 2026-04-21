//! LRCLIB (<https://lrclib.net>) client.

use async_trait::async_trait;
use serde::Deserialize;

use crate::{lrc, Error, Lyrics, Result};

use super::{LyricsProvider, TrackQuery};

const DEFAULT_BASE: &str = "https://lrclib.net/api";
const USER_AGENT: &str = concat!(
    "MeedyaSuite-lyrics/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/MWBMPartners)"
);

pub struct LrclibProvider {
    http: reqwest::Client,
    base: String,
}

impl Default for LrclibProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LrclibProvider {
    pub fn new() -> Self {
        Self::with_base(DEFAULT_BASE)
    }

    pub fn with_base(base: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("reqwest client builds with default settings");
        Self {
            http,
            base: base.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GetResponse {
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

#[async_trait]
impl LyricsProvider for LrclibProvider {
    async fn fetch(&self, q: &TrackQuery) -> Result<Lyrics> {
        let dur_str = q.duration.map(|d| d.as_secs().to_string());
        let mut params: Vec<(&str, &str)> = vec![
            ("track_name", q.track_name.as_str()),
            ("artist_name", q.artist_name.as_str()),
        ];
        if let Some(a) = q.album_name.as_deref() {
            params.push(("album_name", a));
        }
        if let Some(s) = dur_str.as_deref() {
            params.push(("duration", s));
        }

        let resp = self
            .http
            .get(format!("{}/get", self.base))
            .query(&params)
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NotFound);
        }
        let body: GetResponse = resp.error_for_status()?.json().await?;

        let synced = body
            .synced_lyrics
            .as_deref()
            .map(lrc::parse)
            .filter(|v| !v.is_empty());

        Ok(Lyrics {
            plain: body.plain_lyrics.filter(|s| !s.is_empty()),
            synced,
        })
    }
}
