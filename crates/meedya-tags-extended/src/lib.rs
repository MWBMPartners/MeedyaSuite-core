// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-tags-extended — Multi-format tag I/O with DJ metadata support.
//
// Foundation for reading and writing tag metadata across the formats
// MeedyaSuite apps care about (MP3, M4A/MP4, FLAC, WAV, AIFF, OGG, MKV).
// Built on `lofty`, which preserves unrecognised frames across read/save
// cycles — giving MeedyaConverter pass-through of foreign DJ blobs
// (Serato Markers2, Rekordbox PRIV, etc.) without us having to model them.
//
// ## Modules
//
// - `model`     — Unified data model (`ExtendedTags`, `MusicalKey`,
//                 `CuePoint`, `BeatGrid`, `Source`).
// - `io`        — `TagFile`: open/edit/save with foreign-frame preservation.
// - `standard`  — BPM, key, comment via standard non-proprietary tags.
//                 Covers Mixed In Key fully (it only writes standard tags).
// - `mik`       — Mixed In Key reader. Recovers key/energy/tempo from
//                 every location MIK is documented to write to (standard
//                 fields, artist/title/comment/grouping/label prefixes,
//                 suffixes and overwrites), then normalises into standard
//                 `InitialKey` / `IntegerBpm` / `Bpm` plus
//                 `MeedyaMeta:Energy` (no standard for energy).
//
// ## Future modules (proprietary readers — one per session)
//
// - `serato`    — Markers2 (cues, loops), Autotags (BPM/gain/key),
//                 BeatGrid. GEOB frames in ID3v2; freeform atoms in MP4.
// - `rekordbox` — ID3v2 PRIV frames + sidecar `rekordbox.xml`.
// - `traktor`   — NI cue frames + sidecar `collection.nml`.
// - `virtualdj` — `.vdj` sidecar + embedded markers.
//
// Each proprietary reader populates the same `ExtendedTags` shape, with
// `Source` recording origin so consumers can disambiguate when fields
// conflict.

pub mod ai_content;
pub mod io;
pub(crate) mod meedya_atom;
pub mod mik;
pub mod model;
pub mod play_history;
pub mod standard;
pub mod stems;

pub use ai_content::{
    clear_ai_content, parse_bool_truthy, read_ai_content, write_ai_content, AiContentFlags,
};
pub use io::TagFile;
pub use mik::{
    normalise_to_standards, read_mik, MikAnalysis, MikField, MikKinds, MikPosition,
    MikSourceLocation,
};
pub use model::{
    BeatGrid, BeatGridMarker, CuePoint, EnergyValue, ExtendedTags, KeyMode, LoopPoint, MusicalKey,
    Note, Rgb, Source,
};
pub use play_history::{
    clear_play_history, read_play_history, record_play, record_skip, write_play_history,
    PlayHistory,
};
pub use stems::{clear_stems, read_stems, write_stems, StemMetadata, StemRole, StemSource};
