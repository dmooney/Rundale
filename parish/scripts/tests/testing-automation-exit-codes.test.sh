#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/bin" "$TMP/home/.cargo/bin"
cat >"$TMP/bin/cargo" <<'EOF'
#!/usr/bin/env bash
echo "error: deliberate cargo failure" >&2
exit 42
EOF
chmod +x "$TMP/bin/cargo"
cp "$TMP/bin/cargo" "$TMP/home/.cargo/bin/cargo"

if PATH="$TMP/bin:$PATH" bash "$REPO_ROOT/parish/scripts/harness-shadow.sh" "$TMP/shadow.md"; then
    echo "harness-shadow.sh masked a cargo failure" >&2
    exit 1
fi

if HOME="$TMP/home" PATH="$TMP/bin:$PATH" just --justfile "$REPO_ROOT/parish/justfile" game-test-all >/dev/null 2>&1; then
    echo "game-test-all masked a fixture failure" >&2
    exit 1
fi

echo "testing automation exit-code tests passed"
