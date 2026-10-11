#!/usr/bin/env bash
# ensure-deps.sh installs on a fresh tree, does nothing while lockfiles and the
# runtime are unchanged, reinstalls when either changes, never stamps a failed
# install, and fails (not skips) when a tool is missing. Hermetic: npm, node,
# npx, and python3 are stubs that log their calls.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
script="$repo_root/limerick/scripts/ensure-deps.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

stubs="$work/stubs"
log="$work/calls.log"
mkdir -p "$stubs"
export ENSURE_DEPS_LOG="$log"

cat >"$stubs/npm" <<'EOF'
#!/usr/bin/env bash
echo "npm $* @ $(basename "$PWD")" >>"$ENSURE_DEPS_LOG"
[ -z "${FAKE_NPM_FAIL:-}" ] || exit 1
rm -rf node_modules && mkdir -p node_modules/.bin
EOF
cat >"$stubs/node" <<'EOF'
#!/usr/bin/env bash
case "$1" in
    -p) echo "${FAKE_NODE_ABI:-127}" ;;
    -e) [ -f "$FAKE_BROWSER" ] ;;
    *) exit 2 ;;
esac
EOF
cat >"$stubs/npx" <<'EOF'
#!/usr/bin/env bash
echo "npx $*" >>"$ENSURE_DEPS_LOG"
touch "$FAKE_BROWSER"
EOF
cat >"$stubs/python3" <<'EOF'
#!/usr/bin/env bash
case "$1" in
    -c) echo "${FAKE_PY_VERSION:-3.12.0}" ;;
    -m)
        echo "python3 $*" >>"$ENSURE_DEPS_LOG"
        venv="${*: -1}"
        rm -rf "$venv" && mkdir -p "$venv/bin"
        printf '#!/usr/bin/env bash\necho "pip $*" >>"$ENSURE_DEPS_LOG"\n' >"$venv/bin/pip"
        chmod +x "$venv/bin/pip"
        ;;
    *) exit 2 ;;
esac
EOF
chmod +x "$stubs"/*
# Only the stubs and system tools: a real node, npm, fnm, or python3 elsewhere
# on PATH must not leak in.
export PATH="$stubs:/usr/bin:/bin"
export FAKE_BROWSER="$work/browser-installed"

repo="$work/repo"
mkdir -p "$repo/ui"
cd "$repo"
git init -q
echo '{"lockfileVersion": 3}' >ui/package-lock.json
echo '{"name": "ui"}' >ui/package.json
echo 'ruff==1.0' >requirements-dev.txt

fail() {
    echo "ensure-deps test: $*" >&2
    echo "--- calls ---" >&2
    cat "$log" >&2 || true
    exit 1
}
calls() { grep -c "$1" "$log" || true; }

# JS: a fresh tree installs with a frozen npm ci.
: >"$log"
bash "$script" js ui 2>/dev/null
[ "$(calls '^npm ci ')" = 1 ] || fail "fresh tree did not run npm ci once"
grep -q -- '--prefer-offline' "$log" || fail "npm ci not offline-preferred"
[ -f ui/node_modules/.rundale-deps.sha256 ] || fail "no stamp after install"

# Unchanged lockfile: no install work.
: >"$log"
bash "$script" js ui 2>/dev/null
[ ! -s "$log" ] || fail "second run with unchanged lockfile installed again"

# Calling from a subdirectory resolves DIR against the repository root.
(cd ui && bash "$script" js ui 2>/dev/null)
[ ! -s "$log" ] || fail "subdirectory call reinstalled"

# A changed lockfile, manifest, or Node ABI reinstalls.
echo '{"lockfileVersion": 3, "x": 1}' >ui/package-lock.json
bash "$script" js ui 2>/dev/null
[ "$(calls '^npm ci ')" = 1 ] || fail "changed lockfile did not reinstall"
echo '{"name": "ui", "x": 1}' >ui/package.json
bash "$script" js ui 2>/dev/null
[ "$(calls '^npm ci ')" = 2 ] || fail "changed package.json did not reinstall"
FAKE_NODE_ABI=137 bash "$script" js ui 2>/dev/null
[ "$(calls '^npm ci ')" = 3 ] || fail "changed Node ABI did not reinstall"

# A wiped node_modules reinstalls.
rm -rf ui/node_modules
: >"$log"
FAKE_NODE_ABI=137 bash "$script" js ui 2>/dev/null
[ "$(calls '^npm ci ')" = 1 ] || fail "missing node_modules did not reinstall"

# A failed install fails loudly and leaves no stamp, so the next run retries.
echo '{"lockfileVersion": 3, "x": 2}' >ui/package-lock.json
if FAKE_NPM_FAIL=1 bash "$script" js ui 2>"$work/err"; then
    fail "failed npm ci reported success"
fi
grep -q "npm ci failed" "$work/err" || fail "failed npm ci gave no message"
[ ! -f ui/node_modules/.rundale-deps.sha256 ] || fail "failed install left a stamp"
: >"$log"
bash "$script" js ui 2>/dev/null
[ "$(calls '^npm ci ')" = 1 ] || fail "run after a failed install did not retry"

# A missing lockfile is an error, not a skip.
if bash "$script" js nowhere 2>"$work/err"; then fail "missing lockfile passed"; fi
grep -q "package-lock.json not found" "$work/err" || fail "missing lockfile gave no message"

# Python: fresh venv, then no-op, then reinstall on a requirements change or a
# different interpreter.
: >"$log"
bash "$script" py 2>/dev/null
[ "$(calls '^python3 -m venv --clear .venv-dev')" = 1 ] || fail "fresh tree did not create .venv-dev"
[ "$(calls '^pip install --quiet -r requirements-dev.txt')" = 1 ] || fail "requirements not installed"
: >"$log"
bash "$script" py 2>/dev/null
[ ! -s "$log" ] || fail "second py run with unchanged requirements installed again"
echo 'ruff==1.1' >requirements-dev.txt
bash "$script" py 2>/dev/null
[ "$(calls '^pip install --quiet -r')" = 1 ] || fail "changed requirements did not reinstall"
FAKE_PY_VERSION=3.13.0 bash "$script" py 2>/dev/null
[ "$(calls '^pip install --quiet -r')" = 2 ] || fail "new Python did not rebuild the venv"

# Playwright: installs Chromium only when the installed Playwright lacks it.
: >"$log"
bash "$script" playwright ui 2>/dev/null
[ "$(calls '^npx playwright install chromium')" = 1 ] || fail "missing browser not installed"
: >"$log"
bash "$script" playwright ui 2>/dev/null
[ ! -s "$log" ] || fail "present browser installed again"

# External tools: present passes silently, missing fails with the hint.
bash "$script" tool git "unused" || fail "present tool failed"
if bash "$script" tool no-such-tool-xyz "brew install xyz" 2>"$work/err"; then
    fail "missing tool passed"
fi
grep -q "brew install xyz" "$work/err" || fail "missing tool did not name its install command"

# No Node at all fails instead of skipping.
rm "$stubs/node"
if bash "$script" js ui 2>"$work/err"; then fail "missing node passed"; fi
grep -q "node/npm not found" "$work/err" || fail "missing node gave no message"

echo "ensure-deps: ok"
