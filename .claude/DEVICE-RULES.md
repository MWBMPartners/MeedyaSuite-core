# Device-level rules (copy onto each machine)

> The owner asked (2026-09-23) for the **AI fallback** rule to apply to *every* project on
> the device, not just this repo. A repo file cannot do that — it has to live in each AI
> tool's per-user settings on the machine itself. Paste the block below into:
>
> - **Claude Code**: `~/.claude/CLAUDE.md`
> - **Codex**: `~/.codex/AGENTS.md`
> - Any other AI coding tool: its user-level instructions file.
>
> Cloud sessions (claude.ai/code) start from a fresh machine each time, so a copy written
> there does not survive — the owner's own computer is where this needs to go.

```markdown
## AI fallback (applies to every project)

If an AI service (Claude Code, Codex, or any other) or its agents become unavailable or run
out of usage, hand the work to another suitable one — as long as that can be done without
losing context or progress. Switch back to the main service for the project as soon as it is
available again, and when it is, run a FULL review of the work done in its absence.
Cross-AI reviews catch differences in approach between services, which is what makes this
safe. It also means the project's handoff document must be kept up to the minute at all
times, because that is what the stand-in service reads.

## Plain English

When reporting back or explaining, use plain, everyday English — no technical jargon.
```
