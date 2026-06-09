// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Play history metadata — total play count, last-played timestamp, DJ-set
// play count, etc.
//
// Apple Music and foobar2000 track play history in their library DB, but
// the data doesn't travel with the file. These tags carry play history
// inside the file so MeedyaSuite tools (MeedyaPlayer, MeedyaManager,
// MeedyaConverter pass-through) can sort by popularity, prompt "haven't
// played in a while", and surface DJ-relevant play counts separately.
//
// No widely-supported standard exists for play history across formats,
// so all atoms live in `MeedyaMeta:*`.
//
// Timestamps are ISO 8601 UTC with `Z` suffix (`2026-05-18T11:53:28Z`).
// The reader tolerates both `Z` and `+00:00` suffixes per RFC 3339.

use chrono::{DateTime, Utc};
use lofty::tag::Tag;

use crate::meedya_atom::{clear_meedya_atom, read_meedya_atom, write_meedya_atom};

const ATOM_PLAY_COUNT: &str = "PlayCount";
const ATOM_LAST_PLAYED: &str = "LastPlayed";
const ATOM_FIRST_PLAYED: &str = "FirstPlayed";
const ATOM_DJ_PLAY_COUNT: &str = "DjPlayCount";
const ATOM_DJ_LAST_PLAYED: &str = "DjLastPlayed";
const ATOM_SKIP_COUNT: &str = "SkipCount";

// ============================================================
// Public Type
// ============================================================

/// Play history fields. All optional — readers leave absent atoms as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayHistory {
    /// Total times the file has been played (any context).
    pub play_count: Option<u32>,
    /// Most recent play timestamp.
    pub last_played: Option<DateTime<Utc>>,
    /// When this file first entered active rotation.
    pub first_played: Option<DateTime<Utc>>,
    /// Plays specifically in a DJ-set context.
    pub dj_play_count: Option<u32>,
    /// Most recent DJ-set play timestamp.
    pub dj_last_played: Option<DateTime<Utc>>,
    /// Times the file was skipped before completing.
    pub skip_count: Option<u32>,
}

impl PlayHistory {
    pub fn is_empty(&self) -> bool {
        self.play_count.is_none()
            && self.last_played.is_none()
            && self.first_played.is_none()
            && self.dj_play_count.is_none()
            && self.dj_last_played.is_none()
            && self.skip_count.is_none()
    }
}

// ============================================================
// Public API
// ============================================================

/// Read play history from `tag`.
pub fn read_play_history(tag: &Tag) -> PlayHistory {
    PlayHistory {
        play_count: read_meedya_atom(tag, ATOM_PLAY_COUNT).and_then(|s| s.parse().ok()),
        last_played: read_meedya_atom(tag, ATOM_LAST_PLAYED).and_then(parse_iso),
        first_played: read_meedya_atom(tag, ATOM_FIRST_PLAYED).and_then(parse_iso),
        dj_play_count: read_meedya_atom(tag, ATOM_DJ_PLAY_COUNT).and_then(|s| s.parse().ok()),
        dj_last_played: read_meedya_atom(tag, ATOM_DJ_LAST_PLAYED).and_then(parse_iso),
        skip_count: read_meedya_atom(tag, ATOM_SKIP_COUNT).and_then(|s| s.parse().ok()),
    }
}

/// Write play history. Each `Some` field is written; `None` fields are
/// left untouched (use [`clear_play_history`] to actively remove).
pub fn write_play_history(tag: &mut Tag, history: &PlayHistory) {
    if let Some(n) = history.play_count {
        write_meedya_atom(tag, ATOM_PLAY_COUNT, &n.to_string());
    }
    if let Some(ts) = history.last_played {
        write_meedya_atom(tag, ATOM_LAST_PLAYED, &format_iso(ts));
    }
    if let Some(ts) = history.first_played {
        write_meedya_atom(tag, ATOM_FIRST_PLAYED, &format_iso(ts));
    }
    if let Some(n) = history.dj_play_count {
        write_meedya_atom(tag, ATOM_DJ_PLAY_COUNT, &n.to_string());
    }
    if let Some(ts) = history.dj_last_played {
        write_meedya_atom(tag, ATOM_DJ_LAST_PLAYED, &format_iso(ts));
    }
    if let Some(n) = history.skip_count {
        write_meedya_atom(tag, ATOM_SKIP_COUNT, &n.to_string());
    }
}

/// Remove all play history atoms.
pub fn clear_play_history(tag: &mut Tag) {
    for atom in [
        ATOM_PLAY_COUNT,
        ATOM_LAST_PLAYED,
        ATOM_FIRST_PLAYED,
        ATOM_DJ_PLAY_COUNT,
        ATOM_DJ_LAST_PLAYED,
        ATOM_SKIP_COUNT,
    ] {
        clear_meedya_atom(tag, atom);
    }
}

/// Record a play: increment the relevant `*PlayCount`, set the relevant
/// `*LastPlayed` to `now` (UTC), and set `FirstPlayed` if not already set.
/// `now` is taken as an argument rather than computed internally so tests
/// (and deterministic library import) can pin it.
pub fn record_play(tag: &mut Tag, now: DateTime<Utc>, was_dj_set: bool) {
    let mut h = read_play_history(tag);
    h.play_count = Some(h.play_count.unwrap_or(0) + 1);
    h.last_played = Some(now);
    if h.first_played.is_none() {
        h.first_played = Some(now);
    }
    if was_dj_set {
        h.dj_play_count = Some(h.dj_play_count.unwrap_or(0) + 1);
        h.dj_last_played = Some(now);
    }
    write_play_history(tag, &h);
}

/// Record a skip: increment `SkipCount`. Doesn't touch other fields.
pub fn record_skip(tag: &mut Tag) {
    let mut h = read_play_history(tag);
    h.skip_count = Some(h.skip_count.unwrap_or(0) + 1);
    write_play_history(tag, &h);
}

// ============================================================
// Date Helpers
// ============================================================

/// Format a `DateTime<Utc>` as ISO 8601 with `Z` suffix.
fn format_iso(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Parse an ISO 8601 string. Accepts both `Z` and `+00:00` suffixes per RFC
/// 3339. Tolerates trailing whitespace.
fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use lofty::tag::TagType;

    fn fresh() -> Tag {
        Tag::new(TagType::Id3v2)
    }

    fn ts(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
            .unwrap()
    }

    // ---- Round trip ----

    #[test]
    fn round_trip_full_history() {
        let mut tag = fresh();
        let h = PlayHistory {
            play_count: Some(42),
            last_played: Some(ts(2026, 5, 18, 11, 53, 28)),
            first_played: Some(ts(2024, 1, 1, 0, 0, 0)),
            dj_play_count: Some(7),
            dj_last_played: Some(ts(2026, 4, 30, 23, 15, 0)),
            skip_count: Some(3),
        };
        write_play_history(&mut tag, &h);
        assert_eq!(read_play_history(&tag), h);
    }

    #[test]
    fn read_empty_tag_returns_default() {
        assert!(read_play_history(&fresh()).is_empty());
    }

    // ---- Date formatting ----

    #[test]
    fn last_played_formatted_with_z_suffix() {
        let mut tag = fresh();
        write_play_history(
            &mut tag,
            &PlayHistory {
                last_played: Some(ts(2026, 5, 18, 11, 53, 28)),
                ..Default::default()
            },
        );
        assert_eq!(
            read_meedya_atom(&tag, ATOM_LAST_PLAYED),
            Some("2026-05-18T11:53:28Z")
        );
    }

    #[test]
    fn last_played_parses_plus_offset_form() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_LAST_PLAYED, "2026-05-18T11:53:28+00:00");
        let h = read_play_history(&tag);
        assert_eq!(h.last_played, Some(ts(2026, 5, 18, 11, 53, 28)));
    }

    #[test]
    fn last_played_parses_nonzero_offset() {
        let mut tag = fresh();
        // 12:53:28 in +01:00 == 11:53:28 UTC
        write_meedya_atom(&mut tag, ATOM_LAST_PLAYED, "2026-05-18T12:53:28+01:00");
        let h = read_play_history(&tag);
        assert_eq!(h.last_played, Some(ts(2026, 5, 18, 11, 53, 28)));
    }

    #[test]
    fn last_played_tolerates_whitespace() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_LAST_PLAYED, "  2026-05-18T11:53:28Z  ");
        let h = read_play_history(&tag);
        assert_eq!(h.last_played, Some(ts(2026, 5, 18, 11, 53, 28)));
    }

    #[test]
    fn malformed_date_falls_back_to_none() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_LAST_PLAYED, "tomorrow");
        let h = read_play_history(&tag);
        assert_eq!(h.last_played, None);
    }

    // ---- record_play / record_skip ----

    #[test]
    fn record_play_increments_count_and_sets_timestamps() {
        let mut tag = fresh();
        let now = ts(2026, 5, 18, 12, 0, 0);
        record_play(&mut tag, now, false);
        let h = read_play_history(&tag);
        assert_eq!(h.play_count, Some(1));
        assert_eq!(h.last_played, Some(now));
        assert_eq!(h.first_played, Some(now));
        assert_eq!(h.dj_play_count, None);
    }

    #[test]
    fn record_play_preserves_first_played_on_subsequent() {
        let mut tag = fresh();
        let first = ts(2024, 1, 1, 0, 0, 0);
        let later = ts(2026, 5, 18, 12, 0, 0);
        record_play(&mut tag, first, false);
        record_play(&mut tag, later, false);
        let h = read_play_history(&tag);
        assert_eq!(h.play_count, Some(2));
        assert_eq!(h.first_played, Some(first));
        assert_eq!(h.last_played, Some(later));
    }

    #[test]
    fn record_play_dj_increments_dj_count_too() {
        let mut tag = fresh();
        let now = ts(2026, 5, 18, 12, 0, 0);
        record_play(&mut tag, now, true);
        let h = read_play_history(&tag);
        assert_eq!(h.play_count, Some(1));
        assert_eq!(h.dj_play_count, Some(1));
        assert_eq!(h.dj_last_played, Some(now));
    }

    #[test]
    fn record_play_non_dj_does_not_touch_dj_fields() {
        let mut tag = fresh();
        let now = ts(2026, 5, 18, 12, 0, 0);
        // Seed an existing DJ play first.
        record_play(&mut tag, ts(2024, 1, 1, 0, 0, 0), true);
        let before = read_play_history(&tag);
        record_play(&mut tag, now, false);
        let after = read_play_history(&tag);
        assert_eq!(after.dj_play_count, before.dj_play_count);
        assert_eq!(after.dj_last_played, before.dj_last_played);
        // Total play_count still incremented.
        assert_eq!(after.play_count, Some(2));
    }

    #[test]
    fn record_skip_only_touches_skip_count() {
        let mut tag = fresh();
        record_play(&mut tag, ts(2026, 5, 18, 12, 0, 0), false);
        let before = read_play_history(&tag);
        record_skip(&mut tag);
        let after = read_play_history(&tag);
        assert_eq!(after.skip_count, Some(1));
        assert_eq!(after.play_count, before.play_count);
        assert_eq!(after.last_played, before.last_played);
    }

    // ---- Misc ----

    #[test]
    fn clear_removes_all_atoms() {
        let mut tag = fresh();
        record_play(&mut tag, ts(2026, 5, 18, 12, 0, 0), true);
        record_skip(&mut tag);
        clear_play_history(&mut tag);
        assert!(read_play_history(&tag).is_empty());
    }

    #[test]
    fn malformed_count_falls_back_to_none() {
        let mut tag = fresh();
        write_meedya_atom(&mut tag, ATOM_PLAY_COUNT, "many");
        assert_eq!(read_play_history(&tag).play_count, None);
    }

    #[test]
    fn write_skips_none_fields() {
        let mut tag = fresh();
        write_play_history(
            &mut tag,
            &PlayHistory {
                play_count: Some(5),
                ..Default::default()
            },
        );
        assert_eq!(read_meedya_atom(&tag, ATOM_PLAY_COUNT), Some("5"));
        assert_eq!(read_meedya_atom(&tag, ATOM_LAST_PLAYED), None);
    }
}
