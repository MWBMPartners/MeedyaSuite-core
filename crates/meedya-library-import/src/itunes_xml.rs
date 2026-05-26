// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// iTunes / Music.app XML library importer.
//
// Parses `iTunes Music Library.xml` (an Apple plist) and extracts entries
// with a soft `Start Time` and/or `Stop Time` set. The XML export must be
// enabled in modern macOS Music.app under
// Settings → Advanced → "Share Library XML with other applications".

use std::path::{Path, PathBuf};

use plist::Value;

use crate::{EntryLocator, ImportReport, LibraryEntry, SourceInfo};

/// Stable identifier for this importer, used in `SourceInfo` and
/// `EntryLocator::PersistentId`.
pub const KIND: &str = "itunes-xml";

/// Parse an iTunes / Music.app library XML file at `path`.
///
/// Returns one `LibraryEntry` per track that has a `Start Time` or
/// `Stop Time` set. Tracks without soft trim configured are silently
/// skipped (this is the common case — we don't want to emit them).
pub fn import(path: &Path) -> Result<ImportReport, String> {
    let plist = Value::from_file(path)
        .map_err(|e| format!("Failed to parse iTunes XML at {}: {}", path.display(), e))?;

    let root = plist
        .as_dictionary()
        .ok_or_else(|| format!("iTunes XML root is not a dictionary: {}", path.display()))?;

    let tracks = root
        .get("Tracks")
        .and_then(Value::as_dictionary)
        .ok_or_else(|| format!("iTunes XML missing 'Tracks' dictionary: {}", path.display()))?;

    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for (track_key, track_val) in tracks {
        let dict = match track_val.as_dictionary() {
            Some(d) => d,
            None => {
                warnings.push(format!("Track {track_key}: not a dictionary, skipped"));
                continue;
            }
        };

        let start_ms = dict.get("Start Time").and_then(plist_to_u64);
        let stop_ms = dict.get("Stop Time").and_then(plist_to_u64);

        if start_ms.is_none() && stop_ms.is_none() {
            continue;
        }

        let location_path = dict
            .get("Location")
            .and_then(Value::as_string)
            .and_then(decode_file_url);
        let persistent_id = dict
            .get("Persistent ID")
            .and_then(Value::as_string)
            .map(str::to_owned);

        let locator = match (location_path, persistent_id) {
            (Some(p), _) => EntryLocator::Path(p),
            (None, Some(id)) => EntryLocator::PersistentId {
                kind: KIND,
                value: id,
            },
            (None, None) => {
                warnings.push(format!(
                    "Track {track_key}: has Start/Stop but no Location or Persistent ID, skipped"
                ));
                continue;
            }
        };

        entries.push(LibraryEntry {
            locator,
            start_ms,
            stop_ms,
        });
    }

    Ok(ImportReport {
        source: SourceInfo {
            kind: KIND,
            path: path.to_path_buf(),
        },
        entries,
        warnings,
    })
}

fn plist_to_u64(v: &Value) -> Option<u64> {
    if let Some(u) = v.as_unsigned_integer() {
        return Some(u);
    }
    v.as_signed_integer().filter(|n| *n >= 0).map(|n| n as u64)
}

/// Decode an iTunes-style `file://` URL into an absolute `PathBuf`.
///
/// iTunes writes paths as either `file://localhost/...` (older macOS) or
/// `file:///...`. On Windows, the path after the prefix begins with a drive
/// letter (`C:/...`); on macOS/Linux it begins with the first path segment
/// (`Users/...`) and we restore the leading slash. Detection is by drive
/// letter shape rather than build-time `cfg`, so cross-platform imports
/// (macOS user reading a Windows-exported XML) work.
fn decode_file_url(s: &str) -> Option<PathBuf> {
    let rest = s
        .strip_prefix("file://localhost/")
        .or_else(|| s.strip_prefix("file:///"))?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8_lossy()
        .into_owned();

    let looks_windows = decoded.len() >= 2
        && decoded.as_bytes()[0].is_ascii_alphabetic()
        && decoded.as_bytes()[1] == b':';

    if looks_windows {
        Some(PathBuf::from(decoded))
    } else {
        Some(PathBuf::from(format!("/{decoded}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_xml(xml: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".xml")
            .tempfile()
            .expect("create tempfile");
        f.write_all(xml.as_bytes()).expect("write xml");
        f.flush().expect("flush");
        f
    }

    fn xml_with_tracks(tracks_inner: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Major Version</key><integer>1</integer>
    <key>Tracks</key>
    <dict>
        {tracks_inner}
    </dict>
</dict>
</plist>"#
        )
    }

    #[test]
    fn decode_macos_localhost_url() {
        let p = decode_file_url("file://localhost/Users/foo/Music/song.m4a").unwrap();
        assert_eq!(p, PathBuf::from("/Users/foo/Music/song.m4a"));
    }

    #[test]
    fn decode_macos_triple_slash_url() {
        let p = decode_file_url("file:///Users/foo/Music/song.m4a").unwrap();
        assert_eq!(p, PathBuf::from("/Users/foo/Music/song.m4a"));
    }

    #[test]
    fn decode_windows_url() {
        let p = decode_file_url("file://localhost/C:/Users/foo/Music/song.m4a").unwrap();
        assert_eq!(p, PathBuf::from("C:/Users/foo/Music/song.m4a"));
    }

    #[test]
    fn decode_percent_encoded_spaces() {
        let p = decode_file_url("file://localhost/Users/foo/My%20Music/song.m4a").unwrap();
        assert_eq!(p, PathBuf::from("/Users/foo/My Music/song.m4a"));
    }

    #[test]
    fn decode_non_file_scheme_returns_none() {
        assert!(decode_file_url("https://example.com/song.m4a").is_none());
    }

    #[test]
    fn imports_track_with_start_and_stop() {
        let xml = xml_with_tracks(
            r#"
            <key>123</key>
            <dict>
                <key>Track ID</key><integer>123</integer>
                <key>Name</key><string>Test Song</string>
                <key>Location</key><string>file://localhost/Users/foo/Music/song.m4a</string>
                <key>Persistent ID</key><string>ABCD1234EF567890</string>
                <key>Start Time</key><integer>5000</integer>
                <key>Stop Time</key><integer>180000</integer>
            </dict>
            "#,
        );
        let f = write_xml(&xml);
        let report = import(f.path()).expect("import succeeds");

        assert_eq!(report.entries.len(), 1);
        let e = &report.entries[0];
        assert_eq!(e.start_ms, Some(5000));
        assert_eq!(e.stop_ms, Some(180_000));
        assert_eq!(
            e.locator,
            EntryLocator::Path(PathBuf::from("/Users/foo/Music/song.m4a"))
        );
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn imports_track_with_start_only() {
        let xml = xml_with_tracks(
            r#"
            <key>1</key>
            <dict>
                <key>Location</key><string>file://localhost/Users/foo/a.m4a</string>
                <key>Persistent ID</key><string>AAAA</string>
                <key>Start Time</key><integer>2500</integer>
            </dict>
            "#,
        );
        let f = write_xml(&xml);
        let report = import(f.path()).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].start_ms, Some(2500));
        assert_eq!(report.entries[0].stop_ms, None);
    }

    #[test]
    fn imports_track_with_stop_only() {
        let xml = xml_with_tracks(
            r#"
            <key>1</key>
            <dict>
                <key>Location</key><string>file://localhost/Users/foo/b.m4a</string>
                <key>Persistent ID</key><string>BBBB</string>
                <key>Stop Time</key><integer>120000</integer>
            </dict>
            "#,
        );
        let f = write_xml(&xml);
        let report = import(f.path()).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].start_ms, None);
        assert_eq!(report.entries[0].stop_ms, Some(120_000));
    }

    #[test]
    fn skips_track_without_trim_points() {
        let xml = xml_with_tracks(
            r#"
            <key>1</key>
            <dict>
                <key>Location</key><string>file://localhost/Users/foo/c.m4a</string>
                <key>Persistent ID</key><string>CCCC</string>
            </dict>
            "#,
        );
        let f = write_xml(&xml);
        let report = import(f.path()).unwrap();
        assert!(report.entries.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn falls_back_to_persistent_id_when_location_missing() {
        let xml = xml_with_tracks(
            r#"
            <key>1</key>
            <dict>
                <key>Persistent ID</key><string>DEADBEEF</string>
                <key>Start Time</key><integer>1000</integer>
            </dict>
            "#,
        );
        let f = write_xml(&xml);
        let report = import(f.path()).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert_eq!(
            report.entries[0].locator,
            EntryLocator::PersistentId {
                kind: KIND,
                value: "DEADBEEF".into()
            }
        );
    }

    #[test]
    fn warns_when_trim_set_but_no_locator() {
        let xml = xml_with_tracks(
            r#"
            <key>1</key>
            <dict>
                <key>Start Time</key><integer>1000</integer>
            </dict>
            "#,
        );
        let f = write_xml(&xml);
        let report = import(f.path()).unwrap();
        assert!(report.entries.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("no Location or Persistent ID"));
    }

    #[test]
    fn errors_on_missing_tracks_dict() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Major Version</key><integer>1</integer></dict></plist>"#;
        let f = write_xml(xml);
        let err = import(f.path()).unwrap_err();
        assert!(err.contains("missing 'Tracks'"));
    }

    #[test]
    fn source_info_set_correctly() {
        let xml = xml_with_tracks("");
        let f = write_xml(&xml);
        let report = import(f.path()).unwrap();
        assert_eq!(report.source.kind, KIND);
        assert_eq!(report.source.path, f.path().to_path_buf());
    }
}
