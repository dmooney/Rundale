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
# Binary probe order (first existing path wins; build only if none exist):
#   1. $CARGO_TARGET_DIR/debug/parish-mcp           (env var set)
#   2. $(cargo metadata …).target_directory/debug/parish-mcp  (reads config.toml)
#   3. $REPO/parish/target/debug/parish-mcp         (hard fallback)
#
# Contract: everything on stdout must be JSON-RPC (the MCP stdio protocol),
# so all build chatter is forced to stderr. We exec the real binary so it
# inherits our stdin/stdout/stderr and PID.

set -euo pipefail

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# ---------------------------------------------------------------------------
# Resolve the real target directory, honouring ~/.cargo/config.toml.
# cargo metadata is the only portable way to read the effective target-dir;
# $CARGO_TARGET_DIR is only set when the caller explicitly exports it.
# ---------------------------------------------------------------------------
_resolve_mcp_bin() {
    # Probe 1: explicit env var.
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        echo "$CARGO_TARGET_DIR/debug/parish-mcp"
        return
    fi

    # Probe 2: ask cargo itself (reads ~/.cargo/config.toml + workspace).
    local meta_target
    meta_target=$(
        cargo metadata --no-deps --format-version 1 \
            --manifest-path "$REPO/parish/Cargo.toml" 2>/dev/null \
            | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
    ) || meta_target=""
    if [ -n "$meta_target" ]; then
        echo "$meta_target/debug/parish-mcp"
        return
    fi

    # Probe 3: hard fallback (repo-local target).
    echo "$REPO/parish/target/debug/parish-mcp"
}

MCP_BIN="$(_resolve_mcp_bin)"
echo "[parish-mcp-launch] resolved binary path: $MCP_BIN" >&2

if [ ! -x "$MCP_BIN" ]; then
    # Check remaining candidates before triggering a cold build — in case
    # probe 2 failed (e.g. cargo not on PATH yet) but another path exists.
    fallback_candidates=()
    [ -n "${CARGO_TARGET_DIR:-}" ] && fallback_candidates+=("$CARGO_TARGET_DIR/debug/parish-mcp")
    fallback_candidates+=("$REPO/parish/target/debug/parish-mcp")
    for candidate in "${fallback_candidates[@]}"; do
        if [ -x "$candidate" ]; then
            MCP_BIN="$candidate"
            echo "[parish-mcp-launch] found binary at fallback: $MCP_BIN" >&2
            break
        fi
    done
fi

if [ ! -x "$MCP_BIN" ]; then
    echo "[parish-mcp-launch] binary missing — building (one-time cold compile)..." >&2
    # stdout -> stderr so cargo never corrupts the JSON-RPC stream on stdout.
    (cd "$REPO/parish" && cargo build -p parish-mcp --quiet) 1>&2
    # After build, re-resolve so we exec the binary cargo actually produced.
    MCP_BIN="$(_resolve_mcp_bin)"
    echo "[parish-mcp-launch] build complete." >&2
fi

exec "$MCP_BIN" "$@"
