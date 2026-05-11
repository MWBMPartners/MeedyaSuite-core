// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Tag writer — writes metadata tags to M4A files using the tag registry.
//
// Provides config-driven tag writing from API JSON responses, ISRC
// extraction from the Apple Vendor atom, and always-on source/format tags.

use std::path::Path;

use mp4ameta::{Data, FreeformIdent, Tag};

use crate::registry::{self, TagRegistry, ITUNES_NAMESPACE, MEEDYA_NAMESPACE};

// ============================================================
// Registry-Driven Tag Writing
// ============================================================

/// Write tags from the registry to an M4A file's metadata.
///
/// Iterates album-scope and track-scope tag definitions, extracts values
/// from the raw API JSON using configured paths, and writes them as
/// freeform atoms.
///
/// # Arguments
/// * `tag` — Mutable reference to an open MP4 tag
/// * `registry` — The tag registry (typically `&TAG_REGISTRY`)
/// * `album_json` — Raw API JSON for the album (relative to `data[0]`)
/// * `track_json` — Raw API JSON for the matched track, if available
pub fn write_tags_from_registry(
    tag: &mut Tag,
    registry: &TagRegistry,
    album_json: &serde_json::Value,
    track_json: Option<&serde_json::Value>,
) {
    if album_json.is_null() {
        return;
    }

    for def in &registry.album_tags {
        if let Some(raw_value) = registry::extract_json_value(album_json, &def.json_path) {
            if let Some(string_value) = registry::value_to_string(&raw_value, &def.value_type) {
                for atom in &def.atoms {
                    tag.set_data(
                        FreeformIdent::new_borrowed(atom.namespace, &atom.name),
                        Data::Utf8(string_value.clone()),
                    );
                }
            }
        }
    }

    if let Some(track_json) = track_json {
        if !track_json.is_null() {
            for def in &registry.track_tags {
                if let Some(raw_value) =
                    registry::extract_json_value(track_json, &def.json_path)
                {
                    if let Some(string_value) =
                        registry::value_to_string(&raw_value, &def.value_type)
                    {
                        for atom in &def.atoms {
                            tag.set_data(
                                FreeformIdent::new_borrowed(atom.namespace, &atom.name),
                                Data::Utf8(string_value.clone()),
                            );
                        }
                    }
                }
            }
        }
    }
}

// ============================================================
// Always-On Local Tags
// ============================================================

/// Write always-on local tags that don't require any API calls.
///
/// Tags written:
///   - `SourceStore = Apple Music` (iTunes + MeedyaMeta namespaces)
///   - `EncodeSource = Web`
///   - `iTunesMediaType = Music`
///   - `isMedley = Y` (only when title contains "Medley", case-insensitive)
pub fn write_local_tags(tag: &mut Tag) {
    let is_medley = tag
        .title()
        .is_some_and(|t| t.to_ascii_lowercase().contains("medley"));

    tag.set_data(
        FreeformIdent::new_static(ITUNES_NAMESPACE, "SourceStore"),
        Data::Utf8("Apple Music".to_owned()),
    );
    tag.set_data(
        FreeformIdent::new_static(MEEDYA_NAMESPACE, "SourceStore"),
        Data::Utf8("Apple Music".to_owned()),
    );

    tag.set_data(
        FreeformIdent::new_static(ITUNES_NAMESPACE, "EncodeSource"),
        Data::Utf8("Web".to_owned()),
    );

    tag.set_data(
        FreeformIdent::new_static(ITUNES_NAMESPACE, "iTunesMediaType"),
        Data::Utf8("Music".to_owned()),
    );

    if is_medley {
        tag.set_data(
            FreeformIdent::new_static(ITUNES_NAMESPACE, "isMedley"),
            Data::Utf8("Y".to_owned()),
        );
    }
}

// ============================================================
// ISRC Vendor Extraction
// ============================================================

/// Reconcile ISRC between the standardised ISRC atom and the Apple Vendor tag.
///
/// Apple Music M4A files contain a freeform atom under `com.apple.iTunes`
/// with the name `Vendor`, whose value follows the pattern
/// `Label:isrc:ISRCCODE` (e.g., `Warner:isrc:USAT22504136`).
///
/// Three cases:
///   1. ISRC blank, Vendor has ISRC → copy Vendor ISRC to standardised tag
///   2. ISRC set, Vendor has ISRC → if different, append Vendor ISRC as
///      additional value; if identical, do nothing
///   3. ISRC set, no Vendor ISRC → do nothing
pub fn extract_isrc_from_vendor(tag: &mut Tag) {
    let vendor_ident = FreeformIdent::new_static(ITUNES_NAMESPACE, "Vendor");
    let vendor_value = match tag.strings_of(&vendor_ident).next() {
        Some(v) => v.to_owned(),
        None => return,
    };

    let lower = vendor_value.to_ascii_lowercase();
    let vendor_isrc = match lower.find(":isrc:") {
        Some(pos) => {
            let isrc_start = pos + ":isrc:".len();
            let extracted = vendor_value[isrc_start..].trim();
            if extracted.is_empty() {
                return;
            }
            extracted.to_string()
        }
        None => return,
    };

    let isrc_ident = FreeformIdent::new_static(ITUNES_NAMESPACE, "ISRC");
    let existing_isrc: Option<String> = tag.strings_of(&isrc_ident).next().map(|s| s.to_owned());

    match existing_isrc {
        None => {
            log::debug!("ISRC empty — setting from Vendor tag: {vendor_isrc}");
            tag.set_data(isrc_ident, Data::Utf8(vendor_isrc));
        }
        Some(ref current) if current == &vendor_isrc => {
            log::debug!("ISRC matches Vendor tag: {vendor_isrc}");
        }
        Some(ref current) => {
            let combined = format!("{current} / {vendor_isrc}");
            log::debug!(
                "ISRC mismatch — API={current}, Vendor={vendor_isrc} — storing both: {combined}"
            );
            tag.set_data(isrc_ident, Data::Utf8(combined));
        }
    }
}

// ============================================================
// File Utilities
// ============================================================

/// Tag a single M4A file by opening it, applying the writer function,
/// and saving the modified metadata back to disk.
pub fn tag_single_file(path: &Path, tag_writer: &dyn Fn(&mut Tag)) -> Result<(), String> {
    let mut tag = Tag::read_from_path(path)
        .map_err(|e| format!("Failed to read M4A metadata from {}: {}", path.display(), e))?;

    tag_writer(&mut tag);

    tag.write_to_path(path)
        .map_err(|e| format!("Failed to write M4A metadata to {}: {}", path.display(), e))?;

    log::debug!("Tagged: {}", path.display());
    Ok(())
}

/// Recursively walk a directory tree and tag all M4A files found.
/// Returns the count of successfully tagged files.
pub fn tag_directory_recursive(dir: &Path, tag_writer: &dyn Fn(&mut Tag)) -> usize {
    let mut count = 0;

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::debug!("Cannot read directory {}: {}", dir.display(), e);
            return 0;
        }
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();

        if entry_path.is_dir() {
            count += tag_directory_recursive(&entry_path, tag_writer);
        } else if is_m4a(&entry_path) {
            match tag_single_file(&entry_path, tag_writer) {
                Ok(()) => count += 1,
                Err(e) => {
                    log::debug!("Skipping {}: {}", entry_path.display(), e);
                }
            }
        }
    }

    count
}

/// Check whether a file path has an `.m4a` extension (case-insensitive).
pub fn is_m4a(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("m4a"))
}

/// Collect all M4A file paths from a path (file or directory, recursive).
pub fn collect_m4a_files(output_path: &str) -> Vec<std::path::PathBuf> {
    let path = Path::new(output_path);
    let mut files = Vec::new();

    if path.is_file() {
        if is_m4a(path) {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        collect_m4a_recursive(path, &mut files);
    }

    files.sort();
    files
}

fn collect_m4a_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_m4a_recursive(&entry_path, files);
        } else if is_m4a(&entry_path) {
            files.push(entry_path);
        }
    }
}
