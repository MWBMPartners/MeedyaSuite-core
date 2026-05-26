# Durable Project Facts

> Things that are true about this project and unlikely to change session-to-session.
> For mutable state (architecture diffs, in-flight work, branch context), see [CONTEXT.md](CONTEXT.md) and [HISTORY.md](HISTORY.md).

---

## Identity

- **Name**: `MeedyaSuite-core` (canonical). **Never** "Meedya-core" — that's wrong.
- **Repository**: https://github.com/MWBMPartners/MeedyaSuite-core
- **Organisation**: MWBM Partners Ltd
- **License**: MIT
- **Language**: Rust, edition 2021

## File header

Every Rust source file starts with:

```rust
// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
```

`Cargo.toml` for each crate inherits from workspace:

```toml
version.workspace    = true
edition.workspace    = true   # = 2021
authors.workspace    = true   # = ["MeedyaSuite"]
license.workspace    = true   # = "MIT"
repository.workspace = true
description          = "MeedyaSuite Core — <purpose>"
```

## Consumer apps (the "MeedyaSuite family")

| App | Language / stack | Role |
|---|---|---|
| **MeedyaConverter** | Swift 6 / SwiftUI, macOS 15+ | Audio/video conversion + tagging |
| **MeedyaManager** | Rust + Swift/C#/GTK4 | Local library management + tagging |
| **MeedyaDL** | Rust/Tauri + React/TS | Store downloads (Apple Music etc.) |
| **MeedyaPlayer** | Planned | MeedyaSuite-native media player |
| **MeedyaDB** | Empty scaffold (Swift planned) | Database backend |

Each downstream app has a `claude/core-integration` branch where it adopts this workspace as a dependency.

Rust apps consume via direct Cargo git dependency. Swift apps will consume via `bindings/swift` (C FFI / XCFramework — not yet scaffolded on `main`). Web targets via `bindings/wasm` (future).

## Atom namespaces

When writing MP4 freeform atoms (`----` boxes):

- **`com.apple.iTunes`** — the iTunes-recognised namespace. Use for fields with player-compatibility precedent (industry-standard names like `ISRC`, `LABEL`, `COPYRIGHT`, `TOTALTRACKS`).
- **`MeedyaMeta`** — MeedyaSuite-branded namespace. Use for:
  - Fields that have no standard equivalent (playback bounds, custom cue points)
  - Supplementary mirrors of iTunes-namespaced tags (often dual-written for redundancy)
  - Apple-Music-source-specific fields prefixed `Apple*` (e.g., `AppleRecordLabel`, `AppleReleaseDate`)

## Tag-I/O foundations

Two coexist in the workspace — they are NOT redundant, they serve different code paths:

- **`mp4ameta`** (in `meedya-metadata`) — M4A/MP4 only. Used by the Apple Music JSON → atom flow. Tags driven declaratively by [tags.toml](../crates/meedya-metadata/tags.toml).
- **`lofty`** (in `meedya-tags-extended`) — Multi-format (MP3/M4A/FLAC/WAV/AIFF/OGG/MKV). Used by DJ metadata read/write and the general-purpose pass-through flow. Round-trips unknown frames automatically.

Don't try to unify these. They serve genuinely different needs and unifying would compromise both.

## License obligations

- MIT license on all source files (header above).
- Third-party Rust crates: license compatibility is checked at the Cargo.lock level by CI. Dependencies must be MIT, Apache-2.0, BSD, MPL-2.0, or similarly permissive. Avoid GPL/AGPL.
