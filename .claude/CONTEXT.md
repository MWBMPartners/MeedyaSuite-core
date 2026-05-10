# MeedyaSuite-core — Project Context

> Snapshot maintained for Claude Code sessions. Reflects the actual state of `main`, not aspirational state.
> Last updated: 2026-05-10.

## What this repo is

`MeedyaSuite-core` is the shared Rust workspace consumed by the MeedyaSuite app family:

- **MeedyaConverter** (Swift, macOS) — audio/video conversion + tagging
- **MeedyaManager** (Rust + Swift/C#/GTK4) — local library management + tagging
- **MeedyaDL** (Rust/Tauri + React/TS) — store downloads (Apple Music etc.)
- **MeedyaPlayer** (planned) — MeedyaSuite-native media player

Apps consume this via direct Cargo git dependency (Rust apps) or C FFI / WASM bindings (Swift/web). No app-specific logic lives in this workspace.

## Workspace state on `main`

| Crate | Purpose | Status | Tests |
|---|---|---|---|
| [meedya-metadata](crates/meedya-metadata/) | Config-driven M4A tagging (Apple Music JSON → freeform atoms), codec ID tags, playback bounds | **Implemented** | 31 |
| [meedya-library-import](crates/meedya-library-import/) | Ingest playback bounds + metadata from external library DBs (iTunes XML, CUE) | **Implemented** | 30 |
| [meedya-tags-extended](crates/meedya-tags-extended/) | Multi-format tag I/O with DJ metadata support (foundation; proprietary readers pending) | **Implemented** | 29 |
| [meedya-codecs](crates/meedya-codecs/) | Audio/video codec enums, container formats, classification | **Placeholder** | 0 |
| [meedya-db](crates/meedya-db/) | MeedyaDB API client, media record models, export trait | **Placeholder** | 0 |
| [meedya-fingerprint](crates/meedya-fingerprint/) | AcoustID + ReplayGain/EBU R128 | **Placeholder** | 0 |

**Total: 90 tests on `main`.** Workspace builds clean.

> **There is significantly more implementation on branch `claude/interesting-mirzakhani`** — codecs/db/fingerprint were previously implemented there (55 tests total) along with `meedya-providers`. That work has not been merged to `main`. Treat it as a reference, not the source of truth for current state. See [HISTORY.md](HISTORY.md).

## Module-level detail (implemented crates only)

### meedya-metadata

- [src/registry.rs](crates/meedya-metadata/src/registry.rs) — Loads [tags.toml](crates/meedya-metadata/tags.toml) at compile time. `TAG_REGISTRY` static; JSON path extraction (`extract_json_value`); value type conversion (`value_to_string`).
- [src/writer.rs](crates/meedya-metadata/src/writer.rs) — `write_tags_from_registry` (Apple Music JSON → atoms), `write_local_tags` (SourceStore/EncodeSource/iTunesMediaType/isMedley), `extract_isrc_from_vendor`, file I/O helpers. Built on `mp4ameta`.
- [src/codec_tags.rs](crates/meedya-metadata/src/codec_tags.rs) — `CodecKind` enum (Lossless/Atmos/DolbyDigital/Binaural/Downmix/StandardLossy) and tag writers per codec.
- [src/playback_bounds.rs](crates/meedya-metadata/src/playback_bounds.rs) — User-supplied soft playback start/stop atoms (iTunes-Start-Time analog, MeedyaSuite-only). Writes `PlaybackStartMs` + `PlaybackStart` (HH:MM:SS.mmm) pair per endpoint.

**Adding a new tag**: edit [tags.toml](crates/meedya-metadata/tags.toml), zero Rust code changes (per album.* / track.* convention with json_path + value_type + atoms[]).

### meedya-library-import

- [src/itunes_xml.rs](crates/meedya-library-import/src/itunes_xml.rs) — Parses `iTunes Music Library.xml`; emits `LibraryEntry` per track with `Start Time` and/or `Stop Time`. Cross-platform `file://` URL decoding (detects Windows drive letters by shape).
- [src/cuesheet.rs](crates/meedya-library-import/src/cuesheet.rs) — Full CUE parser (`parse_str`, `parse_file`) returning rich `CueSheet { catalog, performer, title, rems, files }` with `CueTime { minutes, seconds, frames }` at CD-frame precision. `import()` adapter emits LibraryEntries only for per-track files with non-zero `INDEX 01`; single-file album rips emit warnings (chapter authoring path, not trim).

**LibraryEntry** is the normalized output: `{ locator: Path | PersistentId { kind, value }, start_ms, stop_ms }`. Matching to local files is the consuming app's job — this crate doesn't touch the filesystem beyond reading the source.

### meedya-tags-extended (foundation only; proprietary readers pending)

- [src/io.rs](crates/meedya-tags-extended/src/io.rs) — `TagFile`: lofty-based open/edit/save with foreign-frame pass-through. `primary_tag()`, `primary_tag_mut()`, typed-tag access.
- [src/model.rs](crates/meedya-tags-extended/src/model.rs) — `ExtendedTags`, `Source` enum, `CuePoint`, `LoopPoint`, `BeatGrid`, `Rgb`, `MusicalKey` with Camelot/Open Key/traditional round-tripping.
- [src/standard.rs](crates/meedya-tags-extended/src/standard.rs) — `read_bpm`/`write_bpm`/`clear_bpm`, `read_key`/`write_key`/`read_key_raw`/`write_key_raw`/`clear_key`, `read_comment`/`write_comment`/`clear_comment`. Covers Mixed In Key fully.

**Pending** (one session each, requires DJ-tagged sample files): Serato (Markers2/Autotags/BeatGrid), Rekordbox (ID3 PRIV + XML sidecar), Traktor (cue frames + collection.nml), Virtual DJ (.vdj sidecar + embedded markers).

## Key design decisions

1. **Two tag-I/O foundations coexist.** `meedya-metadata` uses `mp4ameta` (M4A/MP4 only) for the Apple Music tagging flow. `meedya-tags-extended` uses `lofty` (multi-format: MP3/M4A/FLAC/WAV/AIFF/OGG/MKV) for the DJ-metadata + general-purpose flow. Not unified — they serve different code paths.

2. **Pass-through preservation.** `meedya-tags-extended::TagFile` round-trips unknown frames automatically (lofty design). MeedyaConverter re-encodes don't strip Serato/Rekordbox/Traktor blobs even when we don't model them.

3. **Config-driven where possible.** [tags.toml](crates/meedya-metadata/tags.toml) defines Apple Music JSON → atom mappings declaratively; no Rust changes to add a tag.

4. **Library importers don't match files.** `meedya-library-import` emits normalized records with `EntryLocator::{ Path | PersistentId }`; the consuming app handles filesystem resolution.

5. **MeedyaMeta atom namespace** (`MeedyaMeta`) is for MeedyaSuite-only fields that don't have standard equivalents (playback bounds, custom cue points). `com.apple.iTunes` namespace is used when the field has player compatibility precedent.

## Build / test

```bash
cargo build --workspace       # all crates
cargo test  --workspace       # 90 tests
cargo test -p meedya-metadata # single crate
cargo build -p meedya-tags-extended
```

Workspace uses Rust edition 2021, MIT license, copyright header `// Copyright (c) 2024-2026 MWBM Partners Ltd` on every source file.

## Cross-repo coordination

Each downstream app (MeedyaConverter, MeedyaManager, MeedyaDL) has a `claude/core-integration` branch where it adopts this workspace as a dependency. See auto-memory `project_core_integration.md` for the integration kickoff context.

## What NOT to assume

- Don't assume codecs/db/fingerprint crates have implementations — they're placeholders on `main`. Use the `claude/interesting-mirzakhani` branch only as a reference for what those eventually look like.
- Don't conflate `meedya-metadata` (Apple Music JSON tagger, mp4ameta) with `meedya-tags-extended` (multi-format DJ-aware reader/writer, lofty). They're separate by design.
- Don't push proprietary DJ reader implementations into `meedya-tags-extended` from memory — every Serato/Rekordbox/Traktor format needs validation against real fixture files. See [PROMPTS.md](PROMPTS.md#implementing-a-proprietary-dj-reader).
