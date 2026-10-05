#!/usr/bin/env bash
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# Guard the documented test counts against the real suite (issue #71).
#
# This repo's chronic failure mode is doc-count drift: 248, 466, 533, 546,
# 601, 653 and 664 have all appeared across README/CLAUDE.md/CONTEXT.md/API.md
# at various points, and none matched the code. The counts are a contract
# signal partner apps read, so a silently wrong number is worse than none.
#
# Usage:
#   scripts/check-doc-test-counts.sh                  # measures the suite itself
#   scripts/check-doc-test-counts.sh <default> <all>  # uses counts you already have
#
# The two-argument form exists so CI does not run the suite a third time —
# the build already runs it twice (default and --all-features), so the counts
# are captured there and passed in.
#
# Exit 0 when every documented total matches; 1 otherwise, naming the file.
set -euo pipefail

cd "$(dirname "$0")/.."

sum_passing() {
    # One `test result:` line per test binary (unit, integration, doc), so the
    # workspace total is their sum — not the last line.
    #
    # cargo's own exit status is captured explicitly, and a failing run stops
    # the script with a message: counting the tests that passed in a run
    # where some FAILED would compare a meaningless number. (The pipe used to
    # run straight from cargo into grep and awk. `set -o pipefail` above
    # already made a failure stop the script, but silently - checked with a
    # stand-in cargo that exits 101 - and the CI step that copied this
    # function had no pipefail at all, so there a failing run was counted as
    # if it had passed. Found by Codex's review r7.)
    local out status=0
    out=$(cargo test --workspace "$@" --locked --no-fail-fast 2>&1) || status=$?
    if [ "$status" -ne 0 ]; then
        printf '%s\n' "$out" | tail -n 40 >&2
        echo "FAILED: cargo test --workspace $* exited with status ${status}, so its tests" \
             "were not counted. Fix the failing tests first." >&2
        return "$status"
    fi
    printf '%s\n' "$out" \
        | grep -E '^test result' \
        | awk -F'[ ;]' '{p += $4} END {print p + 0}'
}

if [ "$#" -eq 2 ]; then
    DEFAULT="$1"
    ALL="$2"
    echo "Using supplied counts: ${DEFAULT} default-features, ${ALL} --all-features"
else
    echo "Measuring the real suite (compiles and runs it twice)..."
    ALL=$(sum_passing --all-features)
    DEFAULT=$(sum_passing)
    echo "  measured: ${DEFAULT} default-features, ${ALL} --all-features"
fi

# A count is a whole number, and nothing else: anything else here is a
# mistake in the caller, and matching it against the documents below
# would prove nothing.
for count in "$DEFAULT" "$ALL"; do
    case "$count" in
        '' | *[!0-9]*)
            echo "FAILED: '${count}' is not a whole number, so it cannot be checked." >&2
            exit 1
            ;;
    esac
done

status=0

# Only the totals each document states AS totals are compared - never any
# number that happens to appear in it. (Until Codex's catch-up review of
# revisions 8-10, finding 9, any whole number anywhere in a document passed:
# `scripts/check-doc-test-counts.sh 1 4` passed, because numbered steps and
# version numbers hold a 1 and a 4 in every one of these files, and a
# per-crate count passed as the total.) The forms read, each a total in
# words, are:
#
#   default features: "Total: N tests", "N tests passing", "(N tests;",
#                     "sum to N", and a "cargo test --workspace  # N tests"
#                     comment;
#   --all-features:   "M with --all-features" (the flag in backticks or not)
#                     on a line that states one of the default totals above,
#                     "(sum M)", and a "cargo test --workspace --all-features
#                     # M tests" comment.
#
# Every such number in a document must be the measured one, and each
# document must state at least one total of each kind - so a total that is
# reworded out of these forms fails loudly, rather than going unchecked.
# A per-crate figure ("12 (17 with --all-features)" in a table row) is on a
# line stating no total, so it is never read as one.
# `scripts/test-check-doc-test-counts.sh` proves this fails where it must.
DEFAULT_TOTAL='Total:\**[[:space:]]*[0-9]+ tests|[0-9]+ tests passing|\([0-9]+ tests;|sum to [0-9]+|cargo test[[:space:]]+--workspace[[:space:]]+#[[:space:]]*[0-9]+ tests'
ALL_TOTAL='\(sum [0-9]+\)|cargo test[[:space:]]+--workspace[[:space:]]+--all-features[[:space:]]+#[[:space:]]*[0-9]+ tests'
ALL_BESIDE_A_TOTAL='[0-9]+\**[[:space:]]+with[[:space:]]+`?--all-features'

# The numbers `file` states as its `kind` total ("default" or "all"), one a
# line; nothing when it states none.
stated_totals() {
    local file="$1" kind="$2"
    {
        if [ "$kind" = default ]; then
            grep -oE -- "$DEFAULT_TOTAL" "$file" || true
        else
            grep -oE -- "$ALL_TOTAL" "$file" || true
            { grep -E -- "$DEFAULT_TOTAL" "$file" || true; } \
                | { grep -oE -- "$ALL_BESIDE_A_TOTAL" || true; }
        fi
    } | { grep -oE '[0-9]+' || true; }
}

# Compares what `file` states as its `kind` total (`label` in messages)
# with the measured `value`.
check_stated() {
    local file="$1" kind="$2" value="$3" label="$4" stated n
    stated=$(stated_totals "$file" "$kind")
    if [ -z "$stated" ]; then
        echo "STALE: $file states no ${label} total in a form this check reads (see the top of this script)"
        status=1
        return
    fi
    for n in $stated; do
        if [ "$n" != "$value" ]; then
            echo "STALE: $file states the ${label} total as '${n}'; measured '${value}'"
            status=1
        fi
    done
}

# Where the documents are: the repository, unless the self-test points this
# at changed copies of them.
DOCS="${DOC_COUNTS_ROOT:-.}"
for f in README.md docs/API.md .claude/CONTEXT.md .claude/CLAUDE.md; do
    # Every one of these documents states the totals, so one that is not
    # there is a failure, never a pass. (Until Codex's review of revision
    # 11, finding 4, a missing document was skipped without a word:
    # `DOC_COUNTS_ROOT=/dev/null bash scripts/check-doc-test-counts.sh 1 4`
    # checked nothing at all and said the counts matched.)
    if [ ! -f "$DOCS/$f" ]; then
        echo "MISSING: $DOCS/$f is not there, so the totals it states cannot be checked"
        status=1
        continue
    fi
    check_stated "$DOCS/$f" all "$ALL" "--all-features"
    check_stated "$DOCS/$f" default "$DEFAULT" "default-features"
done

if [ "$status" -eq 0 ]; then
    echo "OK: documented counts match the measured suite (${DEFAULT} / ${ALL})."
else
    cat <<'MSG'

Doc-count drift detected. Do NOT guess the new number — measure it:

  export PATH="$HOME/.cargo/bin:$PATH"   # cargo is not on the default PATH here
  cargo test --workspace --all-features 2>&1 | grep -E '^test result' \
    | awk -F'[ ;]' '{p+=$4} END {print p}'

then write the number you just measured. Never extend a running total or
carry a previous edit's figure forward — that is exactly how the drift
this guard exists to catch accumulated in the first place.
MSG
fi

exit "$status"
