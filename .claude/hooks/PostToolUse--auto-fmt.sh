#!/usr/bin/env bash
set -euo pipefail

# PostToolUse hook: auto-format the edited file by type.
# Matcher: Edit|Write
# Best-effort + fail-soft: each formatter is skipped if its tool is absent.
# CI is the real gate; this just keeps the working tree clean as you edit.

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')
[[ -z "$FILE_PATH" || ! -f "$FILE_PATH" ]] && exit 0

# Make node-based tools (prettier) resolvable if fnm is set up.
command -v fnm >/dev/null 2>&1 && eval "$(fnm env 2>/dev/null)" 2>/dev/null || true

dir=$(dirname "$FILE_PATH")

prettier_fmt() {
    # Resolve prettier from the nearest node_modules (ui or repo root); skip if
    # none. Use basename: FILE_PATH may be relative, and it breaks once we cd
    # into $dir — but the file is right there, so its basename resolves.
    (cd "$dir" && npx --no-install prettier --write "$(basename "$FILE_PATH")" >/dev/null 2>&1) || true
}

case "$FILE_PATH" in
*.rs)
    cargo fmt --quiet 2>/dev/null || true
    ;;
*.ts | *.tsx | *.svelte | *.js | *.mjs | *.cjs | *.json | *.jsonc | *.md | *.yaml | *.yml)
    prettier_fmt
    ;;
*.py)
    if command -v ruff >/dev/null 2>&1; then
        ruff format "$FILE_PATH" >/dev/null 2>&1 || true
        ruff check --fix "$FILE_PATH" >/dev/null 2>&1 || true
    fi
    ;;
*.sh)
    command -v shfmt >/dev/null 2>&1 && shfmt -w "$FILE_PATH" 2>/dev/null || true
    ;;
*.toml)
    command -v taplo >/dev/null 2>&1 && taplo fmt "$FILE_PATH" >/dev/null 2>&1 || true
    ;;
esac

exit 0
