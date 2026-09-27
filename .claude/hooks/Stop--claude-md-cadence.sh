#!/usr/bin/env bash
# Stop hook — warn when any CLAUDE.md is older than the cadence threshold.
#
# Anthropic's "Claude Code in Large Codebases" article recommends reviewing
# CLAUDE.md every 3-6 months — instructions written for older models can
# constrain newer ones. We surface the oldest stale file so the user can
# refresh on their schedule.

set -euo pipefail
trap 'rc=$?; printf "Stop hook %s failed (exit=%d) at line %d\n" "${BASH_SOURCE[0]##*/}" "$rc" "$LINENO" >&2' ERR

THRESHOLD_DAYS=90
NOW=$(date +%s)

# A Stop hook's additionalContext is fed back to the model as a new turn, and
# that turn's Stop re-runs this hook. Without a guard the reminder loops
# forever. Emit at most once per session, and never on a hook-driven stop.
INPUT="$(cat 2>/dev/null || true)"
STOP_ACTIVE="$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false' 2>/dev/null || echo false)"
[ "$STOP_ACTIVE" = "true" ] && exit 0
SESSION_ID="$(printf '%s' "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)"
MARKER_DIR="${TMPDIR:-/tmp}/claude-md-cadence"
if [ -n "$SESSION_ID" ]; then
    mkdir -p "$MARKER_DIR"
    MARKER="$MARKER_DIR/$SESSION_ID"
    [ -e "$MARKER" ] && exit 0
fi

oldest_age=0
oldest_path=""

while IFS= read -r f; do
    if [ "$(uname)" = "Darwin" ]; then
        m=$(stat -f%m "$f")
    else
        m=$(stat -c%Y "$f")
    fi
    age=$(((NOW - m) / 86400))
    if [ "$age" -gt "$oldest_age" ]; then
        oldest_age="$age"
        oldest_path="$f"
    fi
done < <(find . -name CLAUDE.md \
    -not -path "./.claude/worktrees/*" \
    -not -path "./node_modules/*" \
    -not -path "*/node_modules/*" \
    -not -path "./limerick/target/*" \
    -not -path "./target/*" 2>/dev/null)

if [ "$oldest_age" -gt "$THRESHOLD_DAYS" ] && [ -n "$oldest_path" ]; then
    msg="CLAUDE.md cadence: ${oldest_path#./} is ${oldest_age} days old (threshold ${THRESHOLD_DAYS}). Anthropic recommends a 3-6mo review — newer models may benefit from refreshed guidance."
    [ -n "${MARKER:-}" ] && : >"$MARKER"
    jq -nc --arg m "$msg" '{hookSpecificOutput:{hookEventName:"Stop",additionalContext:$m}}'
fi
