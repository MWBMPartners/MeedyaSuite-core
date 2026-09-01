# MeedyaSuite-core — Session Handoff

> **Purpose**: pick up exactly where the last session left off, without re-deriving anything.
> **Read order at session start**: [CLAUDE.md](CLAUDE.md) → this file → [CONTEXT.md](CONTEXT.md).
> **Update rule**: amend this file *as work lands*, not at the end. If a session is interrupted, this file is the only thing that survives.

**Last updated**: 2026-09-01
**Active branch**: `feature/work-in-progress`
**Eventual PR target**: `main` (owner-confirmed 2026-09-01)

---

## 1. Ground truth (measured, not asserted)

Numbers here were **measured by running cargo**, not copied from docs. Docs across the repo
have historically disagreed (248 / 466 / 653 / 664 all appear somewhere and are all wrong).

| Fact | Value | How verified |
|---|---|---|
| Tests on `main` | **601 passing, 0 failing** | `cargo test --workspace --all-features --locked` |
| Tests on `feature/work-in-progress` | **555 default-feature / 688 `--all-features`, 0 failing** | same command |
| `cargo fmt --all -- --check` | clean on both | run directly |
| `cargo clippy --workspace --all-targets --all-features` | 4 unique warnings, all **pre-existing** | run directly |
| `cargo build --workspace --all-features --locked` | 0 errors | run directly |
| Workspace crates | 9 | `ls crates` |
| Web/HTTP-server/OpenAPI surface | **none** | no axum/actix/warp/rocket/utoipa anywhere; `bindings/` is 2 README stubs |

### Toolchain note (important)

**Rust was not installed on this machine** at session start. Installed via rustup:
`~/.cargo/bin`, stable **1.98.0** (same version MeedyaDL pins). `cargo` is **not on the
default PATH** — every command needs:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

CI only triggers on `pull_request`/`push` to `main`, so **a WIP branch gets no CI at all**.
Local verification is the only gate until the PR is opened.

---

## 2. Branch consolidation — COMPLETE

### What was done

Four WIP branches were audited by **full file content** (not commit messages) and
consolidated into a single branch, `feature/work-in-progress`, based on `main`.

| Former branch | Disposition | Evidence |
|---|---|---|
| `claude/branch-audit-musicbrainz-migration-l5h8zh` | merged | ancestor of WIP branch |
| `claude/issue-65-identifier-registry` | merged (was already contained in the above) | ancestor |
| `fix/musicbrainz-lucene-hardening` | merged as **semantic union** | ancestor |
| `feat/60-syllable-schema-and-classifier` | **not merged — nothing to merge** | see below |

All four remote branches were **deleted**. Archive tags were pushed first so no commit
became unreachable:

```
archive/branch-audit-musicbrainz-migration  -> dd3dff5
archive/issue-65-identifier-registry        -> fd2a7c5
archive/musicbrainz-lucene-hardening        -> bc13f21
archive/feat-60-syllable-schema             -> 21a20b3
```

Recover any of them with `git checkout -b <name> archive/<tag>`.

### The critical finding — why a plain merge would have lost work

The two MusicBrainz branches were **divergent siblings, not superset/subset**. Each had
dropped features the other introduced. Taking either side wholesale would have silently
regressed real functionality.

**Only on `fix/musicbrainz-lucene-hardening`** (would have been lost by taking the audit branch):
- 30s reqwest timeout on the MusicBrainz / ISRC / ISWC clients
- `ProviderResult.musicbrainz_id` population
- Genre extraction from MB `genres`/`tags`, highest-vote first (refs #73)
- `album` → `release:"…"` and `year` → `date:NNNN` query-narrowing clauses

**Only on `claude/branch-audit-…`** (would have been lost by taking the fix branch):
- `strip_trailing_bracket_groups()` recall helper
- ISRC normalisation + 12-char validation
- Explicit error when no searchable field is present
- `validate_isrc` ASCII-only fix — a **real panic** on non-ASCII alphanumeric input
- Solr-10 forward-compat response-shape tests
- The entire #65 identifier-types registry

All 21 at-risk features were verified present after the merge by explicit grep audit.
`main` was the weakest baseline of all three (naive `title.replace('"', "")`, no timeout,
no `lucene.rs`).

### `lucene.rs` — API unified

The branches exposed **incompatible public APIs** for the same module:
`lucene_escape`/`lucene_phrase_clause` vs `escape_lucene`/`quote_phrase`. Neither had
shipped in a release (the module doesn't exist on `main`), so they were unified on one
correct API. The distinction now reflects **actual Lucene semantics**, which both branches
had conflated:

| Helper | Escapes | Use for |
|---|---|---|
| `escape_lucene(v)` | all 19 Lucene specials | **bare / unquoted** terms |
| `quote_phrase(v)` | only `\` and `"` | **inside a quoted phrase** |
| `phrase_clause(f, v)` | delegates to `quote_phrase` | a whole `field:"value"` clause |

Inside a quoted phrase Lucene treats `( ) : + - ?` as literal text — escaping them there
would make the backslashes part of the searched string. Outside a phrase they are
operators and must be escaped. The MusicBrainz docs' own `AC/DC` example escapes the
slash for exactly this reason.

Exported from `meedya-providers`: `pub use lucene::{escape_lucene, phrase_clause, quote_phrase};`

### `feat/60` — fully accounted for, nothing lost

Its feature work (#60) landed via PR #62; its agent configs via PR #67. Its only *additive*
unique content was `.claude/settings.local.json` — machine-local Claude Code permission
grants containing absolute `/Users/...` paths, which must never be committed. That file is
now in `.gitignore`. The branch also carried a **deletion** of `.github/workflows/lint.yml`,
deliberately **not** taken: that workflow is the actionlint CI added later by PR #64, and
the branch simply predates it.

### `alpha` / `beta` — NOT touched

Both are **ancestors of `main`** (75 commits behind, zero unique commits) — effectively
stale release pointers. They were outside the audit scope and have been left alone.

> **Open recommendation for the owner**: fast-forward `alpha` and `beta` to `main`, or
> delete them, so they stop reading as live branches. Not done — needs an explicit call.

---

## 3. Decisions taken by the owner this session

| Question | Decision |
|---|---|
| PR target for the consolidated branch | **`main`** |
| Scope of MusicBrainz code changes | **This repo only** — no code changes in MeedyaDL |
| GitHub issues for the MeedyaDL→core MusicBrainz migration | **Log in BOTH repos** (revised mid-session; supersedes the earlier "core only, no MeedyaDL issues") |
| OpenAPI / Swagger UI | **Skip** — no web surface exists in this workspace |
| PR strategy | **One branch, one eventual PR.** No PR stacking. |

---

## 4. MusicBrainz Solr 10 migration (2026-11-30)

Source: <https://blog.metabrainz.org/2026/08/31/search-upgrades-nov-30-2026/>

### Ticket inventory

**Breaking**: SEARCH-444 (relation-list in area/URL JSON), SEARCH-642 (drop `id` for
cdstub/tag), SEARCH-666 (release `quality`: names replace numeric IDs), SEARCH-752
(relationships lose the redundant `target` property), SEARCH-764 (Solr 9 → 10).

**New/changed**: SEARCH-680 (genre annotations in annotation search), SEARCH-681 (new
"Genre" search target type), SEARCH-751 + SEARCH-753 (`target-type` added to relationships
in event/work/area/URL JSON).

**Non-breaking**: SEARCH-452 (index all URL relationships), SEARCH-646 (exact-match
priority for tag search), SEARCH-677 (disambiguation for event places / work recordings).

### Our exposure (verified by reading code, not inferred)

- `crates/meedya-providers/src/providers/iswc.rs` **does parse `relations`** (`MbRelation`)
  → in the SEARCH-752/751/753 blast radius. It reads `type` and the entity-specific
  `artist` object and **never reads `target`**, so it is already forward-compatible. A
  forward-compat test for the new relation shape is present.
- `musicbrainz.rs` / `isrc.rs` use the **search** endpoint, which does not return
  relationships → not exposed to SEARCH-752.
- We do **not** read release `quality` (SEARCH-666) or cdstub/tag `id` (SEARCH-642).
- Forward-compat response-shape tests exist for `musicbrainz.rs` and `isrc.rs`, asserting
  that Solr-10-shaped noise (`target-type`, string `quality`, `release-group`, `genres`)
  parses identically.

### RESOLVED: ISWC query form (was "deliberately unresolved")

Settled by **live probing** `musicbrainz.org/ws/2/` on 2026-09-01, since MusicBrainz
documents the indexed form for neither identifier:

| Field | Query form | Live result |
|---|---|---|
| ISRC | `isrc:GBAYE0601498` (compact) | **matches** |
| ISRC | `isrc:GB-AYE-06-01498` (hyphenated) | 0 results |
| ISWC | `iswc:"T-304.031.869-8"` (dotted display form) | **matches** |
| ISWC | `iswc:T3040318698` (compact) | 0 results |
| ISWC | `iswc:"T-304031869-8"` (hyphen-only) | **parse error** |

This caught a real bug: **both** prior branches were wrong about ISWC, and the
consolidation had inherited the audit branch's hyphen-only form — a parse error in
production. `format_iswc_dotted` now reformats to the stored display form; all three input
forms converge on the working query, verified end-to-end live.

ISRC and ISWC are therefore **deliberately asymmetric**, because MusicBrainz indexes them
asymmetrically. Evidence is recorded in `docs/API.md`, `.claude/HISTORY.md`, and as a
comment on issue #69 with the post-cutover re-check list.

### Solr-10 ticket audit — only one ticket affects us

**SEARCH-764** (Solr 9→10) affects us, and only indirectly: the risk is our own query
construction under a stricter parser, not any response change. Verified **not** applicable:
SEARCH-444, -642, -666, -752, -751, -753, -680, -681, -452, -646, -677 — we never call the
area/url/cdstub/tag/annotation endpoints, never read relationship `target` or release
`quality`, and no response struct uses `deny_unknown_fields`.

Solr 10's upgrade notes document no query-parser changes visible to API clients, so **one
conservative code path is valid on both sides of the cutover**. No runtime switching is
needed — or possible: there is no version-negotiation header, parameter, or response field.

> **Fetch note**: `tickets.metabrainz.org` HTML is behind Anubis anti-bot protection. The
> JIRA REST API works: `https://tickets.metabrainz.org/rest/api/2/issue/SEARCH-<n>`.

---

## 5. Incidental findings (real, verified, not yet fixed)

These were found while reading the code and are **not** covered by the consolidation:

1. **`rate_limiter.rs` is dead code.** `crates/meedya-providers/src/rate_limiter.rs` exists
   and configures `("musicbrainz", 50)`, but **no provider calls it**. MusicBrainz's ToS is
   ~1 req/sec; core currently issues unthrottled requests. (Verified: grep for
   `RateLimiter|acquire` across `src/providers/` returns nothing.)

2. **10+ providers have no HTTP timeout.** These use bare `Client::new()`:
   `tmdb`, `spotify`, `deezer`, `omdb`, `apple_music`, `itunes_store`, `apple_tv`,
   `apple_podcasts`, `thetvdb`, `eidr`. Issue #15 covers only AcoustID — the gap is
   workspace-wide. (`musicbrainz`, `isrc`, `iswc` now have 30s timeouts via this merge.)

3. **`firstreleasedate` exists as a MusicBrainz recording search field** — "the release
   date of the earliest release including this recording". Directly relevant to issue #74
   (earliest-dated release selection); we currently use `date`.

4. **Pre-existing build warning**: unused import `MEEDYA_NAMESPACE` at
   `crates/meedya-tags-extended/src/mik.rs:43`.

---

## 6. MeedyaDL ↔ core MusicBrainz consolidation (owner-requested, issues in both repos)

**Intent**: the MusicBrainz *lookup mechanism* becomes central in MeedyaSuite-core so every
Meedya app shares one implementation. App-specific workflow stays in the app.

**Core cannot absorb MeedyaDL today.** Verified capability gap:

| MeedyaDL needs | Core status |
|---|---|
| `/recording?query=isrc:…` search | present (only overlap) |
| `/recording/{id}?inc=url-rels+recording-rels` entity lookup | **absent** — `lookup()` not overridden, returns `NotSupported` |
| Relationship parsing → external/video URLs | **absent** in `musicbrainz.rs` |
| URL search (`url:"…"` exact + `url:*tail` wildcard) | **absent** |
| Platform URL classification | **absent** |
| Rate limiting | present but **not wired** (see §5.1) |

MeedyaDL has made **no** Solr-10 changes (no such commits) and builds queries by raw
interpolation at `musicbrainz_service.rs:124`, `:533`, `:578`. It *does* already read
`target-type` at `:809`, so its relation parsing is forward-compatible.

**Recommended sequence** (do not skip step 2):
1. ✅ Consolidate + harden core's MB *search* path — done this session.
2. Grow core: `lookup()` with `inc=` includes, URL search, a `target-type`-first relation
   model that tolerates the legacy shape, wire the rate limiter. **No MeedyaDL change.**
3. Oct–Nov, pre-cutover: MeedyaDL swaps `musicbrainz_service.rs` *internals* to delegate to
   core, keeping its public fn signatures as a thin adapter.
4. Post-2026-11-30: validate against live Solr 10 (issue #69).

**Must NOT move into core** (app-specific, violates CLAUDE.md principle #4):
`rewrite_apple_music_storefront`, activity-log emission, progress staging.

---

## 7. Status board

| Task | State |
|---|---|
| Branch audit + consolidation | ✅ Complete — merged, pushed, old branches deleted, archive tags pushed |
| `feat/60` accounted for | ✅ Complete — gitignore commit |
| MusicBrainz Solr-10 hardening (search path) | ✅ Landed via consolidation |
| MusicBrainz Solr-10 audit | ✅ Complete — only SEARCH-764 applies; ISWC form fixed |
| MSRV declared (`rust-version = "1.82"`) | ✅ Complete — workspace + all 9 crates |
| GitHub issue sweep (62 issues verified vs code) | ✅ Complete — 7 closed, 13 updated, 12 relabelled |
| Documentation sweep | ✅ Complete — README, CLAUDE.md, CONTEXT.md, API.md, HISTORY.md |
| MeedyaDL + core migration issues (both repos) | ✅ Complete — core #75, MeedyaDL #1119 |
| New-work proposals | ✅ Complete — 18 proposals, issues #78–#94 opened |
| Claude MEMORY.md / CLAUDE.md / CONTEXT.md / HISTORY.md | ✅ Complete |
| OpenAPI / Swagger | ⛔ N/A by owner decision — no web surface |

---

## 8. Commits on `feature/work-in-progress` (beyond `main`)

```
b201de6 docs: sync all documentation to measured reality after consolidation
05bc5b4 fix(meedya-providers): query ISWC in MusicBrainz's dotted display form
7e3692c docs(api): document unified lucene API + measured test counts
795ad77 docs(claude): add HANDOFF.md session-resumption document
c84a003 chore(git): ignore .claude/settings.local.json
5dc94ce merge: consolidate fix/musicbrainz-lucene-hardening (semantic union)
d15102c merge: consolidate claude/branch-audit-musicbrainz-migration into work-in-progress
dd3dff5 docs(context): fix stale --all-features test count (653 -> 664) in build snippet
ee53944 feat(meedya-providers): strip trailing (…)/[…] before MusicBrainz phrase query
8836967 fix(meedya-providers): prevent validate_isrc panic on non-ASCII alphanumeric input
bc13f21 feat(providers): populate ProviderResult.genre from MusicBrainz results
2d847b0 feat(providers): add request timeouts, populate musicbrainz_id, album/year in MB query
a7354d3 fix(providers): Lucene-escape & phrase-quote MusicBrainz search queries
97ba626 feat(meedya-providers): harden MusicBrainz search queries for the Solr 9->10 upgrade
f72e79a fix(meedya-metadata): complete #65 — reserve GRid/ICPN, per-scheme normalisation
fd2a7c5 feat(metadata): identifier-types registry + CommonTag expansion (#65)
```

---

## 9. If you are resuming cold

```bash
cd "/Users/lance.manasse/Projects/Coding & Development/MWBM Partners Ltd/GitHub/MeedyaSuite/MeedyaSuite-core"
export PATH="$HOME/.cargo/bin:$PATH"
git checkout feature/work-in-progress && git pull
cargo test --workspace --all-features --locked   # expect 688 passing, 0 failing
```

Then read §7 for what is outstanding.

---

## 10. Issue state after the 2026-09-01 verification sweep

All 62 issues (open + closed) were checked against **actual file contents**, not commit
messages or docs.

**Closed as verified-complete** (7): #6 (bundled `tags.toml`), #7 (duplicate of #2), #8
(config-driven registry), #9 (codec detection), #13 (`extract_json_value` hardening), #14
(credential storage — all four fixes present; the file moved to `meedya-providers`), #15
(AcoustID timeout).

**Updated with corrected state** (13): #2, #4, #5, #11, #17, #45, #52, #53, #61, #68, #69,
#73, #74. **Relabelled** (12). **No closed issue needed reopening** — 21 were spot-verified
as genuinely done.

### New issues opened this session

| # | Subject |
|---|---|
| #75 | Core absorbs the shared MusicBrainz lookup mechanism (**blocks** MeedyaDL#1119) |
| #76 | 11 HTTP clients with no timeout (workspace gap left by #15) |
| #77 | Three divergent copies of ISRC normalisation |
| #78–#94 | The 18 ranked review proposals (rank 4 folded into #76) |
| MeedyaDL#1119 | MeedyaDL delegates its MusicBrainz service to core |

> **Correction on #76**: its original body listed `meedya-lyrics/lrclib.rs` as already
> covered. That was an *inference* from it using `Client::builder()` rather than a reading —
> it has **no** timeout. Corrected in a comment; the real count is 11, not 10.

---

## 11. Top recommendations (awaiting your decision)

Ranked by value-for-effort. Nothing here has been implemented — these are proposals.

1. **#78** — `d[..4.min(d.len())]` year extraction panics on multi-byte UTF-8 dates, in
   **11 provider parsers**. Same bug class as the `validate_isrc` panic already fixed. `S`
2. **#79** — insert-then-`unwrap()` panics when lofty silently refuses an unsupported tag
   type. Tagging a freshly-downloaded untagged M4A is a first-class MeedyaSuite flow. `S`
3. **#80** — API keys leak into error text: `reqwest`'s `Display` appends the full URL
   including secret query params, so keys reach logs and bug reports. `S`
4. **#81** — ReplayGain's FFmpeg subprocess has no timeout; one bad file freezes an album
   scan. `S`
5. **#94** — `rate_limiter.rs` is fully built and wired to **nothing**. First real batch run
   risks getting partner apps throttled or banned. `M`

Full ranked list with evidence is in each issue body.
