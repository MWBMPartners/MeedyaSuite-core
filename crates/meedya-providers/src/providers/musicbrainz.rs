// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// MusicBrainz metadata provider.
// Ported from MeedyaManager crates/mm-providers/src/music/mod.rs
// under MeedyaSuite-core#12 / MeedyaManager#136.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::extra_keys::{DURATION_SECS, PROVIDER_ID};
use crate::lucene::phrase_clause;
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

/// Insert duration (seconds) into result metadata using the conventional key.
fn insert_duration(result: &mut ProviderResult, secs: f64) {
    if let Some(num) = serde_json::Number::from_f64(secs) {
        result
            .metadata
            .insert(DURATION_SECS.into(), Value::Number(num));
    }
}

/// Strip trailing parenthetical / bracket groups from a free-text search term
/// before it is phrase-quoted. Tags in real libraries often carry version
/// suffixes — "(2011 Remastered Version)", "[Live]", "(feat. …)" — that are
/// absent from MusicBrainz's canonical title, which would turn the phrase
/// query into a zero-result miss. Removing a trailing group restores recall
/// for that common case.
///
/// Only a *trailing* balanced `(...)` or `[...]` group is removed (a leading
/// one, e.g. "(I Can't Get No) Satisfaction", is preserved), repeatedly while
/// the remainder stays non-empty. If stripping would empty the term (e.g.
/// "[Intro]", "(Reprise)"), the original is kept. Trade-off: this can drop a
/// parenthetical that is genuinely part of the canonical title (e.g.
/// "… (Reprise)"); accepted for the tagging use case. Live-service recall is
/// tracked for post-2026-11-30 validation in issue #69.
fn strip_trailing_bracket_groups(term: &str) -> &str {
    let mut s = term.trim();
    loop {
        let (open, close) = match s.chars().last() {
            Some(')') => ('(', ')'),
            Some(']') => ('[', ']'),
            _ => break,
        };
        let mut depth = 0i32;
        let mut opener = None;
        for (i, c) in s.char_indices().rev() {
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    opener = Some(i);
                    break;
                }
            }
        }
        match opener {
            Some(i) => {
                let candidate = s[..i].trim();
                if candidate.is_empty() {
                    break; // stripping would empty the term — keep it
                }
                s = candidate;
            }
            None => break, // unbalanced trailing closer — leave as-is
        }
    }
    s
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
            // Without an explicit timeout reqwest waits indefinitely; a
            // stalled MusicBrainz connection would hang the caller's task
            // forever. Mirrors the timeout on the ISRC/ISWC providers.
            .timeout(Duration::from_secs(30))
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

    /// Build a Lucene query string for the MusicBrainz `/recording/` search
    /// endpoint from a `SearchQuery`, escaping/quoting user-supplied values
    /// so they cannot be misparsed as Lucene syntax under Solr's (9 or 10)
    /// query parser.
    ///
    /// ISRC takes priority over free-text: when `query.isrc` is present it
    /// is normalised (alphanumerics only, uppercased) and used alone as
    /// `isrc:<CODE>` — an ISRC that doesn't normalise to exactly 12
    /// characters is rejected rather than sent upstream. Otherwise, a
    /// trailing bracket/parenthetical group is stripped from title and
    /// artist (see [`strip_trailing_bracket_groups`]) before they are
    /// combined as `recording:"..." AND artistname:"..."` (either alone if
    /// only one is present). `album` and `year`, when present, further
    /// narrow the search via the MusicBrainz recording-search `release` and
    /// `date` fields. A query with none of title, artist, or ISRC is
    /// rejected — MusicBrainz has nothing to search on (an `album`/`year`
    /// pair alone is far too broad to be useful).
    fn build_lucene_query(query: &SearchQuery) -> Result<String, ProviderError> {
        if let Some(isrc) = &query.isrc {
            let normalised: String = isrc
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .map(|c| c.to_ascii_uppercase())
                .collect();
            if normalised.len() != 12 {
                return Err(ProviderError::Other(format!(
                    "parse error: invalid ISRC: {isrc}"
                )));
            }
            return Ok(format!("isrc:{normalised}"));
        }

        let mut parts = Vec::new();
        if let Some(title) = &query.title {
            let t = strip_trailing_bracket_groups(title);
            if !t.is_empty() {
                parts.push(phrase_clause("recording", t));
            }
        }
        if let Some(artist) = &query.artist {
            let a = strip_trailing_bracket_groups(artist);
            if !a.is_empty() {
                parts.push(phrase_clause("artistname", a));
            }
        }

        // A title or artist is required; album/year only ever narrow an
        // already-anchored query, so they are appended after the emptiness
        // check below rather than counting towards it.
        if parts.is_empty() {
            return Err(ProviderError::NotSupported(
                "musicbrainz: search requires a title, artist, or ISRC".into(),
            ));
        }

        if let Some(album) = &query.album {
            // MusicBrainz's recording-search `release` field matches the
            // title of a release the recording appears on.
            let a = strip_trailing_bracket_groups(album);
            if !a.is_empty() {
                parts.push(phrase_clause("release", a));
            }
        }
        if let Some(year) = &query.year {
            // Constrains via the MB recording-search `date` field (the
            // release date of any release including this recording). This is
            // an exact-year match, not a range — a recording whose only
            // indexed release date falls outside `year` (a reissue vs. the
            // original year) will not match. `date` is a numeric/date field,
            // so the year is emitted bare rather than phrase-quoted.
            parts.push(format!("date:{year}"));
        }

        Ok(parts.join(" AND "))
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
            /// User-submitted genres, each with a vote `count`. Absent on
            /// older/uncategorised recordings.
            genres: Option<Vec<MbTag>>,
            /// Free-form folksonomy tags, each with a vote `count`. Used as
            /// a fallback genre source when `genres` is absent or empty.
            tags: Option<Vec<MbTag>>,
        }

        /// A MusicBrainz genre or tag entry: a name with a community vote
        /// count. Both fields are optional — MusicBrainz returns tags with
        /// no recorded votes as `count: 0` or omits `count` entirely.
        #[derive(Deserialize)]
        struct MbTag {
            name: Option<String>,
            count: Option<u32>,
        }

        /// Pick the `name` of the highest-`count` entry in a genre/tag list.
        /// A missing `count` ranks as `0`. When multiple entries tie for the
        /// highest count, the first one encountered is kept.
        fn top_tag(tags: &[MbTag]) -> Option<String> {
            let mut best: Option<(&str, u32)> = None;
            for tag in tags {
                let Some(name) = tag.name.as_deref() else {
                    continue;
                };
                let count = tag.count.unwrap_or(0);
                if best.is_none_or(|(_, best_count)| count > best_count) {
                    best = Some((name, count));
                }
            }
            best.map(|(name, _)| name.to_owned())
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

                // Prefer the highest-vote genre; fall back to the
                // highest-vote folksonomy tag when no genre is present.
                // Both are optional community data, so either (or both)
                // may be absent or empty. SEARCH-680/681 add genre as a
                // first-class search target but do not change this
                // recording-response shape.
                let genre = rec
                    .genres
                    .as_deref()
                    .filter(|g| !g.is_empty())
                    .and_then(top_tag)
                    .or_else(|| {
                        rec.tags
                            .as_deref()
                            .filter(|t| !t.is_empty())
                            .and_then(top_tag)
                    });

                let mut result = ProviderResult::new(provider_name);
                result.title = rec.title;
                result.artist = artist;
                result.album = album;
                result.year = year;
                result.isrc = rec.isrcs.and_then(|v| v.into_iter().next());
                result.score = score;
                result.genre = genre;

                if let Some(id) = rec.id {
                    result.musicbrainz_id = Some(id.clone());
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

        let lucene_query = Self::build_lucene_query(query)?;

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
        assert_eq!(results[0].musicbrainz_id.as_deref(), Some("abc123"));
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

    /// Forward-compat fixture: the same valid response as
    /// `mb_parse_recordings_valid_json`, plus response-shape noise the
    /// Solr 10 announcement tickets touch (recording `relations` with
    /// `target-type`, a release-level string `quality`, a `release-group`
    /// object, and an unknown `genres` array). None of these are read by
    /// our serde-derive structs, so parsing must produce identical results
    /// (SEARCH-444/666/752/751/753 don't hit us).
    #[test]
    fn mb_parse_recordings_solr10_shape() {
        let json = r#"{
            "recordings": [{
                "id": "abc123",
                "title": "Comfortably Numb",
                "artist-credit": [{"artist": {"name": "Pink Floyd"}}],
                "releases": [{
                    "title": "The Wall",
                    "date": "1979-11-30",
                    "quality": "normal",
                    "release-group": {"id": "rg-1", "primary-type": "Album"}
                }],
                "isrcs": ["GBAYE7900498"],
                "length": 382000,
                "score": 100,
                "genres": [{"name": "progressive rock", "count": 12}],
                "relations": [{
                    "type": "performer",
                    "target-type": "artist",
                    "artist": {"id": "x", "name": "Y"}
                }]
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

    fn sq(title: Option<&str>, artist: Option<&str>, isrc: Option<&str>) -> SearchQuery {
        SearchQuery {
            title: title.map(str::to_owned),
            artist: artist.map(str::to_owned),
            isrc: isrc.map(str::to_owned),
            ..Default::default()
        }
    }

    #[test]
    fn build_lucene_query_title_and_artist() {
        let q = sq(Some("Back in Black"), Some("AC/DC"), None);
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            r#"recording:"Back in Black" AND artistname:"AC/DC""#
        );
    }

    #[test]
    fn build_lucene_query_title_with_question_mark() {
        let q = sq(Some("Where Is My Mind?"), Some("Pixies"), None);
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            r#"recording:"Where Is My Mind?" AND artistname:"Pixies""#
        );
    }

    #[test]
    fn build_lucene_query_artist_with_exclamation() {
        let q = sq(
            Some("Nine in the Afternoon"),
            Some("Panic! at the Disco"),
            None,
        );
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            r#"recording:"Nine in the Afternoon" AND artistname:"Panic! at the Disco""#
        );
    }

    #[test]
    fn build_lucene_query_title_only_with_brackets() {
        let q = sq(Some("[Intro]"), None, None);
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            r#"recording:"[Intro]""#
        );
    }

    #[test]
    fn build_lucene_query_title_with_ampersand() {
        let q = sq(Some("S&M"), Some("Metallica"), None);
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            r#"recording:"S&M" AND artistname:"Metallica""#
        );
    }

    #[test]
    fn build_lucene_query_title_with_embedded_quotes() {
        let q = sq(Some(r#"Say "Hello""#), None, None);
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            "recording:\"Say \\\"Hello\\\"\""
        );
    }

    #[test]
    fn build_lucene_query_isrc_already_normalised() {
        let q = sq(None, None, Some("GBAYE0601498"));
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            "isrc:GBAYE0601498"
        );
    }

    #[test]
    fn build_lucene_query_isrc_lowercase_hyphenated_normalises() {
        let q = sq(None, None, Some("gb-aye-06-01498"));
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            "isrc:GBAYE0601498"
        );
    }

    #[test]
    fn build_lucene_query_invalid_isrc_is_err() {
        let q = sq(None, None, Some("not-an-isrc!"));
        assert!(matches!(
            MusicBrainzProvider::build_lucene_query(&q),
            Err(ProviderError::Other(_))
        ));
    }

    #[test]
    fn build_lucene_query_no_fields_is_err() {
        let q = SearchQuery::default();
        assert!(MusicBrainzProvider::build_lucene_query(&q).is_err());
    }

    #[test]
    fn strip_trailing_bracket_groups_remastered_suffix() {
        assert_eq!(
            strip_trailing_bracket_groups("Comfortably Numb (2011 Remastered Version)"),
            "Comfortably Numb"
        );
    }

    #[test]
    fn strip_trailing_bracket_groups_live_suffix() {
        assert_eq!(strip_trailing_bracket_groups("Song [Live]"), "Song");
    }

    #[test]
    fn strip_trailing_bracket_groups_repeated() {
        assert_eq!(
            strip_trailing_bracket_groups("Song (Live) (Remastered)"),
            "Song"
        );
    }

    #[test]
    fn strip_trailing_bracket_groups_nested() {
        assert_eq!(
            strip_trailing_bracket_groups("Song (Live (Acoustic))"),
            "Song"
        );
    }

    #[test]
    fn strip_trailing_bracket_groups_leading_group_preserved() {
        assert_eq!(
            strip_trailing_bracket_groups("(I Can't Get No) Satisfaction"),
            "(I Can't Get No) Satisfaction"
        );
    }

    #[test]
    fn strip_trailing_bracket_groups_would_empty_intro_kept() {
        assert_eq!(strip_trailing_bracket_groups("[Intro]"), "[Intro]");
    }

    #[test]
    fn strip_trailing_bracket_groups_would_empty_reprise_kept() {
        assert_eq!(strip_trailing_bracket_groups("(Reprise)"), "(Reprise)");
    }

    #[test]
    fn strip_trailing_bracket_groups_unbalanced_unchanged() {
        assert_eq!(strip_trailing_bracket_groups("Song (Live"), "Song (Live");
    }

    #[test]
    fn strip_trailing_bracket_groups_no_brackets_unchanged() {
        assert_eq!(
            strip_trailing_bracket_groups("Comfortably Numb"),
            "Comfortably Numb"
        );
    }

    #[test]
    fn build_lucene_query_strips_trailing_bracket_groups() {
        let q = sq(
            Some("Comfortably Numb (2011 Remastered Version)"),
            Some("Pink Floyd (feat. Someone)"),
            None,
        );
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&q).unwrap(),
            r#"recording:"Comfortably Numb" AND artistname:"Pink Floyd""#
        );
    }

    // ---- genre extraction (MusicBrainz `genres` / `tags` arrays) ----

    #[test]
    fn mb_parse_recordings_genre_picks_highest_count_genre() {
        let json = r#"{
            "recordings": [{
                "id": "abc123",
                "genres": [
                    {"name": "psychedelic rock", "count": 4},
                    {"name": "progressive rock", "count": 12},
                    {"name": "art rock", "count": 7}
                ],
                "tags": [
                    {"name": "classic rock", "count": 99}
                ]
            }]
        }"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        // Genres take priority over tags even when a tag has a higher count.
        assert_eq!(results[0].genre.as_deref(), Some("progressive rock"));
    }

    #[test]
    fn mb_parse_recordings_genre_falls_back_to_highest_count_tag() {
        let json = r#"{
            "recordings": [{
                "id": "abc123",
                "genres": [],
                "tags": [
                    {"name": "guitar solo", "count": 2},
                    {"name": "classic rock", "count": 9}
                ]
            }]
        }"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        assert_eq!(results[0].genre.as_deref(), Some("classic rock"));
    }

    #[test]
    fn mb_parse_recordings_genre_none_when_absent() {
        let json = r#"{"recordings": [{"id": "abc123"}]}"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        assert_eq!(results[0].genre, None);
    }

    #[test]
    fn mb_parse_recordings_genre_missing_count_ranks_as_zero() {
        let json = r#"{
            "recordings": [{
                "id": "abc123",
                "genres": [
                    {"name": "no count"},
                    {"name": "has count", "count": 1}
                ]
            }]
        }"#;
        let results = MusicBrainzProvider::parse_recordings("musicbrainz", json).unwrap();
        assert_eq!(results[0].genre.as_deref(), Some("has count"));
    }

    // ---- album / year narrowing clauses ----

    #[test]
    fn build_lucene_query_title_artist_album_and_year() {
        let query = SearchQuery {
            title: Some("Bohemian Rhapsody".into()),
            artist: Some("Queen".into()),
            album: Some("A Night at the Opera".into()),
            year: Some(1975),
            ..Default::default()
        };
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&query).unwrap(),
            r#"recording:"Bohemian Rhapsody" AND artistname:"Queen" AND release:"A Night at the Opera" AND date:1975"#
        );
    }

    #[test]
    fn build_lucene_query_title_and_album_only() {
        let query = SearchQuery {
            title: Some("Comfortably Numb".into()),
            album: Some("The Wall".into()),
            ..Default::default()
        };
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&query).unwrap(),
            r#"recording:"Comfortably Numb" AND release:"The Wall""#
        );
    }

    #[test]
    fn build_lucene_query_title_and_year_only() {
        let query = SearchQuery {
            title: Some("Comfortably Numb".into()),
            year: Some(1979),
            ..Default::default()
        };
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&query).unwrap(),
            r#"recording:"Comfortably Numb" AND date:1979"#
        );
    }

    #[test]
    fn build_lucene_query_album_or_year_alone_is_still_rejected() {
        // album/year only NARROW an anchored query — on their own they are
        // far too broad, so the "needs title, artist or ISRC" rule wins.
        let query = SearchQuery {
            album: Some("The Wall".into()),
            year: Some(1979),
            ..Default::default()
        };
        assert!(MusicBrainzProvider::build_lucene_query(&query).is_err());
    }

    #[test]
    fn build_lucene_query_album_bracket_group_is_stripped_too() {
        let query = SearchQuery {
            title: Some("Comfortably Numb".into()),
            album: Some("The Wall (Remastered)".into()),
            ..Default::default()
        };
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&query).unwrap(),
            r#"recording:"Comfortably Numb" AND release:"The Wall""#
        );
    }

    #[test]
    fn build_lucene_query_isrc_ignores_album_and_year() {
        // ISRC is an exact identifier — narrowing clauses would only risk
        // excluding the correct recording.
        let query = SearchQuery {
            isrc: Some("GBAYE0601498".into()),
            album: Some("The Wall".into()),
            year: Some(1979),
            ..Default::default()
        };
        assert_eq!(
            MusicBrainzProvider::build_lucene_query(&query).unwrap(),
            "isrc:GBAYE0601498"
        );
    }
}
