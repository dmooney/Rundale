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

oldest_age=0
oldest_path=""

while IFS= read -r f; do
  if [ "$(uname)" = "Darwin" ]; then
    m=$(stat -f%m "$f")
  else
    m=$(stat -c%Y "$f")
  fi
  age=$(( (NOW - m) / 86400 ))
  if [ "$age" -gt "$oldest_age" ]; then
    oldest_age="$age"
    oldest_path="$f"
  fi
done < <(find . -name CLAUDE.md \
  -not -path "./.claude/worktrees/*" \
  -not -path "./node_modules/*" \
  -not -path "*/node_modules/*" \
  -not -path "./parish/target/*" \
  -not -path "./target/*" 2>/dev/null)

if [ "$oldest_age" -gt "$THRESHOLD_DAYS" ] && [ -n "$oldest_path" ]; then
  msg="CLAUDE.md cadence: ${oldest_path#./} is ${oldest_age} days old (threshold ${THRESHOLD_DAYS}). Anthropic recommends a 3-6mo review — newer models may benefit from refreshed guidance."
  jq -nc --arg m "$msg" '{hookSpecificOutput:{hookEventName:"Stop",additionalContext:$m}}'
fi
