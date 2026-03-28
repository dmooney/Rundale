#!/usr/bin/env bash
set -euo pipefail

# Stop hook: warn when code changed but docs were not updated

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Check if .rs files changed
RS_CHANGED=$(git diff --name-only HEAD 2>/dev/null | grep '\.rs$' || true)
RS_UNSTAGED=$(git diff --name-only 2>/dev/null | grep '\.rs$' || true)

if [[ -z "$RS_CHANGED" && -z "$RS_UNSTAGED" ]]; then
    exit 0
fi

# Check if any docs were also updated
DOCS_CHANGED=$(git diff --name-only HEAD 2>/dev/null | grep -E '(\.md$|docs/|///|README)' || true)
DOCS_UNSTAGED=$(git diff --name-only 2>/dev/null | grep -E '(\.md$|docs/|README)' || true)

if [[ -z "$DOCS_CHANGED" && -z "$DOCS_UNSTAGED" ]]; then
    echo "=== Documentation Check ==="
    echo "WARNING: Rust files changed but no documentation files were updated."
    echo "Per project standards, every commit must leave docs current."
    echo "Consider updating: README.md, CLAUDE.md, docs/design/, doc comments (///)"
    echo "==========================="
fi

exit 0
