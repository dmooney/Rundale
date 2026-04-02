#!/usr/bin/env bash
set -euo pipefail
exec >&2

# PostToolUse hook: remind about dependency audit when Cargo.toml is edited
# Matcher: Edit|Write

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

if [[ "$FILE_PATH" == *Cargo.toml ]]; then
    echo "=== Dependency Change Detected ==="
    echo "Cargo.toml was modified. Consider running:"
    echo "  cargo audit    (check for known vulnerabilities)"
    echo "  cargo outdated (check for newer versions)"
    echo "==================================="
fi

exit 0
