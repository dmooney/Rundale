#!/usr/bin/env bash
#
# Compose a PR body that carries a proof bundle in the body itself.
#
# Carrying the bundle in the body (rather than a comment posted after the
# PR opens) is race-free: the body is present on the very FIRST
# `agent-check --source=pr` run (`pull_request.opened`), so the proof gate
# goes green immediately instead of failing spuriously until a comment
# lands and a re-push re-triggers the gate (#1177).
#
# Reads the current/intended PR body on stdin, strips any prior
# `parish-proof-bundle:<task-id>` region for THIS task id, appends a freshly
# rendered block, and writes the merged body to stdout. Pure: no network, no
# `gh`. That keeps it unit-testable and reusable in two places:
#
#   * at creation:  gh pr create --body-file <(printf '%s\n' "$desc" \
#                       | bash parish/scripts/compose-proof-body.sh <task-id>)
#   * on update:    attach-proof.sh pipes the live `gh pr view --json body`
#                   through it, then `gh pr edit --body-file`.
#
# Re-running is idempotent: the prior region is removed before the new block
# is appended, so the body never accumulates duplicate bundles.
#
# Usage: compose-proof-body.sh <task-id> < current-body > merged-body
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: compose-proof-body.sh <task-id> < current-body" >&2
    exit 2
fi

task_id="$1"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Render the bundle block. This also validates that the bundle exists and is
# well-formed (render-proof-comment.sh exits non-zero otherwise), so a
# malformed bundle fails here rather than producing a body with no proof.
block="$(bash "$script_dir/render-proof-comment.sh" "$task_id")"

# `gh pr view --json body --jq .body` prints an empty string for an empty
# body, but a hand-piped "null" can leak in; treat it as empty.
existing="$(cat)"
if [[ "$existing" == "null" ]]; then
    existing=""
fi

# Strip any existing region for THIS task id, matching the opener/closer on
# their own line with the SAME regexes agent-check.sh uses, so a fence quoted
# inline in prose is never mistaken for a region boundary. The id guard
# (`index($0, ":" id " ")`) scopes the strip to this bundle and leaves any
# other bundle's region intact.
stripped="$(
    printf '%s\n' "$existing" | awk -v id="$task_id" '
        $0 ~ /^[[:space:]]*<!--[[:space:]]+parish-proof-bundle:[^[:space:]]+[[:space:]]+v=[0-9]+[[:space:]]*-->[[:space:]]*$/ && index($0, ":" id " ") > 0 {
            skip = 1
            next
        }
        skip && $0 ~ /^[[:space:]]*<!--[[:space:]]+\/parish-proof-bundle:[^[:space:]]+[[:space:]]*-->[[:space:]]*$/ && index($0, ":" id " ") > 0 {
            skip = 0
            next
        }
        !skip { print }
    '
)"

# Drop trailing blank lines from the human body so repeated runs do not pile
# up whitespace between the description and the appended block.
stripped="$(printf '%s\n' "$stripped" | awk 'NF { last = NR } { line[NR] = $0 } END { for (i = 1; i <= last; i++) print line[i] }')"

if [[ -n "$stripped" ]]; then
    printf '%s\n\n%s\n' "$stripped" "$block"
else
    printf '%s\n' "$block"
fi
