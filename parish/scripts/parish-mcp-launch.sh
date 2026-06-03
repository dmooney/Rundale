#!/bin/bash
# Self-healing launcher for the `parish` MCP server declared in .mcp.json.
#
# Why this exists: Claude Code spawns the .mcp.json command at *session init*
# to read its tool list. If the parish-mcp binary doesn't exist yet (a fresh
# git worktree, fresh clone, or after `cargo clean`), the spawn fails and the
# `mcp__parish__*` tools never register for that session — and there is no
# in-session reload, so the whole session is stuck without them.
#
# The SessionStart--build-mcp.sh hook tries to pre-build, but on a cold
# worktree it builds in the *background* and loses the race against this very
# spawn. Fixing the race here makes correctness independent of hook ordering:
# the MCP handshake now WAITS on the build instead of racing it.
#
# Contract: everything on stdout must be JSON-RPC (the MCP stdio protocol),
# so all build chatter is forced to stderr. We exec the real binary so it
# inherits our stdin/stdout/stderr and PID.

set -euo pipefail

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/parish/target}"
MCP_BIN="$TARGET_DIR/debug/parish-mcp"

if [ ! -x "$MCP_BIN" ]; then
    echo "[parish-mcp-launch] binary missing at $MCP_BIN — building (one-time cold compile)..." >&2
    # stdout -> stderr so cargo never corrupts the JSON-RPC stream on stdout.
    (cd "$REPO/parish" && cargo build -p parish-mcp --quiet) 1>&2
    echo "[parish-mcp-launch] build complete." >&2
fi

exec "$MCP_BIN" "$@"
