#!/usr/bin/env bash
set -euo pipefail

# PreToolUse hook: block git commits Mon–Thu 08:00–17:00 Eastern Time
# Matcher: Bash

INPUT=$(cat)

COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""')

# Only act on git commit commands
if ! echo "$COMMAND" | grep -qE '\bgit\s+commit\b'; then
    exit 0
fi

DAY=$(TZ='America/New_York' date +%u)   # 1=Mon … 7=Sun
HOUR=$(TZ='America/New_York' date +%H)  # 00–23

if [ "$DAY" -ge 1 ] && [ "$DAY" -le 4 ] && [ "$HOUR" -ge 8 ] && [ "$HOUR" -lt 17 ]; then
    echo "BLOCKED: Commits are not allowed Monday–Thursday 08:00–17:00 Eastern." >&2
    echo "Current Eastern time: $(TZ='America/New_York' date '+%A %H:%M %Z')" >&2
    exit 2
fi

exit 0
