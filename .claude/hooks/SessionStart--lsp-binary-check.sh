#!/usr/bin/env bash
# SessionStart hook — verify LSP binaries are installed for .lsp.json.
#
# Soft warning only. .lsp.json is opt-in: missing binaries are a no-op for
# Claude Code, but the user loses symbol-level navigation. We surface the
# install hint so they can fix it once.

set -euo pipefail

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
[ -f "$REPO/.lsp.json" ] || exit 0

missing=()
command -v rust-analyzer >/dev/null 2>&1 || missing+=("rust-analyzer")
command -v svelte-language-server >/dev/null 2>&1 || missing+=("svelte-language-server")
command -v typescript-language-server >/dev/null 2>&1 || missing+=("typescript-language-server")

[ ${#missing[@]} -eq 0 ] && exit 0

hint=""
for tool in "${missing[@]}"; do
    case "$tool" in
        rust-analyzer) hint+="  brew install rust-analyzer  (or: rustup component add rust-analyzer)\n" ;;
        svelte-language-server) hint+="  pnpm add -g svelte-language-server\n" ;;
        typescript-language-server) hint+="  pnpm add -g typescript-language-server typescript\n" ;;
    esac
done

msg="LSP binaries missing: ${missing[*]}. Symbol-level navigation disabled until installed:\n${hint}"
jq -nc --arg m "$msg" '{hookSpecificOutput:{hookEventName:"SessionStart",additionalContext:$m}}'
