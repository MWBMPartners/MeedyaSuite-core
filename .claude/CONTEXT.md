# MeedyaSuite-core — Project Context

> Snapshot maintained for Claude Code sessions. Reflects the actual state of `main`, not aspirational state.
> Last updated: 2026-05-18 (post issues #26 SYLT + #27 facade re-exports + #31 Mixed In Key reader, on feature branch `claude/feature-batch-2026-05-18`).

## What this repo is

`MeedyaSuite-core` is the shared Rust workspace consumed by the MeedyaSuite app family:

- **MeedyaConverter** (Swift, macOS) — audio/video conversion + tagging
- **MeedyaManager** (Rust + Swift/C#/GTK4) — local library management + tagging
- **MeedyaDL** (Rust/Tauri + React/TS) — store downloads (Apple Music etc.)
- **MeedyaPlayer** (planned) — MeedyaSuite-native media player
- **MeedyaDB** (Swift, planned)

Apps consume this via direct Cargo git dependency (Rust apps) or C FFI / WASM bindings (Swift/web — bindings not yet scaffolded). No app-specific logic lives in this workspace.

## Workspace state on `main`

| Crate | Purpose | Status | Tests |
|---|---|---|---|
| [meedya-codecs](../crates/meedya-codecs/) | Audio/video/subtitle codecs, container formats, HDR, spatial audio, classification, FFprobe + MediaInfo integration | **Implemented** | 47 |
| [meedya-metadata](../crates/meedya-metadata/) | Two coexisting tag I/O surfaces: `lofty`-backed (multi-format) and `mp4ameta`-backed (sandbox-safe). Tag registry, JSON path extraction, codec ID tags, playback bounds. | **Implemented** | 59 |
| [meedya-tags-extended](../crates/meedya-tags-extended/) | Multi-format DJ metadata (lofty). `ExtendedTags`/`MusicalKey`/`CuePoint`/`LoopPoint`/`BeatGrid`. Standard BPM+key+comment + Mixed In Key reader (`mik`). Other proprietary readers pending. | **Implemented (foundation + MIK)** | 61 |
| [meedya-library-import](../crates/meedya-library-import/) | External library ingestion: iTunes XML, CUE sheets. Emits normalized `LibraryEntry` records. | **Implemented** | 30 |
| [meedya-lyrics](../crates/meedya-lyrics/) | LRCLIB client, LRC parser/writer, sidecar I/O, plain-text and SYLT tag-embed. | **Implemented** | 15 |
| [meedya-providers](../crates/meedya-providers/) | Provider framework: traits, capabilities, rate limiting, credentials, cover art, fuzzy match scoring. | **Implemented** | 27 |
| [meedya-fingerprint](../crates/meedya-fingerprint/) | AcoustID client + ReplayGain EBU R128 analyser. Pure-Rust Chromaprint (no fpcalc). | **Implemented** | 6 |
| [meedya-db](../crates/meedya-db/) | MeedyaDB API client + `Track`/`Album`/`Artist` models + `DbExporter` trait. | **Implemented** | 3 |
| [meedya-core](../crates/meedya-core/) | Facade re-exporting all implemented crates behind feature flags. | **Implemented** | — |

**Total: 248 tests on feature branch (211 → 248 this batch).** Workspace builds clean.

> **Public API specification for partner apps**: see [`docs/API.md`](../docs/API.md). Keep that file in sync with public API changes — see the standing task in [CLAUDE.md](CLAUDE.md#standing-tasks).

## Module-level detail

### meedya-codecs

Public surface: `AudioCodec` (42+ variants), `VideoCodec` (21+), `ContainerFormat` (36+), `ChannelConfig`, `HdrFormat`, `SpatialAudioFormat`, `SpatialType`, `SubtitleCodec`, `CodecRegistry`, `MediaClassification`. Modules: `audio_codec`, `video_codec`, `container`, `channel_config`, `classify`, `ffprobe`, `mediainfo`, `hdr`, `spatial`, `spatial_type`, `subtitle_codec`, `registry`, `tool_path`.

### meedya-metadata

Two surfaces coexist by design:

- **`lofty`-backed**: `common_tags` (CommonTag enum, STANDARD_NAMESPACES), `tag_io` (read_tags, write_tags, write_registry_tags, write_acoustid_tags, write_replaygain_tags, TagMap), `tag_registry` (TagDefinition, TagRegistry, TagScope, TagValueType, AtomTarget), `json_path`.
- **`mp4ameta`-backed (sandbox-safe)**: `registry` (TAG_REGISTRY static loaded from [tags.toml](../crates/meedya-metadata/tags.toml)), `writer` (`write_tags_from_registry`, `write_local_tags`, `extract_isrc_from_vendor`), `codec_tags` (CodecKind enum + per-codec writers), `playback_bounds` (`set_playback_start/stop`, `get_playback_*_ms`, `clear_*`).

**Adding a new tag**: edit `tags.toml`, zero Rust changes (PROMPTS.md has the template).

### meedya-tags-extended

- [src/io.rs](../crates/meedya-tags-extended/src/io.rs) — `TagFile`: lofty-based open/edit/save with foreign-frame pass-through.
- [src/model.rs](../crates/meedya-tags-extended/src/model.rs) — `ExtendedTags`, `Source` enum, `CuePoint`, `LoopPoint`, `BeatGrid`, `Rgb`, `MusicalKey` (Camelot/Open Key/traditional round-tripping).
- [src/standard.rs](../crates/meedya-tags-extended/src/standard.rs) — BPM/key/comment read+write across all lofty-supported formats.
- [src/mik.rs](../crates/meedya-tags-extended/src/mik.rs) — Mixed In Key reader. `read_mik(tag) -> MikAnalysis` scans every documented MIK write location (standard fields, artist/title prefixes+suffixes, comment, grouping, label) and recovers key/energy/tempo. `normalise_to_standards(tag, &analysis)` writes the canonical values to standard tag fields (only Energy falls back to `MeedyaMeta:Energy` because no standard exists). Source fields are read-only — user data preserved.

**Pending** (one session each, fixture-driven): Serato (Markers2/Autotags/BeatGrid), Rekordbox (ID3 PRIV + XML sidecar), Traktor (cue frames + collection.nml), Virtual DJ (.vdj sidecar + embedded markers).

### meedya-library-import

- [src/itunes_xml.rs](../crates/meedya-library-import/src/itunes_xml.rs) — iTunes / Music.app XML parser; cross-platform `file://` URL decoding.
- [src/cuesheet.rs](../crates/meedya-library-import/src/cuesheet.rs) — Full CUE parser at CD-frame precision; rich `CueSheet { catalog, performer, title, rems, files }` model. `import()` adapter emits LibraryEntries only for narrow trim cases.

`LibraryEntry { locator: Path|PersistentId, start_ms, stop_ms }` is the normalized output. Filesystem matching is the consuming app's job.

### meedya-lyrics

- [src/provider/](../crates/meedya-lyrics/src/provider/) — `LyricsProvider` trait + `LrclibProvider`.
- [src/lrc.rs](../crates/meedya-lyrics/src/lrc.rs) — LRC parser/writer (`[mm:ss.xx]`).
- [src/sidecar.rs](../crates/meedya-lyrics/src/sidecar.rs) — `.lrc` sidecar writes.
- [src/embed.rs](../crates/meedya-lyrics/src/embed.rs) — Two embed paths: `embed()` writes plain text via `meedya-metadata::CommonTag::Lyrics` (USLT/©lyr/LYRICS); `embed_synced()` writes ID3v2 SYLT frames (errors on non-ID3v2 containers). UTF-16 BOM, MS timestamp format, lyrics content type.

### meedya-providers

Provider framework. Re-exports: `MetadataProvider`, `ProviderCapabilities`, `ProviderError`, `SearchQuery`, `ProviderResult`, `MediaType`, `CoverArtInfo`, `CoverArtSize`, `CredentialStore`, `CredentialSource`, `ResolvedCredential`, `MatchScorer`, `ScoringWeights`, `ProviderRateLimiter`, `RateLimiterRegistry`. Modules: `traits`, `types`, `cover_art`, `credentials`, `match_scoring`, `rate_limiter`.

### meedya-fingerprint

- `acoustid` — `AcoustIdClient`, `AcoustIdResult`. Pure-Rust Chromaprint, no fpcalc binary. Rate-limited.
- `replaygain` — `ReplayGainAnalyzer`, `ReplayGainResult`, `AlbumGainResult`, `DEFAULT_REFERENCE_LEVEL` (-18 LUFS).

### meedya-db

`MeedyaDbClient` (api.meedya.tv/v1), `DbExporter` trait, `MediaRecord`/`Track`/`Album`/`Artist` models.

### meedya-core

Facade with feature flags (`metadata` / `codecs` / `fingerprint` / `lyrics` / `providers` / `tags-extended` / `library-import` / `db` / `keyring` / `full`). All implemented crates re-exported as top-level modules. `meedya_core::prelude` re-exports common types: `CommonTag`, `MetadataError`, `TagRegistry`, `AudioCodec`, `ChannelConfig`, `CodecRegistry`, `ContainerFormat`, `SpatialType`, `MetadataProvider`, `ProviderCapabilities`, `CredentialStore`, `ProviderRateLimiter`, `ProviderResult`, `SearchQuery`, `Lyrics`, `LyricsProvider`, `SyncedLine`, `TrackQuery`, `TagFile`, `ExtendedTags`, `MusicalKey`, `KeyMode`, `Note`, `CuePoint`, `LoopPoint`, `BeatGrid`, `Source`, `LibraryEntry`, `EntryLocator`, `ImportReport`, `SourceInfo`.

## Key design decisions

0. **Standards-first** (project-wide policy). Use standard metadata tags (ID3v2 / Vorbis / MP4 ilst spec fields) wherever they exist. Fall back to `MeedyaMeta:*` freeform atoms only when no standard equivalent exists — e.g., DJ energy ratings, playback bounds, audit trails. See [CLAUDE.md → Key design principles](CLAUDE.md#key-design-principles).
1. **Two tag-I/O foundations coexist.** `mp4ameta` for sandbox-safe Apple Music flow; `lofty` for multi-format DJ-metadata and general pass-through. Not unified — they serve different code paths.

2. **Pass-through preservation.** `meedya-tags-extended::TagFile` round-trips unknown frames automatically (lofty design). MeedyaConverter re-encodes don't strip Serato/Rekordbox/Traktor blobs even when we don't model them.

3. **Config-driven where possible.** [tags.toml](../crates/meedya-metadata/tags.toml) declarative; no Rust changes to add a tag.

4. **Library importers don't match files.** Normalized records with `EntryLocator::{ Path | PersistentId }`; consuming apps handle filesystem resolution.

5. **MeedyaMeta atom namespace** is for MeedyaSuite-only fields without standard equivalents (playback bounds, custom cue points). `com.apple.iTunes` namespace is used when the field has player compatibility precedent.

6. **Results only, not side effects.** Crates return data; consumers handle I/O. `meedya-fingerprint` exemplifies this — it produces `AcoustIdResult` / `ReplayGainResult`, and `meedya-metadata::tag_io::write_acoustid_tags` / `write_replaygain_tags` handles file writes.

7. **Fixture-based testing for proprietary parsers.** Won't write Serato/etc parsers from memory — every format needs validation against real DJ-tagged sample files. See [PROMPTS.md → Implementing a proprietary DJ reader](PROMPTS.md#implementing-a-proprietary-dj-reader).

## Build / test

```bash
cargo build --workspace          # all 9 crates
cargo test  --workspace          # 248 tests
cargo test  -p meedya-metadata   # single crate
cargo doc   --workspace --no-deps --open  # exhaustive auto-generated reference
```

Workspace uses Rust edition 2021, MIT license, copyright header `// Copyright (c) 2026 MeedyaSuite` on new source files (older files retain `// Copyright (c) 2026 MeedyaSuite`).

## Cross-repo coordination

Each downstream app (MeedyaConverter, MeedyaManager, MeedyaDL) has or will have a `claude/core-integration` branch where it adopts this workspace as a dependency. Pre-drafted GitHub issues per app are in [`docs/cross-repo-issues.md`](../docs/cross-repo-issues.md). See auto-memory `project_core_integration.md` for the integration kickoff context.

## What NOT to assume

- Don't conflate `meedya-metadata` (mp4ameta + lofty surfaces) with `meedya-tags-extended` (lofty-only DJ-aware reader/writer). They're separate by design.
- Don't push proprietary DJ reader implementations into `meedya-tags-extended` from memory — every Serato/Rekordbox/Traktor format needs validation against real fixture files.
- Don't add features beyond what the task requires. Trait abstractions, optional fields, unused error variants — drop them.
- Don't update `docs/API.md` as a follow-up commit when the public API changes — partner apps consume it as the integration reference; stale spec produces silent integration bugs. Update it in the same commit as the code change.
