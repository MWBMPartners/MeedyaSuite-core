# Codex / OpenAI — Durable Project Facts

> Codex-side mirror. The **canonical** long-form facts are in [`../.claude/MEMORY.md`](../.claude/MEMORY.md) —
> read that; this file only adds what matters specifically when Codex is doing the work.
> Last updated: 2026-10-04.

## Role of Codex in this project

- **Primary job: independent reviewer.** Claude Code plans and builds; Codex reviews.
  Findings are fixed, then Codex reviews again, until a review comes back clean
  (standing rule 5 in `.claude/CLAUDE.md`).
- **Stand-in builder** when Claude Code is unavailable or out of usage (standing rule 12).
  When that happens: work from `.claude/HANDOFF.md`, keep it updated as you go, and expect a
  full Claude review of your work once Claude is back.

## Facts that trip up a new agent

- `cargo` is not on the default PATH on the owner's Mac: `export PATH="$HOME/.cargo/bin:$PATH"`.
- CI runs only for `main`, so the working branch has **no CI**. Run fmt, clippy (`-D warnings`)
  and the full test suite locally before every push.
- Doc test counts are guarded by `scripts/check-doc-test-counts.sh` and must match what cargo
  measures, in README.md, docs/API.md, .claude/CLAUDE.md and .claude/CONTEXT.md.
- Some code looks wrong but is deliberate — do not "fix" it without reading `.claude/MEMORY.md`:
  rate limiters keyed by host not provider; MusicBrainz `per_second(1)`; ISRC compact vs ISWC
  dotted query forms; `primary_tag_type()` as the lofty fallback; `kill_on_drop` on every subprocess;
  the tempo detector's "preference only breaks ties, never raises confidence" rule.
- Two tag libraries (`mp4ameta` and `lofty`) coexist on purpose. Do not unify them.
- On `feature/bcp47-language-policy`, several `tag_io` writes are **refused on purpose** rather
  than allowed to lose data: ID3v2 language frames that cannot be merged safely (including an
  old `TLA` frame inside an ID3v2.4 tag, and language frames spread over more than one ID3v2
  tag or ID3 chunk — since revision 10, ANY language frame in a file with two or more such tags),
  and — since revision 9 — any M4A save whose checked copy (made on a temporary file and
  compared with the original: its tags atom by atom in `mp4_save_check`, and since revision 10
  the whole file — audio, chunk offsets, every other atom — in `mp4_file_check`) would change
  anything not asked for or not store exactly what the caller gave (#102, #103). Fragmented
  M4A files, a `meta` with no tag list, and fields with no M4A atom are refused before anything
  is written. That refuses most iTunes / Apple Music files (and any whose disc number mutagen
  wrote) today; it is the interim guard, not a bug. The two public language steps
  must be taken in order —
  `recover_languages_after_reading` right after reading, `gather_languages_before_saving` right
  before saving. Details: `.claude/MEMORY.md`, "lofty: several languages".

