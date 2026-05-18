# Reusable Prompts

> Templates for common MeedyaSuite-core tasks. Copy → fill in the bracketed parts → paste. Each prompt is self-contained so Claude can pick it up cold without needing this file as additional context.

---

## Adding a new metadata tag (Apple Music → atoms)

```
Add a new tag to crates/meedya-metadata/tags.toml:
- Scope: [album | track]
- Tag id: [snake_case_id]
- API JSON path: [e.g., attributes.someField]
- Value type: [string | bool | u32 | u64 | array | first_of_array]
- Atom targets: [list of (namespace, atom name) pairs]

After editing, bump the test count in registry.rs (album_tags_count or track_tags_count) and run `cargo test -p meedya-metadata`. No other Rust code should need to change — the registry is config-driven.
```

---

## Implementing a proprietary DJ reader

```
Implement the [Serato | Rekordbox | Traktor | VirtualDj] reader in crates/meedya-tags-extended/src/[source].rs.

Before writing parser code:
1. Acquire at least 2-3 real fixture files tagged by the target app (commit them to tests/fixtures/[source]/).
2. Document the on-disk frame structure in a module doc comment, citing the source (Mixxx project for Serato, official docs for Rekordbox XML, etc.).
3. DO NOT reverse-engineer from memory — the binary formats have version drift and silent corruption of DJ work is the worst possible outcome.

API contract:
- Public function: `read(tag_file: &TagFile) -> Option<SourceData>` (no I/O, just parse what TagFile has already loaded)
- Output populates the fields on `crate::model::ExtendedTags` with `source = Source::[Variant]`
- Foreign frames must round-trip untouched on save (lofty default; verify with fixture test)

Phase 1 is read-only. No proprietary writers in this crate yet.
```

---

## Adding a new library importer

```
Add a new importer to crates/meedya-library-import/src/[source].rs producing the existing `LibraryEntry` shape.

Pattern (see itunes_xml.rs and cuesheet.rs for reference):
1. `pub const KIND: &str = "[source-id]"` — used in SourceInfo and PersistentId locators
2. `pub fn import(path: &Path) -> Result<ImportReport, String>` — public entry point
3. Emit entries only when meaningful trim semantics apply (filter at source, don't flood consumers with entries where start_ms = 0 and stop_ms = None)
4. Path/ID resolution: prefer EntryLocator::Path with absolute paths; fall back to PersistentId when no usable path is available
5. Warnings (non-fatal issues) go in `ImportReport.warnings`

Tests should cover: valid record extraction, both locator types, malformed input warnings, source-info correctness.
```

---

## Adding a new workspace crate

```
Add a new crate at crates/meedya-[name]/:

1. Update root Cargo.toml workspace members list (alphabetical near similar crates)
2. Create crates/meedya-[name]/Cargo.toml with workspace inheritance:
   - version.workspace = true
   - edition.workspace = true  (= 2021)
   - authors.workspace = true
   - license.workspace = true
   - repository.workspace = true
   - description = "MeedyaSuite Core — [purpose]"
3. Create crates/meedya-[name]/src/lib.rs with copyright header:
   // Copyright (c) 2026 MeedyaSuite
   // Licensed under the MIT License. See LICENSE file in the project root.
4. Run `cargo build -p meedya-[name]` to verify wiring
5. Update .claude/CONTEXT.md crate table

Do not create empty stub modules speculatively — add modules only when implementing them.
```

---

## Verifying cross-platform path handling

```
For any code that decodes file paths from external sources (XML, plist, JSON, sidecar files):

- Test both macOS-style (`/Users/...`) and Windows-style (`C:/Users/...`) inputs
- Detect Windows paths by shape (drive letter pattern), not `cfg(windows)`, so cross-platform imports work (macOS user reading a Windows export)
- Decode percent-encoding for any URL-form inputs (`%20` → space, etc.)
- Strip BOM (`\u{feff}`) from UTF-8 text files

Reference implementation: crates/meedya-library-import/src/itunes_xml.rs::decode_file_url.
```

---

## Updating .claude/ docs

```
After landing substantive work, update:
- .claude/CONTEXT.md — if architecture, crate status, or design decisions changed
- .claude/HISTORY.md — append a session entry under today's date; do NOT rewrite existing entries
- .claude/MEMORY.md — if any durable project fact changed (rare)
- .claude/PROMPTS.md — if you discovered a workflow that should be a reusable prompt

CONTEXT.md should reflect actual state of `main`, not aspirational state. HISTORY.md is append-only.
```

---

## Refresh internal API spec

```
Refresh docs/API.md to match the current public API surface. Triggered by:
- New/renamed/removed `pub use`, `pub mod`, `pub fn`, `pub struct`, `pub enum`, `pub trait` in any crate's lib.rs or public module
- New/renamed/removed feature flag on meedya-core
- New workspace member added or removed
- Workspace test count net change ≥5

Procedure:
1. Run `cargo test --workspace` and capture per-crate test counts:
   for c in meedya-codecs meedya-core meedya-db meedya-fingerprint meedya-library-import meedya-lyrics meedya-metadata meedya-providers meedya-tags-extended; do
     echo -n "$c: "; cargo test -p $c --quiet 2>&1 | grep -E "test result: ok\." | head -1 | grep -oE "[0-9]+ passed" | head -1
   done

2. For each changed crate, read crates/<name>/src/lib.rs to list:
   - `pub use` re-exports (these are the headline types)
   - `pub mod` declarations (these define the API surface area)

3. For changed modules, walk public items (`pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub const`).

4. Update docs/API.md:
   - The workspace overview table at the top (per-crate test count)
   - The crate's API listings section
   - The "Last refreshed" date at the top
   - The TOC anchors if a crate was added/removed

5. Cross-reference README.md — bump total test count and crate table if the workspace total changed materially.

6. Commit the API.md update IN THE SAME COMMIT as the code change. Do not defer to a follow-up PR — partner apps consume API.md as the integration reference and stale spec produces silent integration bugs.

7. If a crate added a feature flag exposed through meedya-core, also update the feature-flag table in API.md and README.md.

Anti-patterns to avoid:
- Don't list internal-only types (anything not exposed via pub use or pub mod).
- Don't paste raw rustdoc output — write the spec for partner-app developers, not as comprehensive auto-generated reference (use `cargo doc` for that).
- Don't skip "Last refreshed" date update.
- Don't bundle stale-API.md fixes into commits where the API didn't actually change (creates churn).
```

---

## Running the full local validation

```
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace -- -D warnings   # if clippy is set up
cargo fmt --all -- --check                # formatting check
```

Workspace currently has 211 tests on `main`. All must pass before push.
