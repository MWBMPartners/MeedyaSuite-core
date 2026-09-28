# AGENTS.md — MeedyaSuite-core

Entry point for Codex and other AI coding agents. Claude Code reads `.claude/CLAUDE.md`;
this file points every other agent at the same rules so the two never drift.

**Read, in order:**

1. [`.claude/CLAUDE.md`](.claude/CLAUDE.md) — conventions, **standing rules** and standing tasks (these apply to you too).
2. [`.claude/HANDOFF.md`](.claude/HANDOFF.md) — where the last session left off. **§0 is the current position.**
3. [`.OpenAI/CONTEXT.md`](.OpenAI/CONTEXT.md) and [`.OpenAI/MEMORY.md`](.OpenAI/MEMORY.md) — Codex-side notes.

**The essentials, in case you read nothing else:**

- Work only on branch `feature/work-in-progress`. One branch, one eventual PR, into `main`. Never open extra PRs.
- `export PATH="$HOME/.cargo/bin:$PATH"` then `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo test --workspace --all-features --locked`. The branch gets **no CI**; these are the gate.
- Never write a test count you have not just measured.
- Report back in plain English, no jargon.
- Update `.claude/HANDOFF.md` as each piece of work lands.

**Languages, tracks, subtitles and lyrics — mandatory:** any work touching BCP 47 language
tags, languages, translations, audio or subtitle tracks, lyrics, track order or naming,
language preferences, or accessibility roles (SDH, audio description, forced, commentary)
MUST read and follow [`docs/standards/media-language-bcp47-policy.md`](docs/standards/media-language-bcp47-policy.md)
(policy `MWBM-MEDIA-LANG`). It is normative and is not repeated here. Its test cases,
[`tests/fixtures/bcp47-language-policy-v1.json`](tests/fixtures/bcp47-language-policy-v1.json),
must pass. **This repository holds the master copy** that other repositories copy and check;
changing it follows the policy's "Changing this policy" section.
