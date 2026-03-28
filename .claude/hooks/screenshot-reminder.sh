#!/usr/bin/env bash
set -euo pipefail

# Stop hook: remind to regenerate screenshots when UI or Tauri backend changed

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

UI_CHANGED=$(git diff --name-only HEAD 2>/dev/null | grep -E '(^ui/src/|^src-tauri/src/)' || true)
UI_UNSTAGED=$(git diff --name-only 2>/dev/null | grep -E '(^ui/src/|^src-tauri/src/)' || true)
UI_UNTRACKED=$(git ls-files --others --exclude-standard 2>/dev/null | grep -E '(^ui/src/|^src-tauri/src/)' || true)

if [[ -n "$UI_CHANGED" || -n "$UI_UNSTAGED" || -n "$UI_UNTRACKED" ]]; then
    echo "=== Screenshot Reminder ==="
    echo "UI or Tauri backend files changed."
    echo "Regenerate screenshots before pushing:"
    echo "  /screenshot"
    echo "==========================="
fi

exit 0
