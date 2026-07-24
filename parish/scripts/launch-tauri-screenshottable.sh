#!/usr/bin/env bash
# Launch the graphical Parish quality-harness runtime.
#
# This is deliberately an attached, static-asset runtime: it builds this
# worktree's UI, serves that frozen snapshot through an owned loopback server,
# and runs the desktop process as a child. There is no Vite child, fixed
# frontend port, or detached process lifetime for an MCP bridge to outlive.
#
# Usage: launch-tauri-screenshottable.sh [MCP_PORT=3030]
set -euo pipefail

PORT="${1:-3030}"
PARISH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(cd "$PARISH_DIR/.." && pwd)"
UI_DIR="$PARISH_DIR/apps/ui"
STATIC_LOG="$(mktemp -t parish-static-ui.XXXXXX)"

cd "$UI_DIR"
npm run build

# Pixi image decoding on WebKit requires HTTP image MIME headers; the Tauri
# static-resource protocol currently gives createImageBitmap an undecodable
# tauri:// response. This server is static-only (no Vite transforms/HMR).
node "$UI_DIR/scripts/static-ui-server.mjs" --root "$UI_DIR/dist" --port 0 >"$STATIC_LOG" 2>&1 &
STATIC_PID=$!
cleanup() {
    kill "$STATIC_PID" 2>/dev/null || true
    wait "$STATIC_PID" 2>/dev/null || true
    rm -f "$STATIC_LOG"
}
trap cleanup EXIT INT TERM
for _ in $(seq 1 50); do
    if grep -q '^READY ' "$STATIC_LOG"; then break; fi
    if ! kill -0 "$STATIC_PID" 2>/dev/null; then
        cat "$STATIC_LOG" >&2
        exit 1
    fi
    sleep 0.1
done
STATIC_URL="$(sed -n 's/^READY //p' "$STATIC_LOG" | head -n 1)"
if [ -z "$STATIC_URL" ]; then
    echo 'static UI server did not become ready' >&2
    exit 1
fi

# Tauri's build script merges this JSON into tauri.conf.json, pointing the
# debug webview at the worktree-owned static server.
# A worktree-local target prevents another checkout's compile-time asset path
# or Tauri config from being reused by this graphical proof runtime.
export TAURI_CONFIG="{\"build\":{\"devUrl\":\"$STATIC_URL\"}}"
export CARGO_TARGET_DIR="${PARISH_GRAPHICAL_HARNESS_TARGET_DIR:-$REPO/.parish-graphical-harness-target}"

cd "$PARISH_DIR"
cargo run -p parish-tauri -- --mcp-port "$PORT"
