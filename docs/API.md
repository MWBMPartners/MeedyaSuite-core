# MeedyaSuite-core — Internal API Specification

> **Audience**: developers of partner apps (MeedyaDL, MeedyaConverter, MeedyaManager, MeedyaPlayer, MeedyaDB) integrating with `MeedyaSuite-core`.
>
> **Scope**: the public API surface of every crate in the workspace — what to import, what types to expect, how the crates compose. This document is the curated, human-readable reference; the exhaustive auto-generated reference is `cargo doc --workspace --no-deps --open`.
>
> **This is not a Swagger/OpenAPI spec.** `MeedyaSuite-core` is a Rust library workspace, not a web service. There are no HTTP endpoints. If you need an HTTP-shaped contract, build one in your downstream app on top of these crates.
>
> **Last refreshed**: 2026-09-02 (`feature/work-in-progress`: provider rate limiting wired up — #94 — on top of the #78/#79/#80/#81 hardening pass, plus the post-review hardening that followed — `apple_podcasts` routed through the shared `net_err` redaction helper, a sanitized-error-string helper + canary test added to `meedya-db`, and an untagged-Ogg regression fixture — test counts re-measured). See the [maintenance section](#maintenance) for how this stays in sync with the code.

---

## Table of Contents

- [Workspace overview](#workspace-overview)
- [Crate APIs](#crate-apis)
  - [`meedya-codecs`](#meedya-codecs)
  - [`meedya-core`](#meedya-core)
  - [`meedya-db`](#meedya-db)
  - [`meedya-fingerprint`](#meedya-fingerprint)
  - [`meedya-library-import`](#meedya-library-import)
  - [`meedya-lyrics`](#meedya-lyrics)
  - [`meedya-metadata`](#meedya-metadata)
  - [`meedya-providers`](#meedya-providers)
  - [`meedya-tags-extended`](#meedya-tags-extended)
- [Common workflows](#common-workflows)
- [Stability and versioning](#stability-and-versioning)
- [Consumption by language](#consumption-by-language)
- [Maintenance](#maintenance)

---

## Workspace overview

All crates are workspace members at `crates/<name>/`. Edition 2021, MIT licensed.

| Crate | Public modules | Tests | Stability |
|---|---|---|---|
| `meedya-codecs` | `audio_codec`, `channel_config`, `classify`, `container`, `ffprobe`, `hdr`, `mediainfo`, `registry`, `spatial`, `spatial_type`, `subtitle_codec`, `tool_path`, `video_codec` | 47 | Stable for partner-app consumption |
| `meedya-core` | (facade re-exports only — `tags-extended` and `library-import` now included) | 0 | Stable |
| `meedya-db` | `client`, `export`, `models` | 4 | Foundation stable; specific endpoints may evolve |
| `meedya-fingerprint` | `acoustid`, `chromaprint` (feature-gated, non-default), `replaygain` | 10 | Stable |
| `meedya-library-import` | `cuesheet`, `itunes_xml` | 30 | Stable |
| `meedya-lyrics` | `embed`, `error`, `lrc`, `lyrics`, `lyricsfile`, `lyricsfile_export`, `lyricsfile_lrc`, `lyricsfile_ttml`, `lyricsfile_ttml_classify`, `provider`, `sidecar` | 130 | Stable (plain + synced via SYLT for ID3v2; Lyricsfile YAML model + TTML import/export) |
| `meedya-metadata` | `codec_tags`, `common_tags`, `identifier_types`, `json_path`, `playback_bounds`, `registry`, `tag_io`, `tag_registry`, `template`, `writer` | 115 | Stable (two co-existing surfaces + identifier-types registry + filename template engine) |
| `meedya-providers` | `cover_art`, `credentials`, `extra_keys`, `lucene`, `match_scoring`, `providers` (feature-gated), `rate_limiter`, `traits`, `types` | 59 | Stable foundation; specific provider implementations may evolve |
| `meedya-tags-extended` | `ai_content`, `conflict_policy`, `genre_hierarchy`, `io`, `mik`, `model`, `play_history`, `quick_tag`, `sidecar_json`, `standard`, `stems` | 180 | Foundation stable + Mixed In Key reader; other proprietary DJ readers pending |

**Total: 575 tests** with default features, **720** with `--all-features` (the CI configuration). All passing, 0 failing.

> These are **measured** figures — `cargo test --workspace [--all-features]` run against `feature/work-in-progress` on 2026-09-02 — not carried forward from a previous edit. For reference, `main` measures 601 with `--all-features`.
>
> Earlier revisions of this file accumulated a long narrative of incremental count deltas (466 → 511 → 533 → 546 → 664 …) which had drifted from reality. That narration has been removed: the only trustworthy number is one you just measured. Guarding these counts automatically in CI is tracked in issue #71.

Per-crate, `--all-features` (measured): `meedya-codecs` 47 · `meedya-core` 0 · `meedya-db` 4 · `meedya-fingerprint` 15 · `meedya-library-import` 30 · `meedya-lyrics` 130 · `meedya-metadata` 115 · `meedya-providers` 199 · `meedya-tags-extended` 180.

---

## Crate APIs

### `meedya-codecs`

Canonical type definitions for audio/video/subtitle codecs, container formats, HDR formats, spatial audio formats, and media classification. Includes FFprobe + MediaInfo integration for runtime detection.

#### Public re-exports

```rust
pub use audio_codec::AudioCodec;                           // 42+ variants
pub use channel_config::ChannelConfig;                     // mono/stereo/5.1/7.1/Atmos etc.
pub use classify::{MediaClass, MediaClassification, MediaFormat, MediaGroup, MediaQuality};
pub use container::ContainerFormat;                        // 36+ variants
pub use error::CodecError;
pub use hdr::HdrFormat;                                    // HDR10, HDR10+, Dolby Vision, HLG
pub use registry::CodecRegistry;                           // TOML-driven runtime registry
pub use spatial::SpatialAudioFormat;
pub use spatial_type::SpatialType;                         // Atmos / DD+ JOC / binaural etc.
pub use subtitle_codec::SubtitleCodec;
pub use video_codec::VideoCodec;                           // 21+ variants
```

#### Key modules

- **`audio_codec`** — `AudioCodec` enum with FFmpeg names, lossless flags, channel-config compatibility, container compatibility matrices.
- **`video_codec`** — `VideoCodec` enum with HDR support flags, VideoToolbox flags, container compatibility.
- **`container`** — `ContainerFormat` enum with extensions, MIME types, codec compatibility.
- **`classify`** — `MediaClass`/`MediaClassification`/`MediaFormat`/`MediaGroup`/`MediaQuality` for sorting/categorising media files (music, audiobook, movie, TV, etc.).
- **`ffprobe`** — Runtime FFprobe invocation + JSON parsing for codec/track detection.
- **`mediainfo`** — MediaInfo CLI integration as an alternative detector.
- **`tool_path`** — Locator for FFprobe/MediaInfo binaries across user-installed locations.
- **`registry::CodecRegistry`** — Optional TOML-driven codec registry loaded at runtime; mirrors the static enum data for callers that want declarative configuration.

#### Typical usage

```rust
use std::path::Path;
use meedya_codecs::{AudioCodec, ContainerFormat, ffprobe};

// Detect codec from a file (async; needs the resolved ffprobe binary path —
// see the `tool_path` module)
let info = ffprobe::detect_audio_info(&ffprobe_bin, Path::new("/path/to/song.m4a")).await;
let codec = info.and_then(|i| ffprobe::resolve_codec(&i)); // Option<AudioCodec>

// Check container compatibility
let is_compatible = ContainerFormat::M4a.supports_audio_codec(AudioCodec::Alac);
```

---

### `meedya-core`

Unified facade crate that re-exports the other implemented crates behind feature flags. Use it when you want one dependency instead of nine.

#### Feature flags

| Feature | Pulls in | Default |
|---|---|---|
| `metadata` | `meedya-metadata` | ✓ |
| `codecs` | `meedya-codecs` | ✓ |
| `fingerprint` | `meedya-fingerprint` | ✓ |
| `lyrics` | `meedya-lyrics` (+ `metadata`) | ✓ |
| `providers` | `meedya-providers` | ✓ |
| `tags-extended` | `meedya-tags-extended` | ✓ |
| `library-import` | `meedya-library-import` | ✓ |
| `db` | `meedya-db` |  |
| `keyring` | OS keyring (pulls `providers`) |  |
| `full` | Everything |  |

#### Re-exports

```rust
pub use meedya_metadata as metadata;
pub use meedya_codecs as codecs;
pub use meedya_fingerprint as fingerprint;
pub use meedya_lyrics as lyrics;
pub use meedya_providers as providers;
pub use meedya_db as db;
pub use meedya_tags_extended as tags_extended;
pub use meedya_library_import as library_import;
```

#### `meedya_core::prelude`

```rust
// With default features
pub use meedya_metadata::{CommonTag, IdentifierType, MetadataError, TagRegistry};
pub use meedya_codecs::{AudioCodec, ChannelConfig, CodecRegistry, ContainerFormat, SpatialType};
pub use meedya_providers::{CredentialStore, MetadataProvider, ProviderCapabilities,
                            ProviderRateLimiter, ProviderResult, SearchQuery};
pub use meedya_lyrics::{Lyrics, LyricsProvider, SyncedLine, TrackQuery};
pub use meedya_tags_extended::{
    BeatGrid, CuePoint, ExtendedTags, KeyMode, LoopPoint, MusicalKey, Note, Source, TagFile,
};
pub use meedya_library_import::{EntryLocator, ImportReport, LibraryEntry, SourceInfo};
```

---

### `meedya-db`

MeedyaDB API client and shared media record models.

#### Public re-exports

```rust
pub use client::MeedyaDbClient;
pub use error::DbError;
pub use export::DbExporter;
pub use models::{Album, Artist, MediaRecord, Track};
```

#### `MeedyaDbClient`

HTTP client for `api.meedya.tv/v1`. Search, match, and lookup operations against the shared MeedyaDB.

#### `DbExporter` trait

Export trait that downstream apps implement to persist `Track`/`Album`/`Artist` records to their local database (SQLite, etc.). The core crate doesn't ship a default backend — apps own their schema.

#### Models

- `MediaRecord` — top-level enum (`Track | Album | Artist`).
- `Track`, `Album`, `Artist` — canonical record types shared across all apps.

---

### `meedya-fingerprint`

Audio fingerprinting and loudness analysis. Returns analysis results — callers handle tag-writing (typically via `meedya-metadata::tag_io::write_acoustid_tags` and `write_replaygain_tags`).

#### Public re-exports

```rust
pub use acoustid::{AcoustIdClient, AcoustIdResult};
pub use error::FingerprintError;
pub use replaygain::{
    AlbumGainResult, ReplayGainAnalyzer, ReplayGainResult,
    DEFAULT_REFERENCE_LEVEL, DEFAULT_ANALYSIS_TIMEOUT
};
```

#### `AcoustIdClient`

AcoustID API client with built-in rate limiting (3 requests/second per the AcoustID terms). Returns `AcoustIdResult` containing matched MusicBrainz recording IDs and scores.

`AcoustIdClient` itself only performs the HTTP lookup — it takes an already-computed fingerprint string. Producing that fingerprint is a **separate, non-default** step: see `chromaprint` below.

#### `chromaprint` — fingerprint generation (opt-in feature)

Fingerprint generation is **not** compiled in by default (`meedya-fingerprint`'s `Cargo.toml` declares `default = []`). It is gated behind the `chromaprint` Cargo feature, which pulls in `rusty-chromaprint` (pure-Rust Chromaprint port) + `symphonia` (pure-Rust audio decode) + `base64` — a real compile-time and binary-size cost that consumers who only want the AcoustID HTTP client or the ReplayGain analyser shouldn't have to pay:

```toml
meedya-fingerprint = { git = "https://github.com/MWBMPartners/MeedyaSuite-core", features = ["chromaprint"] }
```

With the feature enabled, the crate root additionally exposes:

```rust
#[cfg(feature = "chromaprint")]
pub use chromaprint::generate_fingerprint;
```

No external `fpcalc` binary is required either way — this is the path that enables AcoustID support on platforms (e.g. ARM Linux) where no `fpcalc` binaries exist. `meedya-core` does **not** forward this feature — a consumer going through the facade needs `meedya-fingerprint` as a direct dependency to opt in. See MWBMPartners/MeedyaDL#353 Phase 3 for the consumer migration.

#### `ReplayGainAnalyzer`

EBU R128 loudness measurement. Computes track gain + peak; aggregates multiple tracks into `AlbumGainResult` for album-mode normalisation. Reference level defaults to `DEFAULT_REFERENCE_LEVEL` (-18 LUFS).

```rust
pub const DEFAULT_ANALYSIS_TIMEOUT: Duration; // 600s
impl ReplayGainAnalyzer {
    pub fn with_reference_level(self, level: f64) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
}
```

**Subprocess timeout.** `analyze_track` shells out to FFmpeg, and that call is bounded by
`DEFAULT_ANALYSIS_TIMEOUT` (10 minutes), overridable with `with_timeout`. On expiry the
child is **SIGKILLed** (`kill_on_drop`) rather than left running, and the call returns
`FingerprintError::FfmpegTimeout { seconds }`.

The default is deliberately far longer than the 30s used by `meedya-codecs`' `ffprobe` /
`mediainfo` wrappers: `ebur128` decodes the *entire* file, so a two-hour DJ mix on a slow
volume can legitimately take minutes, whereas `ffprobe` only reads headers. Tune it with
`with_timeout` if your media profile differs — library code cannot know it.

`FingerprintError::FfmpegTimeout` is distinct from `FfmpegError` precisely so callers can
retry a timeout with a longer limit, which is sensible, without also retrying a genuine
FFmpeg failure, which is not.

> `FingerprintError` is a non-exhaustive-in-practice error enum; adding `FfmpegTimeout` will
> break a downstream `match` that enumerates every variant without a `_` arm.

**Non-UTF-8 paths** are supported — the file path is passed to FFmpeg as an `OsStr`, so
media under a path that is not valid UTF-8 analyses normally.

---

### `meedya-library-import`

Ingest playback bounds and metadata from external library databases. Emits a normalized `LibraryEntry` stream; the consuming app matches entries to local files and applies them (typically via `meedya_metadata::playback_bounds`).

#### Public types

```rust
pub struct LibraryEntry {
    pub locator: EntryLocator,
    pub start_ms: Option<u64>,
    pub stop_ms: Option<u64>,
}

pub enum EntryLocator {
    Path(PathBuf),
    PersistentId { kind: &'static str, value: String },
}

pub struct SourceInfo { pub kind: &'static str, pub path: PathBuf }

pub struct ImportReport {
    pub source: SourceInfo,
    pub entries: Vec<LibraryEntry>,
    pub warnings: Vec<String>,
}
```

#### `itunes_xml` module

```rust
pub const KIND: &str = "itunes-xml";
pub fn import(path: &Path) -> Result<ImportReport, String>;
```

Parses iTunes / Music.app `iTunes Music Library.xml`. Emits one `LibraryEntry` per track that has `Start Time` and/or `Stop Time` set. Cross-platform `file://` URL decoding (Windows drive-letter detection by path shape, not `cfg(windows)`).

#### `cuesheet` module

```rust
pub const KIND: &str = "cuesheet";
pub fn parse_str(input: &str) -> Result<CueSheet, String>;
pub fn parse_file(path: &Path) -> Result<CueSheet, String>;
pub fn import(path: &Path) -> Result<ImportReport, String>;
```

Public data model:

```rust
pub struct CueSheet { catalog, title, performer, songwriter, rems, files }
pub struct CueFile { path, format: FileFormat, tracks: Vec<CueTrack> }
pub enum  FileFormat { Wave, Aiff, Mp3, Flac, Binary, Other(String) }
pub struct CueTrack { number, kind: TrackKind, title, performer, songwriter,
                      isrc, flags, pregap, postgap, indexes: Vec<CueIndex>, rems }
pub enum  TrackKind { Audio, Other(String) }
pub struct CueIndex { number: u8, time: CueTime }
pub struct CueTime { minutes: u32, seconds: u8, frames: u8 }   // 75 fps
impl CueTime { pub const ZERO: CueTime; pub fn to_milliseconds(self) -> u64 }
pub struct RemEntry { key, value }
```

Use `parse_file()` directly when you need the full structured data (for chapter authoring, metadata enrichment, etc.). Use `import()` only when you specifically want the narrow LibraryEntry adapter.

---

### `meedya-lyrics`

LRCLIB client, LRC parser/writer, sidecar + tag-embed writes.

#### Public re-exports

```rust
pub use embed::{embed, embed_synced, DEFAULT_LANGUAGE};
pub use error::{Error, Result};
pub use lyrics::{Lyrics, SyncedLine};
pub use provider::lrclib::LrclibProvider;
pub use provider::{LyricsProvider, TrackQuery};
```

#### `Lyrics` and `SyncedLine`

```rust
pub struct Lyrics {
    pub plain: Option<String>,                // unsynchronised
    pub synced: Option<Vec<SyncedLine>>,      // [mm:ss.xx] timestamps
    // ... (metadata fields)
}

pub struct SyncedLine {
    pub timestamp_ms: u64,
    pub text: String,
}
```

#### `LyricsProvider` trait

```rust
pub trait LyricsProvider: Send + Sync {
    async fn fetch(&self, query: &TrackQuery) -> Result<Lyrics>;
}
```

Note the return type is `Result<Lyrics>`, **not** `Result<Option<Lyrics>>` — "no lyrics found" is surfaced as an `Err`, not a `None`. Callers should `?`-propagate rather than pattern-match an `Option`.

Implementation: `LrclibProvider` (calls lrclib.net).

#### Write targets

- **`sidecar::write(media: &Path, lyrics: &Lyrics) -> Result<Option<PathBuf>>`** — writes a `.lrc` file next to `media`. Returns `Ok(None)` (not an error) when `lyrics` has no synced lines to write; `Ok(Some(path))` with the sidecar path on success.
- **`embed::embed(media: &Path, lyrics: &Lyrics) -> Result<bool>`** — plain-text tag-embed via `meedya-metadata` (USLT for ID3v2, `LYRICS` for Vorbis, `©lyr` for MP4).
- **`embed::embed_synced(media: &Path, lyrics: &Lyrics, lang: [u8; 3]) -> Result<()>`** — synchronised ID3v2 SYLT frame. ID3v2-only by design; errors with `Error::UnsupportedForSync` on other formats. Encoding: UTF-16 with BOM; timestamp format: milliseconds. Recommended pattern: call both `embed()` and `embed_synced()` — the former handles cross-format plain text, the latter adds SYLT where applicable.
- **`embed::DEFAULT_LANGUAGE`** — `*b"eng"`, the ISO-639-2 default for callers without a known language code.

#### `lrc` module

```rust
pub fn parse(input: &str) -> Vec<SyncedLine>;
pub fn write(lines: &[SyncedLine]) -> String;
```

---

#### Lyricsfile (canonical YAML lyrics model) + TTML

The `.lyrics` YAML format (#34) is the canonical in-memory lyrics model, with TTML and LRC
import/export around it. Missing from earlier revisions of this file despite being
root-re-exported.

```rust
pub struct Lyricsfile;
pub struct LyricsfileMetadata;
pub struct LyricsfileLine;
pub struct LyricsfileWord;
pub struct LyricsfileSyllable;      // syllable-level timing (#60)
pub const LYRICSFILE_VERSION;
pub const INSTRUMENTAL_MARKER;

pub enum TtmlGranularity;            // Unknown | Line | Word | Syllable
pub fn classify_ttml_granularity(..) -> TtmlGranularity;
```

`Unknown` is the parse-failure signal (XML parse error or empty input) — treated as "lowest
known granularity," i.e. it still needs upgrading if a syllable-capable source is reachable.
Consumers matching on `TtmlGranularity` must handle all four variants.

Modules: `lyricsfile` (model + YAML I/O), `lyricsfile_ttml` (Apple Music TTML import,
including `lyricOffset` extraction — see #61), `lyricsfile_lrc` (LRC bridge),
`lyricsfile_export` (multi-format export), `lyricsfile_ttml_classify` (granularity
classifier, consumed by MeedyaDL's enrichment Step 1b), `error` (`Error`, `Result`).

See [`crates/meedya-lyrics/docs/APPLE_MUSIC_TTML_SPEC.md`](../crates/meedya-lyrics/docs/APPLE_MUSIC_TTML_SPEC.md) where present for the TTML dialect notes.

### `meedya-metadata`

Tag schemas, metadata read/write, and a config-driven TOML tag registry. Two parallel surfaces co-exist intentionally — they serve different code paths.

#### Public re-exports

```rust
pub use common_tags::{CommonTag, STANDARD_NAMESPACES};
pub use error::MetadataError;
pub use identifier_types::{
    active_identifier_slugs, identifier_type, identifier_types, IdentifierScope,
    IdentifierStatus, IdentifierType, IdentifierValidation, IDENTIFIER_TYPES_TOML,
};
pub use json_path::{extract_json_value, value_to_string};
pub use tag_io::{read_tags, write_acoustid_tags, write_registry_tags,
                 write_replaygain_tags, write_tags, TagMap};
pub use tag_registry::{AtomTarget, TagDefinition, TagRegistry, TagScope, TagValueType};
pub use template::{TagSource, Template, TemplateError};
```

#### Surface 1: `lofty`-backed (multi-format)

For MP3 / M4A / FLAC / WAV / AIFF / OGG and downstream-app general use.

- **`common_tags`** — `CommonTag` enum (core identifiers — `Isrc`/`Upc`/MusicBrainz IDs/`AcoustId`; basic metadata — `Title`/`Artist`/`Album`/etc.; extended metadata; ReplayGain; catalog/date fields; and, as of #65, MB Release-Group/Work IDs + `Iswc` + core-info/contributor-role fields — see below) with `STANDARD_NAMESPACES` mapping each to its ID3v2 / Vorbis / MP4 ilst frame name. `Bpm`/`InitialKey` are **not** `CommonTag` concepts — those live in `meedya-tags-extended::standard` (DJ metadata), a distinct surface.
- **`identifier_types`** — Cross-repo identifier-type registry (#65). DATA, not an enum: see the dedicated subsection below.
- **`tag_io`** — Lofty-driven file I/O:
  - `read_tags(path: &Path) -> Result<TagMap>`
  - `write_tags(path: &Path, tags: &[(CommonTag, String)]) -> Result<()>`
  - `write_registry_tags(path: &Path, registry: &TagRegistry, json_source: &serde_json::Value, scope: TagScope) -> Result<usize>` — returns the number of tags written
  - `write_acoustid_tags(path, result: &AcoustIdResult) -> Result<()>`
  - `write_replaygain_tags(path: &Path, result: &ReplayGainResult, album_result: Option<&AlbumGainResult>) -> Result<()>`
- **`tag_registry`** — `TagDefinition`, `TagRegistry`, `TagScope`, `TagValueType`, `AtomTarget` for declarative tag mapping loaded from TOML.
- **`json_path`** — Dot-path extraction (`extract_json_value`, `value_to_string`) with array indexing for API JSON → tag-value pipelines.

#### Identifier-types registry (`identifier_types`, #65)

The canonical, cross-repo vocabulary of external/catalogue identifier types: scope → slug → validation shape. This is **DATA, not an enum** — adding an identifier type is a TOML edit (`crates/meedya-metadata/identifier_types.toml`), not a Rust code change.

**Consumers**: MeedyaManager / MeedyaDL (Rust) via `identifier_types()`; MeedyaConverter (Swift, planned bindings) via the raw `IDENTIFIER_TYPES_TOML` byte-level artifact; iHymns (PHP) mirrors the artifact and appends its own domain-only extensions (`ccli`, `hymnary-tune`, ...) — domain IDs never flow upstream into this repo's artifact.

Per-entry schema (`[[identifier]]` in the TOML):

| Field | Type | Required | Meaning |
|---|---|---|---|
| `slug` | string | yes | Canonical kebab-case key (`^[a-z0-9][a-z0-9-]*$`). The map key consumers use in `external_ids` / `ProviderResult.metadata` / iHymns' mirror. |
| `display_name` | string | yes | Human label ("ISRC"). |
| `standard` | string | no | Issuing standard ("ISO 3901:2019"). |
| `scope` | string | yes | Luminate entity: `artist` \| `song` \| `recording` \| `work` \| `release-group` \| `release` \| `product` \| `party` \| `audiovisual-work`. |
| `status` | string | yes | `active` (consumers may store/exchange under this slug now) \| `reserved` (slug + shape claimed at zero cost; no storage surface yet). |
| `validation` | inline table | yes | `{ kind = "regex", pattern = '<anchored regex>' }` or `{ kind = "free" }`. Validates the **canonical compact form** — normalisation to reach that form is **per-scheme**, not a blanket rule: uppercase + strip `-`/`.`/spaces for `isrc`/`iswc`/`isni`/`ipi`/`grid`/`upc`/`icpn`/`label-code`; `musicbrainz-*` and `acoustid` stay **lowercase** hyphenated UUIDs (the regex patterns require lowercase hex + hyphens); `eidr` keeps its `10.5240/` DOI-prefix dot. |
| `check` | string | no | Advisory check-digit algorithm name (`gs1`, `iswc-mod10`, `iso7064-mod11-2`, `iso7064-mod37-36`) — **data only, not executed in v1**. |
| `example` | string | required when `validation.kind = "regex"` | A syntactically valid sample; guard-tested to match its own pattern. |
| `notes` | string | no | Cross-references / caveats. |

**Seed set**: 13 active — `acoustid, bowi, eidr, ipi, isni, isrc, iswc, musicbrainz-artist, musicbrainz-recording, musicbrainz-release, musicbrainz-release-group, musicbrainz-work, upc` — and 6 reserved — `dpid, grid, hfa, icpn, ipn, label-code` (GRid and ICPN reserved per #65: no storage surface yet — no `CommonTag` variant, no `extra_keys` const).

```rust
pub const IDENTIFIER_TYPES_TOML: &str = /* compiled-in artifact, byte-for-byte */;

pub enum IdentifierScope { Artist, Song, Recording, Work, ReleaseGroup, Release, Product, Party, AudiovisualWork } // #[non_exhaustive]
pub enum IdentifierStatus { Active, Reserved }                                                                     // #[non_exhaustive]
pub enum IdentifierValidation { Regex { pattern: String }, Free }                                                  // #[non_exhaustive]

pub struct IdentifierType {
    pub slug: String,
    pub display_name: String,
    pub standard: Option<String>,
    pub scope: IdentifierScope,
    pub status: IdentifierStatus,
    pub validation: IdentifierValidation,
    pub check: Option<String>,
    pub example: Option<String>,
    pub notes: Option<String>,
}
impl IdentifierType {
    pub fn matches_format(&self, value: &str) -> bool; // caller normalises first
}

pub fn identifier_types() -> &'static [IdentifierType];       // all, sorted by slug
pub fn identifier_type(slug: &str) -> Option<&'static IdentifierType>;
pub fn active_identifier_slugs() -> Vec<&'static str>;         // active slugs, sorted
```

**Guard contract**: `crates/meedya-metadata/tests/identifier_registry_guard.rs` holds the *deliberate declaration* side of a change-detector (`EXPECTED_ACTIVE_SLUGS` / `EXPECTED_RESERVED_SLUGS`) — the other side is always **derived** (parsed from the artifact via `identifier_types()`, or iterated from `CommonTag` via `strum::EnumIter`), never a second hand-typed copy. Changing the TOML without updating the expected lists (or vice versa) fails CI. Each downstream repo that mirrors the artifact (MeedyaManager, iHymns) holds its own guard against its own mirror — this repo does not, and cannot, verify another repo's copy stayed in sync.

**FFI note**: `IdentifierType` is Rust-only for now (`String`/`Option<String>` fields, no `#[repr(C)]`) — no Swift binding exists yet. The FFI story is `IDENTIFIER_TYPES_TOML`: bindings and non-Rust consumers parse or pass through the raw artifact, and cross-repo CI diffs the bytes directly.

#### `CommonTag` is `#[non_exhaustive]` as of 0.2.0 (#65)

Downstream crates matching on `CommonTag` must add a `_ =>` wildcard arm (a compile error otherwise — `#[non_exhaustive]` has no effect on in-crate matches, so `common_tags.rs`'s own mapping methods and `tag_io.rs::write_common_tag_to_lofty()` stay exhaustive and total). This landed alongside the workspace `0.1.0 → 0.2.0` bump — the attribute itself is the one breaking change; every variant added *after* it is non-breaking for downstream consumers. Serde behaviour is unaffected: `CommonTag` still (de)serializes as the variant-name string, so an older consumer reading a newer producer's payload can still fail on an unrecognised variant name — cross-version payload tolerance remains the consumer's job.

**12 new variants** (#65), with their container mappings:

| Variant | iTunes/MP4 atom | Vorbis comment | ID3v2 frame |
|---|---|---|---|
| `MusicBrainzReleaseGroupId` | `MusicBrainz Release Group Id` | `MUSICBRAINZ_RELEASEGROUPID` | `TXXX:MusicBrainz Release Group Id` |
| `MusicBrainzWorkId` | `MusicBrainz Work Id` | `MUSICBRAINZ_WORKID` | `TXXX:MusicBrainz Work Id` |
| `Iswc` | `ISWC` | `ISWC` | `TXXX:ISWC` (lofty has no dedicated ISWC key) |
| `Subtitle` | `SUBTITLE` | `SUBTITLE` | `TIT3` |
| `Language` | `LANGUAGE` | `LANGUAGE` | `TLAN` |
| `Lyricist` | `LYRICIST` | `LYRICIST` | `TEXT` |
| `Conductor` | `CONDUCTOR` | `CONDUCTOR` | `TPE3` |
| `Remixer` | `REMIXER` | `REMIXER` | `TPE4` |
| `Arranger` | `ARRANGER` | `ARRANGER` | `TIPL:arranger` (no MP4 ilst mapping in lofty 0.22 — MP4 writes drop silently) |
| `Producer` | `PRODUCER` | `PRODUCER` | `TIPL:producer` |
| `Engineer` | `ENGINEER` | `ENGINEER` | `TIPL:engineer` |
| `Mixer` | `MIXER` | `MIXER` | `TIPL:mix` |

`Performer` and `Translator` were deliberately **excluded**: `Performer` has no lofty 0.22 ID3v2 write mapping (ID3v2 models it as the multi-valued, instrument-qualified TMCL frame — a flat-string variant would silently no-op on MP3); `Translator` has no standard frame in any container and stays an iHymns-domain concept in its own mirror.

```rust
impl CommonTag {
    /// The `identifier_types.toml` registry slug for identifier-carrying
    /// variants; `None` for descriptive tags. Total match — no wildcard —
    /// so a new variant must decide, at compile time, whether it's an
    /// external identifier.
    pub fn identifier_slug(&self) -> Option<&'static str>;
}
```

**Growth-path policy**: new external identifier types go through the `identifier_types` registry + `meedya-providers::extra_keys` + the `external_ids`/`metadata` maps — **not** a new `CommonTag` variant. A `CommonTag` variant is reserved for tags with a genuine per-container frame mapping (ID3v2/Vorbis/MP4 ilst) — like the 3 additions in §3 of the #65 build spec. `CatalogNumber.identifier_slug()` deliberately returns `None`: label catalogue codes are not a global identifier scheme.

#### Surface 2: `mp4ameta`-backed (M4A, sandbox-safe)

For the App Store distribution path. No subprocess spawning, no `lofty` dependency surface.

- **`registry`** — Loads `tags.toml` at compile time. `TAG_REGISTRY` static; functions `extract_json_value`, `value_to_string`, `all_known_paths`.
- **`writer`** — Apple Music JSON → freeform atoms:
  - `write_tags_from_registry(tag, registry, album_json, track_json)`
  - `write_local_tags(tag)` — SourceStore / EncodeSource / iTunesMediaType / isMedley
  - `extract_isrc_from_vendor(tag)` — reconciles Apple's Vendor tag with the standard ISRC atom
  - `tag_single_file(path, tag_writer)`, `tag_directory_recursive`, `is_m4a`, `collect_m4a_files`
- **`codec_tags`** — Codec ID tags:
  - `CodecKind` enum (`Lossless | Atmos | DolbyDigital | Binaural | Downmix | StandardLossy`)
  - `apply_codec_metadata_tags(output_path, codec)`
  - `write_lossless_tags`, `write_atmos_tags`, `write_dolby_digital_tags`, `write_binaural_tags`, `write_downmix_tags`, `write_spatial_codec_tag`, `clear_binaural_downmix_tags`
- **`playback_bounds`** — Soft playback start/stop atoms in the `MeedyaMeta` namespace (iTunes Start/Stop Time analog):
  - `set_playback_start(tag, ms)`, `set_playback_stop(tag, ms)`
  - `clear_playback_start(tag)`, `clear_playback_stop(tag)`
  - `get_playback_start_ms(tag) -> Option<u64>`, `get_playback_stop_ms(tag) -> Option<u64>`
  - `format_hms_ms(ms) -> String` (helper for UI)

Both surfaces share the `json_path` module.

#### `template` — filename template engine (#47)

Format-agnostic filename template engine shared across MeedyaConverter / MeedyaDL /
MeedyaManager for composing filenames from tag values. Root re-exported (`use
meedya_metadata::{Template, TemplateError, TagSource};`).

```rust
pub struct Template { /* parsed AST */ }
pub enum TemplateError {
    UnclosedPlaceholder { column: usize },
    UnexpectedCloseBrace { column: usize },
    EmptyPlaceholder { column: usize },
    UnknownTransform { column: usize, name: String },
    InvalidWidthSpec { column: usize, raw: String },
    MissingVariable { name: String },
}

pub trait TagSource {
    fn get(&self, name: &str) -> Option<String>;
}
// Implemented for HashMap<String, String> and HashMap<&'static str, &'static str>.
// Callers wrap their own Tag type (lofty / mp4ameta / etc.) in a thin newtype.

impl Template {
    pub fn parse(template: &str) -> Result<Self, TemplateError>;
    pub fn render<S: TagSource>(&self, source: &S) -> Result<String, TemplateError>;
}
```

Syntax: `{name}` placeholders, `|` to pipe through transformations, `:NN` for a width
specifier (zero-pads a numeric value, truncates a string):

```text
"{tracknumber:02} - {artist|fallback:albumartist} - {title|sanitize}.{ext}"
→ "03 - Aphex Twin - Selected Ambient Works.flac"
```

Transforms (applied left-to-right in the pipe): `sanitize` (replaces `/ \ : * ? " < > |` and
control characters with `_`), `ascii` (folds common Latin diacritics, e.g. `é` → `e`),
`lower`, `upper`, `title`, `trim`, `round` (numeric strings only; non-numeric input passes
through unchanged), `fallback:VAR` (substitute another variable when the placeholder's own
lookup misses — evaluated before other transforms), `max:N` (truncate to `N` characters).
A missing variable with no `fallback` in its pipe is a `TemplateError::MissingVariable`
returned from `render`, not a panic.

#### Adding a metadata tag

Edit `crates/meedya-metadata/tags.toml`:

```toml
[album.<tag_id>]
json_path  = "attributes.someField"
value_type = "string"
atoms      = [
    { namespace = "itunes", name = "MyAtom" },
    { namespace = "meedya", name = "MyAtom" },
]
```

Zero Rust changes. Bump test count in `registry.rs`. Run `cargo test -p meedya-metadata`.

---

### `meedya-providers`

Shared metadata provider framework — traits, capabilities, registry, rate limiting, credentials, cover art, match scoring.

#### Public re-exports

```rust
pub use cover_art::CoverArtSize;
pub use credentials::{CredentialSource, CredentialStore, ResolvedCredential};
pub use error::CredentialError;
pub use lucene::{escape_lucene, phrase_clause, quote_phrase};
pub use match_scoring::{MatchScorer, ScoringWeights};
pub use rate_limiter::{default_limiter_for, ProviderRateLimiter, RateLimiterRegistry};
pub use traits::{MetadataProvider, ProviderCapabilities, ProviderError};
pub use types::{CoverArtInfo, MediaType, ProviderResult, SearchQuery};
```

#### `lucene`

Lucene/Solr query escaping for MusicBrainz search — always compiled (pure `std`, no feature gate, no dependencies). Every `providers::musicbrainz` / `providers::isrc` / `providers::iswc` query built from user-supplied text goes through this module rather than interpolating raw strings into the Lucene query.

```rust
pub fn escape_lucene(value: &str) -> String;
pub fn quote_phrase(value: &str) -> String;
pub fn phrase_clause(field: &str, value: &str) -> String;
```

The three helpers are **not interchangeable** — they implement two genuinely different
Lucene escaping regimes:

| Context | Helper | Escapes |
| --- | --- | --- |
| Bare / unquoted term | `escape_lucene` | all 19 Lucene special characters |
| Inside a double-quoted phrase | `quote_phrase` | only `\` and `"` |
| A whole `field:"value"` clause | `phrase_clause` | delegates to `quote_phrase` |

Inside a quoted phrase Lucene treats `( ) : + - ?` as literal text, so escaping them there
would make the backslashes part of the searched string. Outside a phrase they are operators
and must be escaped — the MusicBrainz documentation's own `AC/DC` example escapes the slash
for exactly this reason.

- **`escape_lucene`** — backslash-escapes every Lucene special character (`+ - ! ( ) { } [ ] ^ " ~ * ? : \ /` and the boolean operators `&`/`|`, so `&&`/`||` become `\&\&`/`\|\|`). Does not handle whitespace or field-scoping.
- **`quote_phrase`** — the quoting policy providers actually use for free-text field values (`title`, `artist`, etc.): escapes embedded `\` and `"` (backslash first, to avoid double-escaping), then wraps the result in double quotes. Other Lucene special characters are left as-is inside the phrase. The field qualifier stays outside the quoted value, e.g. `format!("recording:{}", quote_phrase(title))`.
- **`phrase_clause`** — convenience over `quote_phrase` producing a complete `field:"value"` clause. `field` is a developer-controlled literal emitted verbatim; only `value` is escaped and quoted. This is the helper to reach for when building recording/release/artist search clauses from tag data.

`MusicBrainzProvider::build_lucene_query` (private, returns `Result`) is the reference consumer:

- **ISRC takes priority.** Normalised to the canonical 12-character form (ASCII alphanumerics only, uppercased) and emitted as `isrc:<CODE>`. Normalisation leaves nothing Lucene could misparse, so no quoting is applied. An ISRC that does not normalise to exactly 12 characters is **rejected** rather than forwarded upstream. `album`/`year` are ignored in this branch — an exact identifier needs no narrowing, and narrowing could only exclude the correct recording.
- **Free-text search.** A trailing parenthetical/bracket group is stripped from `title`, `artist` and `album` (`strip_trailing_bracket_groups`, private) — this mitigates the common `"(2011 Remastered Version)"` / `"[Live]"` recall miss against MusicBrainz's canonical title. A *leading* group (e.g. `"(I Can't Get No) Satisfaction"`), or one whose removal would empty the term, is preserved.
- **Clause composition.** `recording:"…"`, `artistname:"…"`, and — when present — `release:"…"` and `date:NNNN`, joined with ` AND `. A title or artist is **required**; `album`/`year` only narrow an already-anchored query, so an album/year-only query is rejected as too broad.
- **`date` caveat.** This is an exact-year match, not a range: a recording whose only indexed release date falls outside `year` (a reissue vs. the original year) will not match. MusicBrainz also exposes `firstreleasedate` ("the release date of the earliest release including this recording"), which may suit earliest-release selection better — see issue #74.

Tracked for live-service recall validation post-2026-11-30 in issue #69.

**ISRC vs ISWC are normalised differently, on purpose.** MusicBrainz documents neither field's indexed form, so this was settled by **live probing** `musicbrainz.org/ws/2/` on 2026-09-01:

| Field | Query form | Live result |
| --- | --- | --- |
| ISRC | `isrc:GBAYE0601498` (compact) | matches |
| ISRC | `isrc:GB-AYE-06-01498` (hyphenated) | 0 results |
| ISWC | `iswc:"T-304.031.869-8"` (dotted display form) | matches |
| ISWC | `iswc:T3040318698` (compact) | 0 results |
| ISWC | `iswc:"T-304031869-8"` (hyphen-only) | parse error |

So `providers::isrc` strips separators and uppercases (`normalise_isrc`), while `providers::iswc` **reformats to MusicBrainz's stored display form** `T-DDD.DDD.DDD-C` (`normalise_iswc` -> `format_iswc_dotted`) and phrase-quotes it so its `-` and `.` cannot parse as Lucene operators. All accepted input forms (compact, hyphen-only, dotted) converge on the dotted query.

`providers::iswc` additionally exposes `pub fn normalise_iswc(&str) -> String` (compact canonical form). Re-validate after the 2026-11-30 reindex — no ticket announces an identifier-analyzer change, but the stored-form query is the safest bet either way (issue #69).

#### Error text contains no credentials

Every `reqwest` error captured by a provider has its **query string stripped** before being
stringified into `ProviderError::NetworkError`. `meedya-fingerprint`'s AcoustID client does
the same for both its transport and decode errors.

This matters because `reqwest`'s `Display` appends `" for url (…)"` with the *complete* URL,
and three services take their credential as a query parameter — TMDb (`api_key`), OMDb
(`apikey`) and AcoustID (`client`). Without redaction, any send failure (DNS, timeout, TLS)
would produce an error string containing the live API key, which then flows into logs,
tracing output and UI error surfaces.

Scheme, host and path are **kept** — they are the useful part for diagnosing a failure in a
multi-provider batch, and no secret appears in a path in this workspace. So a TMDb failure
reads `error sending request for url (https://api.themoviedb.org/3/search/multi)`.

The redaction is applied to **every** provider, not only the three that currently use query
auth, so a provider added later with query-string credentials is safe by default. Both crates
carry a canary test asserting a known secret cannot appear in the error string.

> Providers using header auth (Spotify, TheTVDB, EIDR) were never exposed this way —
> `reqwest` does not print headers. They go through the same helper regardless.

#### `MetadataProvider` trait

```rust
pub trait MetadataProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError>;
    // ... (lookup, get_by_id, etc.)
}
```

Implemented in-repo, one file per external service, each gated behind its own `provider-<name>` Cargo feature (`crates/meedya-providers/src/providers/`): `musicbrainz`, `spotify`, `apple_music`, `deezer`, `tmdb`, `thetvdb`, `omdb`, `apple_tv`, `itunes_store`, `apple_podcasts`, `isrc`, `eidr`, `iswc`. These are not stubs for downstream apps to fill in — apps opt into the ones they need via Cargo features and get a working `MetadataProvider` impl; a downstream app would only implement this trait itself for a service not already covered here.

**MusicBrainz Solr 9→10 upgrade (2026-11-30)**: `musicbrainz`/`isrc`/`iswc` all default to `https://musicbrainz.org` but expose `with_base_url` for pointing at a self-hosted MusicBrainz mirror (e.g. a local `mbslave`/search-server instance). Anyone running such a mirror is responsible for following MusicBrainz's own Solr 9→10 re-index instructions (SEARCH-764) on their own schedule — this crate's query construction is hardened against the stricter Solr 10 parser (see `lucene` above), but it cannot re-index a mirror's search server for you.

#### Rate limiting

```rust
impl ProviderRateLimiter {
    pub fn new(provider_name: impl Into<String>, rpm: u32) -> Self;         // per-minute quota
    pub fn per_second(provider_name: impl Into<String>, rps: u32) -> Self;  // per-second quota
    pub fn check(&self) -> bool;            // non-blocking; consumes a cell when it returns true
    pub async fn wait_until_ready(&self);   // blocking
    pub fn provider_name(&self) -> &str;
    pub fn rpm(&self) -> u32;               // sustained rate
    pub fn burst(&self) -> u32;             // requests admissible back-to-back
}

pub fn default_limiter_for(provider_id: &str) -> Arc<ProviderRateLimiter>;

// …and on every provider struct:
pub fn with_rate_limiter(self, limiter: Arc<ProviderRateLimiter>) -> Self;
```

**The contract — read this before building a batch caller.**

1. **Providers are throttled by default.** Every provider awaits its limiter immediately
   before each outbound request (Spotify does so twice per search — the token POST and the
   search GET both spend from its budget). There is nothing to opt into and no retry loop to
   write; a caller that ignores rate limiting entirely still behaves.
2. **It blocks, it does not error.** `wait_until_ready` delays the request rather than
   returning `ProviderError::RateLimited`, because the correct response to "too fast" is to
   go slower, and every caller writing its own backoff would be the same loop thirteen times.
   `check()` stays public for fail-fast callers that would rather skip a provider than queue
   behind it.
3. **Budgets are keyed by upstream host, not by provider name.** Several providers share one
   upstream allowance, and a limiter per provider name would multiply it by the number of
   providers pointed at that host:

   | Budget | Default | Providers sharing it |
   | --- | --- | --- |
   | `musicbrainz.org` | 1 req/**sec** | `musicbrainz`, `isrc`, `iswc` |
   | `itunes.apple.com` | 20 RPM | `apple_music`, `apple_tv`, `itunes_store`, `apple_podcasts` |
   | `api.spotify.com` | 100 RPM | `spotify` |
   | `api.deezer.com` | 50 RPM | `deezer` |
   | `api.themoviedb.org` | 40 RPM | `tmdb` |
   | `api4.thetvdb.com` | 30 RPM | `thetvdb` |
   | `www.omdbapi.com` | 10 RPM | `omdb` |
   | `id.eidr.org` | 10 RPM | `eidr` |
   | *(unrecognised id)* | 30 RPM | shared conservative fallback |

   Each figure carries its source in `rate_limiter.rs`; where a service publishes no limit
   the default is labelled a conservative guess.
4. **`per_second` is not `per_minute / 60`.** `governor` treats a quota's cell count as its
   burst capacity too, so `new(name, 60)` admits sixty requests back-to-back. MusicBrainz
   documents a *one-per-second average* and answers such a burst with 503s, which is why its
   budget is `per_second(1)` — `rpm()` reports 60, `burst()` reports 1.
5. **Limiters are shared process-wide, across provider instances.** `default_limiter_for`
   hands out `Arc`s from a `OnceLock` table; two `MusicBrainzProvider`s constructed in
   different tasks share one budget, as do a `MusicBrainzProvider` and an `IsrcProvider`.
   A per-instance limiter would throttle nothing useful, since batch callers construct a
   provider per work item.
6. **Overriding.** Provider constructors are unchanged (this addition is non-breaking); pass
   a different budget with the consuming builder, e.g.
   `MusicBrainzProvider::new(ua).with_rate_limiter(mine)`. Use it for a self-hosted mirror
   with no published limit, a paid tier, or a permissive limiter in a test that points a
   provider at a mock server — a test hitting the shared default would otherwise queue on a
   real 1 req/sec budget.
7. **`RateLimiterRegistry` is the app-level custom-budget mechanism**, not the source of the
   defaults. `RateLimiterRegistry::with_defaults()` is pre-populated with the *same* `Arc`s
   the process table hands to providers, so it observes the budgets already in force;
   `get_or_create(name, rpm)` adds app-specific ones. Registry entries reach a provider only
   when you install one with `with_rate_limiter`.

#### `CredentialStore`

Pluggable credential storage with `CredentialSource` variants (in-memory, env var, OS keyring via the `keyring` feature). `ResolvedCredential` is the result of a lookup.

#### `MatchScorer`

Fuzzy-match scoring for metadata search results. `ScoringWeights` configures per-field weight (title vs artist vs album vs year, etc.).

#### `cover_art`

Helpers for cover art selection. `CoverArtSize` variants: `Unknown`, `Thumbnail` (<200px), `Small` (200–499px), `Medium` (500–999px), `Large` (1000–1999px), `ExtraLarge` (>=2000px) — classified from the larger of an image's width/height via `CoverArtSize::from_dimension(px: u32)`. `CoverArtInfo` carries the URL + dimensions.

`best_cover_art` and `has_cover_art` are crate-root re-exports; the rest live in the `cover_art` module:

```rust
pub fn best_cover_art(r: &ProviderResult) -> Option<&CoverArtInfo>;      // root re-export
pub fn has_cover_art(r: &ProviderResult) -> bool;                        // root re-export

pub fn classify(art: &CoverArtInfo) -> CoverArtSize;
pub fn select_largest(arts: &[CoverArtInfo]) -> Option<&CoverArtInfo>;
pub fn select_smallest(arts: &[CoverArtInfo]) -> Option<&CoverArtInfo>;
pub fn select_best(arts: &[CoverArtInfo], min_size: CoverArtSize) -> Option<&CoverArtInfo>;
pub fn filter_by_min_size(arts: &[CoverArtInfo], min_size: CoverArtSize) -> Vec<&CoverArtInfo>;
pub fn is_valid_art_url(url: &str) -> bool;
pub fn url_has_image_extension(url: &str) -> bool;
pub fn mime_type_for_url(url: &str) -> &'static str;
pub fn deduplicate(arts: &[CoverArtInfo]) -> Vec<CoverArtInfo>;
```

---

### `meedya-tags-extended`

Multi-format tag I/O foundation with DJ metadata support. Built on `lofty`. Designed to host proprietary DJ-software readers (Serato, Rekordbox, Traktor, Virtual DJ) populating a unified `ExtendedTags` shape.

#### Public re-exports

```rust
pub use io::TagFile;
pub use mik::{
    read_mik, normalise_to_standards,
    MikAnalysis, MikField, MikKinds, MikPosition, MikSourceLocation,
};
pub use model::{
    BeatGrid, BeatGridMarker, CuePoint, ExtendedTags, KeyMode,
    LoopPoint, MusicalKey, Note, Rgb, Source,
};
```

#### `io::TagFile`

```rust
pub struct TagFile { /* wraps lofty::TaggedFile */ }

impl TagFile {
    pub fn open(path: &Path) -> Result<Self, String>;
    pub fn save(&mut self) -> Result<(), String>;
    pub fn save_to(&mut self, dest: &Path) -> Result<(), String>;
    pub fn path(&self) -> &Path;
    pub fn primary_tag(&self) -> Option<&lofty::tag::Tag>;
    pub fn primary_tag_mut(&mut self) -> &mut lofty::tag::Tag;
    pub fn tag(&self, tag_type: lofty::tag::TagType) -> Option<&lofty::tag::Tag>;
    pub fn tag_mut(&mut self, tag_type: lofty::tag::TagType) -> Option<&mut lofty::tag::Tag>;
    pub fn inner(&self) -> &lofty::file::TaggedFile;
    pub fn inner_mut(&mut self) -> &mut lofty::file::TaggedFile;
}
```

Lofty preserves unrecognised frames automatically. Open → edit standard fields → save will round-trip Serato/Rekordbox/Traktor blobs untouched.

#### `model::ExtendedTags`

```rust
pub struct ExtendedTags {
    pub bpm: Option<f64>,
    pub key: Option<MusicalKey>,
    /// Source-scale-aware. NOT `Option<u8>` — see EnergyValue below.
    pub energy: Option<EnergyValue>,
    pub cue_points: Vec<CuePoint>,
    pub loops: Vec<LoopPoint>,
    pub beat_grid: Option<BeatGrid>,
    pub comment: Option<String>,
    pub ai_content: AiContentFlags,
    pub stems: Option<StemMetadata>,
    pub play_history: PlayHistory,
}

/// Energy carries its source scale so consumers can canonicalise correctly.
/// `to_canonical()` returns `Option<u8>` on a 1-10 scale, and `None` for
/// `Unknown` — we do not guess about scale.
pub enum EnergyValue {
    Mik(u8),          // canonical 1-10
    Serato(f32),      // float, typically 1.0-10.0
    Rekordbox(u8),    // 1-10
    Beatport(u8),     // 1-10
    Spotify(f32),     // continuous 0.0-1.0
    Normalised(u8),   // already canonical 1-10
    Unknown(f32),     // scale unknown; to_canonical() -> None
}

pub enum Source {
    MeedyaMeta, Standard, Serato, Rekordbox, Traktor,
    VirtualDj, MixedInKey, Unknown
}

pub struct CuePoint {
    pub position_ms: u64,
    pub label: Option<String>,
    pub color: Option<Rgb>,
    pub hot_cue_index: Option<u8>,
    pub source: Source,
}

pub struct MusicalKey { pub tonic: Note, pub mode: KeyMode }
impl MusicalKey {
    pub fn parse(s: &str) -> Option<Self>;       // Accepts Camelot / Open Key / traditional
    pub fn camelot(&self) -> String;             // "8A"
    pub fn open_key(&self) -> String;            // "8m"
    pub fn traditional(&self) -> String;         // "Am"
}
```

#### `standard` module

BPM / key / comment read+write across all `lofty`-supported formats.

```rust
pub fn read_bpm(tag: &Tag) -> Option<f64>;
pub fn write_bpm(tag: &mut Tag, bpm: f64);
pub fn clear_bpm(tag: &mut Tag);
pub fn read_key(tag: &Tag) -> Option<MusicalKey>;
pub fn read_key_raw(tag: &Tag) -> Option<String>;
pub fn write_key(tag: &mut Tag, key: MusicalKey);
pub fn write_key_raw(tag: &mut Tag, value: String);
pub fn clear_key(tag: &mut Tag);
pub fn read_comment(tag: &Tag) -> Option<String>;
pub fn write_comment(tag: &mut Tag, value: String);
pub fn clear_comment(tag: &mut Tag);
```

#### `mik` module — Mixed In Key reader

Recovers MIK's key / energy / tempo from every location MIK is documented to write to (standard fields, artist/title prefixes and suffixes, comment, grouping, label) and normalises into standard tag fields. Standards-first by design — only Energy falls back to `MeedyaMeta:Energy` because no widely-supported standard exists for it.

```rust
pub fn read_mik(tag: &Tag) -> MikAnalysis;
pub fn normalise_to_standards(tag: &mut Tag, analysis: &MikAnalysis);

pub struct MikAnalysis {
    pub key: Option<MusicalKey>,
    pub energy: Option<u8>,        // 1-10
    pub bpm: Option<f64>,
    pub sources: Vec<MikSourceLocation>,
}

pub struct MikSourceLocation {
    pub field: MikField,
    pub position: MikPosition,
    pub kinds: MikKinds,
}
pub enum MikField { InitialKey, Bpm, Artist, Title, Comment, Grouping, Label }
pub enum MikPosition { Whole, Prefix, Suffix }
pub struct MikKinds { pub key: bool, pub bpm: bool, pub energy: bool }
```

**Normalisation** writes:

- Key → `ItemKey::InitialKey` (TKEY / `----:com.apple.iTunes:initialkey` / INITIALKEY)
- BPM → `ItemKey::IntegerBpm` + `ItemKey::Bpm` (TBPM / tmpo / BPM)
- Energy → `MeedyaMeta:Energy` (no standard exists)
- Audit trail → `MeedyaMeta:MikSourceLocations` (which location each datapoint came from)

Source fields (Artist/Title/Comment/Grouping/Label) are **read-only**; the original strings are preserved verbatim. A separate opt-in cleanup pass could strip MIK prefixes later.

**Token classification** (greedy prefix/suffix matching):

- Camelot/OpenKey/traditional (with sharps OR flats) → key. Zero-padded `05A` supported.
- `"Energy N"` (case-insensitive) → energy (1-10).
- Bare integer 1-10 → energy.
- Bare integer 40-250 → tempo.
- `" - "` (space-dash-space) is the separator. `"10A-Feel"` (no spaces) is NOT classified as MIK.

#### Pending (proprietary readers, fixture-driven)

`serato`, `rekordbox`, `traktor`, `virtualdj` modules. Each will be implemented in its own focused session against real DJ-tagged fixture files. See [`.claude/PROMPTS.md`](../.claude/PROMPTS.md#implementing-a-proprietary-dj-reader) for the procedure and guardrails.

#### Modules not covered above

These are fully implemented and root-re-exported, and were missing from earlier revisions of
this file. Signatures below are the crate-root re-exports.

**`ai_content`** — AI-disclosure flags (#43).
```rust
pub struct AiContentFlags { /* is_ai, ai_used, ai_enhanced, detail */ }
pub fn read_ai_content(..); pub fn write_ai_content(..); pub fn clear_ai_content(..);
pub fn parse_bool_truthy(value: &str) -> Option<bool>;
```

**`stems`** — stem-collection metadata (#42).
```rust
pub struct StemMetadata; pub enum StemRole; pub enum StemSource;
pub fn read_stems(..); pub fn write_stems(..); pub fn clear_stems(..);
```

**`play_history`** — play/skip counts with timestamps (#56).
```rust
pub struct PlayHistory;
pub fn read_play_history(..); pub fn write_play_history(..); pub fn clear_play_history(..);
pub fn record_play(..); pub fn record_skip(..);
```

**`genre_hierarchy`** — Beatport-style genre → subgenre → style (#46). Writes the leaf to the
standard `Genre` field and the structured levels to `MeedyaMeta` (standards-first).
```rust
pub struct GenreHierarchy;
pub fn read_genre_hierarchy(..); pub fn write_genre_hierarchy(..); pub fn clear_genre_hierarchy(..);
```

**`quick_tag`** — TOML-driven mood/energy/style buckets (#48).
```rust
pub struct QuickTagSchema; pub struct QuickTagCategory; pub struct QuickTagValues;
pub enum QuickTagValidationError;
pub fn read_quick_tags(..); pub fn write_quick_tags(..); pub fn clear_quick_tags(..);
pub fn validate_quick_tags(..);   // re-export of quick_tag::validate
```

**`conflict_policy`** — declarative tag-conflict resolution with an audit trail (#54). The
caller builds `Vec<Candidate<T>>` from source-specific readers and calls `resolve_conflict`;
`Resolution<T>` carries the winner **and** the losers.
```rust
pub struct Candidate<T>; pub struct ConflictPolicy; pub enum Tiebreak;
pub trait ResolvableField; pub enum ResolutionError;
pub fn resolve_conflict(..);      // re-export of conflict_policy::resolve
```

**`sidecar_json`** — `.meedya.json` sidecar writer (#57). Schema version is strict: the
reader rejects a newer `SCHEMA_VERSION` rather than silently misreading it.
```rust
pub struct MeedyaSidecar; pub enum SidecarFormat; pub enum SidecarError;
pub fn read_sidecar(..); pub fn write_sidecar(..); pub fn write_sidecar_with_format(..);
pub fn sidecar_path_for(..);
pub const SIDECAR_SCHEMA_VERSION; pub const SIDECAR_SUFFIX;
```

---

## Common workflows

### Apple Music download + tag (MeedyaDL flow)

```text
1. Download via MeedyaDL pipeline (out of scope here)
2. meedya_metadata::writer::write_tags_from_registry(tag, &TAG_REGISTRY, album_json, track_json)
3. meedya_metadata::writer::write_local_tags(tag)        // SourceStore etc.
4. meedya_metadata::codec_tags::apply_codec_metadata_tags(path, &codec)
5. meedya_metadata::writer::extract_isrc_from_vendor(tag)
```

### Audio fingerprinting + tagging

```text
1. fingerprint::AcoustIdClient::new(...).lookup(&fingerprint, duration_seconds)?
   → AcoustIdResult
2. metadata::tag_io::write_acoustid_tags(&path, &acoustid_result)?
```

### ReplayGain analysis + tagging

```text
1. let result = fingerprint::ReplayGainAnalyzer::new(ffmpeg_path).analyze_track(&path).await? → ReplayGainResult
2a. Track mode: metadata::tag_io::write_replaygain_tags(&path, &result, None)?
2b. Album mode: collect ReplayGainResult per track into `tracks: Vec<ReplayGainResult>`,
    let album_result = analyzer.compute_album_gain(&tracks); // Option<AlbumGainResult>
    then call write_replaygain_tags(&path, &result, album_result.as_ref())? once per track
    (album_result is threaded through the same call, not a separate write)
```

### Lyrics fetch + write

```text
1. let lyrics = lyrics::LrclibProvider::new().fetch(&TrackQuery { ... }).await?;  // Err, not None, when not found
2a. lyrics::sidecar::write(&media_path, &lyrics)?;        // .lrc next to file; Ok(None) if no synced lines
2b. lyrics::embed::embed(&media_path, &lyrics)?;          // tag-embed via meedya-metadata
```

### Library import → apply soft trim

```text
1. let report = library_import::itunes_xml::import(Path::new("Library.xml"))?;
2. For each entry in report.entries:
   - Resolve entry.locator to a local file path
   - tag_file = meedya_tags_extended::TagFile::open(path)?
   - apply (start_ms, stop_ms) — currently via meedya-metadata mp4ameta surface:
     metadata::playback_bounds::set_playback_start(tag, start_ms)
     metadata::playback_bounds::set_playback_stop(tag, stop_ms)
   - tag_file.save()?
```

### CUE-driven chapter authoring (planned)

```text
1. let sheet = library_import::cuesheet::parse_file(&cue_path)?;
2. For each track in sheet.files[0].tracks:
   - chapter_start_ms = track.indexes.iter().find(|i| i.number == 1)?.time.to_milliseconds()
   - chapter_title    = track.title.clone().unwrap_or_else(|| format!("Track {}", track.number))
3. (Future) Write MP4 chap track + chpl atom via a meedya-chapters crate
```

### Read DJ metadata from a file

```text
1. let mut tag_file = meedya_tags_extended::TagFile::open(&path)?;
2. let tag         = tag_file.primary_tag().ok_or(...)?;
3. let bpm         = meedya_tags_extended::standard::read_bpm(tag);
4. let key         = meedya_tags_extended::standard::read_key(tag);
5. (Future) let serato_data = meedya_tags_extended::serato::read(&tag_file)?;
```

### Recover Mixed In Key analysis and normalise to standard tags

```text
1. let mut tag_file = meedya_tags_extended::TagFile::open(&path)?;
2. let analysis = meedya_tags_extended::read_mik(tag_file.primary_tag().unwrap());
3. // Inspect analysis.key / .bpm / .energy / .sources for UI display, etc.
4. meedya_tags_extended::normalise_to_standards(tag_file.primary_tag_mut(), &analysis);
5. tag_file.save()?;
// Result: standard InitialKey + IntegerBpm + Bpm now populated regardless of
// where MIK originally wrote the data (e.g., comment prefix "10A - 126 - 7").
// MeedyaMeta:Energy carries the energy rating (no standard for that field).
// Original source fields (artist/title/comment/etc.) are NOT modified.
```

### Embed lyrics with both plain text and synchronised SYLT (MP3)

```text
1. let lyrics = LrclibProvider::new().fetch(&query).await?;   // Err (not None) when not found
2. let _ = meedya_lyrics::embed(&path, &lyrics)?;     // USLT/©lyr/LYRICS
3. if lyrics.synced.is_some() {
       // SYLT — succeeds on ID3v2 (MP3) only; ignore Error::UnsupportedForSync.
       let _ = meedya_lyrics::embed_synced(&path, &lyrics, meedya_lyrics::DEFAULT_LANGUAGE);
   }
```

---

## Stability and versioning

| Tier | Crates | Compatibility guarantee |
|---|---|---|
| **Stable** | `meedya-codecs`, `meedya-core`, `meedya-fingerprint`, `meedya-library-import`, `meedya-lyrics`, `meedya-metadata`, `meedya-providers` | Public APIs follow semver; breaking changes get a major-version bump. Foundation types (`AudioCodec`, `ContainerFormat`, `CommonTag`, `Track`/`Album`/`Artist`) are particularly stable. |
| **Foundation stable + MIK reader** | `meedya-tags-extended` | Core types (`ExtendedTags`, `MusicalKey`, `CuePoint`) and the Mixed In Key reader (`read_mik`, `normalise_to_standards`, `MikAnalysis`) are stable. Other proprietary reader modules (`serato`, `rekordbox`, `traktor`, `virtualdj`) are not yet implemented — when added, they will populate the existing `ExtendedTags` shape, not change it. |
| **Experimental** | (none currently) | — |

As of **0.2.0** (#65), `CommonTag` is `#[non_exhaustive]` — downstream exhaustive matches over it stop compiling (the one deliberate breaking change this bump carries) and every variant added from here on is non-breaking. `identifier_types` is a new additive module; its types (`IdentifierScope`/`IdentifierStatus`/`IdentifierValidation`) are also `#[non_exhaustive]` from day one.

All crates share workspace `version = "0.2.0"` (bumped from `0.1.0` by #65). Pre-1.0, minor-version bumps may include breaking changes; please pin to a git revision or tag in downstream apps until 1.0.

---

## Consumption by language

### Rust (MeedyaDL, MeedyaManager)

Direct Cargo dependency. Pick individual crates or use `meedya-core` with feature flags:

```toml
# Individual
meedya-metadata = { git = "https://github.com/MWBMPartners/MeedyaSuite-core", rev = "..." }

# Or facade
meedya-core = { git = "https://github.com/MWBMPartners/MeedyaSuite-core", rev = "...", features = ["full"] }
```

Pin to a specific `rev = "<sha>"` or `tag = "..."` in production — `branch = "main"` will pull the latest and may break unexpectedly until 1.0.

**MSRV**: Rust 1.82 (declared via `rust-version` on `[workspace.package]`, inherited by every member crate; driven by `Option::is_none_or`).

### Swift (MeedyaConverter, MeedyaDB)

Planned via [`bindings/swift/`](../bindings/swift/) — Swift Package wrapping a Rust static library through C FFI / XCFramework. **Not yet scaffolded.** Until then, MeedyaConverter / MeedyaDB cannot directly consume this workspace.

When scaffolded, the binding will expose a C-FFI-compatible subset:

- `AudioCodec`, `VideoCodec`, `ContainerFormat` etc. as C-shaped enums + helpers
- `CommonTag` + tag I/O as opaque-handle-style APIs (init, set, get, save)
- `Track`/`Album`/`Artist` as serialized JSON across the FFI boundary (simpler than fully marshalling structs)

### Web (future)

Planned via [`bindings/wasm/`](../bindings/wasm/) — `wasm-bindgen` wrapping a subset of the workspace for browser/Node.js targets. **Not yet scaffolded.**

---

## Maintenance

This document is the curated human-readable reference. **It must be kept in sync with the code.**

### When to update

Refresh this spec whenever a public API surface changes:

- New crate added or renamed
- New public module added
- New `pub` type, function, trait, or constant added at module root
- Existing public item removed or renamed
- Trait method signature changed
- Feature flag added / renamed / removed in `meedya-core`
- Workspace test count materially changes (≥5 net change)

Cosmetic edits (doc comment changes, internal refactors) do not require this update.

### Refresh procedure

The procedure is captured in [`.claude/PROMPTS.md`](../.claude/PROMPTS.md#refresh-internal-api-spec). Summary:

1. Run `cargo test --workspace` and capture per-crate test counts.
2. Read each crate's `src/lib.rs` to list `pub use` re-exports and `pub mod` declarations.
3. For changed modules, walk `pub fn` / `pub struct` / `pub enum` / `pub trait` items.
4. Update the relevant crate section in this file, the overview table at the top, and the "Last refreshed" date.
5. Cross-reference [`README.md`](../README.md) — bump test counts if the totals changed.
6. Commit alongside the API-touching change (not as a follow-up PR).

### Auto-generated companion

```bash
cargo doc --workspace --no-deps --open
```

Produces the full auto-generated reference. Use it for exhaustive signatures and trait bounds; use this `API.md` for orientation and integration patterns.

### Stale-spec safeguard

Future improvement: a CI check that diffs `cargo public-api` output against the previous `main` and fails if `docs/API.md` wasn't touched in the same commit. Not yet implemented.
