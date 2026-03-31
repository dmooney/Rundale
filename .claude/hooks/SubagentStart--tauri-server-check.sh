#!/usr/bin/env bash
set -euo pipefail

# SubagentStart hook: check if Tauri/Vite dev server is running

# Only relevant if working on UI-related tasks
if lsof -Pi :5173 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "Vite dev server is running on :5173"
elif lsof -Pi :1420 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "Tauri dev server is running on :1420"
else
    echo "NOTE: No Vite/Tauri dev server detected. If working on UI, run: cargo tauri dev"
fi

exit 0
