// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// File I/O wrapper around `lofty`.
//
// `TagFile` is the read/edit/save handle used by the rest of the crate.
// Lofty's design preserves frames it doesn't recognise across read+save
// cycles, which gives us pass-through of foreign DJ blobs (Serato GEOBs,
// Rekordbox PRIVs, etc.) for free — provided the caller doesn't strip
// tags between read and write.

use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::file::TaggedFile;
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

/// Read/edit/save handle for a single audio (or video) file.
///
/// On `open`, lofty parses every tag the file contains. On `save`, every
/// tag is written back — including frames lofty doesn't model (Serato
/// Markers2, Rekordbox PRIV, etc.) which round-trip as opaque bytes.
pub struct TagFile {
    path: PathBuf,
    inner: TaggedFile,
}

impl TagFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let inner = lofty::read_from_path(path)
            .map_err(|e| format!("Failed to read tags from {}: {}", path.display(), e))?;
        Ok(Self {
            path: path.to_path_buf(),
            inner,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Save back to the original path. Preserves foreign frames.
    pub fn save(&mut self) -> Result<(), String> {
        self.save_to(&self.path.clone())
    }

    /// Save to a different path.
    pub fn save_to(&mut self, dest: &Path) -> Result<(), String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dest)
            .map_err(|e| format!("Failed to open {} for write: {}", dest.display(), e))?;
        let mut file = file;
        self.inner
            .save_to(&mut file, WriteOptions::default())
            .map_err(|e| format!("Failed to save tags to {}: {}", dest.display(), e))
    }

    /// Read access to the file's primary tag (e.g., ID3v2 for MP3, ilst for MP4).
    /// Returns `None` if the file has no tag of any kind.
    pub fn primary_tag(&self) -> Option<&Tag> {
        self.inner.primary_tag()
    }

    /// Mutable access to the primary tag, creating one if absent.
    ///
    /// Lofty requires the caller to know the tag type up-front; we use the
    /// file's `primary_tag_type()` so this is correct per format.
    pub fn primary_tag_mut(&mut self) -> &mut Tag {
        if self.inner.primary_tag_mut().is_none() {
            let tag_type = self.inner.primary_tag_type();
            self.inner.insert_tag(Tag::new(tag_type));
        }
        self.inner
            .primary_tag_mut()
            .expect("primary tag was just inserted")
    }

    /// Read access to a specific tag type (when one file holds multiple,
    /// e.g., an MP3 with both ID3v2 and APE).
    pub fn tag(&self, tag_type: TagType) -> Option<&Tag> {
        self.inner.tag(tag_type)
    }

    pub fn tag_mut(&mut self, tag_type: TagType) -> Option<&mut Tag> {
        self.inner.tag_mut(tag_type)
    }

    /// Underlying `TaggedFile` for low-level format-specific reads
    /// (used by future Serato/Rekordbox/Traktor parsers).
    pub fn inner(&self) -> &TaggedFile {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut TaggedFile {
        &mut self.inner
    }
}
