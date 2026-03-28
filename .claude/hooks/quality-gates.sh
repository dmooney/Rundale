#!/usr/bin/env bash
set -euo pipefail

# Stop hook: run fmt + clippy + test if any .rs files changed
# Only runs when Rust files have been modified, skips conversation-only turns

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Check if any .rs files have been modified (staged, unstaged, or untracked)
CHANGED_RS=$(git diff --name-only HEAD 2>/dev/null | grep '\.rs$' || true)
UNSTAGED_RS=$(git diff --name-only 2>/dev/null | grep '\.rs$' || true)
UNTRACKED_RS=$(git ls-files --others --exclude-standard 2>/dev/null | grep '\.rs$' || true)

if [[ -z "$CHANGED_RS" && -z "$UNSTAGED_RS" && -z "$UNTRACKED_RS" ]]; then
    # No Rust files changed, skip quality gates
    exit 0
fi

echo "=== Parish Quality Gates ==="
echo "Rust files changed -- running checks..."
echo ""

FAILED=0

# 1. Format check
echo "--- cargo fmt --check ---"
if ! cargo fmt --check 2>&1; then
    echo "FAIL: formatting issues detected. Run 'cargo fmt' to fix."
    FAILED=1
else
    echo "PASS"
fi
echo ""

# 2. Clippy
echo "--- cargo clippy ---"
if ! cargo clippy -- -D warnings 2>&1; then
    echo "FAIL: clippy warnings found."
    FAILED=1
else
    echo "PASS"
fi
echo ""

# 3. Tests
echo "--- cargo test ---"
if ! cargo test 2>&1; then
    echo "FAIL: tests failed."
    FAILED=1
else
    echo "PASS"
fi
echo ""

if [[ $FAILED -ne 0 ]]; then
    echo "=== Quality gates FAILED ==="
    exit 1
else
    echo "=== All quality gates passed ==="
    exit 0
fi
