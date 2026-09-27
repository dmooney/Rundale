#!/usr/bin/env bash
# Posts (or updates) one sticky PR comment with the differential proof report.
#
# Usage: prove-diff-comment.sh <report.md> <pr-number> <run-url>
# Needs GH_TOKEN and GITHUB_REPOSITORY. The comment is found by its marker and
# edited in place, so re-runs never pile up comments.
set -euo pipefail

report="$1"
pr="$2"
run_url="$3"
marker="<!-- limerick-prove-diff -->"
# GitHub caps comment bodies at 65536 characters; leave room for the frame.
cap=60000

body="$(mktemp)"
trap 'rm -f "$body"' EXIT
{
    echo "$marker"
    echo "## Differential proof"
    echo
    echo "\`limerick/scripts/proof/prove_diff.py\` on \`main\` and this PR ([run]($run_url))."
    echo "Declare intended differences in a \`toml intended-diffs\` block in the PR body"
    echo "(see docs/agent/agent-check.md, Differential Proof)."
    echo
    if [[ -f "$report" ]]; then
        head -c "$cap" "$report"
        if [[ "$(wc -c <"$report")" -gt "$cap" ]]; then
            echo
            echo "... [truncated; the full report is in the run's prove-diff artifact] ..."
        fi
    else
        echo "No report: the differential did not complete. See the run log."
    fi
} >"$body"

existing="$(gh api --paginate "repos/$GITHUB_REPOSITORY/issues/$pr/comments" \
    --jq ".[] | select(.body | startswith(\"$marker\")) | .id" | head -n 1)"
if [[ -n "$existing" ]]; then
    gh api --method PATCH "repos/$GITHUB_REPOSITORY/issues/comments/$existing" \
        -F "body=@$body" >/dev/null
else
    gh api --method POST "repos/$GITHUB_REPOSITORY/issues/$pr/comments" \
        -F "body=@$body" >/dev/null
fi
