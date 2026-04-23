use std::time::Duration;

use async_trait::async_trait;

use crate::{Lyrics, Result};

pub mod lrclib;

#[derive(Debug, Clone)]
pub struct TrackQuery {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub duration: Option<Duration>,
}

impl TrackQuery {
    pub fn new(track_name: impl Into<String>, artist_name: impl Into<String>) -> Self {
        Self {
            track_name: track_name.into(),
            artist_name: artist_name.into(),
            album_name: None,
            duration: None,
        }
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album_name = Some(album.into());
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
}

#[async_trait]
pub trait LyricsProvider: Send + Sync {
    async fn fetch(&self, query: &TrackQuery) -> Result<Lyrics>;
}
