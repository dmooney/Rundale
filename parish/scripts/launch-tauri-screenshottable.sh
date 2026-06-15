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
# cover the two ways a capture goes blank — an unrendered frontend (this script
# starts vite) and an asleep/locked display. This script now ALSO proactively
# holds the display awake (caffeinate) for the app's whole lifetime, so per-turn
# captures never race a display that has already locked; the in-handler one-shot
# wake (`caffeinate -u -d -t 90`) remains as a reactive fallback.
#
# Usage: launch-tauri-screenshottable.sh [MCP_PORT=3030]
set -euo pipefail

PORT="${1:-3030}"
VITE_PORT="${PARISH_VITE_PORT:-5173}"
PARISH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" # .../parish
VITE_LOG="/tmp/parish-vite-${USER:-shared}.log"
TAURI_LOG="/tmp/parish-tauri-${USER:-shared}.log"

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
        nohup npm run dev -- --port "${VITE_PORT}" >"${VITE_LOG}" 2>&1 &
    )
    for _ in $(seq 1 40); do
        curl -sf -o /dev/null "http://localhost:${VITE_PORT}" 2>/dev/null && break
        sleep 1
    done
    curl -sf -o /dev/null "http://localhost:${VITE_PORT}" 2>/dev/null \
        || {
            echo "ERROR: vite did not come up on :${VITE_PORT} (see ${VITE_LOG})"
            exit 1
        }
    echo "vite up on :${VITE_PORT}"
fi

# 2) Launch the desktop app (auto-starts the bundled vllm-mlx models).
echo "launching parish-tauri on --mcp-port ${PORT} ..."
cd "${PARISH_DIR}"
nohup cargo run -p parish-tauri -- --mcp-port "${PORT}" >"${TAURI_LOG}" 2>&1 &
APP_PID=$!

# 2b) Hold the display awake for the app's lifetime so per-turn screenshots never
# hit a slept/locked display (native capture goes black; the html-to-image DOM
# fallback hangs, #1355). `-d` blocks user-idle DISPLAY sleep (which also stops the
# screensaver, hence the password auto-lock, from ever engaging), `-i` idle system
# sleep, `-s` system sleep on AC. `-w "$APP_PID"` binds the assertion to the app:
# when it is killed (quality-harness §7 close), caffeinate observes the PID exit and
# self-terminates — no orphaned assertion, no system-settings change.
CAFFEINATE_PIDFILE="/tmp/parish-caffeinate-${USER:-shared}.pid"
if command -v caffeinate >/dev/null 2>&1; then
    nohup caffeinate -d -i -s -w "${APP_PID}" >/dev/null 2>&1 &
    echo $! >"${CAFFEINATE_PIDFILE}"
    echo "holding display awake (caffeinate pid $(cat "${CAFFEINATE_PIDFILE}"), bound to app pid ${APP_PID})"
else
    # caffeinate is macOS-only; on Linux/CI there's no display to keep awake.
    echo "caffeinate not found (non-macOS?) — skipping display-awake hold"
fi

# 3) Wait for the MCP bridge / HTTP health.
for _ in $(seq 1 120); do
    curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null 2>&1 && break
    sleep 1
done
curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null 2>&1 \
    || {
        echo "ERROR: parish-tauri health never came up on :${PORT} (see ${TAURI_LOG})"
        exit 1
    }

echo "parish-tauri ready on :${PORT} with frontend served on :${VITE_PORT} — screenshots are capturable."
