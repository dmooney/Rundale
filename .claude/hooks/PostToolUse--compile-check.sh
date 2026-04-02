#!/usr/bin/env bash
set -euo pipefail
exec >&2

# PostToolUse hook: run cargo check after .rs edits to catch compile errors immediately
# Matcher: Edit|Write
# (Clippy runs at Stop time; this is a fast compile check for immediate feedback)

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

if [[ "$FILE_PATH" == *.rs ]]; then
    # cargo check is much faster than clippy -- catches type errors and borrow issues
    OUTPUT=$(cargo check --message-format=short 2>&1 || true)
    ERRORS=$(echo "$OUTPUT" | grep -c '^error' || true)

    if [[ "$ERRORS" -gt 0 ]]; then
        echo "=== Compile Check ==="
        echo "$OUTPUT" | grep -E '^error' | head -5
        if [[ "$ERRORS" -gt 5 ]]; then
            echo "... and $((ERRORS - 5)) more errors"
        fi
        echo "====================="
    fi
fi

exit 0
