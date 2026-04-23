# MeedyaSuite-core — Claude Code Project Instructions

## Project Overview

MeedyaSuite-core is the **shared core library** for all MeedyaSuite applications.
Written in Rust, it provides canonical type definitions and shared functionality
that eliminates code duplication across the Meedya product family.

**Repository**: https://github.com/MWBMPartners/MeedyaSuite-core
**License**: MIT
**Language**: Rust 2021 edition

## Architecture

Rust workspace with 5 crates:

| Crate | Purpose | Status |
|-------|---------|--------|
| `meedya-codecs` | Audio/video/subtitle codecs, container formats, HDR, spatial audio, classification | Implemented (23 tests) |
| `meedya-metadata` | Tag registry, JSON path extraction, common tag definitions, namespace mapping | Implemented (23 tests) |
| `meedya-fingerprint` | AcoustID fingerprinting, ReplayGain/EBU R128 loudness analysis | Implemented (6 tests) |
| `meedya-db` | MeedyaDB API client, media record models (Track/Album/Artist), DB export trait | Implemented (3 tests) |
| `meedya-providers` | Shared metadata provider framework (traits, registry, rate limiting) | Scaffolded |

### Consumption Paths

- **Rust/Tauri apps** (MeedyaDL, MeedyaManager) — direct Cargo dependency via git
- **Swift/SwiftUI apps** (MeedyaConverter, MeedyaDB) — via `bindings/swift` C FFI / XCFramework
- **Web targets** — via `bindings/wasm` (future)

### Consumer Projects

| Project | Language | Integration Status |
|---------|----------|--------------------|
| MeedyaDL | Rust/Tauri + React/TS | Pending — integration branch planned |
| MeedyaConverter | Swift 6, macOS 15+ | Pending — needs Swift bindings |
| MeedyaManager | Rust + Swift/C#/GTK4 | Pending — integration branch planned |
| MeedyaDB | Empty scaffold | Not started |

## Code Conventions

- **Copyright header**: `// Copyright (c) 2026 MWBMPartners` on every source file
- **License line**: `// Licensed under the MIT License.`
- **Module documentation**: Comment block explaining purpose, source (which project it was extracted from), and consumers
- **Error handling**: `thiserror` derive for all error types
- **Serialization**: `serde` Serialize/Deserialize on all public types
- **Enums**: Use `strum` for Display/EnumIter/EnumString where appropriate
- **Testing**: Unit tests in each module, run with `cargo test --workspace`
- **Naming**: `snake_case` for modules/functions, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants
- **Dependencies**: Managed via `[workspace.dependencies]` in root Cargo.toml

## Key Design Principles

1. **Results only, not side effects**: Crates return data/results; consumers handle I/O (file writing, UI updates)
2. **Config-driven where possible**: Tag definitions, codec registries, etc. loaded from TOML — zero code changes to add entries
3. **No app-specific logic**: No Tauri, no SwiftUI, no CLI framework dependencies in core crates
4. **FFI-friendly types**: All public types should be C FFI compatible for Swift bindings
5. **Feature-gated heavy deps**: Large dependencies (symphonia, rusty-chromaprint) behind optional features

## Working With This Codebase

### Building
```bash
cargo build --workspace
```

### Testing
```bash
cargo test --workspace    # All 55 tests
cargo test -p meedya-codecs  # Single crate
```

### Adding a New Codec/Format
Edit the relevant enum in `crates/meedya-codecs/src/` and implement all trait methods. Run tests to verify.

### Adding a New Metadata Tag
Edit the consuming app's `tags.toml` file — zero Rust code changes needed. The tag registry loads definitions from TOML at compile time.

## Files and Directories

```
Cargo.toml                     — Workspace root with shared dependencies
crates/
  meedya-codecs/               — Codec enums, container formats, classification
  meedya-metadata/             — Tag registry, JSON path extraction, common tags
  meedya-fingerprint/          — AcoustID, ReplayGain analysis
  meedya-db/                   — MeedyaDB API client, media models, export trait
  meedya-providers/            — Metadata provider framework (traits)
bindings/
  swift/                       — Swift Package (C FFI) — not yet scaffolded
  wasm/                        — WebAssembly target — not yet scaffolded
docs/
  integration-assessment.md    — Cross-project duplication analysis
```

## Git Workflow

- `main` — stable, reviewed code
- Feature branches: `feature/<description>` or `claude/<task-id>`
- Commit messages: conventional commits (`feat:`, `fix:`, `docs:`, `chore:`)
- Always run `cargo test --workspace` before pushing

## Important Context

This library was created after a thorough analysis of code duplication across
MeedyaDL (349 issues), MeedyaConverter (370+ issues), and MeedyaManager (131 issues).
The integration assessment at `docs/integration-assessment.md` documents the full
findings and extraction plan.
