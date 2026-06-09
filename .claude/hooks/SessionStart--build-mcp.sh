#!/bin/bash
# SessionStart hook — ensure parish-mcp binary exists locally so the project
# MCP server in .mcp.json can spawn at session start without a cold compile.
#
# Skipped on remote sandboxes (CLAUDE_CODE_REMOTE=true), where
# SessionStart--install-system-deps.sh already handles the build alongside
# apt-installed system deps.
#
# Fast path: if the binary is already built (at the real cargo target dir,
# honouring ~/.cargo/config.toml), exit immediately so the session prompt
# comes up instantly. Cold path runs cargo in the background.
#
# Binary probe order (mirrors parish-mcp-launch.sh):
#   1. $CARGO_TARGET_DIR/debug/parish-mcp           (env var set)
#   2. $(cargo metadata …).target_directory/debug/parish-mcp  (reads config.toml)
#   3. $REPO/parish/target/debug/parish-mcp         (hard fallback)

set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" = "true" ]; then
    exit 0
fi

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

if [ ! -d "$REPO/parish/crates/parish-mcp" ]; then
    exit 0
fi

# ---------------------------------------------------------------------------
# Resolve the real target directory, honouring ~/.cargo/config.toml.
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
    )
    if [ -n "$meta_target" ]; then
        echo "$meta_target/debug/parish-mcp"
        return
    fi

    # Probe 3: hard fallback (repo-local target).
    echo "$REPO/parish/target/debug/parish-mcp"
}

MCP_BIN="$(_resolve_mcp_bin)"

# Also check the repo-local fallback before deciding a build is needed.
if [ ! -x "$MCP_BIN" ] && [ -n "${CARGO_TARGET_DIR:-}" ]; then
    if [ -x "$REPO/parish/target/debug/parish-mcp" ]; then
        MCP_BIN="$REPO/parish/target/debug/parish-mcp"
    fi
fi

if [ -x "$MCP_BIN" ]; then
    exit 0
fi

echo '{"async": true, "asyncTimeout": 600000}'

echo "[session-start-hook] Building parish-mcp (binary missing at $MCP_BIN)..." >&2
(cd "$REPO/parish" && cargo build -p parish-mcp --quiet) \
    || echo "[session-start-hook] WARN: parish-mcp build failed" >&2
echo "[session-start-hook] parish-mcp build complete." >&2
