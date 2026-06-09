// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Sidecar JSON metadata — `.meedya.json` companion files.
//
// Carries the rich `ExtendedTags` shape in a portable, diff-friendly,
// tool-agnostic form. Survives lossy re-encoding without round-trip risk
// because the data lives separately from the audio container.
//
// Pairs alongside the existing `.lrc` sidecar pattern from `meedya-lyrics`.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::ExtendedTags;

/// Current sidecar schema version. Bump on breaking changes; reader
/// rejects unknown versions with `IoError::UnsupportedSchemaVersion`.
pub const SCHEMA_VERSION: u32 = 1;

/// Sidecar suffix appended to media filenames: `track.flac` → `track.flac.meedya.json`.
pub const SIDECAR_SUFFIX: &str = ".meedya.json";

// ============================================================
// Sidecar Type
// ============================================================

/// The full sidecar document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeedyaSidecar {
    /// Schema version. Readers reject unknown versions rather than guessing.
    pub version: u32,
    /// Tool that wrote this sidecar (e.g. `"meedya-manager 0.5.0"`).
    pub generated_by: String,
    /// UTC timestamp the sidecar was generated.
    pub generated_at: DateTime<Utc>,
    /// The full ExtendedTags payload.
    pub extended_tags: ExtendedTags,
}

impl MeedyaSidecar {
    /// Construct a v1 sidecar from `extended_tags` + tooling metadata.
    pub fn new(extended_tags: ExtendedTags, generated_by: impl Into<String>) -> Self {
        Self {
            version: SCHEMA_VERSION,
            generated_by: generated_by.into(),
            generated_at: Utc::now(),
            extended_tags,
        }
    }
}

/// Pretty-print or compact JSON output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarFormat {
    /// Pretty-printed with 2-space indent. Git-friendly.
    Pretty,
    /// Single-line compact JSON.
    Compact,
}

/// Errors from sidecar I/O.
#[derive(Debug)]
pub enum SidecarError {
    /// Filesystem I/O failed.
    Io(io::Error),
    /// JSON parse or serialise failed.
    Json(serde_json::Error),
    /// Sidecar's `version` is newer than this build understands.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "sidecar io: {e}"),
            Self::Json(e) => write!(f, "sidecar json: {e}"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "sidecar schema version {found} is newer than supported {supported}"
            ),
        }
    }
}

impl std::error::Error for SidecarError {}

impl From<io::Error> for SidecarError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SidecarError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

// ============================================================
// Public API
// ============================================================

/// Compute the sidecar path for `media`: `track.flac` → `track.flac.meedya.json`.
pub fn sidecar_path_for(media: &Path) -> PathBuf {
    let mut s = media.as_os_str().to_owned();
    s.push(SIDECAR_SUFFIX);
    PathBuf::from(s)
}

/// Write `sidecar` next to `media` as `<media>.meedya.json` in pretty
/// format. Returns the written path.
pub fn write_sidecar(media: &Path, sidecar: &MeedyaSidecar) -> Result<PathBuf, SidecarError> {
    write_sidecar_with_format(media, sidecar, SidecarFormat::Pretty)
}

/// Write `sidecar` next to `media` in the requested format. Returns the
/// written path.
pub fn write_sidecar_with_format(
    media: &Path,
    sidecar: &MeedyaSidecar,
    format: SidecarFormat,
) -> Result<PathBuf, SidecarError> {
    let path = sidecar_path_for(media);
    let text = match format {
        SidecarFormat::Pretty => serde_json::to_string_pretty(sidecar)?,
        SidecarFormat::Compact => serde_json::to_string(sidecar)?,
    };
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Read the sidecar for `media`, if one exists. Returns `Ok(None)` when
/// no sidecar is present; `Err` for read or parse failures or unsupported
/// schema versions.
pub fn read_sidecar(media: &Path) -> Result<Option<MeedyaSidecar>, SidecarError> {
    let path = sidecar_path_for(media);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(SidecarError::Io(e)),
    };
    let sidecar: MeedyaSidecar = serde_json::from_str(&text)?;
    if sidecar.version > SCHEMA_VERSION {
        return Err(SidecarError::UnsupportedSchemaVersion {
            found: sidecar.version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(Some(sidecar))
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_content::AiContentFlags;
    use crate::model::{EnergyValue, KeyMode, MusicalKey, Note};
    use tempfile::TempDir;

    fn rich_tags() -> ExtendedTags {
        ExtendedTags {
            bpm: Some(128.5),
            key: Some(MusicalKey {
                tonic: Note::A,
                mode: KeyMode::Minor,
            }),
            energy: Some(EnergyValue::Mik(7)),
            cue_points: vec![],
            loops: vec![],
            beat_grid: None,
            comment: Some("Δοκιμή — round-trip 🎵".to_owned()),
            ai_content: AiContentFlags {
                is_ai: Some(false),
                ai_enhanced: Some(true),
                ai_enhance_detail: Some("iZotope RX 11 voice de-noise".to_owned()),
                ..Default::default()
            },
            stems: None,
            play_history: Default::default(),
        }
    }

    #[test]
    fn sidecar_path_appends_suffix() {
        let p = sidecar_path_for(Path::new("/music/track.flac"));
        assert_eq!(p, Path::new("/music/track.flac.meedya.json"));
    }

    #[test]
    fn sidecar_path_works_for_mp3() {
        let p = sidecar_path_for(Path::new("song.mp3"));
        assert_eq!(p, Path::new("song.mp3.meedya.json"));
    }

    #[test]
    fn write_then_read_round_trip() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        // Touch the media file so the path is realistic; not required by the API.
        std::fs::write(&media, b"fake flac").unwrap();

        let sidecar = MeedyaSidecar::new(rich_tags(), "meedya-test 1.0");
        let written = write_sidecar(&media, &sidecar).unwrap();
        assert!(written.exists());

        let read = read_sidecar(&media).unwrap().expect("sidecar present");
        assert_eq!(read.version, SCHEMA_VERSION);
        assert_eq!(read.extended_tags, sidecar.extended_tags);
        assert_eq!(read.generated_by, "meedya-test 1.0");
    }

    #[test]
    fn read_missing_sidecar_returns_none() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("nope.flac");
        let read = read_sidecar(&media).unwrap();
        assert!(read.is_none());
    }

    #[test]
    fn pretty_format_is_human_readable() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        let sidecar = MeedyaSidecar::new(rich_tags(), "test");
        let path = write_sidecar_with_format(&media, &sidecar, SidecarFormat::Pretty).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        // Pretty output has newlines between fields.
        assert!(text.contains('\n'), "pretty format should have newlines");
        assert!(text.contains("\"version\""));
    }

    #[test]
    fn compact_format_is_single_line() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        let sidecar = MeedyaSidecar::new(rich_tags(), "test");
        let path = write_sidecar_with_format(&media, &sidecar, SidecarFormat::Compact).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        // Compact JSON has no embedded newlines (apart from trailing — but
        // serde_json::to_string doesn't add a trailing newline either).
        assert!(!text.contains('\n'), "compact format should be single line");
    }

    #[test]
    fn unsupported_future_version_rejected() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        let sidecar_path = sidecar_path_for(&media);
        // Hand-write a future-version sidecar.
        let fake = format!(
            r#"{{"version":999,"generated_by":"future","generated_at":"2099-01-01T00:00:00Z","extended_tags":{}}}"#,
            serde_json::to_string(&rich_tags()).unwrap()
        );
        std::fs::write(&sidecar_path, fake).unwrap();
        let err = read_sidecar(&media).unwrap_err();
        assert!(matches!(
            err,
            SidecarError::UnsupportedSchemaVersion {
                found: 999,
                supported: SCHEMA_VERSION
            }
        ));
    }

    #[test]
    fn malformed_json_returns_json_error() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        let sidecar_path = sidecar_path_for(&media);
        std::fs::write(&sidecar_path, b"not json").unwrap();
        let err = read_sidecar(&media).unwrap_err();
        assert!(matches!(err, SidecarError::Json(_)));
    }

    #[test]
    fn unicode_preserved_in_comment() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        let sidecar = MeedyaSidecar::new(rich_tags(), "test");
        write_sidecar(&media, &sidecar).unwrap();
        let read = read_sidecar(&media).unwrap().unwrap();
        assert_eq!(
            read.extended_tags.comment.as_deref(),
            Some("Δοκιμή — round-trip 🎵")
        );
    }

    #[test]
    fn round_trips_ai_content() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("track.flac");
        let sidecar = MeedyaSidecar::new(rich_tags(), "test");
        write_sidecar(&media, &sidecar).unwrap();
        let read = read_sidecar(&media).unwrap().unwrap();
        assert_eq!(read.extended_tags.ai_content.ai_enhanced, Some(true));
        assert_eq!(
            read.extended_tags.ai_content.ai_enhance_detail.as_deref(),
            Some("iZotope RX 11 voice de-noise")
        );
    }
}
