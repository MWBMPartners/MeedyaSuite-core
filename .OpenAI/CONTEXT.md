# Codex / OpenAI — Project Context

> Codex-side mirror. The **canonical** architecture snapshot is [`../.claude/CONTEXT.md`](../.claude/CONTEXT.md),
> and the live state of work is [`../.claude/HANDOFF.md`](../.claude/HANDOFF.md) §0. This file is a short
> orientation so Codex can start without reading everything. Last updated: 2026-09-23.

## What this is

A Rust library (10 crates, a "workspace") shared by the MeedyaSuite apps — MeedyaDL,
MeedyaManager, MeedyaConverter and others. It has **no web server and no web API**; partner apps
use it as a code dependency. Its contract with those apps is [`docs/API.md`](../docs/API.md).

## Where things stand (2026-09-23)

- Working branch `feature/work-in-progress`, well ahead of `main`, **no PR open yet**. The PR will target `main` (owner-confirmed 2026-09-23).
- Latest work: new crate `meedya-audio-analysis` (tempo and musical key detection, issue #16),
  built and reviewed by Claude on 2026-09-09. **A Codex review of that work is the next step.**
- Full status board and open questions: `.claude/HANDOFF.md` §0.
