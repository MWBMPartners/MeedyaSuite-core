# MeedyaSuite-core — Claude Code Project Instructions

> Conventions, principles, and standing tasks. Read first at session start.
> For the current architectural snapshot (which evolves more often), see [CONTEXT.md](CONTEXT.md).

## Project Overview

MeedyaSuite-core is the **shared core library** for all MeedyaSuite applications. Written in Rust, it provides canonical type definitions and shared functionality that eliminates code duplication across the MeedyaSuite product family.

- **Repository**: https://github.com/MWBMPartners/MeedyaSuite-core
- **License**: MIT
- **Language**: Rust, edition 2021
- **Workspace**: 9 crates

## Architecture (high-level)

Rust workspace, 9 crates, 211 tests passing. Two co-existing tag-I/O foundations by design: `mp4ameta`-backed (sandbox/App Store safe) and `lofty`-backed (multi-format). See the full per-crate table in [CONTEXT.md](CONTEXT.md) and the public API surface in [`docs/API.md`](../docs/API.md).

### Consumption paths

- **Rust apps** (MeedyaDL, MeedyaManager) — direct Cargo git dependency
- **Swift apps** (MeedyaConverter, MeedyaDB) — via `bindings/swift` (C FFI / XCFramework; planned)
- **Web** — via `bindings/wasm` (planned)

## Code conventions

- **Copyright header** on every source file:
  ```
  // Copyright (c) 2024-2026 MWBM Partners Ltd
  // Licensed under the MIT License. See LICENSE file in the project root.
  ```
- **Module documentation**: comment block explaining purpose, source (which downstream app it was extracted from, if any), and consumers.
- **Error handling**: `thiserror` derive on every error type; or `Result<_, String>` for module-local errors that don't escape.
- **Serialization**: `serde` Serialize/Deserialize on public types where appropriate.
- **Enums**: `strum` derive for Display/EnumIter/EnumString where it helps.
- **Naming**: `snake_case` for modules/functions, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- **Workspace dependencies**: declare common deps in root `Cargo.toml` `[workspace.dependencies]`; individual crates inherit via `workspace = true`.

## Key design principles

1. **Standards-first**: Use standard metadata tags (ID3v2 / Vorbis / MP4 ilst spec fields) wherever they exist. Fall back to `MeedyaMeta:*` freeform atoms **only** when no standard equivalent exists (e.g., DJ energy ratings, internal audit trails, MeedyaSuite-only soft-playback bounds). Standards-first applies to every crate — not just MIK or DJ-metadata.
2. **Results only, not side effects**: Crates return data; consumers handle I/O (file writes, UI updates, DB persistence).
3. **Config-driven where possible**: Tag definitions, codec registries etc. loaded from TOML — zero Rust changes to add entries.
4. **No app-specific logic**: No Tauri, no SwiftUI, no CLI framework dependencies in core crates.
5. **FFI-friendly types**: Public types in crates targeted at Swift consumption should be C-FFI compatible.
6. **Feature-gated heavy deps**: Large dependencies (`symphonia`, `rusty-chromaprint`, OS keyring) behind optional features.
7. **Two tag-I/O foundations coexist intentionally** — `mp4ameta` for the sandbox-safe Apple Music flow, `lofty` for multi-format DJ-metadata and pass-through. Do not try to unify them.
8. **Fixture-based testing for proprietary format parsers** — don't reverse-engineer Serato/Rekordbox/Traktor formats from memory. Require real tagged sample files.

## Standing tasks

### Keep `docs/API.md` in sync with public API changes

**Trigger**: any commit that changes the public API surface (new/renamed/removed `pub use`, `pub mod`, `pub fn`, `pub struct`, `pub enum`, `pub trait`; new/renamed feature flag on `meedya-core`; ≥5 net test-count change).

**Action**: in the *same commit* as the code change, update [`docs/API.md`](../docs/API.md):

1. Crate section affected — update the relevant API listings.
2. Workspace overview table at the top — update per-crate test count if changed.
3. "Last refreshed" date at the top of `docs/API.md`.
4. If a crate was added or removed, update the table-of-contents anchors.
5. Cross-reference [`README.md`](../README.md) — bump the total test count there if the workspace total changed.

Do not defer this to a follow-up PR. The spec is the contract partner apps reference during their development; stale spec is worse than missing spec because it produces silent integration bugs in downstream apps.

The procedure is captured as a reusable prompt at [`PROMPTS.md` → Refresh internal API spec](PROMPTS.md#refresh-internal-api-spec).

### Keep `CONTEXT.md` reflective of `main`

Update [CONTEXT.md](CONTEXT.md) whenever the workspace structure changes meaningfully — new crate, retired crate, status flip (placeholder → implemented), substantial new module within an implemented crate. CONTEXT.md is the "what does this repo look like right now" snapshot; out-of-date here makes future Claude sessions waste turns rediscovering.

### Append to `HISTORY.md` per session

Append a dated entry to [HISTORY.md](HISTORY.md) at the end of any substantial session — what landed, design decisions, deferred follow-ups. **Append, do not rewrite.** The history value is the chronological narrative across sessions; rewriting older entries destroys context.

## Working with this codebase

### Build / test

```bash
cargo build --workspace
cargo test  --workspace                       # 211 tests
cargo test  -p meedya-tags-extended           # single crate
cargo doc   --workspace --no-deps --open      # exhaustive auto-generated docs
```

### Adding a metadata tag

Edit `crates/meedya-metadata/tags.toml` (see the procedure in [PROMPTS.md → Adding a new metadata tag](PROMPTS.md#adding-a-new-metadata-tag)). Zero Rust changes. Bump test count in `registry.rs`.

### Adding a workspace crate

See [PROMPTS.md → Adding a new workspace crate](PROMPTS.md#adding-a-new-workspace-crate).

### Implementing a proprietary DJ reader

**Do not start without real fixture files.** See [PROMPTS.md → Implementing a proprietary DJ reader](PROMPTS.md#implementing-a-proprietary-dj-reader) for the full procedure and anti-corruption guardrails.

## Files and directories

```text
Cargo.toml                          # Workspace root + shared dependencies
crates/
  meedya-codecs/                    # Codec/container/HDR/spatial enums + detection
  meedya-core/                      # Facade with feature flags
  meedya-db/                        # MeedyaDB API client + media models
  meedya-fingerprint/               # AcoustID + ReplayGain
  meedya-library-import/            # iTunes XML, CUE sheet importers
  meedya-lyrics/                    # LRCLIB client, LRC I/O, sidecar + embed
  meedya-metadata/                  # Tag registry + lofty/mp4ameta surfaces
  meedya-providers/                 # Provider framework (traits, rate limit, cover art)
  meedya-tags-extended/             # DJ metadata foundation (lofty)
bindings/
  swift/                            # Swift Package (planned)
  wasm/                             # WebAssembly (planned)
docs/
  API.md                            # Internal API spec for partner apps (KEEP IN SYNC)
  integration-assessment.md         # Original cross-project duplication analysis
  cross-repo-issues.md              # Pre-drafted issues for downstream apps
.claude/
  CLAUDE.md (this file)             # Conventions + standing tasks
  CONTEXT.md                        # Current architecture snapshot
  HISTORY.md                        # Append-only session log
  MEMORY.md                         # Durable project facts
  PROMPTS.md                        # Reusable task prompts
```

## Git workflow

- `main` — stable, reviewed code. Branch protection: required status checks (Backend + Frontend CI), no approval required as of 2026-05-18.
- Feature branches: `feature/<description>` or `claude/<task-id>`
- Commit messages: conventional commits (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`)
- Run `cargo test --workspace` before pushing
- For substantial public API changes, update `docs/API.md` in the same commit (see standing task above)

## Important context

This workspace was created after a thorough analysis of code duplication across MeedyaDL (349 issues), MeedyaConverter (370+ issues), and MeedyaManager (131 issues). The original assessment at [`docs/integration-assessment.md`](../docs/integration-assessment.md) documents the full findings; the implementation status is captured at the top of that file. Pre-drafted GitHub issues for downstream-app adoption live in [`docs/cross-repo-issues.md`](../docs/cross-repo-issues.md).
