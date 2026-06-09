# MeedyaSuite-core

Shared core library for the [MeedyaSuite](https://github.com/MWBMPartners) family of applications.

Written in Rust. Distributable to all Meedya apps via:

- **Rust/Tauri apps** (MeedyaDL, MeedyaManager) — native Cargo dependency
- **Swift/SwiftUI apps** (MeedyaConverter, MeedyaDB) — via [`bindings/swift`](bindings/swift) Swift Package (C FFI / XCFramework, planned)
- **Web targets** — via [`bindings/wasm`](bindings/wasm) (planned)

## Crates

| Crate | Purpose | Status | Tests |
|---|---|---|---|
| [`meedya-codecs`](crates/meedya-codecs) | Audio/video/subtitle codecs, container formats, HDR, spatial audio, media classification, FFprobe + MediaInfo integration | Implemented | 47 |
| [`meedya-metadata`](crates/meedya-metadata) | Two coexisting tag-I/O surfaces: lofty-backed multi-format (`CommonTag`, `tag_io`, `tag_registry`) and `mp4ameta`-backed sandbox-safe registry for the Apple Music tagging flow. Includes `playback_bounds` (soft start/stop atoms) and codec ID tags. | Implemented | 59 |
| [`meedya-tags-extended`](crates/meedya-tags-extended) | Multi-format tag I/O foundation with DJ metadata support. `ExtendedTags` model, `MusicalKey` (Camelot/Open Key/traditional), `CuePoint`/`LoopPoint`/`BeatGrid`, standard BPM+key+comment read/write, **Mixed In Key reader** (`mik` module). Other proprietary readers (Serato/Rekordbox/Traktor/VDJ) pending fixture-based sessions. | Implemented | 61 |
| [`meedya-library-import`](crates/meedya-library-import) | Ingest playback bounds + metadata from external library DBs. `itunes_xml` parses Music.app exports; `cuesheet` is a full CUE parser at CD-frame precision. | Implemented | 30 |
| [`meedya-lyrics`](crates/meedya-lyrics) | LRCLIB client, LRC parser/writer, `.lrc` sidecar writes, plain-text + synchronised ID3v2 SYLT tag-embed. | Implemented | 15 |
| [`meedya-providers`](crates/meedya-providers) | Metadata provider framework: traits, capabilities, registry, rate limiting, cover art helpers, match scoring. | Implemented | 27 |
| [`meedya-fingerprint`](crates/meedya-fingerprint) | AcoustID fingerprinting + ReplayGain/EBU R128 loudness analysis. | Implemented | 6 |
| [`meedya-db`](crates/meedya-db) | MeedyaDB API client, shared media models (Track/Album/Artist), database export trait. | Implemented | 3 |
| [`meedya-core`](crates/meedya-core) | Unified facade crate re-exporting the implemented crates behind feature flags. | Implemented | — |

**Total: 466 tests passing**, workspace builds clean.

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run the full test suite (466 tests)
cargo test --workspace

# Build a single crate
cargo build -p meedya-codecs

# Test a single crate
cargo test -p meedya-tags-extended
```

## Using in Your MeedyaSuite App (Rust)

Depend on individual crates or the `meedya-core` facade with feature flags:

```toml
[dependencies]
# Individual crates
meedya-codecs        = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-metadata      = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-tags-extended = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-library-import = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-lyrics        = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-providers     = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-fingerprint   = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-db            = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }

# Or the unified facade
meedya-core = { git = "https://github.com/MWBMPartners/MeedyaSuite-core", features = ["full"] }
```

## What's Shared

This library eliminates code duplication across MeedyaDL, MeedyaConverter, MeedyaManager, and MeedyaDB. Highlights:

### Codecs and formats (`meedya-codecs`)

- **42+ audio codecs** with lossless/spatial/object-based classification and FFmpeg mappings
- **21+ video codecs** with HDR/VideoToolbox support flags
- **36+ container formats** with extension/MIME/codec compatibility matrices
- **FFprobe + MediaInfo integration** for runtime detection (`ChannelConfig`, `SpatialType` etc.)

### Tag I/O (`meedya-metadata` + `meedya-tags-extended`)

Two coexisting foundations by design — they serve different code paths:

- **`mp4ameta`-backed** (in `meedya-metadata`) — App Store / sandbox safe, no subprocess spawning, drives the Apple Music JSON → atom flow. TOML-driven `tags.toml` registry adds tags with zero Rust changes.
- **`lofty`-backed** (in `meedya-metadata::tag_io` and `meedya-tags-extended`) — multi-format (MP3/M4A/FLAC/WAV/AIFF/OGG/MKV) with automatic foreign-frame pass-through (preserves Serato/Rekordbox/Traktor blobs on save).

Cross-format `CommonTag` enum maps the same logical tag across iTunes atoms, Vorbis Comments, and ID3v2 frames.

### DJ metadata (`meedya-tags-extended`)

- `ExtendedTags` aggregator across all source apps
- `MusicalKey` with full Camelot / Open Key / traditional round-tripping
- `CuePoint`, `LoopPoint`, `BeatGrid`, `Source` enum
- BPM + key + comment read/write works today
- **Mixed In Key reader** — recovers key/energy/tempo from every documented MIK write location (standard fields, artist/title prefixes+suffixes, comment, grouping, label) and normalises into standard tag fields. `MeedyaMeta:Energy` only when no standard exists.
- Serato, Rekordbox, Traktor, Virtual DJ proprietary readers pending — each requires fixture-based work against real DJ-tagged files

### Library import (`meedya-library-import`)

- **iTunes / Music.app XML** — emits one record per track with `Start Time` / `Stop Time` set
- **CUE sheets** — full parser at CD-frame precision (75 fps); rich `CueSheet` model preserves `CATALOG`, performers, ISRC, REMs. Designed for future chapter-authoring use cases (CD TOC → MP4 `chap`)
- Normalized `LibraryEntry` output; matching to local files is the consuming app's job

### Lyrics (`meedya-lyrics`)

- LRCLIB API client with pluggable `LyricsProvider` trait
- `.lrc` parser/writer with synced `[mm:ss.xx]` timestamps
- Sidecar writes + tag-embed via `meedya-metadata::CommonTag::Lyrics`

### Audio identification + loudness (`meedya-fingerprint`)

- AcoustID API client with rate limiting and MusicBrainz recording extraction
- ReplayGain analyzer — EBU R128 loudness measurement, track + album gain

### Metadata providers (`meedya-providers`)

- Provider trait + capabilities system
- Rate limiting (`governor`-backed), credential storage, cover art helpers, fuzzy-match scoring
- Foundation for shared MusicBrainz / TMDB / TheTVDB / Discogs / FanArt.tv clients

### Database integration (`meedya-db`)

- MeedyaDB API client (`api.meedya.tv/v1`) — search, match, lookup
- Shared `Track` / `Album` / `Artist` record types
- `DbExporter` trait for multi-backend export

See [`docs/integration-assessment.md`](docs/integration-assessment.md) for the original cross-project analysis and [`docs/cross-repo-issues.md`](docs/cross-repo-issues.md) for pre-drafted integration issues per downstream app.

## API documentation

This is a Rust library workspace, not a web service — there is **no HTTP server, no REST endpoints, and no Swagger/OpenAPI specification**. API documentation is maintained in two complementary forms:

- **[`docs/API.md`](docs/API.md)** — curated internal API specification for partner-app developers. Per-crate public surface, common workflows, integration patterns, stability tiers.
- **`cargo doc`** — exhaustive auto-generated reference:

  ```bash
  cargo doc --workspace --no-deps --open
  ```

External APIs that crates *consume* (AcoustID, MusicBrainz, LRCLIB, MeedyaDB) have their own specs maintained outside this repository.

## Platform Support

macOS · Windows · Linux · Raspberry Pi (armv7/arm64) · iOS · iPadOS · visionOS

## App Store Compatibility

The `mp4ameta`-backed surface in `meedya-metadata` is sandbox-safe (no subprocess spawning). When App Store distribution is the target, prefer it over the `lofty`-backed surface or `meedya-codecs` integrations that shell out to FFprobe/MediaInfo. Feature flags on `meedya-core` will scope what's compiled in.

## Repository Layout

```text
Cargo.toml                          # Workspace root with shared dependencies
crates/
  meedya-codecs/                    # Codec/container/HDR/spatial enums + detection
  meedya-core/                      # Facade with feature flags
  meedya-db/                        # MeedyaDB API client + media models
  meedya-fingerprint/               # AcoustID + ReplayGain
  meedya-library-import/            # iTunes XML, CUE sheet importers
  meedya-lyrics/                    # LRCLIB client, LRC I/O, sidecar
  meedya-metadata/                  # Tag registry, mp4ameta + lofty surfaces
  meedya-providers/                 # Provider framework (traits, rate limit, cover art)
  meedya-tags-extended/             # DJ metadata foundation (lofty-based)
bindings/
  swift/                            # Swift Package (planned)
  wasm/                             # WebAssembly target (planned)
docs/
  API.md                            # Internal API spec for partner apps
  integration-assessment.md         # Original cross-project duplication analysis
  cross-repo-issues.md              # Pre-drafted GitHub issues for downstream apps
.claude/
  CLAUDE.md / CONTEXT.md            # Project state + architecture for Claude Code sessions
  HISTORY.md                        # Append-only session log
  MEMORY.md / PROMPTS.md            # Durable facts + reusable task templates
```

## License

MIT. See [LICENSE](LICENSE).
