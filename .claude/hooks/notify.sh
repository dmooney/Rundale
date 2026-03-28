#!/usr/bin/env bash
set -euo pipefail

# Notification hook: desktop notification when Claude needs attention

INPUT=$(cat)
MESSAGE=$(echo "$INPUT" | jq -r '.message // "Claude Code needs your attention"')

# Linux desktop notification
if command -v notify-send &>/dev/null; then
    notify-send "Parish -- Claude Code" "$MESSAGE" --urgency=normal 2>/dev/null || true
fi

# Fallback: terminal bell
printf '\a'

exit 0
