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
- Third-party Rust crates: dependencies must be MIT, Apache-2.0, BSD, MPL-2.0, or similarly permissive. Avoid GPL/AGPL. **Enforced by CI** since `bcb7766` (2026-09-02): a `cargo-deny` job in `ci.yml` checks advisories, the licence allowlist and duplicate versions against [`deny.toml`](../deny.toml) (issue #84). Known snag: an older locally-installed `cargo-deny` rejects `highlight = "all-duplicates"` in `deny.toml`, so the check may only run in CI.

## Development environment

- **`cargo` is not on the default PATH** on the primary dev machine. Every command needs
  `export PATH="$HOME/.cargo/bin:$PATH"`. Toolchain: rustup stable (1.98.0 as of
  2026-09-01), which matches the version MeedyaDL pins.
- **MSRV is `rust-version = "1.82"`**, declared on `[workspace.package]` and inherited by
  all 10 member crates via `rust-version.workspace = true`. Driven by `Option::is_none_or`.
  Member crates must opt in explicitly — inheriting `edition`/`authors` does not carry it.
- **CI only triggers on `pull_request` and `push` to `main`.** A feature branch gets **no
  CI at all**, so local `cargo fmt --check` + `cargo test --workspace --all-features` is the
  only gate until the PR opens.

## Test counts are measured, never carried forward

Doc-count drift is this repo's chronic failure mode — 248, 466, 533, 546, 601, 653 and 664
have all appeared in the docs and none matched the code. **Only ever write a number you just
measured**, with the date. Never extend a narrative of incremental deltas. CI enforcement is
tracked in issue #71.

## MusicBrainz query construction

Two things here are counter-intuitive and were both settled empirically, so don't "simplify"
them away:

1. **Bare-term and in-phrase Lucene escaping are different regimes.** Inside a double-quoted
   phrase only `\` and `"` are structurally significant — `( ) : + - ?` are literal text, and
   escaping them there embeds literal backslashes into the phrase and kills the match.
   Outside a phrase the full 19-character special set must be escaped. Hence three helpers:
   `escape_lucene` (bare), `quote_phrase` (phrase), `phrase_clause` (whole `field:"value"`).

2. **ISRC and ISWC are queried in different forms, deliberately.** MusicBrainz documents
   neither; this was established by live probing on 2026-09-01:
   - ISRC → **compact** (`isrc:GBAYE0601498`). Hyphenated returns 0 results.
   - ISWC → **dotted display form** (`iswc:"T-304.031.869-8"`). Compact returns 0 results;
     hyphen-only is a **parse error**.

   `normalise_isrc` and `format_iswc_dotted` encode this asymmetry. Re-verify after the
   2026-11-30 Solr 10 reindex (issue #69).

**Fetch note**: `tickets.metabrainz.org` HTML is behind Anubis anti-bot protection. Use the
JIRA REST API instead: `https://tickets.metabrainz.org/rest/api/2/issue/SEARCH-<n>`.

## Rate limiting is keyed by host budget, not provider name

`meedya-providers`' default limiters are shared per **upstream host budget**, not per
provider id:

- `musicbrainz.org` — musicbrainz + isrc + iswc share one `per_second(1)` limiter
- `itunes.apple.com` — apple_music + apple_tv + itunes_store + apple_podcasts share one
  20 RPM limiter

The obvious per-provider-name design would give the four Apple providers **4× Apple's per-IP
allowance** while looking correct. Pinned by tests
(`itunes_backed_providers_share_one_host_budget`).

Two more things here that look wrong and are not:

- **`per_second(1)`, not `per_minute(60)`, for MusicBrainz.** governor's per-minute quota
  permits an immediate 60-request *burst* — precisely what MusicBrainz's published "one
  request per second on average" forbids, and it answers bursts with 503s.
- **Defaults live in a process-global `OnceLock` table**, so limiters are shared across
  provider *instances*. A per-instance limiter is useless: batch apps construct a provider
  per task, so N instances would mean N independent budgets.

Providers are **throttled by default** and block (`wait_until_ready`) rather than erroring,
so callers get correct behaviour without writing retry loops. `check()` is public for
fail-fast callers; `with_rate_limiter` injects or shares one.

## Error strings never contain credentials

Every `reqwest` error captured in `meedya-providers` and `meedya-fingerprint` has its
**query string stripped** before being stringified. reqwest's `Display` appends the full URL,
and TMDb (`api_key`), OMDb (`apikey`) and AcoustID (`client`) put the credential there.
Host and path are kept — they are the useful part when diagnosing a batch failure.

Applied to *every* provider, not only those three, so a provider added later with
query-string auth is safe by default. Canary tests in both crates assert a known secret
cannot appear. Providers using header auth (Spotify, TheTVDB, EIDR) were never exposed —
reqwest does not print headers.

## lofty: never hardcode a fallback tag type

`insert_tag` **silently does nothing** when the container does not support the tag type, so
`insert_tag(...)` followed by `tag_mut(...).unwrap()` panics. Always derive the fallback from
`primary_tag_type()`, which is a total function of the *file type* (`Mp4 -> Mp4Ilst`,
`Flac|Opus|Vorbis|Speex -> VorbisComments`, `Aac|Aiff|Mpeg|Wav -> Id3v2`,
`Ape|Mpc|WavPack -> Ape`) and whose result is always both insert- and save-supported.

Also: `supports_tag_type(Id3v2)` is **too permissive** as a guard — lofty reports Id3v2 as
read-only supported for FLAC/APE/MPC, so those pass the check and then fail at `save`. Use
`primary_tag_type() != Id3v2` when you need *writable* Id3v2.

## lofty: several languages, and what the M4A route loses (policy branch, 2026-09-28)

These refusals are deliberate. Do not "fix" them by letting the save go ahead.

- **ID3v2 language frames.** lofty keeps only the LAST of several language frames, so every
  save in `tag_io` (and `meedya-lyrics`' `embed_synced`) first reads them from the file's bytes
  (`id3v2_language_frames`) and merges them. A language frame is `TLAN`, `TLA` in an ID3v2.2
  tag, or `TLA` plus a zero byte in an ID3v2.3 tag — lofty reads all of those as `TLAN`. The
  same `TLA`-and-zero inside an **ID3v2.4** tag is NOT a language to lofty (mutagen says it is),
  and lofty's save turns it into `TXXX:TLA`, so it refuses the save. Anything the reader cannot
  read safely refuses too.
- **Two steps, in this order**, for a caller saving its own lofty `TaggedFile`:
  `recover_languages_after_reading` straight after reading (it reads the disk), then
  `gather_languages_before_saving` just before saving (it never reads the disk). A single
  helper doing both before the save undid the caller's own language edits; it was removed.
- **A language frame in a file with more than one ID3v2 tag** (an MP3's tags one after another,
  a WAV or AIFF file's several ID3 chunks) refuses the save: lofty reads and rewrites only one of
  them, so merging left the languages in two places, swapping order save after save (revision 9),
  and a language in the OTHER tag was never seen — a write of `deu` "succeeded" while the file
  kept `eng` (revision 10: refused whenever ANY of the tags holds a language frame).
- **M4A (#102, #103): every save is CHECKED, not predicted (revision 9).** Every M4A write
  except the language atom goes through lofty's format-neutral `Tag`, and that route changes
  atoms nobody asked for — the first value only, flags rewritten as text, freeform names
  respelled, unusual cover-art types dropped, 1- and 2-byte numbers rewritten as 4, a 6-byte
  `disk` as 8. Revision 8 LISTED such cases and the stand-in review found more, so the list is
  gone: `tag_io` saves to a temporary copy, `mp4_save_check` reads the `ilst` atoms of both
  files from their bytes, and the copy replaces the original (one rename) only when every atom
  not asked for is byte for byte unchanged and every atom asked for holds exactly what was asked,
  nothing beside it. "Asked for" is RECORDED as each write is made (`keys_written`, the registry
  keys), not guessed from which values changed — writing the artist replaces the whole `©ART`
  atom on purpose, even when the value was already first. One deliberate exception: several
  language atoms are mended into one, checked value by value. **Consequence:** a typical
  iTunes / Apple Music file — and any file whose disc number mutagen wrote (GAMDL, Picard,
  beets: a 6-byte `disk`) — is refused even for a title-only write, until the real fix (a route
  that keeps those atoms) — still open in #102. `write_registry_tags` also refuses
  `namespace:name` keys an M4A file cannot store (#103), and an ISRC registry write on a file
  holding one ISRC atom (the copy would hold two). Real test files for all of this are in
  `crates/meedya-metadata/testdata/m4a/` and `testdata/id3/` (made by ffmpeg + mutagen; the
  scripts are beside them, and re-create every file byte for byte).
- **M4A, revision 10: the WHOLE saved file is checked, not just its tags** (`mp4_file_check`).
  lofty does NOT correct every offset — it broke the audio of a fragmented file and wrote the
  tag list over the handler of a `meta` with no `ilst`, both while the tag check passed. Now the
  copy must also prove: every `mdat` byte for byte the same; every `stco`/`co64` entry moved by
  exactly as much as the `mdat` it points into; everything else in `moov` byte for byte the same
  (except the `ilst` contents, the size fields on `moov`→`udta`→`meta`→`ilst`, and `free`/`skip`
  in `udta`/`meta`); every other top-level atom the same, in order. Fragmented files (`moof`,
  `mfra`, `sidx`, `mvex`) and a `meta` with parts but no `ilst` are refused before anything is
  written — and the comparison refuses a fragmented file by itself too, because (a)–(d) alone
  PASS one (every `moof` is unchanged, which is exactly the damage). Values are measured against
  what the CALLER gave (track/disc numbers 1–65535 in digits; a four-digit year; compilation `1`
  or `0`), and a field with no M4A atom (`Arranger`, `AcoustId`, `ReplayGainReferenceLoudness`)
  is refused by name — so `write_acoustid_tags` / `write_replaygain_tags` fail on every M4A
  file. The copy is written through the `create_new` handle (`save_by_copy`), flushed, and both
  names checked (device+inode; Windows file index via `winapi-util`) before the rename; it no
  longer keeps extended attributes on macOS. The comparisons count their steps and are linear.
- **M4A, revision 11 (Codex's catch-up review of revisions 8–10).** The copy is created
  owner-only (0600) — it was 0644, readable by every account while checked — and gets the
  original's permissions only just before the rename (Windows: the folder's access rules,
  written down). Refused before saving: a `meta` holding fewer than 4 bytes (it crashed the
  comparison), 1–7 bytes left over at the end of an `ilst` (lofty dropped them unseen), and a
  container lofty follows (`moov`, `udta`, `moof`, `trak`, `mdia`, `minf`, `stbl`) sitting
  where no M4A file has one. **lofty's own SAVE recurses into those containers with no depth
  limit and overflowed the stack at 1,000 nested `minf`** — so that refusal must stay in
  `refuse_what_a_save_would_damage`, BEFORE the save; a fix in the comparison alone cannot
  protect `write_tags`. The comparison follows only the fixed path `moov → trak → mdia → minf
  → stbl`, numbers each container's atoms once (a `Place` chain, words made only for a
  difference), and reads the files without a buffered reader (it threw its buffer away at every
  jump). Every error path deletes the copy (`TempCopy::discard`) and names it when it cannot.
  Two test-only aids: `WHILE_THE_COPY_EXISTS` (look at the copy mid-save) and
  `permissions_are_ignored_here` (a permission test may only skip when this is shown).
- **M4A, revision 12 (Codex's review of revision 11).** New private module `access_rules`: the
  copy lets no access control list in. Linux/Android: its folder's list is taken off before a
  byte is written and the file's own put on before the rename, through the handles (`rustix`).
  macOS: lists are only READ, by name, each read bracketed by the identity check (`exacl`,
  macOS-only — the workspace keeps `unsafe` out; writing by name, `/dev/fd/N` and raw `acl_*`
  were rejected); a folder that passes entries on to new files, a file with a list of its own,
  or a copy that got one, is refused. Any other Unix system: every M4A save is refused. The
  copy gets the file's group, then its list, then its bits (that order; refused if the group
  cannot be given and the file lets its group in); set-user-ID/set-group-ID files are refused.
  A copy counts as deleted only when its count of names is 0 (Unix); a moved copy is reported.
  **Known limit:** on macOS a FAT/exFAT disk changes a file's number once data is written, so
  the identity check (revision 10) refuses every M4A save there and leaves the copy — now said
  in the error; the likely fix (compare with the handle's CURRENT number) awaits the lead.
  Test aids: `rerun_with_the_usual_mask` (a child process under umask 022), `AtomName` +
  `NAME_WORK` (counted name comparisons), a counting reader. CI runs on Linux only, so every
  test must exist on macOS and Linux alike (branch inside, print a reason) or the documented
  counts differ.
- **ID3 language frames, revisions 11–12: a 1 MiB budget, charged for more than text.** Tags are
  found by their headers, looked at a block at a time (a 3-byte carry, so a name across a block
  join counts once), and only language frames' contents are read, within a 1 MiB budget for the
  whole file — charged each frame's bytes AND a fixed 64 bytes for every language frame (empty
  ones too) and every value; at most 256 language frames in a tag, 1,024 in a file. Until revision
  12 only text was charged ("at most 1 MiB held" was false: a million empty frames, 52 MB). An
  empty frame is passed over, as lofty passes over it. The budget applies only when there is
  something to merge — one large `TLAN` is never read here. lofty's own
  read comes FIRST (it holds each frame whole, up to its 16 MiB allocation limit) and its save
  of an MP3/WAV/AIFF reads the whole file — this guard cannot bound either.
- **The documented-test-count check reads labelled totals only** (revision 11): "Total: N
  tests", "N tests passing", "(N tests;", "sum to N", "M with --all-features" on such a line,
  "(sum M)", and `cargo test --workspace [--all-features]  # N tests` comments. Reword a total
  out of those forms and the check FAILS ("states no total") — on purpose.
  `scripts/test-check-doc-test-counts.sh` proves it fails where it must; CI runs it.

## tokio: `timeout` alone does not kill a child process

`tokio::time::timeout` around `Command::output()` unblocks the caller but leaves the child
**running**. `.kill_on_drop(true)` is required for the dropped future to SIGKILL it. Every
subprocess call in this workspace (ReplayGain's ffmpeg, codecs' ffprobe and mediainfo) sets
both. Note `kill_on_drop` is a hard SIGKILL — no cleanup runs.

## How the owner wants us to work (see CLAUDE.md "Standing rules")

- **Plain English** in every report or explanation — no jargon.
- **Handoff updated as work lands** — it is what a fresh session (or a stand-in AI) reads.
- **One working branch, one eventual PR** (`feature/work-in-progress`). No PR stacking.
- **Cross-AI review**: build with Claude, review with Codex (and vice versa); loop until clean.
- **Planning** with Opus agents in sequence; **implementation** with Sonnet/Haiku (Opus if complex).
- **AI fallback**: if one AI service runs out, hand over via the handoff, return to the main one
  as soon as possible, then do a full review.
- Codex/OpenAI mirror of these notes lives in `.OpenAI/`; `AGENTS.md` at the root points Codex there.
