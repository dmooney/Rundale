#!/bin/bash
# SessionStart hook — install Linux system libraries the parish-tauri build
# needs in Claude Code on the web. Locally we leave the host alone.
#
# parish-tauri pulls Tauri 2 + wry, which bind to GTK 3 and WebKit2GTK 4.1.
# Without these `.pc` files installed, `cargo check -p parish-tauri` fails
# at the `gdk-sys` build script with "Package gdk-3.0 was not found".
#
# Idempotent: a fast-path at the top exits early when pkg-config already
# resolves the relevant package files, so resumed sessions skip the
# apt-get cost on warm containers.

set -euo pipefail

# Only run in the remote Claude Code on the web sandbox; on a local
# checkout the user has their own GTK install (or runs `cargo run` via the
# desktop without ever crossing this codepath).
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
    exit 0
fi

# Fast-path: warm container already has the package files. Cheaper than
# even an `apt-get update`. Run synchronously (no async marker) so the
# session starts instantly when nothing needs to install.
if pkg-config --exists 'gdk-3.0' 'webkit2gtk-4.1' 'libsoup-3.0' \
    'javascriptcoregtk-4.1' 2>/dev/null; then
    exit 0
fi

# Cold container: install runs in the background so the session prompt
# appears immediately. The 5-minute timeout is generous; on this sandbox
# the install is ~30s. Agent code that tries `cargo check -p parish-tauri`
# before the install finishes will see the gdk-sys build failure and can
# wait + retry.
echo '{"async": true, "asyncTimeout": 300000}'

echo "[session-start-hook] Installing parish-tauri system deps (GTK 3 + WebKit2GTK 4.1)..." >&2

# Refresh apt indices first — observed that the packaged container's apt
# lists can be stale enough to 404 individual .deb URLs without an update.
sudo -n apt-get update -qq

# --no-install-recommends keeps the install lean; the named packages pull
# in the actual headers + .pc files we need and not much else.
sudo -n apt-get install -y --no-install-recommends \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev

echo "[session-start-hook] parish-tauri system deps ready." >&2
