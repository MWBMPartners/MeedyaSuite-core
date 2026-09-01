# MeedyaSuite-core — Internal API Specification

> **Audience**: developers of partner apps (MeedyaDL, MeedyaConverter, MeedyaManager, MeedyaPlayer, MeedyaDB) integrating with `MeedyaSuite-core`.
>
> **Scope**: the public API surface of every crate in the workspace — what to import, what types to expect, how the crates compose. This document is the curated, human-readable reference; the exhaustive auto-generated reference is `cargo doc --workspace --no-deps --open`.
>
> **This is not a Swagger/OpenAPI spec.** `MeedyaSuite-core` is a Rust library workspace, not a web service. There are no HTTP endpoints. If you need an HTTP-shaped contract, build one in your downstream app on top of these crates.
>
> **Last refreshed**: 2026-09-01 (post issue #65 completion pass: GRid/ICPN reserved, per-scheme normalisation guidance, `write_tags` signature fix, AcoustID read-back; same-day, post MusicBrainz Solr 9→10 search-hardening pass — new `lucene` escaping module, `MusicBrainzProvider::build_lucene_query`, ISRC/ISWC query normalisation, forward-compat parse fixtures). See the [maintenance section](#maintenance) for how this stays in sync with the code.

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
| `meedya-db` | `client`, `export`, `models` | 3 | Foundation stable; specific endpoints may evolve |
| `meedya-fingerprint` | `acoustid`, `replaygain` | 6 | Stable |
| `meedya-library-import` | `cuesheet`, `itunes_xml` | 30 | Stable |
| `meedya-lyrics` | `embed`, `lrc`, `lyrics`, `provider`, `sidecar` | 128 | Stable (plain + synced via SYLT for ID3v2) |
| `meedya-metadata` | `codec_tags`, `common_tags`, `identifier_types`, `json_path`, `playback_bounds`, `registry`, `tag_io`, `tag_registry`, `writer` | 112 | Stable (two co-existing surfaces + identifier-types registry) |
| `meedya-providers` | `cover_art`, `credentials`, `extra_keys`, `lucene`, `match_scoring`, `providers` (feature-gated), `rate_limiter`, `traits`, `types` | 39 | Stable foundation; specific provider implementations may evolve |
| `meedya-tags-extended` | `io`, `mik`, `model`, `standard` | 180 | Foundation stable + Mixed In Key reader; other proprietary DJ readers pending |

**Total: 546 tests** (653 with --all-features, the CI configuration). All passing (post #65 identifier-types registry batch — 511 → 533 measured; the +4 over the batch's 529 are tag-I/O save/reload round-trip tests added with the #65 silent-data-loss fix — plus +1 from the 2026-09-01 #65 completion pass' AcoustID read-back regression test, 533 → 534; plus +12 default-feature / +29 --all-features from the same-day MusicBrainz Solr-10 search-hardening pass, 534 → 546 / 624 → 653 — the new always-compiled `lucene` escaping module accounts for the default-feature delta (11 unit tests + 1 doctest), and the `--all-features` delta additionally reflects `build_lucene_query`/forward-compat-fixture tests added to the feature-gated `provider-musicbrainz`, `provider-isrc`, and `provider-iswc` modules). Previous totals in this file were stale: it long read 466, but the measured pre-#65 count was actually 511 — the count-drift itself is tracked as a follow-up (§9 of the #65 build spec, "for consideration").

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
pub use meedya_metadata::{CommonTag, MetadataError, TagRegistry};
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
pub trait LyricsProvider {
    async fn fetch(&self, query: &TrackQuery) -> Result<Option<Lyrics>>;
}
```

Implementation: `LrclibProvider` (calls lrclib.net).

#### Write targets

- **`sidecar::write(lyrics: &Lyrics, target_path: &Path) -> Result<()>`** — writes a `.lrc` file next to the source media.
- **`embed::embed(media: &Path, lyrics: &Lyrics) -> Result<bool>`** — plain-text tag-embed via `meedya-metadata` (USLT for ID3v2, `LYRICS` for Vorbis, `©lyr` for MP4).
- **`embed::embed_synced(media: &Path, lyrics: &Lyrics, lang: [u8; 3]) -> Result<()>`** — synchronised ID3v2 SYLT frame. ID3v2-only by design; errors with `Error::UnsupportedForSync` on other formats. Encoding: UTF-16 with BOM; timestamp format: milliseconds. Recommended pattern: call both `embed()` and `embed_synced()` — the former handles cross-format plain text, the latter adds SYLT where applicable.
- **`embed::DEFAULT_LANGUAGE`** — `*b"eng"`, the ISO-639-2 default for callers without a known language code.

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
pub use identifier_types::{
    active_identifier_slugs, identifier_type, identifier_types, IdentifierScope,
    IdentifierStatus, IdentifierType, IdentifierValidation, IDENTIFIER_TYPES_TOML,
};
pub use json_path::{extract_json_value, value_to_string};
pub use tag_io::{read_tags, write_acoustid_tags, write_registry_tags,
                 write_replaygain_tags, write_tags, TagMap};
pub use tag_registry::{AtomTarget, TagDefinition, TagRegistry, TagScope, TagValueType};
```

#### Surface 1: `lofty`-backed (multi-format)

For MP3 / M4A / FLAC / WAV / AIFF / OGG and downstream-app general use.

- **`common_tags`** — `CommonTag` enum (core identifiers — `Isrc`/`Upc`/MusicBrainz IDs/`AcoustId`; basic metadata — `Title`/`Artist`/`Album`/etc.; extended metadata; ReplayGain; catalog/date fields; and, as of #65, MB Release-Group/Work IDs + `Iswc` + core-info/contributor-role fields — see below) with `STANDARD_NAMESPACES` mapping each to its ID3v2 / Vorbis / MP4 ilst frame name. `Bpm`/`InitialKey` are **not** `CommonTag` concepts — those live in `meedya-tags-extended::standard` (DJ metadata), a distinct surface.
- **`identifier_types`** — Cross-repo identifier-type registry (#65). DATA, not an enum: see the dedicated subsection below.
- **`tag_io`** — Lofty-driven file I/O:
  - `read_tags(path: &Path) -> Result<TagMap>`
  - `write_tags(path: &Path, tags: &[(CommonTag, String)]) -> Result<()>`
  - `write_registry_tags(path, json: &Value, registry: &TagRegistry) -> Result<()>`
  - `write_acoustid_tags(path, result: &AcoustIdResult) -> Result<()>`
  - `write_replaygain_tags(path, result: &ReplayGainResult) -> Result<()>`
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
pub use lucene::{escape_lucene, quote_phrase};
pub use match_scoring::{MatchScorer, ScoringWeights};
pub use rate_limiter::{ProviderRateLimiter, RateLimiterRegistry};
pub use traits::{MetadataProvider, ProviderCapabilities, ProviderError};
pub use types::{CoverArtInfo, MediaType, ProviderResult, SearchQuery};
```

#### `lucene`

Lucene/Solr query escaping for MusicBrainz search — always compiled (pure `std`, no feature gate, no dependencies). Every `providers::musicbrainz` / `providers::isrc` / `providers::iswc` query built from user-supplied text goes through this module rather than interpolating raw strings into the Lucene query.

```rust
pub fn escape_lucene(value: &str) -> String;
pub fn quote_phrase(value: &str) -> String;
```

- **`escape_lucene`** — backslash-escapes every Lucene special character (`+ - ! ( ) { } [ ] ^ " ~ * ? : \ /` and the boolean operators `&`/`|`, so `&&`/`||` become `\&\&`/`\|\|`). Does not handle whitespace or field-scoping.
- **`quote_phrase`** — the quoting policy providers actually use for free-text field values (`title`, `artist`, etc.): escapes embedded `\` and `"` (backslash first, to avoid double-escaping), then wraps the result in double quotes. Other Lucene special characters are left as-is inside the phrase — Lucene does not interpret them specially within quotes. The field qualifier stays outside the quoted value, e.g. `format!("recording:{}", quote_phrase(title))`.

`MusicBrainzProvider::build_lucene_query` (private) is the reference consumer: it normalises and validates ISRC values (`isrc:<12-char-code>`, no quoting needed — normalisation strips everything that isn't alphanumeric) and, for free-text search, quotes title/artist as `recording:"..." AND artistname:"..."`.

#### `MetadataProvider` trait

```rust
pub trait MetadataProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    async fn search(&self, query: &SearchQuery) -> Result<ProviderResult, ProviderError>;
    // ... (lookup, get_by_id, etc.)
}
```

Implemented in-repo, one file per external service, each gated behind its own `provider-<name>` Cargo feature (`crates/meedya-providers/src/providers/`): `musicbrainz`, `spotify`, `apple_music`, `deezer`, `tmdb`, `thetvdb`, `omdb`, `apple_tv`, `itunes_store`, `apple_podcasts`, `isrc`, `eidr`, `iswc`. These are not stubs for downstream apps to fill in — apps opt into the ones they need via Cargo features and get a working `MetadataProvider` impl; a downstream app would only implement this trait itself for a service not already covered here.

**MusicBrainz Solr 9→10 upgrade (2026-11-30)**: `musicbrainz`/`isrc`/`iswc` all default to `https://musicbrainz.org` but expose `with_base_url` for pointing at a self-hosted MusicBrainz mirror (e.g. a local `mbslave`/search-server instance). Anyone running such a mirror is responsible for following MusicBrainz's own Solr 9→10 re-index instructions (SEARCH-764) on their own schedule — this crate's query construction is hardened against the stricter Solr 10 parser (see `lucene` above), but it cannot re-index a mirror's search server for you.

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
1. let lyrics = LrclibProvider::new().fetch(&query).await?.unwrap();
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
