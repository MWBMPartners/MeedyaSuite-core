# MeedyaSuite-core — Internal API Specification

> **Audience**: developers of partner apps (MeedyaDL, MeedyaConverter, MeedyaManager, MeedyaPlayer, MeedyaDB) integrating with `MeedyaSuite-core`.
>
> **Scope**: the public API surface of every crate in the workspace — what to import, what types to expect, how the crates compose. This document is the curated, human-readable reference; the exhaustive auto-generated reference is `cargo doc --workspace --no-deps --open`.
>
> **This is not a Swagger/OpenAPI spec.** `MeedyaSuite-core` is a Rust library workspace, not a web service. There are no HTTP endpoints. If you need an HTTP-shaped contract, build one in your downstream app on top of these crates.
>
> **Last refreshed**: 2026-10-05 (`feature/bcp47-language-policy`: the fixes for Codex's catch-up review of revisions 8–10 — the temporary copy an M4A save is checked on is private (owner-only) from the moment it exists, and an error names a copy that could not be deleted; an M4A file whose metadata box is too short for its version, whose tag list ends with bytes belonging to no atom, or whose containers are nested where no M4A file has them is refused before anything is written, instead of stopping the program or losing those bytes; the whole-file comparison takes work in step with the file; reading a file's ID3 language frames holds at most 1 MiB in memory; the documented-test-count check compares the stated totals only (#102). Before that, on 2026-10-04: the fixes for the stand-in review of revision 9 — every M4A save is checked across the whole file (the audio, every chunk offset, every atom outside the tags), a fragmented M4A file or one whose metadata box has no tag list is refused before anything is written, a value an M4A file would not store exactly as given (or a field it has no atom for) is refused, the temporary copy is written through its own handle, flushed and checked before the rename, and a language frame in a file with two or more ID3 tags refuses the save (#102). Before that, on 2026-09-28: the fixes for the stand-in review of revision 8 — every `tag_io` write to an M4A file is made on a temporary copy and compared atom by atom with the original, and replaces it only when nothing not asked for would change and everything asked for is stored exactly (#102, #103; this refuses most iTunes-style files until #102's real fix), and a write to an MP3, WAV or AIFF file whose language frames sit in more than one ID3v2 tag or ID3 chunk is refused. Before that the same day: the fixes for Codex's review of revisions 5–7 — old `TLA` language frames in an ID3v2.3 tag are merged like `TLAN` ones, and one in an ID3v2.4 tag refuses the save; merging a file's language frames takes work in step with the values; the one public helper that undid a caller's own language change is replaced by two steps, `tag_io::recover_languages_after_reading` (straight after reading) and `gather_languages_before_saving` (just before saving); an M4A write that would lose a value is refused (#102), and `write_registry_tags` refuses an M4A key it cannot store instead of reporting it written (#103); `Lyricsfile::to_yaml` quotes text holding a carriage return, U+0085, U+2028 or U+2029 even where it would have written a block. Before that the same day: the fixes for the stand-in review of revision 6 — `write_tags` judges an unchanged language against the tag it writes into, so a language `read_tags` found in a WAV file's RIFF INFO list or an MP3's APE tag is carried into the new ID3v2 tag instead of being lost; a file that already holds one `TLAN` frame per language keeps every language through every save (a file whose repeated frames cannot be read is refused rather than saved); `Lyricsfile::to_yaml` writes the language as `parse` reads it and quotes every text value. Before that the same day: the fixes for the stand-in review of revision 5 — every write in `tag_io` keeps a file's languages whole (new `tag_io::gather_languages_before_saving`; MP4 languages are one atom with one `data` atom each), several `Language` entries in one call are all written, an unchanged language value is left alone, `MetadataError` is `#[non_exhaustive]`, `Lyricsfile::parse` reads the language through the policy reader and `to_yaml` quotes language values. Before that the same day: the fixes for Codex's review r7 — `CommonTag::Language` writes every language a value lists and refuses an unrecognised one with the new `MetadataError::UnrecognisedLanguage`; an unrecognised `xml:lang` becomes `und` with the text kept in the new `LyricsfileMetadata::language_original`; `label` leaves out an empty part. Before that the same day: `meedya-lang` brought into line with policy revision 4 — a shared `RoleItem` trait under `TrackItem`/`PresentationItem`/`SelectableTrack`, serde on the public data types, `from_legacy_three_letter_all`, `SidecarParts::ignored`, `TagMatch::distance` as `usize`, `SubtitleMode`/`Role` words and defaults, and the behaviour fixes listed in its section. Before that the same day: `meedya-lyrics` and `meedya-metadata` brought into line with the Media Language & BCP 47 Policy, `MWBM-MEDIA-LANG` — `embed::DEFAULT_LANGUAGE`/`embed::id3_language`, `xml:lang` reading, and `CommonTag::Language` writing all now go through the shared `meedya-lang` crate rather than guessing or passing values through unchanged). See the [maintenance section](#maintenance) for how this stays in sync with the code.

---

## Table of Contents

- [Workspace overview](#workspace-overview)
- [Crate APIs](#crate-apis)
  - [`meedya-codecs`](#meedya-codecs)
  - [`meedya-core`](#meedya-core)
  - [`meedya-db`](#meedya-db)
  - [`meedya-audio-analysis`](#meedya-audio-analysis)
  - [`meedya-fingerprint`](#meedya-fingerprint)
  - [`meedya-lang`](#meedya-lang)
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
| `meedya-audio-analysis` | `tempo`, `key`, `decode` (feature-gated, default-on) | 67 | Experimental |
| `meedya-fingerprint` | `acoustid`, `chromaprint` (feature-gated, non-default), `replaygain` | 12 | Stable |
| `meedya-lang` | `tag`, `canonical`, `roles`, `tracks`, `presentation`, `matching`, `select`, `sidecar` | 120 | Stable — fixture-conformance tested against `tests/fixtures/bcp47-language-policy-v1.json` (290 cases) |
| `meedya-library-import` | `cuesheet`, `itunes_xml` | 30 | Stable |
| `meedya-lyrics` | `embed`, `error`, `lrc`, `lyrics`, `lyricsfile`, `lyricsfile_export`, `lyricsfile_lrc`, `lyricsfile_ttml`, `lyricsfile_ttml_classify`, `provider`, `sidecar` | 157 | Stable (plain + synced via SYLT for ID3v2; Lyricsfile YAML model + TTML import/export) |
| `meedya-metadata` | `codec_tags`, `common_tags`, `identifier_types`, `json_path`, `playback_bounds`, `registry`, `tag_io`, `tag_registry`, `template`, `writer` | 255 | Stable (two co-existing surfaces + identifier-types registry + filename template engine) |
| `meedya-providers` | `cover_art`, `credentials`, `extra_keys`, `lucene`, `match_scoring`, `providers` (feature-gated), `rate_limiter`, `traits`, `types` | 59 | Stable foundation; specific provider implementations may evolve |
| `meedya-tags-extended` | `ai_content`, `conflict_policy`, `genre_hierarchy`, `io`, `mik`, `model`, `play_history`, `quick_tag`, `sidecar_json`, `standard`, `stems` | 180 | Foundation stable + Mixed In Key reader; other proprietary DJ readers pending |

**Total: 931 tests** with default features, **1078** with `--all-features` (the CI configuration). All passing, 0 failing.

> These are **measured** figures — `cargo test --workspace [--all-features] --locked` run against `feature/bcp47-language-policy` on 2026-10-05, after the fixes for Codex's catch-up review of revisions 8–10 (revision 11; `meedya-metadata` +18 tests, 255 measured on its own with and without `--all-features`, no other crate changed: the temporary copy private while it exists, seen through a test seam; a short `meta`, bytes left over in a tag list and containers nested without limit refused, on built files and through `write_tags` on a real file; the whole-file walk counted linear over 100,000 atoms; a copy that cannot be deleted named in the error; permission tests that cannot pass by skipping and a `co64` case; ID3 language frames read within 1 MiB on made-up files of hundreds of megabytes). Before that, 913 / 1060 on 2026-10-04, after the fixes for the stand-in review of revision 9 (revision 10; `meedya-metadata` +36 tests: the whole saved M4A file checked — the audio, every chunk offset, every atom outside the tags — on built files and on real files with audio and chapters made by ffmpeg (a fragmented file and a lofty-cleared file refused untouched; a plain and two chapter files saved with identical audio, offsets moved correctly, chapters kept); the copy made through its own handle and checked before the rename; values measured against what the caller gave and fields with no M4A atom refused by name; a language frame in a file with two ID3 tags or chunks refused, on real MP3 and WAV files; step counts showing the comparisons linear; messages that never show before and after as the same; the review's surviving planted faults each caught; no other crate changed). Before that, on 2026-09-28, 877 / 1024, after the fixes for the stand-in review of revision 8 (revision 9; `meedya-metadata` +24 tests: every M4A save checked atom by atom on a temporary copy — the checker's own tests, and real files made by ffmpeg and tagged by mutagen that must be refused and left untouched (flags, respelled freeform names, cover art with an odd type, a type or locale changed alone, two track numbers, an iTunes-style file, a registry value duplicated or overridden, a value lofty would drop) or written with every other atom unchanged (one value per atom, a two-artist atom replaced on purpose, a link followed); language frames in two ID3v2 tags or ID3 chunks refused; the order inside `edit_and_save` pinned; a frame name ending in a zero byte; `meedya-lyrics` unchanged in number, one test now lists its four line breaks itself) — not carried forward from a previous edit. Every per-crate figure was also measured on its own (`cargo test -p <crate> [--all-features] --locked`), and the per-crate figures add up to the totals. For reference, the previous measurements were 877 / 1024 (after the fixes for the stand-in review of revision 8), 853 / 1000 (after the fixes for Codex's review of revisions 5–7), 837 / 984 (after the fixes for the stand-in review of revision 6), 812 / 959 (after the fixes for the stand-in review of revision 5), 796 / 943 (after the fixes for Codex's review r7), 780 / 927 (after policy revision 4), 736 / 883 (after bringing `meedya-lyrics` and `meedya-metadata` into line with the policy), 716 / 863 (after `meedya-lang` was added), and 644 / 791 (before `meedya-lang`).
>
> Earlier revisions of this file accumulated a long narrative of incremental count deltas (466 → 511 → 533 → 546 → 664 …) which had drifted from reality. That narration has been removed: the only trustworthy number is one you just measured. Guarding these counts automatically in CI is tracked in issue #71.

Per-crate, `--all-features` (measured): `meedya-audio-analysis` 67 · `meedya-codecs` 47 · `meedya-core` 0 · `meedya-db` 4 · `meedya-fingerprint` 17 · `meedya-lang` 120 · `meedya-library-import` 30 · `meedya-lyrics` 157 · `meedya-metadata` 255 · `meedya-providers` 201 · `meedya-tags-extended` 180 (sum 1078). The table above gives default-features figures, which sum to 931.

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

### `meedya-audio-analysis`

Works out a track's tempo (beats per minute) and musical key by listening to the audio itself. Returns what it found and how sure it is — writing tags is the caller's job, as with `meedya-fingerprint`.

**Stability: Experimental.** The confidence figures have been calibrated against generated test signals, not against a large body of real music, so the thresholds may move.

#### The rule that shapes the whole crate

**It refuses rather than guesses.** A wrong tempo written into somebody's music is worse than none at all: it is hard to notice, annoying to undo, and other software will simply trust it. So every answer comes with a confidence, and the caller is expected to write nothing below a threshold. Noise, silence, and a track that genuinely could be read as either 70 or 140 beats per minute all come back with low confidence by design.

The caller should also **read any tempo already in the file first and skip the analysis entirely if one is present**. Somebody who beat-matched a track by hand has better information than any detector, and skipping saves the work as well.

#### Public re-exports

```rust
pub use error::AnalysisError;
pub use key::KeyEstimate;
pub use signal::MonoSignal;
pub use tempo::TempoEstimate;
pub use {AudioAnalyser, AudioAnalysis, detect_key, detect_tempo};
pub use {DEFAULT_MAX_ANALYSIS_SECONDS, DEFAULT_MAX_BPM, DEFAULT_MIN_BPM};
```

#### `AudioAnalyser`

```rust
impl AudioAnalyser {
    pub fn new() -> Self;
    pub fn with_max_duration(self, seconds: f64) -> Self;
    pub fn with_tempo_range(self, min_bpm: f64, max_bpm: f64) -> Self;
    pub fn with_tempo(self, enabled: bool) -> Self;
    pub fn with_key(self, enabled: bool) -> Self;

    #[cfg(feature = "decode")]
    pub fn analyse_file(&self, path: &Path) -> Result<AudioAnalysis, AnalysisError>;
    pub fn analyse_samples(&self, interleaved: &[f32], channels: u16, sample_rate: u32)
        -> Result<AudioAnalysis, AnalysisError>;
}
```

**These are ordinary functions, not `async`.** The work is entirely processor-bound and never waits for anything. An `async` function that computes without pausing blocks the worker thread it lands on for its whole duration, while its signature tells the caller it is cheap to await. Async callers should use `spawn_blocking`, exactly as they already do for `meedya-fingerprint`'s fingerprint generation.

#### What comes back

```rust
pub struct AudioAnalysis {
    pub tempo: Option<TempoEstimate>,
    pub key: Option<KeyEstimate>,
    pub analysed_seconds: f64,
    pub truncated: bool,
    pub working_sample_rate: u32,
}
```

`None` means "could not work it out at all". `Some` with a low confidence means "worked something out, but do not trust it". They are deliberately not two separate optional fields, so a value without a confidence cannot be represented.

`TempoEstimate` carries `bpm`, `confidence`, and three diagnostics — `prominence`, `segment_agreement`, and an optional `rival_bpm`/`rival_score`. The rival is the other tempo the audio could plausibly be, which is what makes an ambiguous track visibly ambiguous rather than silently halved.

`KeyEstimate` carries a `MusicalKey` from `meedya-tags-extended` — not a string, so it round-trips traditional, Camelot and Open Key notation and matches what the tag writer takes. Plus `confidence`, the raw `correlation`, the `runner_up` key, and `tuning_cents` for recordings that are not at concert pitch.

#### Honest expectations

Key detection by this method is roughly **65 to 75% accurate on real polyphonic music**. Combined with the refusal rule, that means **a large share of real tracks will come back with no key**. That is correct behaviour, not a shortfall — but it does mean key should not be expected on most tracks.

Tempo is considerably more reliable on music with clear percussion, and correspondingly less so on ballads, ambient and classical, where low confidence is the honest answer.

#### Features

`decode` (**on by default**) brings in the audio decoder so `analyse_file` works. Turn it off if you already have decoded samples in hand and only want `analyse_samples` — that skips compiling the decoding stack entirely.

Note that the decoder has no Opus support, and high-efficiency AAC decodes as its core layer, so expect reduced bandwidth there. Neither affects the analysis, which only looks below 5 kHz.

### `meedya-fingerprint`

Audio fingerprinting and loudness analysis. Returns analysis results — callers handle tag-writing (typically via `meedya-metadata::tag_io::write_acoustid_tags` and `write_replaygain_tags`; on an M4A file both are refused, because an M4A file has no atom for the AcoustID item or the reference loudness they always write — write the other fields with `write_tags` there).

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

```rust
impl AcoustIdClient {
    pub fn new(api_key: String) -> Self;
    pub fn with_base_url(api_key: String, base_url: impl Into<String>) -> Self;
    pub async fn lookup(&self, fingerprint: &str, duration_secs: u32) -> Result<AcoustIdResult, FingerprintError>;
    pub async fn rate_limit_delay();
}
```

`with_base_url` points the client at a caller-supplied endpoint instead of the real `api.acoustid.org` — useful for test mocking (e.g. against a `wiremock` server). `new` is unchanged and delegates to it with the real API URL, mirroring the `with_base_url` convention used throughout `meedya-providers`.

**Lookups are POST, not GET (#87).** `lookup` sends `client`, `meta`, `fingerprint` and `duration` as a form-encoded POST body, not as a GET query string. A compressed Chromaprint fingerprint scales with track duration, and DJ mixes / continuous albums — core MeedyaSuite content — routinely produce base64 blobs that reach several KB, well past the ~8KB URL length many proxies and CDNs cap. AcoustID's own docs direct clients to POST for fingerprint lookups. This also moves the API key out of the URL entirely (it now travels in the body, not a `client` query parameter), which is strictly better for the #80 leak class than a key sitting in a URL that gets logged, cached, or captured by an intermediary. Response parsing, `NoMatch` handling and score semantics are unchanged — this was a transport-only change.

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

### `meedya-lang`

The shared Rust implementation of policy **MWBM-MEDIA-LANG** — see
[`docs/standards/media-language-bcp47-policy.md`](../docs/standards/media-language-bcp47-policy.md)
for the normative rules. Identifies, orders, matches and selects languages for audio tracks,
subtitle tracks, lyrics, translations and multilingual metadata. Synchronous, no I/O, no
network — the only dependencies are `serde` (derives on the public data types, so an app can
store them or send them over IPC) and `serde_json` (to parse the reference data compiled into
the crate, `docs/standards/data/bcp47-language-data-v1.json`, embedded byte-for-byte via
`include_str!`).

**Two orders, kept as two separate algorithms on purpose**: [`canonical::sort_canonical`] is
Part A, the order things are *stored* in (a file, a database) — it depends only on the
language tags, so every machine agrees. [`presentation::sort_for_presentation`] is Part B, the
order a *menu* shows to a person — it depends on their preferences and interface language, and
must never be written back into stored content. The policy forbids implementing both with one
comparison function, and this crate doesn't.

#### Public re-exports

```rust
pub use canonical::{sort_canonical, LanguageItem};
pub use matching::{match_tags, MatchLevel, TagMatch};
pub use presentation::{
    label, sort_for_presentation, subtitle_menu, Accessibility, MenuEntry, PresentationContext,
    PresentationItem, PresentationKind,
};
pub use roles::{role_rank, Role, RoleItem, TrackType, UnknownWordError};
pub use select::{
    compare_identifiers, select_audio, select_subtitle, DuplicateIdentifierError, SelectableTrack,
    SubtitleMode,
};
pub use sidecar::{build_sidecar_name, parse_sidecar_name, InvalidSidecarNumber, SidecarParts};
pub use tag::{
    canonicalise, from_legacy_three_letter, from_legacy_three_letter_all, from_posix_locale,
    iso639_2_code, iso639_2_write, Extension, Iso639Form, Iso639Write, LanguageTag, TagKind,
    TagNote,
};
pub use tracks::{sort_tracks, TrackItem};
pub use embedded_data_version; // the reference-data version this build was compiled against
```

#### The item traits

```rust
pub trait LanguageItem { fn language(&self) -> &LanguageTag; fn is_original(&self) -> bool { false } }
pub trait RoleItem: LanguageItem { fn roles(&self) -> &[Role] { &[] } }
pub trait TrackItem: RoleItem { fn track_type(&self) -> TrackType; }
pub trait PresentationItem: RoleItem { fn kind(&self) -> Option<PresentationKind> { None } }
pub trait SelectableTrack: RoleItem {
    type Id: Clone + Eq + Hash + AsRef<str>;
    fn id(&self) -> Self::Id;
    fn is_default(&self) -> bool { false }
}
```

`language()`, `is_original()` and `roles()` are each declared exactly once, so one app track
type can implement `TrackItem`, `PresentationItem` and `SelectableTrack` together and still call
`track.roles()` without naming a trait. **Breaking change (policy revision 4):** before it,
all three traits declared their own `roles()` and `SelectableTrack` its own `language()` and
`is_original()`. An implementation now writes `impl LanguageItem` + `impl RoleItem` (an empty
`impl RoleItem for T {}` for an item with no roles) and drops those methods from the other
impls.

#### Key modules

- **`tag`** — `canonicalise(&str) -> LanguageTag` (LANG-001, never panics: anything not a
  well-formed tag comes back `TagKind::Malformed` with its trimmed text kept, not guessed at).
  `from_legacy_three_letter` reads an old ISO 639-2 field (LANG-002 — `eng` → `en`, `fre-ca` →
  `fr-CA`, `XXX` → `und`, unrecognised → `None`); for a field holding several null-separated
  values it gives the first (the primary language), and `from_legacy_three_letter_all` gives
  every one as `Vec<Option<LanguageTag>>` — one entry per value, in order, `None` for an
  unrecognised one, entry 0 always equal to what `from_legacy_three_letter` returns.
  `from_posix_locale` converts an OS locale name (LANG-004 — `en_US.UTF-8` → `en-US`; only
  LANG-001's four whitespace characters are trimmed, so a no-break space makes it malformed).
  `iso639_2_code`/`iso639_2_write` produce the bibliographic/terminology forms for writing an old
  three-letter field back out (TRACK-070; only a three-letter `qaa`–`qtz` code is written as
  itself — `qb` writes `und`). A `TagNote` on `LanguageTag.notes` records anything worth telling
  a person about a tag — an unregistered subtag, one deprecated with no replacement, one replaced
  during canonicalisation — without ever refusing to use the tag itself. `LanguageTag` equality
  and hashing use `tag` and `kind` only, never `notes` (so `canonicalise("iw") ==
  canonicalise("he")`); an ordinary tag ending in a private-use part keeps it in
  `private_use` (`en-x-foo` → `["foo"]`).
- **`canonical`** — Part A's comparator: `sort_canonical<T: LanguageItem>` implements LANG-010
  to LANG-027 (original-language promotion, ordering by primary language code, specificity,
  stability).
- **`roles`** / **`tracks`** — `TrackType`, `Role`, the shared `RoleItem` trait, and
  `sort_tracks<T: TrackItem>`, the role-aware variant of Part A's ordering for container tracks
  (TRACK-050, TRACK-060; roles never reorder malformed values among themselves, LANG-026).
  `Role::as_str()`, `Display` and `FromStr` use the policy's words (`alternate`,
  `audio_description`, `commentary`, `sdh`, `forced`, `other`); an unknown word is an
  `UnknownWordError` (TRACK-050 says to treat it as `other`: `word.parse().unwrap_or(Role::Other)`).
- **`presentation`** — Part B's comparator: `sort_for_presentation<T: PresentationItem>`
  (UI-020 to UI-050 — preferences, then the original, then everything else by localised name,
  which the crate never invents itself; the caller's closure compares names only, and the crate
  itself breaks a tie by primary language code), `subtitle_menu` (UI-060, prepends a fixed
  "Off"; `MenuEntry` is always `Clone`/`Copy`, and `PartialEq`/`Eq` when the item type is), and
  `label` (UI-070, builds a menu label from structured data, each role once; an empty part —
  empty channels, an empty role name, an empty language name — adds nothing, not even a separator).
- **`matching`** — `match_tags(&LanguageTag, &LanguageTag) -> TagMatch` (MATCH-010 to
  MATCH-040): exact, general, specific, related or none, with a distance count for the
  first two (`TagMatch::distance` is a `usize` — it was a `u8`, which wrapped past 255). A
  malformed value matches nothing, not even an identical one. `MatchLevel` derives `Ord` so the
  best match sorts first.
- **`select`** — `select_audio`/`select_subtitle` (AUTO-010 to AUTO-040): automatic selection,
  never influenced by list order — every tie-break bottoms out in the track's own identifier
  (compared per `compare_identifiers`: ASCII-digit-only identifiers first, as numbers of any
  length — no number type, so a 40-digit identifier still counts — then as text; everything
  else after, as text) or a position in stored order among *all* tracks of the type, computed
  from an identifier-sorted copy, never a position in whatever slice the caller happened to
  pass. Malformed preferences are ignored, and all-malformed preferences count as none; when
  every audio track is commentary or other, commentary ranks first; forced-only mode matches a
  private-use or grandfathered audio tag to a forced track with exactly that tag. Both return
  `Result<Option<T::Id>, DuplicateIdentifierError>` — two tracks sharing an identifier is
  refused rather than guessed at. `SubtitleMode` defaults to `Automatic` and has `as_str()`,
  `Display` and `FromStr` with the policy's words (`automatic`, `always`, `forced_only`, `off`).
- **`sidecar`** — `build_sidecar_name`/`parse_sidecar_name` (TEXT-030): sidecar file naming
  (`Film.en-GB.sdh.srt`) and reading one back, both through the old three-letter reader — so
  `eng` in a file name is understood, and a builder given `fre` writes `Film.fr.srt` (and
  `Film.und.srt` for a value the reader does not recognise): what it writes is what the reader
  reads back. `build_sidecar_name` returns `Result<String, InvalidSidecarNumber>` — a
  clash-avoiding number outside 2..=999,999,999 is refused rather than silently written into an
  unreadable name. `SidecarParts::ignored` lists the parts that were neither a role word nor a
  number (`Film.en.sdh.backup.srt` → `["backup"]`), which TEXT-030 says should be reported.

#### Serialisation

The public data types derive serde's `Serialize` and `Deserialize`: `LanguageTag`, `TagKind`,
`Extension`, `TagNote`, `Iso639Form`, `Iso639Write`, `MatchLevel`, `TagMatch`, `Role`,
`TrackType`, `UnknownWordError`, `Accessibility`, `PresentationContext`, `PresentationKind`,
`SubtitleMode`, `DuplicateIdentifierError`, `SidecarParts`, `InvalidSidecarNumber`. Words
match the policy's test cases (`TagKind` → `ordinary`/`grandfathered`/`privateuse`/`malformed`;
`Role` → `audio_description` …; `SubtitleMode` → `forced_only` …). `MenuEntry` is not
serialisable — it borrows the items it lists. Deserialising a `LanguageTag` trusts its fields
as given; for storage, keep the canonical `tag` text and read it back through `canonicalise`
or `from_legacy_three_letter`.

#### Typical usage

```rust
use meedya_lang::{
    canonicalise, select_audio, sort_canonical, sort_tracks, Accessibility, LanguageItem,
    LanguageTag, Role, RoleItem, SelectableTrack, TrackItem, TrackType,
};

struct Track { id: String, tag: LanguageTag, roles: Vec<Role>, original: bool }
impl LanguageItem for Track {
    fn language(&self) -> &LanguageTag { &self.tag }
    fn is_original(&self) -> bool { self.original }
}
impl RoleItem for Track {
    fn roles(&self) -> &[Role] { &self.roles }
}
impl TrackItem for Track {
    fn track_type(&self) -> TrackType { TrackType::Audio }
}
impl SelectableTrack for Track {
    type Id = String;
    fn id(&self) -> String { self.id.clone() }
}

let mut tracks = vec![
    Track { id: "a".into(), tag: canonicalise("en-US"), roles: vec![], original: false },
    Track { id: "b".into(), tag: canonicalise("ja"), roles: vec![], original: true },
];
sort_canonical(&mut tracks); // Japanese (the original) comes first
sort_tracks(&mut tracks);    // the same, role-aware, for container tracks
let chosen = select_audio(&tracks, &[canonicalise("en")], &Accessibility::default());
assert_eq!(chosen, Ok(Some("a".to_string())));
```

#### Conformance

Every rule the policy defines has a fixture-driven test in
`crates/meedya-lang/tests/conformance.rs`, which loads
`tests/fixtures/bcp47-language-policy-v1.json` (290 cases) — the same file the PHP
implementation runs against — and fails loudly (collecting every mismatch, not just the first)
rather than stopping at the first one. Besides comparing each case's answer, it also checks: a
canonical-form **stability** property (canonicalising a `canonicalise` case's non-null answer
again must return it completely unchanged); that no section was silently skipped (the number of
cases actually run must equal the number the file has); that the fixture file names no section
this harness does not know how to run, and that none of the sections it needs is empty (policy
8.1); that the file's own fields are right (`policy`, `policy_version`, a `fixtures_version` of
three dot-separated numbers, a `data_version` matching the embedded data, a string `$schema`); and
that every case matches the schema's shape — a missing required field (including inside a nested
`expected` object and in every item and track), a field the schema does not allow (so an
`error: true` flag in a section with no refusal cases), a value of the wrong type (including `null`
where the schema allows none, such as an optional `description` or a track's `original`, and a
JSON list where the schema wants an object: serde would otherwise read a struct from a list, so
`accessibility: []` passed as "no preferences" until the stand-in review of revision 5), or an
`error` flag that is not `true` or sits on a case that does not expect `null` fails the run, naming
the case, rather than being quietly defaulted or ignored. Twenty-one `harness_refuses::*` tests
prove that by running the harness on damaged copies of the real case file — eighteen with one
damage each, and three that each run a table of damaged copies (ten to the top-level fields,
thirty-one to field types and nulls, fourteen with a list where an object belongs or the other way
round). The harness's stand-in collation compares names only, so the
crate's own tie-break by language code (UI-040) is what the tied-name case checks. A unit test separately asserts the crate's embedded copy of the reference
data is byte-for-byte identical to the master copy under `docs/standards/`.

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
pub use embed::{embed, embed_synced, id3_language, DEFAULT_LANGUAGE};
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
- **`embed::embed_synced(media: &Path, lyrics: &Lyrics, lang: [u8; 3]) -> Result<()>`** — synchronised ID3v2 SYLT frame. ID3v2-only by design; errors with `Error::UnsupportedForSync` on other formats. Encoding: UTF-16 with BOM; timestamp format: milliseconds. Recommended pattern: call both `embed()` and `embed_synced()` — the former handles cross-format plain text, the latter adds SYLT where applicable. Takes `meedya_metadata::tag_io`'s two language steps — `recover_languages_after_reading` straight after reading the file, `gather_languages_before_saving` just before saving it — so a file listing several languages keeps them all in one `TLAN` frame — including a file that already held one `TLAN` frame per language (before the stand-in review of revision 5, adding lyrics cut several languages down to the last one; before the stand-in review of revision 6, it still did so for a file whose languages were already split into several frames). When such split frames cannot be read, it fails with `Error::Metadata(MetadataError::WriteError(..))` and saves nothing.
- **`embed::DEFAULT_LANGUAGE`** — `*b"XXX"`, ID3's own "language not known" marker (policy MWBM-MEDIA-LANG's LANG-003: an unknown language is never guessed). Changed from `*b"eng"` — the old value silently claimed English.
- **`embed::id3_language(value: &str) -> [u8; 3]`** — turns a BCP 47 tag or an old three-letter code into the ISO 639-2 terminology code the SYLT frame's language field wants, via the shared `meedya-lang` reader; falls back to `DEFAULT_LANGUAGE` when `value` does not resolve to a real language. Use this to build `embed_synced`'s `lang` argument instead of hand-writing three bytes.

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

**Language (policy MWBM-MEDIA-LANG).** `LyricsfileMetadata::language` is always a canonical
BCP 47 tag or `None`, never free text, when it comes from this crate. Both `Lyricsfile::from_ttml`
(reading `xml:lang`) and `Lyricsfile::parse` (reading a file's `language`) go through the LANG-002
reader: a recognised value is stored canonicalised (`EN-gb` → `en-GB`, `eng` → `en`); an
unrecognised one (`zzz`, `English`, `en_GB`, empty) is stored as `und`, with the text kept,
exactly as found, in the new field **`LyricsfileMetadata::language_original: Option<String>`**
(LANG-002: "the original text SHOULD be kept alongside"); an absent (or, in YAML, `null`) value
leaves both `None`. From `from_ttml`, `language_original` is set ONLY when `language` is `und`
for an unrecognised value. `parse` differs: when the file already has a `language_original`, it
is kept exactly as it is — so an unrecognised `language` then becomes `und` and its own text is
not kept a second time, and a recognised `language` can come with a `language_original` — and
`parse` never changes or removes a `language_original` the file gives. `to_yaml` applies the
same reading as `parse` to its own copy before writing (stand-in review of revision 6): a struct
built by hand with `language: English` is written as `language: 'und'` with
`language_original: 'English'`, and `eng` as `en`, so a file never holds a name or an old code
as its language and `parse` reads back exactly what was written; the struct is not changed. `language_original` is a MeedyaSuite addition to LRCGET's
schema, written to YAML only when present; readers that ignore unknown keys (this module's stated
forward-compatibility policy) still read it. Adding the field is a breaking change for code that
builds `LyricsfileMetadata` with a struct literal: add `language_original: None`. (Changed after
Codex's review r7: the unrecognised text used to be stored as the language itself; changed after
the stand-in review of revision 5: `parse` used to store a file's `language` text as it was.)

**Every text value is quoted.** `Lyricsfile::to_yaml` writes every text value in quotes — the
version, title, artist, album, `language`, `language_original`, every line's, word's and
syllable's text, and the plain lyrics (`title: 'no'`, `language: 'no'`). The YAML library this
crate uses follows YAML 1.2 and would write `no` bare, but a YAML 1.1 reader (PyYAML; older Ruby
and JavaScript libraries) reads a bare `no` — a song called "No", or Norwegian — as false, and
likewise `yes`, `on`, `off`, `y` and `n` (and `~`/`null` as nothing, `12:30` as a number). The
language values were quoted after the stand-in review of revision 5; the review of revision 6
found `title: no` and `artist: yes` read as false and true, so every text value is quoted now.
Single quotes are used when every character may appear in them, double quotes with escapes
otherwise. One exception: text holding a line break that the YAML library writes as a block
(`plain: |-` followed by the lines) stays a block — no YAML reader reads a block as anything but
text, and it keeps multi-line lyrics readable and editable one line per line, as LRCGET writes
them. Text holding a carriage return, NEXT LINE (U+0085), LINE SEPARATOR (U+2028) or PARAGRAPH
SEPARATOR (U+2029) is never left as a block, though: a block holds such a character as it is,
and readers disagree about it (a YAML 1.1 reader turns some into a plain line break; js-yaml, a
YAML 1.2 reader, read the next line's indent into the text after U+2028, or failed), so it is
quoted, the character written as an escape (`\u2028`) — found by Codex's review of revisions
5–7; measured before the fix, the library wrote `"a\nb\u{2028}c"` as a block holding the
character. Checked with PyYAML 6.0.3 and serde_yaml: 65 awkward values (the review's 55 plus ten
line-break cases), each in all ten text fields and in the language, read back as the text
written; and 20 values holding those four characters (alone, and before and after an ordinary
line break) read back exactly by serde_yaml, PyYAML 6.0.3 and js-yaml 4.1.1. Every file therefore reads `title: 'Hello'` where it used to read `title: Hello`; both
are the same string to any YAML reader. Serialising the struct with `serde_yaml` directly
bypasses this — use `to_yaml`.

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
  - `write_registry_tags(path: &Path, registry: &TagRegistry, json_source: &serde_json::Value, scope: TagScope) -> Result<usize>` — returns the number of tags written. **On an M4A file it refuses** with `MetadataError::WriteError`, naming the key, and saves nothing, when a key it would write cannot be stored as a proper MP4 freeform atom (`----:mean:name`): lofty leaves such a key out of the saved file without a word, and the call used to return `Ok(1)` for a file that gained nothing (#103; found by the stand-in review of revision 6 and Codex's review of revisions 5–7). The keys this function builds today are `namespace:name` (`MeedyaMeta:ISRC`), so every registry write to an M4A file that has a value to write is refused until #103's real fix; other formats are unchanged. A key in the proper form is checked like every M4A write (next item): a value that would be left beside an older atom of the same name (a file already holding two ISRC atoms), or put back over by the file's own languages (a registry key aimed at `----:com.apple.iTunes:LANGUAGE`), refuses the call instead of being reported written (the stand-in review of revision 8).
  - **Every M4A write is checked before it replaces the file (#102).** Every M4A write in `tag_io` (the language atom apart) goes through lofty's format-neutral `Tag`, and that route changes things nobody asked it to change. Revision 8 refused a LIST of such cases; the stand-in review of revision 8 found more the list missed (the `pgap`/`hdvd`/`shwm` flags rewritten as text, freeform names respelled, cover art with an unusual data type dropped), so no list decides. `write_tags`, `write_replaygain_tags`, `write_acoustid_tags` and `write_registry_tags` save an M4A file to a **temporary copy** in the same folder and compare it with the original; the copy replaces the original (in one rename) only when:
    - **its tags** — every atom the call did not ask to change is byte for byte as it was (same name — a freeform atom's `mean` and `name` exactly — same values in the same order, each with the same data type, locale and bytes), and every atom it did ask to change holds exactly what was asked, the language atom included, with nothing else of that name beside it. Atoms of DIFFERENT names may come out in a different order: lofty's save moves an atom it rewrites to the end of the tag list, and no reader depends on that order. The order of atoms of the same name, and of the values inside one atom, is compared;
    - **and the rest of the file** (since the stand-in review of revision 9, which found a save damaging what lies outside the tags while the tag check passed): every `mdat` (the audio) is byte for byte the same; every chunk offset in every `stco`/`co64` table equals its old value plus exactly how far the audio it points into moved; everything else in `moov` is byte for byte the same apart from the tag list itself, the size fields on the path `moov` → `udta` → `meta` → `ilst`, and `free`/`skip` padding in `udta` and `meta`; and every other atom at the top of the file is byte for byte the same, in the same order — except `free`/`skip` padding at the top of the file, which the save may add, remove or resize (it holds nothing). The one addition allowed is the `udta`/`meta` lofty makes for a file with no tags yet, checked to hold exactly its standard handler and the tag list.

    Otherwise the call fails with `MetadataError::WriteError`, naming what would have changed and how, and the original is left byte for byte as it was. When an atom would change but reads the same in words, the message says so and says what differs (revision 9's message could read "now the text "Album"; after saving, the text "Album""). Writing a field replaces its whole atom on purpose: `write_tags(Artist, "Carol")` on a file whose `©ART` holds `Alice` and `Bob` is allowed and reads back `Carol`. One change nobody asked for is allowed because it loses nothing, and is checked value by value: a file with one language atom per language (revision 5's form) has them put into one atom, every value kept in order.
  - **Refused before anything is written** (the stand-in review of revision 9): a **fragmented** M4A file (any `moof`, `mfra` or `sidx` atom, or `mvex` in `moov` — the way streaming tools write M4A), because lofty moves its pieces without correcting where each says its audio starts, and the audio no longer decodes (measured: hundreds of decode errors after a title-only write); and a file whose **metadata box has no tag list** (`meta` holding its handler but no `ilst` — what a tag removal through lofty itself leaves), because lofty would write the tag list over the handler, after which ffprobe and Apple's AVFoundation read no tags at all. A file mutagen cleared, which keeps an empty tag list, is saved normally. Since Codex's catch-up review of revisions 8–10 (revision 11), also refused before anything is written: a file with one of the containers lofty's save follows down (`moov`, `udta`, `moof`, `trak`, `mdia`, `minf`, `stbl`) sitting where no M4A file has one — a `minf` inside a `minf`, say — because lofty follows them with no limit on how deep (measured: 1,000 nested `minf` atoms stopped the program inside lofty's save); and, as a file whose tags "cannot be checked", one whose `meta` box holds fewer than the four bytes of version and flags it must start with (the whole-file comparison used to read past it and stop the program in a build that checks for overflow) or whose tag list ends with 1 to 7 bytes belonging to no atom (the save dropped them, and neither comparison saw it).
  - **What the caller gives is what must be stored** (revision 10). On an M4A file a value the file would not store exactly as given is refused before anything is written, whatever the file holds: track and disc numbers and their totals must be whole numbers from 1 to 65535 in digits only (`70000` used to be accepted and the track number lost); a year must be four digits, 1000 to 9999 (lofty keeps an existing full date's month and day, so `2021` over `2019-03-01` stores `2021-03-01` — a full date can be written as `ReleaseDate`); the compilation flag must be `1` or `0`; every other field is checked to be stored as the text given. A field an M4A file has no atom for is refused by name instead of being left out without a word: found from lofty 0.22.4's own conversion of every `CommonTag`, these are `Arranger`, `AcoustId` (the `Acoustid Id` item) and `ReplayGainReferenceLoudness` (`REPLAYGAIN_REFERENCE_LOUDNESS`). (`Producer` and `Engineer` ARE stored, as `----:com.apple.iTunes:PRODUCER` and `…:ENGINEER`; an older note here said otherwise.) So **`write_acoustid_tags` and `write_replaygain_tags` are refused on every M4A file** — each always writes one of those fields. Write the gains and peaks, or the MusicBrainz recording ID, with `write_tags`.
  - **What this refuses in practice.** lofty's route also rewrites 1- and 2-byte whole numbers (`stik`, `rtng`, `tmpo`, `akID`) as 4 bytes, a 6-byte `disk` atom as 8, and the gapless flag as text. iTunes and Apple Music files typically hold all of those, so a write to them is refused, even a title-only one. The 6-byte `disk` is also the form **mutagen** writes, so the same refusal reaches **any file whose disc number was written by mutagen** — GAMDL, Picard and beets all write through it — not just iTunes and Apple Music files. And a **registry write of an ISRC** (`write_registry_tags` with the key `----:com.apple.iTunes:ISRC`) on a file that already holds ONE ISRC atom is refused: lofty keeps the old atom and adds the new one beside it, so the copy would hold two (`write_tags(Isrc, …)` replaces it normally). The real fix — a route that keeps these atoms as they are — is still open in #102.
  - **How the copy is made, and what saving by copy costs.** The file is opened once, for reading and writing (a file this program may not write to is refused with a plain message). The copy is a NEW file made with `create_new` — a name already taken, by a file or a symbolic link, is never opened or written through — and filled through the handle that call returned. It is **private from the moment it exists**: on Unix it is made readable and writable by its owner only (0600), before anything is copied into it, and gets the original's permission bits only just before the rename (revision 11: it used to be made with the usual permissions, 0644, so the whole of a private recording could be read by other accounts while the save was checked, even when the save was then refused). Windows has no such permission bits: there the copy gets whatever access rules its folder gives every new file, which may let others read it where the original's own rules did not, while it exists and — since a file's own rules are not copied — after the save too. lofty saves into that same handle (revision 10: revision 9 filled it with `std::fs::copy`, which opened the name again and would follow a link put there meanwhile). Before the rename the copy gets the original's permission bits, is flushed to the disk (`sync_all`), and both names are checked to still name the files the handles hold (device and inode on Unix; volume serial number and file index on Windows) — refused if either was swapped. A symbolic link to the file is followed first, so the file it points to is the one replaced. The folder must be writable: saving makes a copy beside the file, and an unwritable folder gets a plain message saying so. Compared with writing into the file, a save by copy **loses**: other names for the same file (a hard link keeps the OLD tags); the owner and group, when the program saving is not the file's owner (the new file is the saver's); access control lists, and extended attributes on Linux and on macOS (Finder tags and comments among them); the creation time; and, **on Windows (NTFS), any named alternate data stream** beside the file's contents (`song.m4a:notes` — where some programs keep notes, or where a download came from): only the file's main stream is copied, so such streams are gone after the save, and nothing detects them first, so the save goes ahead without a word (Codex's catch-up review of revisions 8-10; worked out from the calls used, not tried on Windows). It **cannot** close the instant between the name check and the rename (a rename works on names), and the flush does not cover the folder itself, so a power cut soon after a save can still undo the rename — the file then has its old tags, never half of a file. On every refusal and error the copy is deleted by its name before the error is returned — only while that name still names the copy (a file put at its name meanwhile is someone else's and is left alone) — and it counts as deleted only when nothing names it any more (on Unix its open handle says so: the file's count of names is 0; elsewhere, only when this program deleted it by its own name). Otherwise the error — then always a `WriteError` — says what became of it: deleting it failed (its folder made read-only meanwhile, say), and it is named so it can be deleted by hand; or it was moved, or given another name, while the save was being checked, so it was left wherever it went, which this program does not know (revision 11: a failed deletion used to be ignored without a word; revision 12, after Codex's review of revision 11: a copy moved away used to count as deleted). The check of the name and the deletion are two steps on a name, so a file swapped in at that very instant would be deleted instead. If the program is killed or crashes mid-save, the copy cannot be deleted at all. Wherever it stays beside the file, it is a hidden file named `.meedya-tag-save-<process id>-<number>.tmp` (the original untouched), and can be deleted by hand. Every save copies the whole file, and lofty reads the whole file into memory, so saving a large file is slow.
  - `write_acoustid_tags(path, result: &AcoustIdResult) -> Result<()>` — refused on an M4A file (it always writes the `Acoustid Id` item, which an M4A file has no atom for; see above).
  - `write_replaygain_tags(path: &Path, result: &ReplayGainResult, album_result: Option<&AlbumGainResult>) -> Result<()>` — refused on an M4A file (it always writes the reference loudness, which an M4A file has no atom for; see above).
  - **Keeping a file's languages whole through your own save** — two steps, taken at two different moments, by a caller that reads and saves a lofty `TaggedFile` itself instead of calling `write_tags` (every save in `tag_io`, and `meedya-lyrics`' `embed_synced`, takes both):

    ```rust
    let mut tagged_file = lofty::read_from_path(path)?;
    tag_io::recover_languages_after_reading(&mut tagged_file, path)?; // 1: straight after reading
    // … your own changes, languages included …
    tag_io::gather_languages_before_saving(&mut tagged_file);        // 2: just before saving
    tagged_file.save_to_path(path, WriteOptions::default())?;
    ```

    The order matters, and is why these are two functions. Until Codex's review of revisions 5–7 there was one helper, `keep_languages_whole_before_saving`, called just before the save; because it read the languages from the disk at that moment, it put the file's old languages back over the caller's own change (a language replaced with `deu` came back as `eng` and `fra`; a language deleted on purpose came back). It was on this unreleased branch only, and no consuming app used it (checked in every consumer clone), so it was **removed**, not kept alongside.
  - `recover_languages_after_reading(tagged_file: &mut lofty::file::TaggedFile, path: &Path) -> Result<(), MetadataError>` — **new (Codex's review of revisions 5–7). Step 1: call straight after reading `tagged_file` from `path`, before changing anything.** When the file holds its languages in several ID3v2 language frames — one `TLAN` frame per language (as this crate wrote files before revision 6, and as `meedya-tags-extended`'s `TagFile::save` still does, #100), or the older names lofty also reads as `TLAN` (`TLA` in an ID3v2.2 tag, and `TLA` followed by a zero byte in an ID3v2.3 tag) — lofty has read only the last of them. The frames are read straight from the file's bytes and every language, in file order, replaces what lofty read (exact repeats and empty values dropped). Called after a change to the languages, it would undo that change — hence "straight after reading". It reads only the plain case — frame headers (sizes plain in ID3v2.3, synchsafe in 2.4), the text-encoding byte, null-separated values, all read as lofty reads them — and only when the names occur at least twice in the file's ID3v2 tags. A file whose repeated frames it cannot read (an unsynchronised tag, an extended header, a compressed or encrypted frame, an unknown encoding, text that does not decode, a frame running past its tag, a frame name lofty would not accept), or that holds a `TLA`-and-zero frame in an ID3v2.4 tag (mutagen reads that as a language, lofty does not, and lofty's save turns it into an ordinary `TXXX:TLA` text frame), makes it fail with `MetadataError::WriteError`, saying why: do not save then, because the save would lose languages. The work grows in step with the number of values (a crafted frame of 100,000 values used to take some five billion comparisons; since revision 12 such a frame is refused by the memory budget below, and the merge stays in step with whatever is let through). What it reads into memory is bounded (revisions 11 and 12): the tags are found by their headers alone, each tag is looked at a block at a time, and only the language frames' contents are read — within a budget of 1 MiB for the whole file, which is charged each frame's bytes and also a fixed 64 bytes for every language frame (an empty one too) and for every value in one. More than the budget, or more than 256 language frames in one tag or 1,024 in the file, and it fails with `MetadataError::WriteError`. An empty language frame is passed over, as lofty passes over it, and nothing is kept for it. (It used to read every tag, and every WAV/AIFF ID3 chunk whole, into memory: a WAV whose ID3 chunk held a small tag in 400 MiB of padding took the program to 410 MiB; now under 8 MiB. And until revision 12 only the text was charged, so "at most 1 MiB held", which this said, was not true: a million empty `TLAN` frames took the program to 52 MB and were let through, half a million one-letter values in one frame to 26 MB; both are now refused, measured at no more than 2 MB above the program's own use. What is held while reading stays within a few times the budget — decoding a frame's text can briefly double it.) The budget applies only when there is something to merge: a file with one language frame (in one tag) is not read here at all, however large that frame is. It cannot bound lofty's own reading, which comes first (`read_tags` and every write read the file through lofty before calling this): lofty holds each ID3v2 frame whole while reading it, up to its 16 MiB allocation limit, and lofty's save of an MP3, WAV or AIFF file reads the whole file into memory.
  - `gather_languages_before_saving(tagged_file: &mut lofty::file::TaggedFile)` — **new (stand-in review of revision 5). Step 2: call just before saving.** Joins every language item of the file's ID3v2 (and APE) tag into one null-separated item, so the save writes one `TLAN` frame holding every language. lofty reads such a frame as one item per language and would save one frame each, of which a reader keeps only the last. It works only on what `tagged_file` holds and **never reads the file**, so whatever the caller changed is what is saved. On its own (without step 1) it cannot see the languages of a file that already holds several `TLAN` frames (lofty reads only the last), so a save would delete the others — they are not "already lost" beforehand, as this entry once said; mutagen still reads them all. It does not reach `meedya-tags-extended`'s `TagFile::save`, which uses lofty 0.21 (see the note under `meedya-tags-extended`).
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
| `Language` | `LANGUAGE`¹ | `LANGUAGE`¹ | `TLAN`¹ |
| `Lyricist` | `LYRICIST` | `LYRICIST` | `TEXT` |
| `Conductor` | `CONDUCTOR` | `CONDUCTOR` | `TPE3` |
| `Remixer` | `REMIXER` | `REMIXER` | `TPE4` |
| `Arranger` | `ARRANGER` | `ARRANGER` | `TIPL:arranger` (no MP4 ilst mapping in lofty 0.22 — MP4 writes drop silently) |
| `Producer` | `PRODUCER` | `PRODUCER` | `TIPL:producer` |
| `Engineer` | `ENGINEER` | `ENGINEER` | `TIPL:engineer` |
| `Mixer` | `MIXER` | `MIXER` | `TIPL:mix` |

`Performer` and `Translator` were deliberately **excluded**: `Performer` has no lofty 0.22 ID3v2 write mapping (ID3v2 models it as the multi-valued, instrument-qualified TMCL frame — a flat-string variant would silently no-op on MP3); `Translator` has no standard frame in any container and stays an iHymns-domain concept in its own mirror.

¹ **`Language`'s write is not a plain pass-through** (policy MWBM-MEDIA-LANG, TRACK-070). `write_tags` reads the caller's value with the LANG-002 reader for several values (`meedya_lang::from_legacy_three_letter_all`) first, so a value listing several languages separated by a null character (`"eng\0fra"`) has **every** language written, in order — and so do several `Language` entries in one call (`[(Language, "eng"), (Language, "fra")]`), which are written together, once, in order (each used to replace the one before, until the stand-in review of revision 5). On ID3v2 (which has no full-tag language field — only `TLAN`) each becomes its ISO 639-2 **terminology** three-letter code (`und` for a language with no ISO 639-2 code of its own), all in one `TLAN` frame separated by null characters (ID3v2.4's multi-value form); on Vorbis `LANGUAGE` (a free-text field) each becomes its canonical BCP 47 tag, one field per language; on MP4 the canonical tags go in **one** `----:com.apple.iTunes:LANGUAGE` atom holding one `data` atom per language (the form iTunes, mutagen and mp4ameta write, and the one ffprobe takes its first value from — revision 5 wrote one atom per language, and ffprobe showed the last); on APE, the canonical tags go in one item separated by null characters (APEv2's list form). **A value the reader does not recognise — `zzz`, a name such as `English`, a locale name such as `en_GB`, an empty value — is refused** with `MetadataError::UnrecognisedLanguage { value, problem }`, whose message says what was wrong and gives examples (`en`, `pt-BR`, `und` for not known); one unrecognised value among several refuses them all, and the whole `write_tags` call writes nothing to the file. **But a value identical to what the tag being written already holds is left alone** — that tag's language values joined with null characters, compared as text — not checked, converted or rewritten, exactly as if no `Language` entry had been given; so reading a file whose `LANGUAGE` another tool set to `English`, changing the title and writing every field back works (it used to refuse the whole save). Only a changed value is checked. The tag being written is the file's main tag, which is not always the one `read_tags` reads: for a WAV file holding only a RIFF INFO list, or an MP3 holding only an APE tag, `read_tags` reports that tag's language, and a write goes into a new ID3v2 tag. A value carried back exactly as `read_tags` returned it is then written into the ID3v2 tag when it is recognised, and skipped — neither written nor refused — when it is not; the other tag keeps its language. (Stand-in review of revision 6: the comparison used to be made against the tag `read_tags` read, so such a value counted as unchanged, the new tag got no language, and `read_tags` then reported none.) `und`, `mul`, `zxx`, `mis`, local-use (`qaa`–`qtz`), private-use and grandfathered tags are recognised and written normally. (Changed after Codex's review r7: before, only the first of several languages was written, and an unrecognised value was written unchanged — or as `und` on ID3v2.) **Every write keeps the file's languages whole** — `write_tags`, `write_replaygain_tags`, `write_acoustid_tags` and `write_registry_tags` alike: an unrelated write used to cut several ID3v2 languages down to the last one (lofty splits the `TLAN` frame on reading and saved one frame per language) and to delete every MP4 value but the first (lofty's format-neutral tag keeps only an atom's first `data` atom); MP4 files are now read and saved through lofty's own `Mp4File`/`Ilst` for the language atom, and an MP4 file written by revision 5, with one atom per language, is mended on its next save. An ID3v2 file (MP3, WAV, AIFF) that already holds one `TLAN` frame per language — written by this crate before revision 6, or by `meedya-tags-extended`'s `TagFile::save` (#100) — also keeps every language through every write, merged into one frame, and `read_tags` returns them all (stand-in review of revision 6: lofty reads only the last such frame, and the next save deleted the rest) — and so does a file whose language frames carry the older names lofty also reads as `TLAN` (`TLA` in an ID3v2.2 tag, `TLA` followed by a zero byte in an ID3v2.3 tag; Codex's review of revisions 5–7, which found two such frames still losing a language); when those frames cannot be read (an unsynchronised tag, a compressed frame, an unknown text encoding and the like), or an ID3v2.4 tag holds a `TLA`-and-zero frame (a language to mutagen, which lofty's save would turn into an ordinary `TXXX:TLA` text frame), or the language frames are in more than one ID3v2 tag (an MP3 file's tags one after another, or several ID3 chunks of a WAV or AIFF file — lofty's save rewrites only one of them, so merging left the languages in two places, in an order that changed from one save to the next; the stand-in review of revision 8), the write fails with `MetadataError::WriteError`, saying why, and saves nothing. `read_tags` returns every language the file lists, one entry per language (one entry with null characters between the values for APE); on MP4 every `data` atom is now returned, not just the first. **A caller reading `CommonTag::Language` back MUST pass each entry through `meedya_lang::from_legacy_three_letter_all` (or `from_legacy_three_letter` for the primary language only) before treating it as a language**; the raw value could be a three-letter code, a BCP 47 tag, or text another tool wrote.

#### `MetadataError` is `#[non_exhaustive]` (policy MWBM-MEDIA-LANG, this branch)

**Two breaking changes for a crate that matches every `MetadataError` variant**, taken together
on purpose: the variant `UnrecognisedLanguage { value: String, problem: String }` was added
(after Codex's review r7 — see footnote ¹), and the enum is now `#[non_exhaustive]` (after the
stand-in review of revision 5). A `match` on `MetadataError` in another crate needs a `_ =>` arm.
Because the enum was already changing, callers take one break now instead of one per future
variant: from here on, adding a variant does not break them. Matches inside `meedya-metadata`
are not affected, and nothing else in this workspace matches every variant.

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

**Known fault, not yet fixed: several languages on an ID3v2 file do not survive `save`.** Measured on a real MP3 (stand-in review of revision 5): a file whose one `TLAN` frame lists `por`, `deu` and `zho` comes back from `open` → `save` with three `TLAN` frames, and lofty then reads only the last (`zho`). It is the fault `meedya-metadata`'s `gather_languages_before_saving` fixes for that crate's saves, but this crate is built on lofty **0.21**, whose `TaggedFile` is a different type that helper cannot take; moving this crate to lofty 0.22 changes the lofty types its public API hands out (`inner`, `primary_tag`, …), so it is left for a separate change. Until then, a caller that needs several languages kept should not save such a file through `TagFile`. Since the stand-in review of revision 6, a file split this way is mended by `meedya-metadata`'s next save (`tag_io` writes and `embed_synced` read every split frame back and write one), and `read_tags` returns every language; it is `TagFile::save` itself that still splits them (#100).

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
   // refused on an M4A file (no atom for the AcoustID item): there, write
   // CommonTag::MusicBrainzRecordingId with write_tags instead
```

### ReplayGain analysis + tagging

```text
1. let analyzer = fingerprint::ReplayGainAnalyzer::new(ffmpeg_path);
2. let result = analyzer.analyze_track(&path).await? → ReplayGainResult
3a. Track mode: metadata::tag_io::write_replaygain_tags(&path, &result, None)?
3b. Album mode: collect one ReplayGainResult per track into `tracks: Vec<ReplayGainResult>`,
    then let album_result = analyzer.compute_album_gain(&tracks); // Option<AlbumGainResult>
    and call write_replaygain_tags(&track_path, &track_result, album_result.as_ref())? per
    track (album_result is threaded through the same call, not a separate write)
   // refused on an M4A file (no atom for the reference loudness): there, write
   // the gains and peaks with write_tags instead
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

On `feature/bcp47-language-policy` (policy MWBM-MEDIA-LANG), `meedya_metadata::MetadataError` gained the variant `UnrecognisedLanguage` and became `#[non_exhaustive]` — both breaking for a crate that matches every variant, taken together so that later variants are not (see "`MetadataError` is `#[non_exhaustive]`" under `meedya-metadata`). More changes on that branch alter what existing calls produce without changing any signature: `Lyricsfile::to_yaml` writes every text value in quotes (the language values since the stand-in review of revision 5, every other text value since revision 6 — a file that said `title: Hello` now says `title: 'Hello'`; multi-line text stays a YAML block) and writes the language as `parse` reads it (revision 6: `English` goes out as `und` with the text in `language_original`); `Lyricsfile::parse` reads a file's language through the policy's reader (see the Lyricsfile section); and a `tag_io` write to an ID3v2 file holding several `TLAN` frames that cannot be read now fails with `MetadataError::WriteError` instead of saving (revision 6), as does one to an ID3v2.4 file holding an old `TLA` language frame (Codex's review of revisions 5–7). Also from that review: a `tag_io` write to an M4A file that would lose a value (several cover images, several values in one text atom, a colon inside a freeform atom's name) fails with `MetadataError::WriteError` instead of saving (#102), and `write_registry_tags` on an M4A file fails with `MetadataError::WriteError` instead of returning a count of tags it did not store (#103) — with today's `namespace:name` keys, whenever it has a value to write. From the stand-in review of revision 8: every `tag_io` write to an M4A file is made on a temporary copy and compared atom by atom with the original, and fails with `MetadataError::WriteError` whenever anything not asked for would change or something asked for would not be stored — which, in practice, includes most files tagged by iTunes or Apple Music (see `tag_io` above); a successful M4A save now replaces the file with a new one (a hard link keeps the old tags, and the folder must be writable); and a `tag_io` write to an MP3, WAV or AIFF file whose language frames are in more than one ID3v2 tag or ID3 chunk fails the same way instead of merging them. From the stand-in review of revision 9 (revision 10): an M4A save is checked across the whole file, not just its tags, and fails with `MetadataError::WriteError` when the audio, a chunk offset or any atom outside the tags would change; a fragmented M4A file, or one whose metadata box has no tag list, fails before anything is written; on an M4A file a track or disc number or total outside 1–65535 (or not in digits), a year that is not four digits, or a compilation flag other than `1`/`0` fails instead of being dropped or changed, and so does a field the file has no atom for (`Arranger`, `AcoustId`, `ReplayGainReferenceLoudness`) — which makes `write_acoustid_tags` and `write_replaygain_tags` fail on every M4A file; a read-only M4A file fails with `WriteError` rather than `LoftyError`; an M4A save no longer keeps extended attributes on macOS either (the copy is written through its own handle); and a write to an MP3, WAV or AIFF file with two or more ID3v2 tags or ID3 chunks fails whenever ANY of them holds a language frame, not only when two do. From Codex's catch-up review of revisions 8–10 (revision 11): an M4A file whose `meta` box holds fewer than four bytes, whose tag list ends with 1 to 7 bytes belonging to no atom, or which has containers nested where no M4A file has them (a `minf` inside a `minf`, say) fails with `MetadataError::WriteError` before anything is written — the first used to stop the program in a build that checks for overflow, the second to lose those bytes, the third to crash it in lofty's save; when a failed M4A save cannot delete its temporary copy, the error is a `WriteError` naming the copy, even where the failure itself was an I/O error (which otherwise still comes back as `IoError`); and a write to an MP3, WAV or AIFF file whose two or more language frames need more than a 1 MiB reading budget fails with `WriteError` (since Codex's review of revision 11 that budget counts a fixed 64 bytes for every language frame and every value as well as the text, and more than 256 language frames in one tag or 1,024 in a file fail too; a file with only one language frame is not read by this check at all, however large the frame — lofty reads it, within its own 16 MiB limit). `tag_io::recover_languages_after_reading` is new (additive); `tag_io::keep_languages_whole_before_saving`, added by revision 6 on this unreleased branch, was removed again after Codex's review of revisions 5–7 (it undid a caller's own language change; see `tag_io` above) — no consuming app used it.

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
