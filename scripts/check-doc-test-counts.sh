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
    # shellcheck disable=SC2086
    cargo test --workspace $1 --locked --no-fail-fast 2>&1 \
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
    DEFAULT=$(sum_passing "")
    echo "  measured: ${DEFAULT} default-features, ${ALL} --all-features"
fi

status=0

# Substring checks rather than a parse: deliberately tolerant of prose
# rewording, but still catches a figure that has gone stale.
assert_contains() {
    local file="$1" value="$2" label="$3"
    if ! grep -qF -- "$value" "$file"; then
        echo "STALE: $file does not mention the measured ${label} count '${value}'"
        status=1
    fi
}

for f in README.md docs/API.md .claude/CONTEXT.md .claude/CLAUDE.md; do
    [ -f "$f" ] || continue
    assert_contains "$f" "$ALL" "--all-features"
    assert_contains "$f" "$DEFAULT" "default-features"
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
