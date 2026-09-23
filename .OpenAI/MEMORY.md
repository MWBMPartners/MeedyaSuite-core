# Codex / OpenAI — Durable Project Facts

> Codex-side mirror. The **canonical** long-form facts are in [`../.claude/MEMORY.md`](../.claude/MEMORY.md) —
> read that; this file only adds what matters specifically when Codex is doing the work.
> Last updated: 2026-09-23.

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
