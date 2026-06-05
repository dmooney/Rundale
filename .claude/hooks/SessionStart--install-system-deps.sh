#!/bin/bash
# SessionStart hook — prepare the Claude Code on the web sandbox for parish work.
#
# Three responsibilities:
# 1. Install GTK 3 + WebKit2GTK 4.1 dev packages so `cargo check -p parish-tauri`
#    works (Tauri 2 + wry depend on `gdk-3.0`, `webkit2gtk-4.1`, `libsoup-3.0`,
#    `javascriptcoregtk-4.1` `.pc` files that aren't preinstalled).
# 2. Install `just` (the command runner) so the project's justfile recipes
#    (`just check`, `just run-headless`, `just web`, ...) are available — the
#    web sandbox image ships without it.
# 3. Pre-build `parish-mcp` and `parish` binaries so the project-level MCP
#    server in `.mcp.json` can spawn parish-mcp at session start, and so the
#    agent can run `parish web` as the bridge backend without a cold cargo
#    compile.
#
# All responsibilities are gated by a fast-path: if pkg-config already finds
# the .pc files, `just` is on PATH, AND both binaries are executable, the hook
# exits before emitting the async marker so the prompt comes up instantly. The
# cold path runs the needed steps in the background.
#
# Locally (CLAUDE_CODE_REMOTE unset) the hook is a no-op.

set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
    exit 0
fi

REPO="$(git rev-parse --show-toplevel)"
MCP_BIN="$REPO/parish/target/debug/parish-mcp"
PARISH_BIN="$REPO/parish/target/debug/parish"

needs_gtk=false
if ! pkg-config --exists 'gdk-3.0' 'webkit2gtk-4.1' 'libsoup-3.0' \
    'javascriptcoregtk-4.1' 2>/dev/null; then
    needs_gtk=true
fi

needs_just=false
if ! command -v just >/dev/null 2>&1; then
    needs_just=true
fi

needs_build=false
if [ ! -x "$MCP_BIN" ] || [ ! -x "$PARISH_BIN" ]; then
    needs_build=true
fi

if ! $needs_gtk && ! $needs_just && ! $needs_build; then
    exit 0
fi

# Cold container: do everything in the background so the session prompt
# appears immediately. Generous 10-minute timeout — apt is ~30s, the cold
# cargo build of parish-mcp + parish is the bulk.
echo '{"async": true, "asyncTimeout": 600000}'

# Refresh apt indices once if either apt-based step is needed — packaged
# container lists can go stale enough to 404 individual .deb URLs.
if $needs_gtk || $needs_just; then
    sudo -n apt-get update -qq
fi

if $needs_gtk; then
    echo "[session-start-hook] Installing parish-tauri system deps (GTK 3 + WebKit2GTK 4.1)..." >&2
    sudo -n apt-get install -y --no-install-recommends \
        libgtk-3-dev \
        libwebkit2gtk-4.1-dev \
        libsoup-3.0-dev \
        libjavascriptcoregtk-4.1-dev
fi

if $needs_just; then
    echo "[session-start-hook] Installing just (command runner)..." >&2
    sudo -n apt-get install -y --no-install-recommends just \
        || echo "[session-start-hook] WARN: just install failed" >&2
fi

if $needs_build; then
    echo "[session-start-hook] Pre-building parish-mcp + parish binaries..." >&2
    # parish-mcp has no tauri dep; parish (the CLI) has no tauri dep. Both
    # build without GTK. parish-tauri itself is left to the user's local
    # `cargo run` since it needs a display.
    (cd "$REPO/parish" && cargo build -p parish-mcp --quiet) \
        || echo "[session-start-hook] WARN: parish-mcp build failed" >&2
    (cd "$REPO/parish" && cargo build -p parish --bin parish --quiet) \
        || echo "[session-start-hook] WARN: parish build failed" >&2
fi

echo "[session-start-hook] Setup complete." >&2
