#!/bin/bash
# Launcher for the `limerick` MCP server declared in .mcp.json.
#
# Claude Code spawns this at *session init* to read the tool list, within a
# short startup window, and there is no in-session reload (#1352). On a fresh
# worktree the Rust `limerick-mcp` binary does not exist yet. The previous version
# compiled it *synchronously* here, which overran that window so the
# `mcp__limerick__*` tools never registered (limerick-mcp-cold-register).
#
# This version never builds at init. It resolves the real cargo target dir and:
#   - if the binary exists, execs it (warm fast path);
#   - otherwise execs a no-build Python shim that registers the tools instantly
#     from the committed manifest.json and hands off to the real binary on the
#     first tools/call once it exists.
# The binary is produced by the normal `cargo build` / `just build` — limerick-mcp
# is a workspace default member, so nothing limerick-mcp-specific has to run here.
#
# Binary probe order (first existing path wins):
#   1. $CARGO_TARGET_DIR/debug/limerick-mcp           (env var set)
#   2. $(cargo metadata …).target_directory/debug/limerick-mcp  (reads config.toml)
#   3. $REPO/limerick/target/debug/limerick-mcp         (hard fallback)
#
# Contract: everything on stdout must be JSON-RPC (the MCP stdio protocol), so
# all chatter is forced to stderr. We exec the chosen process so it inherits our
# stdin/stdout/stderr and PID.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
MANIFEST="$REPO/limerick/crates/limerick-mcp/manifest.json"

# ---------------------------------------------------------------------------
# Resolve the real target directory, honouring ~/.cargo/config.toml.
# cargo metadata is the only portable way to read the effective target-dir;
# $CARGO_TARGET_DIR is only set when the caller explicitly exports it.
# ---------------------------------------------------------------------------
_resolve_mcp_bin() {
    # Probe 1: explicit env var.
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        echo "$CARGO_TARGET_DIR/debug/limerick-mcp"
        return
    fi

    # Probe 2: ask cargo itself (reads ~/.cargo/config.toml + workspace).
    local meta_target
    meta_target=$(
        cargo metadata --no-deps --format-version 1 \
            --manifest-path "$REPO/limerick/Cargo.toml" 2>/dev/null \
            | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
    ) || meta_target=""
    if [ -n "$meta_target" ]; then
        echo "$meta_target/debug/limerick-mcp"
        return
    fi

    # Probe 3: hard fallback (repo-local target).
    echo "$REPO/limerick/target/debug/limerick-mcp"
}

MCP_BIN="$(_resolve_mcp_bin)"

# Check the remaining candidates in case probe 2 failed (e.g. cargo not on PATH)
# but another path already holds a built binary.
if [ ! -x "$MCP_BIN" ]; then
    fallback_candidates=()
    [ -n "${CARGO_TARGET_DIR:-}" ] && fallback_candidates+=("$CARGO_TARGET_DIR/debug/limerick-mcp")
    fallback_candidates+=("$REPO/limerick/target/debug/limerick-mcp")
    for candidate in "${fallback_candidates[@]}"; do
        if [ -x "$candidate" ]; then
            MCP_BIN="$candidate"
            echo "[limerick-mcp-launch] found binary at fallback: $MCP_BIN" >&2
            break
        fi
    done
fi

# Warm fast path: hand the connection straight to the real binary (full
# functionality, registers instantly, backend connection lazy).
if [ -x "$MCP_BIN" ]; then
    exec "$MCP_BIN" "$@"
fi

# Cold path: no binary yet. Do NOT build here — a synchronous build overruns
# Claude Code's MCP init window and the tools never register. Serve the no-build
# shim from the committed manifest so the tools register instantly; the shim
# hands off to the real binary on the first tools/call once the normal build
# (limerick-mcp is a default member) has produced it.
if command -v python3 >/dev/null 2>&1 && [ -f "$MANIFEST" ]; then
    echo "[limerick-mcp-launch] binary not built at $MCP_BIN — serving no-build cold shim from manifest." >&2
    exec python3 "$SCRIPT_DIR/limerick-mcp-cold-shim.py" --manifest "$MANIFEST" --bin "$MCP_BIN" -- "$@"
fi

# Last-resort fallback (no python3, or manifest missing): the old synchronous
# build. Never worse than the previous behaviour; only reached in degraded envs.
echo "[limerick-mcp-launch] no python3/manifest for cold shim — falling back to synchronous build." >&2
(cd "$REPO/limerick" && cargo build -p limerick-mcp --quiet) 1>&2
MCP_BIN="$(_resolve_mcp_bin)"
echo "[limerick-mcp-launch] build complete." >&2
exec "$MCP_BIN" "$@"
