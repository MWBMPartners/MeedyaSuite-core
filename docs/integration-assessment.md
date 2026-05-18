# MeedyaSuite-core Integration Assessment

**Original date**: 2026-04-08
**Last updated**: 2026-05-18
**Status**: **Implementation substantially complete.** 9 of 9 workspace crates implemented (211 tests passing). Downstream-app adoption pending.

> This document captures the original cross-project duplication analysis from 2026-04-08 when all core crates were placeholder stubs. The findings below are preserved as historical context; for the current state of the implementation, see [`README.md`](../README.md) and [`docs/API.md`](API.md).

## Current implementation status (2026-05-18)

| Crate | Status | Tests | Resolves |
|---|---|---|---|
| `meedya-codecs` | ✅ Implemented | 47 | "Codec/Format Definitions" dup (HIGH) |
| `meedya-metadata` | ✅ Implemented | 59 | "Metadata Tag Registry" dup (HIGH) |
| `meedya-tags-extended` | ✅ Implemented (foundation) | 29 | DJ-metadata reading/writing (NEW scope, not in original assessment) |
| `meedya-library-import` | ✅ Implemented | 30 | iTunes Library / CUE sheet ingestion (NEW scope) |
| `meedya-lyrics` | ✅ Implemented | 10 | Shared lyrics fetch/parse/write path |
| `meedya-providers` | ✅ Implemented | 27 | "Metadata Provider Framework" (MEDIUM) |
| `meedya-fingerprint` | ✅ Implemented | 6 | "Audio Fingerprinting + ReplayGain" (MEDIUM) |
| `meedya-db` | ✅ Implemented | 3 | "MeedyaDB API Client + Records" (MEDIUM) |
| `meedya-core` | ✅ Implemented (facade) | — | Unified consumption with feature flags |

**Total: 211 tests passing**. See [`README.md`](../README.md) for current capabilities and [`docs/API.md`](API.md) for the public API surface.

### Outstanding work (proprietary DJ readers)

`meedya-tags-extended` ships foundation only. Proprietary DJ-software metadata readers (Serato Markers2, Rekordbox PRIV+XML, Traktor cue, Virtual DJ sidecar+embedded) each require their own focused session against real DJ-tagged fixture files. See [`.claude/PROMPTS.md`](../.claude/PROMPTS.md#implementing-a-proprietary-dj-reader) for the procedure.

### Downstream adoption status

| Project | Language | Adoption status |
|---|---|---|
| MeedyaDL | Rust/Tauri | Not yet — see [`docs/cross-repo-issues.md`](cross-repo-issues.md) |
| MeedyaConverter | Swift 6 | Blocked on Swift bindings ([`bindings/swift/`](../bindings/swift/) — not yet scaffolded) |
| MeedyaManager | Rust + Swift/C#/GTK4 | Not yet |
| MeedyaDB | Empty scaffold | Not started |

---

## Original 2026-04-08 Assessment

The remainder of this document is the original analysis, retained for historical reference and to inform any future scope decisions.

## Executive Summary

All four MeedyaSuite-core crates are currently placeholder stubs. This document
captures the findings from a thorough review of all Meedya projects to identify
what shared functionality should be extracted into each crate.

Three active projects (MeedyaDL, MeedyaConverter, MeedyaManager) contain
significant duplicated code across codec definitions, metadata handling,
audio fingerprinting, and database integration. MeedyaDB is an empty scaffold.

## Project Maturity

| Project | Language | Issues | Status |
|---------|----------|--------|--------|
| MeedyaDL | Rust/Tauri + React/TS | 349 (27 open) | Active, v0.28.1 |
| MeedyaConverter | Swift 6, macOS 15+ | ~370 (10 open) | Active, v0.1.0 alpha |
| MeedyaManager | Rust + Swift/C#/GTK4 | 131 (2 open) | Active, 8 Rust crates |
| MeedyaDB | None | 0 | Empty (README only) |

## Duplication Map

### 1. Codec/Format Definitions (ALL 3 repos) — HIGH

All three repos independently define audio codec enums, video codec enums,
container format lists, extension-to-format mappings, and codec-container
compatibility rules:

- **MeedyaDL**: `codec_registry.rs` + `codecs.toml`
- **MeedyaManager**: `filetype_registry.rs` + `classify/mod.rs` + `filetypes.json5`
- **MeedyaConverter**: `AudioCodec.swift` (42 cases), `VideoCodec.swift` (21 cases), `ContainerFormat.swift` (28 cases), `SubtitleFormat.swift` (15 cases)

### 2. Metadata Tag Registry (2 Rust repos) — HIGH

Nearly identical `TagRegistry`/`TagDefinition`/`TagValueType` implementations:

- **MeedyaDL**: `tag_registry.rs` + `tags.toml` + `metadata_tag_service.rs`
- **MeedyaManager**: `metadata/tag_registry.rs` + `tags.json5`

Both use the same config-driven pattern with `include_str!()` + `LazyLock`.

### 3. Audio Fingerprinting (2 repos) — MODERATE

- **MeedyaDL**: `acoustid_service.rs` (pure Rust Chromaprint) + `replaygain_service.rs`
- **MeedyaConverter**: `AudioFingerprinter.swift` (Chromaprint via fpcalc CLI)

### 4. MeedyaDB API Client (1 repo, planned for 2 others) — LOW (forward-looking)

- **MeedyaConverter**: `MeedyaDBClient` at `api.meedya.tv/v1` (search, match, API key auth)
- MeedyaDL and MeedyaManager reference MeedyaDB in planning docs

### 5. Metadata Providers (ALL 3 repos) — MODERATE

MusicBrainz, TMDB, TheTVDB, AcoustID, and others are integrated independently
in each project. MeedyaManager has the most comprehensive provider framework
(15+ providers with traits, auto-registration, rate limiting).

## Crate Implementation Plan

### `meedya-codecs` — Priority 1

**Source**: MeedyaDL `codec_registry.rs` + MeedyaManager `filetype_registry.rs`

Should provide:
- `AudioCodec` enum (42+ variants, spatial/object-based flags)
- `VideoCodec` enum (21+ variants, HDR flags)
- `SubtitleCodec` enum (15+ variants, bitmap vs text)
- `ContainerFormat` enum (28+ variants) with extensions, MIME types, codec compat
- `MediaClassification` (4-level: Group/Format/Class/Quality)
- `SpatialAudioFormat`, `HDRFormat` enums
- TOML/JSON5 config loading for extensible registries

### `meedya-metadata` — Priority 2

**Source**: MeedyaDL `tag_registry.rs` + MeedyaManager `metadata/tag_registry.rs`

Should provide:
- `TagRegistry`, `TagDefinition`, `TagValueType`, `AtomTarget` types
- Canonical `tags.toml` tag definitions
- JSON path extraction engine
- Value-to-string conversion
- Namespace constants
- MP4 freeform atom read/write abstraction (over `mp4ameta`/`lofty`)
- Template engine for file renaming (from MeedyaManager `rule_engine/`)

### `meedya-fingerprint` — Priority 3

**Source**: MeedyaDL `acoustid_service.rs` + `replaygain_service.rs`

Should provide:
- Chromaprint fingerprint generation (`rusty-chromaprint` + `symphonia`)
- AcoustID API lookup with rate limiting
- EBU R128 loudness measurement (FFmpeg ebur128)
- ReplayGain calculation (track + album)
- Multi-format tag writing (MP4, Vorbis, ID3v2)

### `meedya-db` — Priority 4

**Source**: MeedyaConverter `MeedyaDBClient` + MeedyaManager `mm-export/`

Should provide:
- MeedyaDB API client (search, match, lookup)
- Core media record types (Track, Album, Artist, Playlist)
- `DbExporter` trait + schema definitions
- Database backend support (SQLite, MySQL, PostgreSQL)

## MeedyaDL Integration Path

MeedyaDL is the recommended first consumer:

1. `meedya-codecs` — extract `codec_registry.rs` + `codecs.toml` (cleanest, TOML-driven)
2. `meedya-metadata` — extract `tag_registry.rs` + `tags.toml` (same pattern)
3. `meedya-fingerprint` — extract `acoustid_service.rs` + `replaygain_service.rs`
4. `meedya-db` — forward-looking (MeedyaDB API not yet live)

Both MeedyaDL and MeedyaManager are Rust, so they consume as direct Cargo
dependencies. MeedyaConverter (Swift) consumes via `bindings/swift` C FFI.

## Key Dependencies to Include

| Crate | Key Dependencies |
|-------|-----------------|
| `meedya-codecs` | `serde`, `toml` |
| `meedya-metadata` | `serde`, `toml`, `lofty`, `mp4ameta` |
| `meedya-fingerprint` | `rusty-chromaprint`, `symphonia`, `reqwest` |
| `meedya-db` | `reqwest`, `serde`, `sqlx` (optional) |
