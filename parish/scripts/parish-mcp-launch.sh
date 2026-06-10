#!/bin/bash
# Launcher for the `parish` MCP server declared in .mcp.json.
#
# Claude Code spawns this at *session init* to read the tool list, within a
# short startup window, and there is no in-session reload (#1352). On a fresh
# worktree the Rust `parish-mcp` binary does not exist yet. The previous version
# compiled it *synchronously* here, which overran that window so the
# `mcp__parish__*` tools never registered (parish-mcp-cold-register).
#
# This version never builds. It resolves the real cargo target dir and:
#   - if the binary exists, execs it (warm fast path);
#   - otherwise execs a no-build Python shim that registers the tools instantly
#     from the committed manifest.json and hands off to the real binary on the
#     first tools/call once it exists.
# The binary is produced by the normal `cargo build` / `just build` — parish-mcp
# is a workspace default member, so nothing parish-mcp-specific has to run here.
#
# Contract: stdout is the JSON-RPC stream; all chatter goes to stderr. We exec
# the chosen process so it inherits our stdin/stdout/stderr and PID.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# Resolve the real cargo target dir. This repo's worktrees share a target dir
# via ~/.cargo/config.toml (build.target-dir = ~/.cargo/target), so the naive
# "$REPO/parish/target" guess is wrong here — the binary would never be found.
# Honour CARGO_TARGET_DIR, else ask cargo (offline, no compile), else fall back.
TARGET_DIR="${CARGO_TARGET_DIR:-}"
if [ -z "$TARGET_DIR" ]; then
    TARGET_DIR="$( (cd "$REPO/parish" && cargo metadata --no-deps --offline --format-version 1 2>/dev/null) \
        | python3 -c 'import sys,json; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null || true)"
fi
[ -z "$TARGET_DIR" ] && TARGET_DIR="$REPO/parish/target"

MCP_BIN="$TARGET_DIR/debug/parish-mcp"
MANIFEST="$REPO/parish/crates/parish-mcp/manifest.json"

# Warm fast path: hand the connection straight to the real binary (unchanged
# behaviour, full functionality, registers instantly, backend connection lazy).
if [ -x "$MCP_BIN" ]; then
    exec "$MCP_BIN" "$@"
fi

# Cold path. Prefer the no-build shim so tools register inside the init window.
if command -v python3 >/dev/null 2>&1 && [ -f "$MANIFEST" ]; then
    echo "[parish-mcp-launch] binary not built at $MCP_BIN — serving no-build cold shim from manifest." >&2
    exec python3 "$SCRIPT_DIR/parish-mcp-cold-shim.py" --manifest "$MANIFEST" --bin "$MCP_BIN" -- "$@"
fi

# Last-resort fallback (no python3, or manifest missing): the old synchronous
# build. Never worse than the previous behaviour; only reached in degraded envs.
echo "[parish-mcp-launch] no python3/manifest for cold shim — falling back to synchronous build." >&2
(cd "$REPO/parish" && cargo build -p parish-mcp --quiet) 1>&2
echo "[parish-mcp-launch] build complete." >&2
exec "$MCP_BIN" "$@"
