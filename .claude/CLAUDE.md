# MeedyaSuite-core — Claude Code Project Instructions

> Conventions, principles, and standing tasks. Read first at session start.
> Then read [HANDOFF.md](HANDOFF.md) for where the last session left off, and
> [CONTEXT.md](CONTEXT.md) for the current architectural snapshot.

## Project Overview

MeedyaSuite-core is the **shared core library** for all MeedyaSuite applications. Written in Rust, it provides canonical type definitions and shared functionality that eliminates code duplication across the MeedyaSuite product family.

- **Repository**: https://github.com/MWBMPartners/MeedyaSuite-core
- **License**: MIT
- **Language**: Rust, edition 2021
- **Workspace**: 10 crates

## Architecture (high-level)

Rust workspace, 10 crates, 644 tests passing (791 with `--all-features`). Two co-existing tag-I/O foundations by design: `mp4ameta`-backed (sandbox/App Store safe) and `lofty`-backed (multi-format). See the full per-crate table in [CONTEXT.md](CONTEXT.md) and the public API surface in [`docs/API.md`](../docs/API.md).

### Consumption paths

- **Rust apps** (MeedyaDL, MeedyaManager) — direct Cargo git dependency
- **Swift apps** (MeedyaConverter, MeedyaDB) — via `bindings/swift` (C FFI / XCFramework; planned)
- **Web** — via `bindings/wasm` (planned)

## Code conventions

- **Copyright header** on every source file:
  ```
  // Copyright (c) 2026 MeedyaSuite
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

## Standing rules — how we work (owner-set, revised 2026-09-23)

These apply to every session. Where one of them is ambiguous, ask the owner rather than guess.
Open questions about them are listed in [HANDOFF.md](HANDOFF.md) §0.

### 1. Plain English

When reporting back or explaining anything, use plain, everyday English. Avoid technical
jargon — it confuses even technically strong readers. If a technical term is unavoidable,
say what it means in the same sentence. Short sentences, concrete examples, the "so what"
first.

### 2. Keep the handoff current, as you go

Update [HANDOFF.md](HANDOFF.md) *as each piece of work lands* — not at the end of the
session. It must always be good enough for a brand-new session, with no chat history, to
pick up exactly where we left off. (Detail: see the standing task below.)

### 3. Thinking, planning and who does the work

- Think hard about the work before starting (ultrathink). Use workflows to plan and do it.
- **Deep analysis and deep planning**: Opus agents, run **one after another, not in
  parallel**. (Owner's reasoning at the time of writing: the latest Opus is cheaper and at
  least as good as the latest Fable.)
- **Implementation**: Sonnet or Haiku, whichever fits the job. Complex implementation: Opus.
- Aim: spend tokens and usage credits carefully, but produce correct, top-quality code.
  **GIRFT — Get It Right First Time.**

### 4. Use plugins, and use a *different* AI to check the work

Use the `dev-team-plugins` functionality freely — for the work itself and for suggesting
fixes, tweaks, enhancements and new features. Use it to cross-check across AI systems: work
planned and built with Claude Code is reviewed with Codex, and vice versa.

### 5. Code review loop

All code goes through review with Codex. Issues it finds are fixed automatically, then Codex
reviews again — **repeat until a review finds nothing**. Only then is the work "done".

### 6. After each piece of work

1. Commit and push it to the working branch (`feature/work-in-progress` — the single branch
   that will eventually be merged into `main`, see rule 8), and update its GitHub issue(s) — **each task
   updated individually**, not one bulk comment.
2. Update the Claude memory and context files in `.claude/` (MEMORY.md, CONTEXT.md).
3. Update the OpenAI/Codex memory and context files in `.OpenAI/`.
4. Update [HANDOFF.md](HANDOFF.md).

### 7. Thorough documentation updates

When asked for (and at natural milestones), update *all* documentation: every `.md` file,
any in-app help and guides, and everything in `.claude/` and `.OpenAI/`. If the project
offers a web API, update its OpenAPI/Swagger description too, and if there is no browsable
Swagger UI, add one that works on plain shared hosting (no Docker).
*This repo today:* it is a library with **no web API**, so OpenAPI/Swagger does not apply
(owner decision 2026-09-01, still true 2026-09-23). The partner-app contract is
[`docs/API.md`](../docs/API.md).

### 8. No PR stacking

One working branch, one eventual pull request. Do not open multiple PRs — it invites
merge race conditions. Everything goes to `feature/work-in-progress`; the PR is created
later, when the owner asks. The PR targets **`main`** (owner-confirmed 2026-09-23).

### 9. Be efficient

Re-order and bundle tasks where that is smarter. The list order is not a strict sequence.

### 10. Work autonomously; ask up front

Work through the whole queue without stopping. Pause only for a decision that genuinely
needs the owner's explicit approval — and then say, as simply as possible, exactly what is
needed and why. **Collect such questions at the start**, not as they come up, then carry on
with everything that is not blocked by them.

### 11. Progress updates

Give frequent progress updates as a table of the queued tasks and each one's status.

### 12. If an AI service runs out, hand over — then come back

If an AI service (Claude Code, Codex, or any other) or its agents become unavailable or run
out of usage, hand the work to another suitable one — provided that can be done without
losing context or progress. Switch back to the main service as soon as it is available
again, and when it is, run a **full** review of what was done in its absence. The cross-AI
reviews (rule 5) catch differences in approach between services, which is what makes this
safe. It is also why the handoff must be up to the minute at all times (rule 2): it is what
the stand-in service reads. This rule is not tied to any named tool.

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

### Keep `HANDOFF.md` current *as work lands*

[HANDOFF.md](HANDOFF.md) is the file that survives an interrupted session. Update it **as
each piece of work lands**, not at the end of the session — measured ground truth, the
active branch, decisions the owner has taken, and a status board of what is done vs
outstanding. If you would have to re-derive something next session, it belongs here.

### Never write a test count you did not just measure

This repo's chronic failure mode is doc-count drift: 248, 466, 533, 546, 601, 653 and 664
have all appeared across README/CLAUDE.md/CONTEXT.md/API.md, and none matched the code.
Before writing any count, run it:

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # cargo is not on the default PATH on the dev machine
cargo test --workspace --all-features 2>&1 | grep -E '^test result' \
  | awk -F'[ ;]' '{p+=$4} END {print p}'
```

Do **not** extend a narrative of incremental deltas ("+12 from X, +11 from Y") — that is how
the drift accumulated. Write the measured number and the date. CI enforcement: issue #71.

### Append to `HISTORY.md` per session

Append a dated entry to [HISTORY.md](HISTORY.md) at the end of any substantial session — what landed, design decisions, deferred follow-ups. **Append, do not rewrite.** The history value is the chronological narrative across sessions; rewriting older entries destroys context.

### Open a GitHub issue for every feature, tweak, and enhancement raised — even uncommitted ones

**Trigger**: any substantive new feature, design tweak, code-quality enhancement, follow-up, or research direction surfaced during a session — including ones the user hasn't yet committed to building.

**Action**: before the session ends, open a GitHub issue in this repo with full detail (problem statement, scope, acceptance criteria, references, complexity estimate). Use the issue templates implicitly modelled by issues #21–#58 — they read like mini design docs.

**Why**: "ideas raised once and not picked up are otherwise lost." Even rejected or deferred ideas deserve a tracked record so the team can revisit decisions, the rationale survives staff turnover, and partner-app developers can see what's on the roadmap. The cost of opening an issue is small; the cost of losing a decision is not.

**How to apply**:

- One issue per distinct idea — don't bundle multiple proposals into a single tracker.
- Ideas the user explicitly **rejected** still get captured (one reference issue listing them plus the rationale), so the "no" is on record and reversible.
- Label appropriately: `enhancement` for new features, `meedya-<crate>` for crate-targeted work, plus the topical labels (`dj-metadata`, `bindings`, etc.) where they fit.
- If the project has the `for consideration` label set up, use it for ideas the user hasn't actively endorsed, so they're distinguishable from owner-confirmed enhancements.
- Cross-reference related issues at the bottom of each body so reviewers see the constellation.

**Skip rule**: trivial typo fixes, single-line lint adjustments, and questions answered inline don't need their own issue. The line is "would a partner-app dev care to see this on the roadmap?" — if yes, open the issue.

## Working with this codebase

### Build / test

```bash
cargo build --workspace
cargo test  --workspace                       # 644 tests
cargo test  --workspace --all-features        # 791 tests (the CI configuration)
cargo test  -p meedya-tags-extended           # single crate
cargo doc   --workspace --no-deps --open      # exhaustive auto-generated docs
```

### Adding a metadata tag

Edit `crates/meedya-metadata/tags.toml` (see the procedure in [PROMPTS.md → Adding a new metadata tag](PROMPTS.md#adding-a-new-metadata-tag-apple-music--atoms)). Zero Rust changes. Bump test count in `registry.rs`.

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
  HANDOFF.md                        # Session-resumption state (READ SECOND)
  CONTEXT.md                        # Current architecture snapshot
  HISTORY.md                        # Append-only session log
  MEMORY.md                         # Durable project facts
  PROMPTS.md                        # Reusable task prompts
  ProjectBrief_Chat.claude          # High-level project brief (non-technical audience)
  agents/                           # Claude Code subagent configs (deep-architect, quick-edits)
.OpenAI/                            # Codex/OpenAI memory, context + handoff pointer (mirrors .claude/)
AGENTS.md                           # Entry point Codex reads automatically; points to .OpenAI/ and .claude/
```

## Git workflow

- `main` — stable, reviewed code. Branch protection: required status checks (Backend + Frontend CI), no approval required as of 2026-05-18.
- Feature branches: `feature/<description>` or `claude/<task-id>`
- **Current working branch: `feature/work-in-progress`** — the single branch all work goes to
  (standing rule 8). Eventual PR target: **`main`** (owner-confirmed 2026-09-23).
- CI (`.github/workflows/ci.yml`) only runs on pushes/PRs to `main`, so the working branch gets
  **no CI** — local fmt/clippy/test is the only gate until the PR opens.
- Commit messages: conventional commits (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`)
- Run `cargo test --workspace` before pushing
- For substantial public API changes, update `docs/API.md` in the same commit (see standing task above)

## Important context

This workspace was created after a thorough analysis of code duplication across MeedyaDL (349 issues), MeedyaConverter (370+ issues), and MeedyaManager (131 issues). The original assessment at [`docs/integration-assessment.md`](../docs/integration-assessment.md) documents the full findings; the implementation status is captured at the top of that file. Pre-drafted GitHub issues for downstream-app adoption live in [`docs/cross-repo-issues.md`](../docs/cross-repo-issues.md).
