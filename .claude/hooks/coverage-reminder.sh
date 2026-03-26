#!/usr/bin/env bash
set -euo pipefail

# Stop hook: remind about coverage when new .rs files are added

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Check specifically for NEW (untracked) .rs files -- not just modified ones
NEW_RS=$(git ls-files --others --exclude-standard 2>/dev/null | grep '\.rs$' || true)

if [[ -n "$NEW_RS" ]]; then
    echo "=== Coverage Reminder ==="
    echo "New Rust files detected:"
    echo "$NEW_RS"
    echo ""
    echo "Ensure test coverage stays above 90%."
    echo "Run: cargo tarpaulin --out Stdout"
    echo "========================="
fi

exit 0
