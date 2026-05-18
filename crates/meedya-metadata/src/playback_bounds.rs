// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Soft playback start/stop bounds — MeedyaSuite-only metadata.
//
// Mirrors iTunes' "Start Time" / "Stop Time" feature (Get Info → Options),
// which iTunes itself stored only in its library database — never in the
// file. These atoms make the preference travel with the file so MeedyaSuite
// tools (MeedyaConverter, MeedyaManager, future MeedyaPlayer) can apply
// soft trimming without modifying media samples. Third-party players will
// ignore them.
//
// Each endpoint is written as two atoms in the MeedyaMeta namespace:
//   - `PlaybackStartMs` / `PlaybackStopMs`  — canonical u64 milliseconds
//   - `PlaybackStart`   / `PlaybackStop`    — `HH:MM:SS.mmm` for tag editors
//
// On read, the `*Ms` atom is authoritative; the display atom is decorative
// and re-derived on every write to stay in sync.

use mp4ameta::{Data, FreeformIdent, Tag};

use crate::registry::MEEDYA_NAMESPACE;

const START_MS_NAME: &str = "PlaybackStartMs";
const STOP_MS_NAME: &str = "PlaybackStopMs";
const START_DISPLAY_NAME: &str = "PlaybackStart";
const STOP_DISPLAY_NAME: &str = "PlaybackStop";

/// Set the soft playback start point. Writes both ms and display atoms.
pub fn set_playback_start(tag: &mut Tag, start_ms: u64) {
    write_pair(tag, START_MS_NAME, START_DISPLAY_NAME, start_ms);
}

/// Set the soft playback stop point. Writes both ms and display atoms.
pub fn set_playback_stop(tag: &mut Tag, stop_ms: u64) {
    write_pair(tag, STOP_MS_NAME, STOP_DISPLAY_NAME, stop_ms);
}

/// Remove the soft playback start atoms (both ms and display).
pub fn clear_playback_start(tag: &mut Tag) {
    tag.remove_data_of(&FreeformIdent::new_static(MEEDYA_NAMESPACE, START_MS_NAME));
    tag.remove_data_of(&FreeformIdent::new_static(
        MEEDYA_NAMESPACE,
        START_DISPLAY_NAME,
    ));
}

/// Remove the soft playback stop atoms (both ms and display).
pub fn clear_playback_stop(tag: &mut Tag) {
    tag.remove_data_of(&FreeformIdent::new_static(MEEDYA_NAMESPACE, STOP_MS_NAME));
    tag.remove_data_of(&FreeformIdent::new_static(
        MEEDYA_NAMESPACE,
        STOP_DISPLAY_NAME,
    ));
}

/// Read the soft playback start in milliseconds. Returns `None` if absent
/// or unparseable. The `*Ms` atom is canonical; the display atom is ignored.
pub fn get_playback_start_ms(tag: &Tag) -> Option<u64> {
    read_ms(tag, START_MS_NAME)
}

/// Read the soft playback stop in milliseconds. Returns `None` if absent
/// or unparseable.
pub fn get_playback_stop_ms(tag: &Tag) -> Option<u64> {
    read_ms(tag, STOP_MS_NAME)
}

fn write_pair(tag: &mut Tag, ms_name: &'static str, display_name: &'static str, ms: u64) {
    tag.set_data(
        FreeformIdent::new_static(MEEDYA_NAMESPACE, ms_name),
        Data::Utf8(ms.to_string()),
    );
    tag.set_data(
        FreeformIdent::new_static(MEEDYA_NAMESPACE, display_name),
        Data::Utf8(format_hms_ms(ms)),
    );
}

fn read_ms(tag: &Tag, name: &'static str) -> Option<u64> {
    let ident = FreeformIdent::new_static(MEEDYA_NAMESPACE, name);
    let raw = tag.strings_of(&ident).next()?.to_owned();
    raw.trim().parse().ok()
}

/// Format a millisecond count as `HH:MM:SS.mmm` for human-readable tag display.
pub fn format_hms_ms(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let millis = ms % 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_zero() {
        assert_eq!(format_hms_ms(0), "00:00:00.000");
    }

    #[test]
    fn format_sub_second() {
        assert_eq!(format_hms_ms(7), "00:00:00.007");
        assert_eq!(format_hms_ms(500), "00:00:00.500");
    }

    #[test]
    fn format_seconds_and_millis() {
        assert_eq!(format_hms_ms(12_500), "00:00:12.500");
    }

    #[test]
    fn format_minutes() {
        assert_eq!(format_hms_ms(65_000), "00:01:05.000");
    }

    #[test]
    fn format_hours() {
        assert_eq!(format_hms_ms(3_725_123), "01:02:05.123");
    }

    #[test]
    fn format_double_digit_hours() {
        assert_eq!(format_hms_ms(36_000_000), "10:00:00.000");
    }
}
