// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Database export trait.
// Extracted from MeedyaManager mm-export/src/traits.rs.
//
// Defines a common interface for exporting media metadata to various
// database backends (SQLite, MySQL, PostgreSQL, MariaDB, SQL Server).

use crate::error::DbError;
use crate::models::{Album, Artist, Track};

/// Trait for exporting media data to a database backend.
///
/// Implementors provide the SQL dialect-specific logic for creating
/// tables and inserting records. The schema is standardised across
/// all backends.
pub trait DbExporter: Send + Sync {
    /// Human-readable name of the export backend (e.g., "SQLite", "PostgreSQL").
    fn backend_name(&self) -> &str;

    /// Create the schema tables if they don't exist.
    fn create_schema(&self) -> Result<(), DbError>;

    /// Export a single track record.
    fn export_track(&self, track: &Track) -> Result<(), DbError>;

    /// Export a single album record.
    fn export_album(&self, album: &Album) -> Result<(), DbError>;

    /// Export a single artist record.
    fn export_artist(&self, artist: &Artist) -> Result<(), DbError>;

    /// Export a batch of tracks.
    fn export_tracks(&self, tracks: &[Track]) -> Result<usize, DbError> {
        let mut count = 0;
        for track in tracks {
            self.export_track(track)?;
            count += 1;
        }
        Ok(count)
    }
}

/// Standard table names used across all export backends.
pub mod schema {
    /// Tracks table name.
    pub const TRACKS_TABLE: &str = "meedya_tracks";
    /// Albums table name.
    pub const ALBUMS_TABLE: &str = "meedya_albums";
    /// Artists table name.
    pub const ARTISTS_TABLE: &str = "meedya_artists";
    /// Tags table name (key-value metadata).
    pub const TAGS_TABLE: &str = "meedya_tags";
    /// Cover art table name.
    pub const COVER_ART_TABLE: &str = "meedya_cover_art";
}
