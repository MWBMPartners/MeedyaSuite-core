# Session History

> Chronological log of Claude Code sessions and notable branch work. Maintained per session — append, don't rewrite.
> For exact commit messages and diffs, see `git log`. This file captures decisions, design context, and pending follow-ups that don't fit a commit message.

---

## 2026-05-10 — DJ-metadata foundation + library importers (current `main`)

Three substantial additions landed in a single working session.

### `meedya-metadata::playback_bounds` (~100 lines)
- Soft playback start/stop atoms in the `MeedyaMeta` namespace
- Mirrors iTunes' Get Info → Options Start/Stop Time, which iTunes itself stored only in its library DB (never in the file). MeedyaSuite-only honored — third-party players ignore the atoms.
- Writes a paired `PlaybackStartMs` (canonical u64) + `PlaybackStart` (HH:MM:SS.mmm display) per endpoint; `Ms` atom is authoritative on read.

### `meedya-library-import` crate
- New workspace member.
- `itunes_xml` module — parses `iTunes Music Library.xml` exports; emits `LibraryEntry` per track with Start/Stop Time. Cross-platform `file://` URL decoding (Windows drive-letter detection by shape, not `cfg(windows)`).
- `cuesheet` module — full CUE parser at CD-frame precision (`CueTime { minutes, seconds, frames }`, 75 fps). Rich `CueSheet` model preserves CATALOG, performers, ISRC, REMs at disc and track scope. `import()` adapter emits LibraryEntries only for the narrow case where soft-trim semantics apply (per-track files with non-zero `INDEX 01`); single-file album rips emit warnings pointing at the future chapter-writer path.
- Designed so future `mediamonkey` (SQLite) module slots in alongside.

### `meedya-tags-extended` crate (foundation only)
- New workspace member.
- Built on `lofty` (vs `mp4ameta` used by `meedya-metadata`) — multi-format support (MP3/M4A/FLAC/WAV/AIFF/OGG/MKV) and automatic foreign-frame round-tripping.
- Unified data model: `ExtendedTags`, `MusicalKey` (Camelot/Open Key/traditional round-tripping), `CuePoint`, `LoopPoint`, `BeatGrid`, `Source` enum.
- `TagFile` wrapper with `open` / `save` / `save_to` / typed-tag access.
- `standard` module — BPM/key/comment read/write across all formats. Covers **Mixed In Key** fully (MIK writes only standard tags).

### Pending for future sessions
- Serato readers (Markers2, Autotags, BeatGrid) — biggest scope; mirror Mixxx project's vetted approach rather than reverse-engineering fresh. Requires real DJ-tagged fixture files.
- Rekordbox reader — ID3v2 PRIV frames + cleaner alternative path: `rekordbox.xml`.
- Traktor reader — embedded cue frames + `collection.nml`.
- Virtual DJ reader — `.vdj` XML sidecar + embedded markers.
- Chapter authoring — MeedyaConverter consumer for `CueSheet` track indexes; writes MP4 `chap` track + `chpl` atom. Disc TOC alternate input shape.

### Notable design decisions
- **Two tag-I/O foundations coexist.** `meedya-metadata` stays on `mp4ameta` for the Apple Music flow; `meedya-tags-extended` uses `lofty` for everything else. Not unified — they serve different code paths.
- **Importers don't match files.** `meedya-library-import` emits records with `EntryLocator::{ Path | PersistentId }`; the consuming app handles filesystem resolution.
- **No premature trait abstraction.** Each importer is a free function; trait extraction deferred until ≥2 implementations share a meaningful contract.
- **Fixture-based testing for proprietary parsers.** Won't write Serato/etc parsers from memory — need real tagged sample files to validate against.

### Commits on `main` after this session
(Pending — this session's work is not yet committed.)

### Tests
- meedya-metadata: 31 (was 24 pre-session)
- meedya-library-import: 30 (new)
- meedya-tags-extended: 29 (new)
- Stubs: 0
- **Workspace total: 90**

---

## Branch context (not on `main`)

### `claude/interesting-mirzakhani` (last commit 2026-04-24)
Substantially fuller implementation than current `main`. Contains:
- `meedya-codecs` (23 tests) — full codec/container/HDR/spatial enums, FFprobe + MediaInfo integration
- `meedya-metadata` (23 tests) — earlier registry version + CommonTag enum
- `meedya-fingerprint` (6 tests) — AcoustID client + ReplayGain EBU R128
- `meedya-db` (3 tests) — MeedyaDB API client, Track/Album/Artist models, DbExporter trait
- `meedya-providers` — provider framework with traits, rate limiting, cover art
- `meedya-lyrics` — LRCLIB client + LRC sidecar I/O
- `meedya-core` — unified facade crate with feature flags
- `.claude/CLAUDE.md` and `.claude/ProjectBrief_Chat.claude` (Claude Code v1 conventions)

Current state of this branch is unclear (was it abandoned, is it being merged piecemeal, or is the work being re-extracted onto `main`?). Treat as a reference, not source of truth.

### `origin/alpha` and `origin/beta`
Not inspected this session. Likely contain rolling release candidates.

### Recent merged work
- `claude/assess-meedyadl-integration-tbu6v` — integration assessment for MeedyaDL adoption
- `claude/merge-diverged-branches-MROij` — branch reconciliation work
- PR #18 — `claude/evaluate-lrcget-integration-4zlXZ` (LRCLIB integration, merged into prior working branch)

---

## Pre-session reference (per `git log main`)

```
14f31cb 2026-04-05  Fix README.md title and formatting issues
bb8d5b5 2026-04-05  chore: initial workspace scaffold — Cargo workspace + crate stubs + Swift/WASM binding placeholders
6f1877c 2026-04-05  Initial commit
```

`main` was a stub workspace until this 2026-05-10 session.
