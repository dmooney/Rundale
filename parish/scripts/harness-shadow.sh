#!/usr/bin/env bash
#
# harness-shadow.sh — run the GameTestHarness corpus in differential ("shadow")
# mode and summarize the divergence ledger.
#
# Drives the whole harness corpus with PARISH_HARNESS_SHADOW=1 so every
# GameTestHarness::execute also replays its input through the real
# parish_core::game_loop and records any divergence (#1159). The aggregated
# JSONL ledger is summarized into a Markdown report.
#
# Divergences are intentionally non-gating: they measure the current
# legacy-vs-real delta. Cargo compilation and test failures are gating and are
# returned to the caller after the partial ledger has been summarized.
#
# Usage:
#   bash parish/scripts/harness-shadow.sh [SUMMARY_MD]
#
# Env overrides:
#   PARISH_HARNESS_SHADOW_LEDGER   ledger JSONL path (default: target/harness-shadow-ledger.jsonl)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 1

MANIFEST="parish/Cargo.toml"
LEDGER="${PARISH_HARNESS_SHADOW_LEDGER:-$REPO_ROOT/parish/target/harness-shadow-ledger.jsonl}"
# Default summary lives under docs/proofs/ (committed, like the other durable
# bench artifacts) rather than .proofs/ (gitignored proof bundles).
SUMMARY="${1:-$REPO_ROOT/docs/proofs/harness-shadow/initial-ledger.md}"

export PARISH_HARNESS_SHADOW=1
export PARISH_HARNESS_SHADOW_LEDGER="$LEDGER"

mkdir -p "$(dirname "$LEDGER")" "$(dirname "$SUMMARY")"
rm -f "$LEDGER"

# Each invocation tags its records with a case label so the summary can break
# divergences down by corpus area. The grep is presentation-only; capture the
# pipeline statuses before any follow-up command can overwrite PIPESTATUS.
corpus_status=0
run_case() {
    local case_label="$1"
    shift
    echo "=== shadow corpus: $case_label ==="
    PARISH_HARNESS_SHADOW_CASE="$case_label" cargo test --manifest-path "$MANIFEST" "$@" 2>&1 \
        | grep -E "test result|FAILED|error\["
    local pipeline_status=("${PIPESTATUS[@]}")
    local rc=${pipeline_status[0]}
    if [ "$rc" -ne 0 ]; then
        echo "WARNING: cargo test for '$case_label' exited $rc — ledger may be incomplete for this case."
        corpus_status=1
    fi
}

run_case engine-unit -p parish-engine --lib
run_case engine-integration -p parish-engine --tests
run_case core -p parish-core --lib

if [ "$corpus_status" -ne 0 ]; then
    echo "NOTE: at least one corpus case had test failures; the divergence ledger below may be partial."
fi

echo
echo "=== summarizing $LEDGER -> $SUMMARY ==="
python3 "$REPO_ROOT/parish/scripts/harness-shadow-summarize.py" "$LEDGER" >"$SUMMARY"
echo "wrote $SUMMARY"
cat "$SUMMARY"

exit "$corpus_status"
