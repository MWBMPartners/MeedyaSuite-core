#!/usr/bin/env bash
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# Proves scripts/check-doc-test-counts.sh fails where it must.
#
# A check that cannot fail is not a check. Until Codex's catch-up review of
# revisions 8-10 (finding 9), the documented-count check accepted any
# number found anywhere in a document: `check-doc-test-counts.sh 1 4`
# passed, and so would a per-crate count given as the total. This runs the
# check against cases that must FAIL, and the right totals, which must
# pass - without running the test suite (it takes a second), so CI runs it
# before measuring.
#
# Usage: scripts/test-check-doc-test-counts.sh
# Exit 0 when every case comes out as it must; 1 otherwise, naming it.
set -euo pipefail

cd "$(dirname "$0")/.."

check=scripts/check-doc-test-counts.sh
wrong=0

# expect <pass|fail> <what is being shown> <command...>
expect() {
    local want="$1" what="$2" got did
    shift 2
    if "$@" >/dev/null 2>&1; then
        got=pass did=passes
    else
        got=fail did=fails
    fi
    if [ "$got" = "$want" ]; then
        echo "ok: ${what} - the check ${did}, as it must"
    else
        echo "WRONG: ${what} - the check must ${want}, but it ${did}"
        wrong=1
    fi
}

# The totals the documents state now, read from the README's own label.
# (The backticks below are the documents' own, matched as they are - not
# commands, so single quotes are meant: hence the shellcheck notes.)
DEFAULT=$(grep -oE 'Total:\**[[:space:]]*[0-9]+ tests' README.md | head -n 1 | grep -oE '[0-9]+')
# shellcheck disable=SC2016
ALL=$(grep -oE '[0-9]+ with `--all-features`, the CI configuration' README.md | head -n 1 \
    | grep -oE '^[0-9]+')
# A number the documents hold that is NOT a total: meedya-metadata's own
# count, from the per-crate list in docs/API.md.
# shellcheck disable=SC2016
CRATE=$(grep -oE '`meedya-metadata` [0-9]+' docs/API.md | head -n 1 | grep -oE '[0-9]+$')
for n in "$DEFAULT" "$ALL" "$CRATE"; do
    if [ -z "$n" ]; then
        echo "WRONG: could not read the documents' own totals to test with" >&2
        exit 1
    fi
done

expect pass "the totals the documents state (${DEFAULT} / ${ALL})" \
    bash "$check" "$DEFAULT" "$ALL"
expect fail "1 and 4, which numbered steps hold all over the documents" \
    bash "$check" 1 4
expect fail "a per-crate count (${CRATE}) given as the default-features total" \
    bash "$check" "$CRATE" "$ALL"
expect fail "a per-crate count (${CRATE}) given as the --all-features total" \
    bash "$check" "$DEFAULT" "$CRATE"
expect fail "the two totals the wrong way round" \
    bash "$check" "$ALL" "$DEFAULT"

# Changed copies of the documents, checked with the right totals.
copies=$(mktemp -d)
trap 'rm -rf "$copies"' EXIT
fresh_copies() {
    # `f` is this function's own: the callers' loops have theirs.
    local f
    rm -rf "${copies:?}"
    mkdir -p "$copies/docs" "$copies/.claude"
    for f in README.md docs/API.md .claude/CONTEXT.md .claude/CLAUDE.md; do
        cp "$f" "$copies/$f"
    done
}
# Replaces the first `old` with `new` in the copy of `file` (a fixed text,
# not a pattern), failing loudly if it is not there.
change() {
    local file="$1" old="$2" new="$3"
    OLD="$old" NEW="$new" perl -0pi -e 's/\Q$ENV{OLD}\E/$ENV{NEW}/' "$copies/$file"
    if ! grep -qF -- "$new" "$copies/$file"; then
        echo "WRONG: could not change '${old}' in the copy of ${file}" >&2
        exit 1
    fi
}

fresh_copies
expect pass "unchanged copies of the documents" \
    env DOC_COUNTS_ROOT="$copies" bash "$check" "$DEFAULT" "$ALL"

# No documents at all, and each document missing in turn, with the right
# totals: a document that is not there must fail, never be skipped (Codex's
# review of revision 11, finding 4 - both used to pass).
expect fail "no documents at all (DOC_COUNTS_ROOT=/dev/null)" \
    env DOC_COUNTS_ROOT=/dev/null bash "$check" "$DEFAULT" "$ALL"
for missing in README.md docs/API.md .claude/CONTEXT.md .claude/CLAUDE.md; do
    fresh_copies
    rm "$copies/$missing"
    expect fail "one document missing: ${missing}" \
        env DOC_COUNTS_ROOT="$copies" bash "$check" "$DEFAULT" "$ALL"
done

fresh_copies
change .claude/CLAUDE.md "# ${ALL} tests" "# $((ALL - 1)) tests"
expect fail "one stale total in one place: a command's comment in .claude/CLAUDE.md" \
    env DOC_COUNTS_ROOT="$copies" bash "$check" "$DEFAULT" "$ALL"

fresh_copies
change docs/API.md "which sum to ${DEFAULT}" "which sum to $((DEFAULT + 1))"
expect fail "one stale total in one place: the per-crate sum in docs/API.md" \
    env DOC_COUNTS_ROOT="$copies" bash "$check" "$DEFAULT" "$ALL"

fresh_copies
change README.md "**Total: ${DEFAULT} tests passing**" "**${DEFAULT} tests in all**"
change README.md "(${DEFAULT} tests; ${ALL} with --all-features)" "(see above)"
expect fail "a document that no longer states its totals in a form the check reads" \
    env DOC_COUNTS_ROOT="$copies" bash "$check" "$DEFAULT" "$ALL"

if [ "$wrong" -ne 0 ]; then
    echo "FAILED: scripts/check-doc-test-counts.sh does not fail where it must." >&2
    exit 1
fi
echo "OK: scripts/check-doc-test-counts.sh fails where it must, and passes the right totals."
