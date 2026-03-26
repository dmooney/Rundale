#!/usr/bin/env bash
set -euo pipefail

# WorktreeCreate hook: compile-check all workspace members in new worktree

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

echo "=== Worktree Compile Check ==="
echo "Verifying all workspace members compile..."

if cargo check --all --quiet 2>&1; then
    echo "PASS: All workspace members compile."
else
    echo "FAIL: Workspace compilation errors detected. Fix before proceeding."
fi

echo "==============================="

exit 0
