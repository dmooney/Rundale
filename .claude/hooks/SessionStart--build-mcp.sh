#!/bin/bash
# SessionStart hook — ensure parish-mcp binary exists locally so the project
# MCP server in .mcp.json can spawn at session start without a cold compile.
#
# Skipped on remote sandboxes (CLAUDE_CODE_REMOTE=true), where
# SessionStart--install-system-deps.sh already handles the build alongside
# apt-installed system deps.
#
# Fast path: if the binary is already built, exit immediately so the session
# prompt comes up instantly. Cold path runs cargo in the background.

set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" = "true" ]; then
    exit 0
fi

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
MCP_BIN="$REPO/parish/target/debug/parish-mcp"

if [ -x "$MCP_BIN" ]; then
    exit 0
fi

if [ ! -d "$REPO/parish/crates/parish-mcp" ]; then
    exit 0
fi

echo '{"async": true, "asyncTimeout": 600000}'

echo "[session-start-hook] Building parish-mcp (binary missing at $MCP_BIN)..." >&2
(cd "$REPO/parish" && cargo build -p parish-mcp --quiet) \
    || echo "[session-start-hook] WARN: parish-mcp build failed" >&2
echo "[session-start-hook] parish-mcp build complete." >&2
