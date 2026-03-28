#!/usr/bin/env bash
set -euo pipefail

# PreToolUse hook: block direct edits to Cargo.lock
# Matcher: Edit|Write

INPUT=$(cat)

FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

# Normalize to relative path from project root
PROJECT_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || echo "")
if [[ -n "$PROJECT_ROOT" ]]; then
    REL_PATH="${FILE_PATH#"$PROJECT_ROOT/"}"
else
    REL_PATH="$FILE_PATH"
fi

case "$REL_PATH" in
    Cargo.lock)
        echo "BLOCKED: Cargo.lock should not be edited directly. Run cargo commands to update it." >&2
        exit 2
        ;;
esac

exit 0
