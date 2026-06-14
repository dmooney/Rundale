#!/usr/bin/env bash
# Launch the Parish/Rundale desktop app ready for SCREENSHOT capture.
#
# Why this exists: the debug `parish-tauri` binary loads its UI from the vite
# dev server (`devUrl` = http://localhost:5173 in tauri.conf.json). `cargo tauri
# dev` / `just run` auto-start vite via `beforeDevCommand`, but launching the raw
# binary (`cargo run -p parish-tauri -- --mcp-port 3030`, as the MCP/quality
# harness does) skips it — so the webview can't load the frontend and renders a
# blank WHITE window. The engine still works over MCP, but every screenshot comes
# back as a rejected blank frame. This helper starts vite first, then the app, so
# the window actually renders the game and captures are real.
#
# Pair with the in-app display-wake fix (commands/screenshot.rs): together they
# cover the two ways a capture goes blank — an unrendered frontend (this script)
# and an asleep display (the handler wakes + holds it).
#
# Usage: launch-tauri-screenshottable.sh [MCP_PORT=3030]
set -euo pipefail

PORT="${1:-3030}"
VITE_PORT="${PARISH_VITE_PORT:-5173}"
PARISH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" # .../parish

# 1) Ensure the frontend is being served (otherwise the window is blank white).
if curl -sf -o /dev/null "http://localhost:${VITE_PORT}" 2>/dev/null; then
    echo "vite already serving on :${VITE_PORT}"
else
    echo "starting vite dev server on :${VITE_PORT} ..."
    (
        cd "${PARISH_DIR}/apps/ui"
        # Node via fnm (the repo pins Node 22); fall back to whatever node is on PATH.
        eval "$(fnm env 2>/dev/null)" 2>/dev/null || true
        fnm use 22 2>/dev/null || true
        nohup npm run dev -- --port "${VITE_PORT}" >/tmp/parish-vite.log 2>&1 &
    )
    for _ in $(seq 1 40); do
        curl -sf -o /dev/null "http://localhost:${VITE_PORT}" 2>/dev/null && break
        sleep 1
    done
    curl -sf -o /dev/null "http://localhost:${VITE_PORT}" 2>/dev/null ||
        {
            echo "ERROR: vite did not come up on :${VITE_PORT} (see /tmp/parish-vite.log)"
            exit 1
        }
    echo "vite up on :${VITE_PORT}"
fi

# 2) Launch the desktop app (auto-starts the bundled vllm-mlx models).
echo "launching parish-tauri on --mcp-port ${PORT} ..."
(
    cd "${PARISH_DIR}"
    nohup cargo run -p parish-tauri -- --mcp-port "${PORT}" >/tmp/parish-tauri.log 2>&1 &
)

# 3) Wait for the MCP bridge / HTTP health.
for _ in $(seq 1 120); do
    curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null 2>&1 && break
    sleep 1
done
curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null 2>&1 ||
    {
        echo "ERROR: parish-tauri health never came up on :${PORT} (see /tmp/parish-tauri.log)"
        exit 1
    }

echo "parish-tauri ready on :${PORT} with frontend served on :${VITE_PORT} — screenshots are capturable."
