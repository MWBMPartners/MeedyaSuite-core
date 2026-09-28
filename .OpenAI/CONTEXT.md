# Codex / OpenAI — Project Context

> Codex-side mirror. The **canonical** architecture snapshot is [`../.claude/CONTEXT.md`](../.claude/CONTEXT.md),
> and the live state of work is [`../.claude/HANDOFF.md`](../.claude/HANDOFF.md) §0. This file is a short
> orientation so Codex can start without reading everything. Last updated: 2026-09-28.

## What this is

A Rust library (10 crates, a "workspace") shared by the MeedyaSuite apps — MeedyaDL,
MeedyaManager, MeedyaConverter and others. It has **no web server and no web API**; partner apps
use it as a code dependency. Its contract with those apps is [`docs/API.md`](../docs/API.md).

## Where things stand (2026-09-23)

- Working branch `feature/work-in-progress`, well ahead of `main`, **no PR open yet**. The PR will target `main` (owner-confirmed 2026-09-23).
- Latest work: new crate `meedya-audio-analysis` (tempo and musical key detection, issue #16),
  built and reviewed by Claude on 2026-09-09. **A Codex review of that work is the next step.**
- Separate branch in flight (from 2026-09-28): `feature/bcp47-language-policy`, the shared
  language policy MWBM-MEDIA-LANG (issue #99). Its latest round is revision 9, acting on the
  stand-in review of revision 8 (a fresh Opus agent standing in for Codex, which was out of
  allowance) — not yet reviewed itself. See `.claude/HANDOFF.md` §0.0.
- Full status board and open questions: `.claude/HANDOFF.md` §0.

## Languages, tracks, subtitles and lyrics (mandatory)

Work touching language tags, translations, audio or subtitle tracks, lyrics, track order or
naming, language preferences or accessibility roles MUST follow
[`docs/standards/media-language-bcp47-policy.md`](../docs/standards/media-language-bcp47-policy.md)
(policy `MWBM-MEDIA-LANG`) — the master copy lives in this repository. See the matching
section in [`.claude/CLAUDE.md`](../.claude/CLAUDE.md); the rules are not repeated here.
