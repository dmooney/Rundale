#!/usr/bin/env bash
#
# Unit test for compose-proof-body.sh — the race-free proof-bundle carrier
# (#1177). Pure: builds a throwaway git repo + fake bundle, never touches the
# network or `gh`. Run directly or via CI (shell-quality job).
#
# Asserts:
#   1. An empty body yields exactly one fenced bundle region.
#   2. A human-written body is preserved AND gains exactly one region.
#   3. Re-composing an already-composed body is idempotent (still one
#      region, human text still present once) — so re-running attach-proof
#      never piles up duplicate bundles.
set -euo pipefail

scripts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose="$scripts_dir/compose-proof-body.sh"

fails=0
check() {
    local desc="$1" actual="$2" expected="$3"
    if [[ "$actual" == "$expected" ]]; then
        echo "ok   - $desc"
    else
        echo "FAIL - $desc (got '$actual', want '$expected')" >&2
        fails=$((fails + 1))
    fi
}

count_openers() { grep -c '^<!-- parish-proof-bundle:' <<<"$1" || true; }
count_closers() { grep -c '^<!-- /parish-proof-bundle:' <<<"$1" || true; }
count_lines() { grep -cF "$2" <<<"$1" || true; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# render-proof-comment.sh resolves the bundle under the git toplevel of the
# CWD, so build a throwaway repo and run the composer from inside it.
# `|| result=$?` keeps `set -e` from aborting the parent at the subshell's
# non-zero exit, so the failure summary below still prints.
result=0
(
    cd "$tmp"
    git init -q
    id="demo-1177"
    mkdir -p ".proofs/$id"
    printf '## Acceptance criteria\n\n- A: thing happens\n' >".proofs/$id/acceptance-criteria.md"
    printf 'Evidence type: live gameplay transcript\n\nA happened on line 7.\n' >".proofs/$id/evidence.md"
    printf 'Verdict: sufficient\nTechnical debt: clear\nAcceptance criteria: met\n' >".proofs/$id/judge.md"

    # 1. Empty body -> one region.
    empty_out="$(printf '' | bash "$compose" "$id")"
    check "empty body: one opener" "$(count_openers "$empty_out")" "1"
    check "empty body: one closer" "$(count_closers "$empty_out")" "1"

    # 2. Human body preserved + one region appended.
    human="Fixes the thing. See discussion."
    human_out="$(printf '%s\n' "$human" | bash "$compose" "$id")"
    check "human body: text preserved once" "$(count_lines "$human_out" "$human")" "1"
    check "human body: one opener" "$(count_openers "$human_out")" "1"
    check "human body: one closer" "$(count_closers "$human_out")" "1"

    # 3. Idempotent: re-compose the already-composed body.
    again_out="$(printf '%s' "$human_out" | bash "$compose" "$id")"
    check "idempotent: still one opener" "$(count_openers "$again_out")" "1"
    check "idempotent: still one closer" "$(count_closers "$again_out")" "1"
    check "idempotent: human text still once" "$(count_lines "$again_out" "$human")" "1"

    exit "$fails"
) || result=$?

if [[ "$result" -ne 0 ]]; then
    echo "compose-proof-body.test.sh: $result assertion(s) failed." >&2
    exit 1
fi
echo "compose-proof-body.test.sh: all assertions passed."
