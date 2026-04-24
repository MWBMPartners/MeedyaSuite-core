# MeedyaSuite-core

Shared core library for the [MeedyaSuite](https://github.com/MWBMPartners) family of applications.

Written in Rust. Distributable to all Meedya apps via:
- **Rust/Tauri apps** (MeedyaDL, MeedyaManager) — native Cargo dependency
- **Swift/SwiftUI apps** (MeedyaConverter, MeedyaDB) — via `bindings/swift` Swift Package (C FFI / XCFramework)
- **Web targets** — via `bindings/wasm` (future)

## Crates

| Crate | Purpose | Tests |
|---|---|---|
| `meedya-codecs` | Audio/video/subtitle codecs, container formats, HDR, spatial audio, media classification | 23 |
| `meedya-metadata` | Tag registry (TOML-driven), JSON path extraction, common tag definitions (iTunes/Vorbis/ID3v2) | 23 |
| `meedya-fingerprint` | AcoustID fingerprinting, ReplayGain/EBU R128 loudness analysis | 6 |
| `meedya-db` | MeedyaDB API client, shared media models (Track/Album/Artist), database export trait | 3 |
| `meedya-providers` | Metadata provider framework (traits, capabilities, shared provider implementations) | — |

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run all tests (55 total)
cargo test --workspace

# Build a single crate
cargo build -p meedya-codecs
```

## Using in Your Meedya App (Rust)

Add to your `Cargo.toml`:

```toml
[dependencies]
meedya-codecs = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-metadata = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-fingerprint = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-db = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
meedya-providers = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
```

## What's Shared

This library was created after a thorough analysis of code duplication across
MeedyaDL, MeedyaConverter, and MeedyaManager. Key shared functionality:

- **42+ audio codecs** with lossless/spatial/object-based classification and FFmpeg mappings
- **21+ video codecs** with HDR/VideoToolbox support flags
- **36+ container formats** with extension/MIME/codec compatibility matrices
- **TOML-driven tag registry** — add metadata tags with zero code changes
- **Cross-format tag mapping** — same tag name across iTunes atoms, Vorbis Comments, and ID3v2
- **AcoustID API client** with rate limiting and MusicBrainz recording extraction
- **ReplayGain analyzer** — EBU R128 loudness measurement, track + album gain
- **MeedyaDB API client** — search, match, lookup at `api.meedya.tv/v1`
- **Media record types** — Track, Album, Artist shared across all apps
- **Metadata provider traits** — unified interface for MusicBrainz, TMDB, etc.

See [`docs/integration-assessment.md`](docs/integration-assessment.md) for the full analysis.

## Platform Support

macOS · Windows · Linux · Raspberry Pi (armv7/arm64) · iOS · iPadOS · visionOS

## App Store Compatibility

`meedya-core` compiles to native static libraries with no interpreted code,
making it fully compatible with Mac App Store and iOS App Store distribution.

## License

MIT
