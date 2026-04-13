#!/usr/bin/env bash
set -euo pipefail

# Notification hook: desktop notification when Claude needs attention

INPUT=$(cat)
MESSAGE=$(echo "$INPUT" | jq -r '.message // "Claude Code needs your attention"')

# macOS desktop notification
if command -v osascript &>/dev/null; then
    osascript -e "display notification \"$MESSAGE\" with title \"Parish -- Claude Code\"" 2>/dev/null || true
# Linux desktop notification
elif command -v notify-send &>/dev/null; then
    notify-send "Parish -- Claude Code" "$MESSAGE" --urgency=normal 2>/dev/null || true
fi

# Fallback: terminal bell
printf '\a'

exit 0
