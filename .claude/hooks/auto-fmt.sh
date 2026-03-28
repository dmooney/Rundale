#!/usr/bin/env bash
set -euo pipefail

# PostToolUse hook: auto-run cargo fmt after .rs file edits
# Matcher: Edit|Write

INPUT=$(cat)

FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

# Only act on .rs files
if [[ "$FILE_PATH" == *.rs ]]; then
    cargo fmt --quiet 2>/dev/null || true
fi

exit 0
