// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-library-import — Ingest soft playback bounds (and adjacent metadata)
// from external library databases.
//
// Produces a normalized stream of `LibraryEntry` records that a downstream
// app (e.g., MeedyaManager) can match to local files and feed into
// `meedya_metadata::playback_bounds`. The crate intentionally does no file
// matching, no atom writing, and no codec inspection — those concerns live
// in the calling app.
//
// ## Modules
//
// - `itunes_xml` — Parses iTunes / Music.app `iTunes Music Library.xml`
//   exports. Emits `Start Time` / `Stop Time` per track.
// - `cuesheet`   — Parses CUE sheets into a rich `CueSheet` model. The
//   `import()` adapter emits LibraryEntries only for the narrow case of
//   per-track file rips with a non-zero `INDEX 01` (pregap inside file);
//   single-file rips expose their structure via `parse_file()` for use
//   by future chapter-authoring code.
//
// Future importers (not yet implemented):
//   - `mediamonkey`  — MediaMonkey `MM.DB` / `MM5.DB` SQLite libraries

pub mod cuesheet;
pub mod itunes_xml;

use std::path::PathBuf;

/// A normalized record from an external library source.
///
/// Importers only emit entries where at least one of `start_ms` / `stop_ms`
/// is set — tracks with no soft trim configured are filtered out at the
/// source so consumers don't need to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// Best identifier we have for matching this entry to a local file.
    pub locator: EntryLocator,
    /// Soft start point in milliseconds, if set.
    pub start_ms: Option<u64>,
    /// Soft stop point in milliseconds, if set.
    pub stop_ms: Option<u64>,
}

/// How a library entry identifies itself for file-matching.
///
/// `Path` is preferred when the source provides a usable file URL or path.
/// `PersistentId` is the fallback for sources without paths, or for entries
/// whose paths failed to decode (e.g., orphaned tracks in the library).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryLocator {
    /// Absolute filesystem path, decoded from a source-specific URL or path.
    Path(PathBuf),
    /// Source-specific persistent identifier.
    PersistentId {
        /// Stable identifier for the source (e.g., `"itunes-xml"`).
        kind: &'static str,
        value: String,
    },
}

/// Where the imported records came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    /// Stable identifier for the importer (e.g., `"itunes-xml"`).
    pub kind: &'static str,
    /// Path to the source file or database.
    pub path: PathBuf,
}

/// Result of running an importer over a single source.
#[derive(Debug, Clone)]
pub struct ImportReport {
    pub source: SourceInfo,
    pub entries: Vec<LibraryEntry>,
    /// Non-fatal warnings (malformed entries skipped, missing locators, etc.).
    pub warnings: Vec<String>,
}
