# MeedyaSuite-core — Project Context

> Snapshot maintained for Claude Code sessions. Reflects the actual state of `main`, not aspirational state.
> Last updated: 2026-09-01 — **branch-consolidation pass**. The four in-flight WIP branches were audited by full file content and merged into a single branch, `feature/work-in-progress` (based on `main`, eventual PR target `main`). This file describes that branch. See [HANDOFF.md](HANDOFF.md) for in-flight state and [HISTORY.md](HISTORY.md) for the narrative.

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
| [meedya-metadata](../crates/meedya-metadata/) | Two coexisting tag I/O surfaces: `lofty`-backed (multi-format) and `mp4ameta`-backed (sandbox-safe). Tag registry, JSON path extraction, codec ID tags, playback bounds, cross-repo `identifier_types` registry (#65). | **Implemented** | 112 |
| [meedya-tags-extended](../crates/meedya-tags-extended/) | Multi-format DJ metadata (lofty). `ExtendedTags`/`MusicalKey`/`CuePoint`/`LoopPoint`/`BeatGrid`. Standard BPM+key+comment + Mixed In Key reader (`mik`). Other proprietary readers pending. | **Implemented (foundation + MIK)** | 180 |
| [meedya-library-import](../crates/meedya-library-import/) | External library ingestion: iTunes XML, CUE sheets. Emits normalized `LibraryEntry` records. | **Implemented** | 30 |
| [meedya-lyrics](../crates/meedya-lyrics/) | LRCLIB client, LRC parser/writer, sidecar I/O, plain-text and SYLT tag-embed. | **Implemented** | 128 |
| [meedya-providers](../crates/meedya-providers/) | Provider framework: traits, capabilities, rate limiting, credentials, cover art, fuzzy match scoring, Lucene/Solr query escaping (`lucene`). In-repo `MetadataProvider` impls (feature-gated): MusicBrainz, Spotify, Apple Music, Deezer, TMDB, TheTVDB, OMDb, Apple TV, iTunes Store, Apple Podcasts, ISRC, EIDR, ISWC. | **Implemented** | 49 (177 all-features) |
| [meedya-fingerprint](../crates/meedya-fingerprint/) | AcoustID client + ReplayGain EBU R128 analyser. Pure-Rust Chromaprint (no fpcalc). | **Implemented** | 6 |
| [meedya-db](../crates/meedya-db/) | MeedyaDB API client + `Track`/`Album`/`Artist` models + `DbExporter` trait. | **Implemented** | 3 |
| [meedya-core](../crates/meedya-core/) | Facade re-exporting all implemented crates behind feature flags. | **Implemented** | — |

**Total: 555 tests with default features, 688 with `--all-features`** (the CI configuration) on `feature/work-in-progress`. All passing, 0 failing. `main` measures **601** with `--all-features`.

> **Measured, not carried forward.** From `cargo test --workspace [--all-features]` run on 2026-09-01. Earlier revisions accumulated a narrative of incremental deltas (466 → 511 → 533 → 546 → 664) that had drifted from reality. Doc-count drift is this repo's chronic failure mode: **only ever write a number you just measured.** CI guarding is tracked in issue #71.

Per-crate, `--all-features`: `meedya-codecs` 47 · `meedya-core` 0 · `meedya-db` 3 · `meedya-fingerprint` 11 · `meedya-library-import` 30 · `meedya-lyrics` 128 · `meedya-metadata` 112 · `meedya-providers` 177 · `meedya-tags-extended` 180. `meedya-providers` measures 49 with default features (provider impls are feature-gated).

> **Public API specification for partner apps**: see [`docs/API.md`](../docs/API.md). Keep that file in sync with public API changes — see the standing task in [CLAUDE.md](CLAUDE.md#standing-tasks).

## Module-level detail

### meedya-codecs

Public surface: `AudioCodec` (42+ variants), `VideoCodec` (21+), `ContainerFormat` (36+), `ChannelConfig`, `HdrFormat`, `SpatialAudioFormat`, `SpatialType`, `SubtitleCodec`, `CodecRegistry`, `MediaClassification`. Modules: `audio_codec`, `video_codec`, `container`, `channel_config`, `classify`, `ffprobe`, `mediainfo`, `hdr`, `spatial`, `spatial_type`, `subtitle_codec`, `registry`, `tool_path`.

### meedya-metadata

Two surfaces coexist by design:

- **`lofty`-backed**: `common_tags` (CommonTag enum — `#[non_exhaustive]` as of #65/0.2.0, STANDARD_NAMESPACES), `tag_io` (read_tags, write_tags, write_registry_tags, write_acoustid_tags, write_replaygain_tags, TagMap), `tag_registry` (TagDefinition, TagRegistry, TagScope, TagValueType, AtomTarget), `json_path`.
- **`identifier_types`** (#65) — cross-repo identifier-type registry loaded from [identifier_types.toml](../crates/meedya-metadata/identifier_types.toml) (scope→slug→validation vocabulary; DATA, not an enum). `IdentifierType`/`IdentifierScope`/`IdentifierStatus`/`IdentifierValidation`; `identifier_types()`/`identifier_type()`/`active_identifier_slugs()`; raw artifact re-exported as `IDENTIFIER_TYPES_TOML`. Guard-held: `crates/meedya-metadata/tests/identifier_registry_guard.rs` declares the expected active/reserved slug sets and fails CI if the artifact drifts from that declaration or from `CommonTag::identifier_slug()`.
- **`mp4ameta`-backed (sandbox-safe)**: `registry` (TAG_REGISTRY static loaded from [tags.toml](../crates/meedya-metadata/tags.toml)), `writer` (`write_tags_from_registry`, `write_local_tags`, `extract_isrc_from_vendor`), `codec_tags` (CodecKind enum + per-codec writers), `playback_bounds` (`set_playback_start/stop`, `get_playback_*_ms`, `clear_*`).

**Adding a new tag**: edit `tags.toml`, zero Rust changes (PROMPTS.md has the template). **Adding a new identifier type**: edit `identifier_types.toml` + update the expected-slug guard, zero other Rust changes.

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

Provider framework. Re-exports: `MetadataProvider`, `ProviderCapabilities`, `ProviderError`, `SearchQuery`, `ProviderResult`, `MediaType`, `CoverArtInfo`, `CoverArtSize`, `CredentialStore`, `CredentialSource`, `ResolvedCredential`, `MatchScorer`, `ScoringWeights`, `ProviderRateLimiter`, `RateLimiterRegistry`, `escape_lucene`, `quote_phrase`, `phrase_clause`. Modules: `traits`, `types`, `cover_art`, `credentials`, `match_scoring`, `rate_limiter`, `lucene` (Lucene/Solr query escaping — always compiled, no feature gate), `providers` (feature-gated concrete `MetadataProvider` impls: `musicbrainz`, `isrc`, `iswc`, `spotify`, `apple_music`, `deezer`, `tmdb`, `thetvdb`, `omdb`, `apple_tv`, `itunes_store`, `apple_podcasts`, `eidr`).

**MusicBrainz Solr 9→10 search hardening (2026-09-01)**: audited the announced breaking tickets (SEARCH-444/642/666/752/764) against `musicbrainz`/`isrc`/`iswc` — none hit us (we never search `area`/`url`/`cdstub`/`tag`, never read relationship `target` or release `quality`, and all response parsers are serde-derive structs that ignore unknown fields). The real risk was our own unescaped Lucene query construction under the stricter Solr 10 parser; `lucene::{escape_lucene, quote_phrase, phrase_clause}` now hardens every user-supplied value going into a query, `MusicBrainzProvider::build_lucene_query` replaced the old dead `search_term` fallback, and ISRC/ISWC queries are normalised before being embedded.

The three `lucene` helpers implement **two different escaping regimes and are not interchangeable**: inside a quoted phrase only `\` and `"` are structurally significant (escaping the full special set there would embed literal backslashes into the phrase and kill the match), while a bare term needs the full 19-character set escaped. Both prior branches had conflated these.

**Identifier query forms were settled by live probing**, not documentation — MusicBrainz documents neither: ISRC matches compact (`isrc:GBAYE0601498`) and misses hyphenated; ISWC matches only in the stored dotted display form (`iswc:"T-304.031.869-8"`), while the compact form returns 0 results and the hyphen-only form is a parse error. Hence the deliberate asymmetry between `normalise_isrc` and `format_iswc_dotted`. Forward-compat parse fixtures (Solr-10-shaped response JSON with `relations`/`quality`/`release-group`/`genres` noise) prove the parsers are unaffected. Genre search (SEARCH-681) deferred — see [HISTORY.md](HISTORY.md).

### meedya-fingerprint

- `acoustid` — `AcoustIdClient`, `AcoustIdResult`. Pure-Rust Chromaprint, no fpcalc binary. Rate-limited.
- `replaygain` — `ReplayGainAnalyzer`, `ReplayGainResult`, `AlbumGainResult`, `DEFAULT_REFERENCE_LEVEL` (-18 LUFS).

### meedya-db

`MeedyaDbClient` (api.meedya.tv/v1), `DbExporter` trait, `MediaRecord`/`Track`/`Album`/`Artist` models.

### meedya-core

Facade with feature flags (`metadata` / `codecs` / `fingerprint` / `lyrics` / `providers` / `tags-extended` / `library-import` / `db` / `keyring` / `full`). All implemented crates re-exported as top-level modules. `meedya_core::prelude` re-exports common types: `CommonTag`, `IdentifierType`, `MetadataError`, `TagRegistry`, `AudioCodec`, `ChannelConfig`, `CodecRegistry`, `ContainerFormat`, `SpatialType`, `MetadataProvider`, `ProviderCapabilities`, `CredentialStore`, `ProviderRateLimiter`, `ProviderResult`, `SearchQuery`, `Lyrics`, `LyricsProvider`, `SyncedLine`, `TrackQuery`, `TagFile`, `ExtendedTags`, `MusicalKey`, `KeyMode`, `Note`, `CuePoint`, `LoopPoint`, `BeatGrid`, `Source`, `LibraryEntry`, `EntryLocator`, `ImportReport`, `SourceInfo`.

## Key design decisions

0. **Standards-first** (project-wide policy). Use standard metadata tags (ID3v2 / Vorbis / MP4 ilst spec fields) wherever they exist. Fall back to `MeedyaMeta:*` freeform atoms only when no standard equivalent exists — e.g., DJ energy ratings, playback bounds, audit trails. See [CLAUDE.md → Key design principles](CLAUDE.md#key-design-principles).
1. **Two tag-I/O foundations coexist.** `mp4ameta` for sandbox-safe Apple Music flow; `lofty` for multi-format DJ-metadata and general pass-through. Not unified — they serve different code paths.

2. **Pass-through preservation.** `meedya-tags-extended::TagFile` round-trips unknown frames automatically (lofty design). MeedyaConverter re-encodes don't strip Serato/Rekordbox/Traktor blobs even when we don't model them.

3. **Config-driven where possible.** [tags.toml](../crates/meedya-metadata/tags.toml) declarative; no Rust changes to add a tag.

4. **Library importers don't match files.** Normalized records with `EntryLocator::{ Path | PersistentId }`; consuming apps handle filesystem resolution.

5. **MeedyaMeta atom namespace** is for MeedyaSuite-only fields without standard equivalents (playback bounds, custom cue points). `com.apple.iTunes` namespace is used when the field has player compatibility precedent.

6. **Results only, not side effects.** Crates return data; consumers handle I/O. `meedya-fingerprint` exemplifies this — it produces `AcoustIdResult` / `ReplayGainResult`, and `meedya-metadata::tag_io::write_acoustid_tags` / `write_replaygain_tags` handles file writes.

7. **Fixture-based testing for proprietary parsers.** Won't write Serato/etc parsers from memory — every format needs validation against real DJ-tagged sample files. See [PROMPTS.md → Implementing a proprietary DJ reader](PROMPTS.md#implementing-a-proprietary-dj-reader).

8. **Identifier vocabulary is data, not an enum** (#65). `identifier_types.toml` is the cross-repo scope→slug→validation registry; adding an identifier type is a TOML edit plus one line in the guard test's expected-slug declaration — never a new Rust type. `CommonTag` is `#[non_exhaustive]` — new variants are reserved for tags with a genuine per-container frame mapping (ID3v2/Vorbis/MP4 ilst); a bare external identifier with no container frame belongs in the registry instead.

## Build / test

```bash
cargo build --workspace          # all 9 crates
cargo test  --workspace          # 555 tests
cargo test  --workspace --all-features   # 688 tests (the CI configuration)
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
