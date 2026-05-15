#!/usr/bin/env bash
set -euo pipefail
exec >&2

# Stop hook: remind about coverage when .rs files are changed

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

CHANGED_RS=$({
    git diff --name-only HEAD 2>/dev/null
    git diff --cached --name-only 2>/dev/null
    git diff --name-only 2>/dev/null
    git ls-files --others --exclude-standard 2>/dev/null
} | grep '\.rs$' | sort -u || true)

if [[ -n "$CHANGED_RS" ]]; then
    echo "=== Coverage Reminder ==="
    echo "Rust files changed:"
    echo "$CHANGED_RS"
    echo ""
    echo "Ensure the Rust coverage ratchet still passes."
    echo "Run: just coverage-check"
    echo "========================="
fi

exit 0
