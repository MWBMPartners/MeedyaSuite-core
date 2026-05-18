# MeedyaSuite-core — Internal API Specification

> **Audience**: developers of partner apps (MeedyaDL, MeedyaConverter, MeedyaManager, MeedyaPlayer, MeedyaDB) integrating with `MeedyaSuite-core`.
>
> **Scope**: the public API surface of every crate in the workspace — what to import, what types to expect, how the crates compose. This document is the curated, human-readable reference; the exhaustive auto-generated reference is `cargo doc --workspace --no-deps --open`.
>
> **This is not a Swagger/OpenAPI spec.** `MeedyaSuite-core` is a Rust library workspace, not a web service. There are no HTTP endpoints. If you need an HTTP-shaped contract, build one in your downstream app on top of these crates.
>
> **Last refreshed**: 2026-05-18. See the [maintenance section](#maintenance) for how this stays in sync with the code.

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
| `meedya-core` | (facade re-exports only) | 0 | Stable |
| `meedya-db` | `client`, `export`, `models` | 3 | Foundation stable; specific endpoints may evolve |
| `meedya-fingerprint` | `acoustid`, `replaygain` | 6 | Stable |
| `meedya-library-import` | `cuesheet`, `itunes_xml` | 30 | Stable |
| `meedya-lyrics` | `embed`, `lrc`, `lyrics`, `provider`, `sidecar` | 10 | Stable |
| `meedya-metadata` | `codec_tags`, `common_tags`, `json_path`, `playback_bounds`, `registry`, `tag_io`, `tag_registry`, `writer` | 59 | Stable (two co-existing surfaces) |
| `meedya-providers` | `cover_art`, `credentials`, `match_scoring`, `rate_limiter`, `traits`, `types` | 27 | Stable foundation; specific provider implementations may evolve |
| `meedya-tags-extended` | `io`, `model`, `standard` | 29 | Foundation stable; proprietary DJ readers pending |

**Total: 211 tests.** All passing on `main`.

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
use meedya_codecs::{AudioCodec, ContainerFormat, ffprobe};

// Detect codec from a file
let info = ffprobe::probe("/path/to/song.m4a")?;
let codec = info.audio_codec(); // Option<AudioCodec>

// Check container compatibility
let is_compatible = ContainerFormat::M4a.supports_audio(AudioCodec::Alac);
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
```

#### `meedya_core::prelude`

```rust
// With default features
pub use meedya_metadata::{CommonTag, MetadataError, TagRegistry};
pub use meedya_codecs::{AudioCodec, ChannelConfig, CodecRegistry, ContainerFormat, SpatialType};
pub use meedya_providers::{CredentialStore, MetadataProvider, ProviderCapabilities,
                            ProviderRateLimiter, ProviderResult, SearchQuery};
pub use meedya_lyrics::{Lyrics, LyricsProvider, SyncedLine, TrackQuery};
```

> **Note**: `meedya-tags-extended` and `meedya-library-import` are not yet re-exported through `meedya-core`. Consume them directly until a feature flag is added.

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
    AlbumGainResult, ReplayGainAnalyzer, ReplayGainResult, DEFAULT_REFERENCE_LEVEL
};
```

#### `AcoustIdClient`

AcoustID API client with built-in rate limiting (3 requests/second per the AcoustID terms). Returns `AcoustIdResult` containing matched MusicBrainz recording IDs and scores. Uses pure-Rust Chromaprint via `rusty-chromaprint` — no `fpcalc` binary required.

#### `ReplayGainAnalyzer`

EBU R128 loudness measurement. Computes track gain + peak; aggregates multiple tracks into `AlbumGainResult` for album-mode normalisation. Reference level defaults to `DEFAULT_REFERENCE_LEVEL` (-18 LUFS).

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
pub trait LyricsProvider {
    async fn fetch(&self, query: &TrackQuery) -> Result<Option<Lyrics>>;
}
```

Implementation: `LrclibProvider` (calls lrclib.net).

#### Write targets

- **`sidecar::write(lyrics: &Lyrics, target_path: &Path) -> Result<()>`** — writes a `.lrc` file next to the source media.
- **`embed::embed(...)`** — plain-text tag-embed via `meedya-metadata` (USLT for ID3v2, `LYRICS` for Vorbis, `©lyr` for MP4). Synchronised ID3v2 SYLT is **not yet** supported.

#### `lrc` module

```rust
pub fn parse(text: &str) -> Result<Lyrics>;
pub fn write(lyrics: &Lyrics) -> String;
```

---

### `meedya-metadata`

Tag schemas, metadata read/write, and a config-driven TOML tag registry. Two parallel surfaces co-exist intentionally — they serve different code paths.

#### Public re-exports

```rust
pub use common_tags::{CommonTag, STANDARD_NAMESPACES};
pub use error::MetadataError;
pub use json_path::{extract_json_value, value_to_string};
pub use tag_io::{read_tags, write_acoustid_tags, write_registry_tags,
                 write_replaygain_tags, write_tags, TagMap};
pub use tag_registry::{AtomTarget, TagDefinition, TagRegistry, TagScope, TagValueType};
```

#### Surface 1: `lofty`-backed (multi-format)

For MP3 / M4A / FLAC / WAV / AIFF / OGG and downstream-app general use.

- **`common_tags`** — `CommonTag` enum (`Title`, `Artist`, `Album`, `Lyrics`, `Bpm`, `InitialKey`, etc.) with `STANDARD_NAMESPACES` mapping each to its ID3v2 / Vorbis / MP4 ilst frame name.
- **`tag_io`** — Lofty-driven file I/O:
  - `read_tags(path: &Path) -> Result<TagMap>`
  - `write_tags(path: &Path, tags: &TagMap) -> Result<()>`
  - `write_registry_tags(path, json: &Value, registry: &TagRegistry) -> Result<()>`
  - `write_acoustid_tags(path, result: &AcoustIdResult) -> Result<()>`
  - `write_replaygain_tags(path, result: &ReplayGainResult) -> Result<()>`
- **`tag_registry`** — `TagDefinition`, `TagRegistry`, `TagScope`, `TagValueType`, `AtomTarget` for declarative tag mapping loaded from TOML.
- **`json_path`** — Dot-path extraction (`extract_json_value`, `value_to_string`) with array indexing for API JSON → tag-value pipelines.

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
pub use match_scoring::{MatchScorer, ScoringWeights};
pub use rate_limiter::{ProviderRateLimiter, RateLimiterRegistry};
pub use traits::{MetadataProvider, ProviderCapabilities, ProviderError};
pub use types::{CoverArtInfo, MediaType, ProviderResult, SearchQuery};
```

#### `MetadataProvider` trait

```rust
pub trait MetadataProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    async fn search(&self, query: &SearchQuery) -> Result<ProviderResult, ProviderError>;
    // ... (lookup, get_by_id, etc.)
}
```

Downstream apps implement this for each external service (MusicBrainz, TMDB, TheTVDB, Discogs, FanArt.tv, etc.).

#### `ProviderRateLimiter`

`governor`-backed rate limiter with configurable quotas per provider. `RateLimiterRegistry` manages multiple providers' limits.

#### `CredentialStore`

Pluggable credential storage with `CredentialSource` variants (in-memory, env var, OS keyring via the `keyring` feature). `ResolvedCredential` is the result of a lookup.

#### `MatchScorer`

Fuzzy-match scoring for metadata search results. `ScoringWeights` configures per-field weight (title vs artist vs album vs year, etc.).

#### `cover_art`

Helpers for cover art selection — `CoverArtSize` (e.g., `Thumbnail`, `Square500`, `Full`), `CoverArtInfo` (URL + dimensions).

---

### `meedya-tags-extended`

Multi-format tag I/O foundation with DJ metadata support. Built on `lofty`. Designed to host proprietary DJ-software readers (Serato, Rekordbox, Traktor, Virtual DJ) populating a unified `ExtendedTags` shape.

#### Public re-exports

```rust
pub use io::TagFile;
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
    pub energy: Option<u8>,
    pub cue_points: Vec<CuePoint>,
    pub loops: Vec<LoopPoint>,
    pub beat_grid: Option<BeatGrid>,
    pub comment: Option<String>,
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

BPM / key / comment read+write across all `lofty`-supported formats. Covers Mixed In Key fully (MIK writes only standard tags).

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

#### Pending (foundation only; not yet implemented)

`serato`, `rekordbox`, `traktor`, `virtualdj` modules. Each will be implemented in its own focused session against real DJ-tagged fixture files. See [`.claude/PROMPTS.md`](../.claude/PROMPTS.md#implementing-a-proprietary-dj-reader) for the procedure and guardrails.

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
1. fingerprint::ReplayGainAnalyzer.analyze(&path)? → ReplayGainResult
2. metadata::tag_io::write_replaygain_tags(&path, &replaygain_result)?
   // For album mode: collect ReplayGainResult per track, build AlbumGainResult, then write both
```

### Lyrics fetch + write

```text
1. let lyrics = lyrics::LrclibProvider::new().fetch(&TrackQuery { ... }).await?;
2a. lyrics::sidecar::write(&lyrics, &media_path)?;        // .lrc next to file
2b. lyrics::embed::embed(&lyrics, &media_path)?;          // tag-embed via meedya-metadata
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
1. let tag_file = meedya_tags_extended::TagFile::open(&path)?;
2. let tag      = tag_file.primary_tag().ok_or(...)?
3. let bpm      = meedya_tags_extended::standard::read_bpm(tag);
4. let key      = meedya_tags_extended::standard::read_key(tag);
5. (Future) let serato_data = meedya_tags_extended::serato::read(&tag_file)?;
```

---

## Stability and versioning

| Tier | Crates | Compatibility guarantee |
|---|---|---|
| **Stable** | `meedya-codecs`, `meedya-core`, `meedya-fingerprint`, `meedya-library-import`, `meedya-lyrics`, `meedya-metadata`, `meedya-providers` | Public APIs follow semver; breaking changes get a major-version bump. Foundation types (`AudioCodec`, `ContainerFormat`, `CommonTag`, `Track`/`Album`/`Artist`) are particularly stable. |
| **Foundation stable** | `meedya-tags-extended` | Core types (`ExtendedTags`, `MusicalKey`, `CuePoint`) are stable. Proprietary reader modules (`serato`, `rekordbox`, `traktor`, `virtualdj`) are not yet implemented — when added, they will populate the existing `ExtendedTags` shape, not change it. |
| **Experimental** | (none currently) | — |

All crates share workspace `version = "0.1.0"`. Pre-1.0, minor-version bumps may include breaking changes; please pin to a git revision or tag in downstream apps until 1.0.

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
