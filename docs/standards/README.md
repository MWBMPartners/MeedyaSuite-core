<!--
  docs/standards/README.md
  Copyright (c) 2026 MeedyaSuite. Licensed under the MIT License.
-->

# Standards

Normative documents shared by every MWBM / MeedyaSuite application. The
copies here are the **masters**; other repositories keep exact copies and
check them in CI.

| Standard | Version | Document | Test cases | Data |
|---|---|---|---|---|
| Media Language & BCP 47 Policy (`MWBM-MEDIA-LANG`) | 1.0.0 | [media-language-bcp47-policy.md](media-language-bcp47-policy.md) | [`tests/fixtures/bcp47-language-policy-v1.json`](../../tests/fixtures/bcp47-language-policy-v1.json) | [`data/bcp47-language-data-v1.json`](data/bcp47-language-data-v1.json) |

## Using a standard in another repository

1. Copy the files with the checker, pinned to a full commit of this repository:

   ```sh
   python3 scripts/media-lang/check_copies.py --init <40-character commit> \
     --file docs/standards/media-language-bcp47-policy.md=docs/standards/media-language-bcp47-policy.md \
     --file tests/fixtures/bcp47-language-policy-v1.json=tests/fixtures/bcp47-language-policy-v1.json \
     --file tests/fixtures/bcp47-language-policy-v1.schema.json=tests/fixtures/bcp47-language-policy-v1.schema.json \
     --file docs/standards/data/bcp47-language-data-v1.json=docs/standards/data/bcp47-language-data-v1.json \
     --file docs/standards/data/bcp47-language-data-v1.schema.json=docs/standards/data/bcp47-language-data-v1.schema.json \
     --file scripts/media-lang/check_copies.py=scripts/media-lang/check_copies.py
   ```

   (Get `check_copies.py` itself first, from this repository at that commit.)
   Local paths on the left may differ to suit the repository's layout.

2. Run `python3 scripts/media-lang/check_copies.py` in the repository's existing CI.
   It fails if a copy is edited or does not match the master.
3. Point the repository's agent-instruction files (`AGENTS.md`, `CLAUDE.md`,
   `.claude/`, `.OpenAI/`, `GEMINI.md` …) at the local copy of the policy — a
   pointer, not a pasted copy of the rules.
4. Run the test cases the repository's profile needs (policy section 8.1) in CI.

## Changing a standard

Follow the standard's own "Changing this policy" section. In short: a change
that alters any expected answer is a new **major** version, and the document,
its changelog, the test cases and every implementation change together.
