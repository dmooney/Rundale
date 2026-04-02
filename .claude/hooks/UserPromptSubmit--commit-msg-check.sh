#!/usr/bin/env bash
set -euo pipefail
exec >&2

# UserPromptSubmit hook: validate conventional commit format when user asks to commit

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | jq -r '.prompt // ""')

# Only act if the prompt mentions committing
if ! echo "$PROMPT" | grep -qiE '\bcommit\b'; then
    exit 0
fi

# Check if a commit message is provided and validate format
# Look for quoted strings that might be commit messages
MSG=$(echo "$PROMPT" | grep -oE '"[^"]+"' | head -1 | tr -d '"' || true)

if [[ -n "$MSG" ]]; then
    if ! echo "$MSG" | grep -qE '^(feat|fix|refactor|docs|test|chore|style|perf|ci|build|revert):'; then
        echo "=== Commit Message Check ==="
        echo "WARNING: Commit message does not follow conventional format."
        echo "Expected: feat:|fix:|refactor:|docs:|test:|chore:|style:|perf:|ci:|build:|revert:"
        echo "Got: $MSG"
        echo "==========================="
    fi
fi

exit 0
