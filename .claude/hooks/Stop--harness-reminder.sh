#!/usr/bin/env bash
set -euo pipefail
exec >&2

# Stop hook: remind to run game harness when parish-core or world logic changed

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Check if game logic files changed
CORE_CHANGED=$(git diff --name-only HEAD 2>/dev/null | grep -E '(crates/parish-core/|crates/parish-npc/|crates/parish-world/|crates/parish-cli/src/testing\.rs)' || true)
CORE_UNSTAGED=$(git diff --name-only 2>/dev/null | grep -E '(crates/parish-core/|crates/parish-npc/|crates/parish-world/|crates/parish-cli/src/testing\.rs)' || true)
CORE_UNTRACKED=$(git ls-files --others --exclude-standard 2>/dev/null | grep -E '(crates/parish-core/|crates/parish-npc/|crates/parish-world/|crates/parish-cli/src/testing\.rs)' || true)

if [[ -n "$CORE_CHANGED" || -n "$CORE_UNSTAGED" || -n "$CORE_UNTRACKED" ]]; then
    echo "=== Game Logic Changed ==="
    echo "Files in parish-core or world/ were modified."
    echo "Run the game harness to verify mechanics:"
    echo "  cargo run -- --script testing/fixtures/test_walkthrough.txt"
    echo "Or use: /game-test"
    echo "==========================="
fi

exit 0
