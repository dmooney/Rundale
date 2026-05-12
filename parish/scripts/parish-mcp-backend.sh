#!/bin/bash
# Start / stop / status helper for the backend that parish-mcp drives.
#
# parish-mcp itself is a stateless HTTP client; it needs *something* serving
# the `/api/*` routes on `127.0.0.1:3030` for tool calls to succeed. The
# easiest backend in a sandbox is `parish web`, the headless Axum server
# (the desktop `parish-tauri --mcp-port 3030` is the alternative when a
# display is available).
#
# Usage:
#   bash parish/scripts/parish-mcp-backend.sh start   # spawn parish web in background
#   bash parish/scripts/parish-mcp-backend.sh stop    # kill the spawned process
#   bash parish/scripts/parish-mcp-backend.sh status  # report PID + health
#   bash parish/scripts/parish-mcp-backend.sh logs    # tail the log file
#
# Defaults:
#   PORT=3030    PARISH_MCP_BACKEND_PORT=4000 to override
#   PID file:    parish/.parish-mcp-backend.pid
#   Log file:    parish/.parish-mcp-backend.log

set -euo pipefail

REPO="$(git rev-parse --show-toplevel)"
cd "$REPO/parish"

PORT="${PARISH_MCP_BACKEND_PORT:-3030}"
PID_FILE=".parish-mcp-backend.pid"
LOG_FILE=".parish-mcp-backend.log"
HEALTH_URL="http://127.0.0.1:${PORT}/api/health"

cmd_start() {
    if cmd_running; then
        echo "parish-mcp backend already running (pid=$(cat "$PID_FILE"))"
        return 0
    fi

    echo "Starting parish web --port $PORT in background..."
    # nohup detaches so the server survives the script's exit.
    # `cargo run --quiet` reuses the cached build if `parish` is already
    # compiled (the SessionStart hook pre-builds it).
    nohup cargo run --quiet -p parish --bin parish -- web --port "$PORT" \
        >"$LOG_FILE" 2>&1 &
    echo $! >"$PID_FILE"

    # Wait up to 60s for the health endpoint to come up. parish web has
    # provider bootstrap that may take a few seconds on first run.
    for _ in $(seq 1 60); do
        if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
            echo "parish-mcp backend ready (pid=$(cat "$PID_FILE"), port=$PORT)"
            return 0
        fi
        sleep 1
    done

    echo "parish-mcp backend failed to come up in 60s. Last 20 log lines:" >&2
    tail -20 "$LOG_FILE" >&2 || true
    return 1
}

cmd_stop() {
    if [ ! -f "$PID_FILE" ]; then
        echo "parish-mcp backend not running (no pid file)"
        return 0
    fi
    local pid
    pid="$(cat "$PID_FILE")"
    if kill -0 "$pid" 2>/dev/null; then
        # Send TERM to the cargo wrapper; cargo forwards to the child binary.
        kill "$pid" 2>/dev/null || true
        for _ in $(seq 1 10); do
            kill -0 "$pid" 2>/dev/null || break
            sleep 1
        done
        # If still alive, escalate to KILL so the port is freed.
        kill -9 "$pid" 2>/dev/null || true
        echo "parish-mcp backend stopped (pid=$pid)"
    else
        echo "parish-mcp backend pid $pid not running; cleaning up pid file"
    fi
    rm -f "$PID_FILE"
}

cmd_status() {
    if cmd_running; then
        local pid
        pid="$(cat "$PID_FILE")"
        echo "parish-mcp backend: running (pid=$pid, port=$PORT)"
        if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
            echo "health: ok ($HEALTH_URL)"
        else
            echo "health: NOT responding at $HEALTH_URL"
        fi
    else
        echo "parish-mcp backend: not running"
    fi
}

cmd_logs() {
    if [ -f "$LOG_FILE" ]; then
        tail -50 "$LOG_FILE"
    else
        echo "no log file at $LOG_FILE"
    fi
}

cmd_running() {
    [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null
}

case "${1:-status}" in
    start)  cmd_start ;;
    stop)   cmd_stop ;;
    status) cmd_status ;;
    logs)   cmd_logs ;;
    *)
        echo "usage: $0 {start|stop|status|logs}" >&2
        exit 1
        ;;
esac
