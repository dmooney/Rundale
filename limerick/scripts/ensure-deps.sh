#!/usr/bin/env bash
# Install a gate's dependencies when they are missing or stale, and do nothing
# when they are current. Every `just` gate that needs node_modules, the Python
# dev venv, Playwright's browser, or an external tool runs this first, so a
# fresh worktree or clone needs no manual install step and a gate never skips
# for want of a dependency (#2154).
#
#   ensure-deps.sh js DIR          frozen npm install in DIR (package-lock.json)
#   ensure-deps.sh py              .venv-dev from requirements-dev.txt
#   ensure-deps.sh playwright DIR  Playwright's Chromium for the project in DIR
#   ensure-deps.sh tool NAME HINT  fail, naming HINT, when NAME is not on PATH
#
# Freshness is a content stamp, not a timestamp: git checkouts reset mtimes, so
# "lockfile newer than node_modules" would reinstall after every branch switch.
# The stamp hashes the lockfile and manifest (or requirements file) together
# with the runtime ABI, is written only after a successful install, and lives
# inside the installed tree, so a wiped or half-written install never matches.
#
# Paths are relative to the repository root. Set ENSURE_DEPS_VERBOSE=1 to log
# up-to-date checks too.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

stamp_name=".rundale-deps.sha256"

die() {
    echo "ensure-deps: $*" >&2
    exit 1
}

note() {
    echo "ensure-deps: $*" >&2
}

hash_stdin() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 | cut -d' ' -f1
    else
        die "neither sha256sum nor shasum is on PATH"
    fi
}

# Put fnm's Node on PATH when fnm manages it; a Node already on PATH is used
# as is.
use_node() {
    if command -v fnm >/dev/null 2>&1; then
        eval "$(fnm env)"
    fi
    command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 \
        || die "node/npm not found on PATH; install Node $(cat .nvmrc 2>/dev/null || echo 24) (see 'just setup')"
}

# True when STAMP_FILE holds exactly WANT.
stamp_matches() {
    [ -f "$1" ] && [ "$(cat "$1")" = "$2" ]
}

ensure_js() {
    local dir="${1:?usage: ensure-deps.sh js DIR}"
    [ -f "$dir/package-lock.json" ] || die "$dir/package-lock.json not found"
    use_node
    # The module ABI changes when Node's major version does, which breaks
    # native addons built by the previous install.
    local want
    want="$(cat "$dir/package-lock.json" "$dir/package.json" \
        <(node -p 'process.versions.modules') | hash_stdin)"
    local stamp="$dir/node_modules/$stamp_name"
    if stamp_matches "$stamp" "$want"; then
        [ -z "${ENSURE_DEPS_VERBOSE:-}" ] || note "$dir/node_modules is current"
        return 0
    fi
    note "installing $dir/node_modules (npm ci)..."
    rm -f "$stamp"
    (cd "$dir" && npm ci --prefer-offline --no-audit --no-fund --loglevel error) \
        || die "npm ci failed in $dir"
    printf '%s\n' "$want" >"$stamp"
}

ensure_py() {
    local venv=".venv-dev" reqs="requirements-dev.txt"
    [ -f "$reqs" ] || die "$reqs not found"
    command -v python3 >/dev/null 2>&1 || die "python3 not found on PATH"
    local want
    want="$(cat "$reqs" <(python3 -c 'import sys; print(sys.version)') | hash_stdin)"
    local stamp="$venv/$stamp_name"
    if stamp_matches "$stamp" "$want"; then
        [ -z "${ENSURE_DEPS_VERBOSE:-}" ] || note "$venv is current"
        return 0
    fi
    note "installing $venv from $reqs..."
    rm -f "$stamp"
    # --clear rebuilds a venv left by another Python, which pip cannot repair.
    python3 -m venv --clear "$venv" \
        || die "python3 -m venv failed (on Debian/Ubuntu: apt install python3-venv)"
    "$venv/bin/pip" install --quiet --upgrade pip || die "pip upgrade failed in $venv"
    "$venv/bin/pip" install --quiet -r "$reqs" || die "pip install -r $reqs failed"
    printf '%s\n' "$want" >"$stamp"
}

ensure_playwright() {
    local dir="${1:?usage: ensure-deps.sh playwright DIR}"
    ensure_js "$dir"
    # Ask the installed Playwright where its Chromium lives, so a browser
    # cache for another Playwright version does not count.
    local check='const fs = require("fs"); const { chromium } = require("@playwright/test");
process.exit(fs.existsSync(chromium.executablePath()) ? 0 : 1);'
    if (cd "$dir" && node -e "$check"); then
        [ -z "${ENSURE_DEPS_VERBOSE:-}" ] || note "Playwright Chromium is installed"
        return 0
    fi
    note "installing Playwright Chromium..."
    (cd "$dir" && npx playwright install chromium) \
        || die "npx playwright install chromium failed (see above; missing system libraries need: npx playwright install-deps chromium)"
}

ensure_tool() {
    local name="${1:?usage: ensure-deps.sh tool NAME HINT}" hint="${2:?usage: ensure-deps.sh tool NAME HINT}"
    command -v "$name" >/dev/null 2>&1 || die "$name not found on PATH; install it with: $hint"
}

cmd="${1:-}"
[ $# -gt 0 ] && shift
case "$cmd" in
    js) ensure_js "$@" ;;
    py) ensure_py ;;
    playwright) ensure_playwright "$@" ;;
    tool) ensure_tool "$@" ;;
    *) die "usage: ensure-deps.sh js DIR | py | playwright DIR | tool NAME HINT" ;;
esac
