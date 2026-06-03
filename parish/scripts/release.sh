#!/usr/bin/env bash
# Bump release version across the workspace, then create a tagged commit.
#
# Usage:
#   parish/scripts/release.sh <version> [--dry-run] [--no-tag]
#
# <version>    — semver `MAJOR.MINOR.PATCH` (without the leading `v`)
# --dry-run    — show the would-be diff and exit without writing anything
# --no-tag     — write the version bumps and commit, but skip the `vX.Y.Z` tag
#
# Files touched:
#   parish/crates/parish-engine/Cargo.toml         [package].version
#   parish/crates/parish-tauri/tauri.conf.json  .version
#   parish/apps/ui/package.json                 .version
#   parish/Cargo.lock                           refreshed via `cargo update -p parish`
#
# The script is intentionally narrow: only the user-visible version sources
# are bumped. Internal leaf crates stay on their unpublished `0.1.0` baseline.
set -euo pipefail

usage() {
    cat <<'EOF' >&2
Usage: parish/scripts/release.sh <version> [--dry-run] [--no-tag]
EOF
    exit 2
}

VERSION=""
DRY_RUN=0
NO_TAG=0

while (("$#")); do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --no-tag) NO_TAG=1 ;;
        -h | --help) usage ;;
        -*)
            echo "Unknown flag: $1" >&2
            usage
            ;;
        *)
            if [[ -n "$VERSION" ]]; then
                echo "Multiple version arguments: '$VERSION' and '$1'" >&2
                usage
            fi
            VERSION="$1"
            ;;
    esac
    shift
done

[[ -n "$VERSION" ]] || usage

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "Version must be MAJOR.MINOR.PATCH (with optional -prerelease): got '$VERSION'" >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

CARGO_TOML="parish/crates/parish-engine/Cargo.toml"
TAURI_JSON="parish/crates/parish-tauri/tauri.conf.json"
UI_PKG="parish/apps/ui/package.json"

for f in "$CARGO_TOML" "$TAURI_JSON" "$UI_PKG"; do
    [[ -f "$f" ]] || {
        echo "Missing expected file: $f" >&2
        exit 1
    }
done

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required (sudo apt-get install jq)" >&2
    exit 1
fi

if ((DRY_RUN == 0)); then
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "Working tree is dirty — commit or stash before releasing." >&2
        git status --short >&2
        exit 1
    fi
    if git rev-parse "v$VERSION" >/dev/null 2>&1; then
        echo "Tag v$VERSION already exists locally." >&2
        exit 1
    fi
fi

# ─── Apply version bumps to a temporary tree ────────────────────────────────
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

cp "$CARGO_TOML" "$WORK_DIR/Cargo.toml.new"
cp "$TAURI_JSON" "$WORK_DIR/tauri.conf.json.new"
cp "$UI_PKG" "$WORK_DIR/package.json.new"

# parish-engine Cargo.toml: replace the `[package]` table's `version = "..."`.
# We only touch the first `version =` line, which sits in `[package]` per the
# convention used across the workspace.
awk -v new="$VERSION" '
    BEGIN { done = 0 }
    /^version = "/ && !done { print "version = \"" new "\""; done = 1; next }
    { print }
' "$CARGO_TOML" >"$WORK_DIR/Cargo.toml.new"

# tauri.conf.json: targeted line replacement to preserve unrelated formatting
# (jq would normalize inline arrays like `"icon": ["icons/icon.png"]`).
# Sanity-check first that the top-level `"version"` line exists and is unique.
tauri_matches=$(grep -cE '^[[:space:]]*"version"[[:space:]]*:' "$TAURI_JSON" || true)
if [[ "$tauri_matches" != "1" ]]; then
    echo "Expected exactly one top-level \"version\" line in $TAURI_JSON, found $tauri_matches" >&2
    exit 1
fi
sed -E "s|^([[:space:]]*\"version\"[[:space:]]*:[[:space:]]*\")[^\"]+(\".*)|\1${VERSION}\2|" \
    "$TAURI_JSON" >"$WORK_DIR/tauri.conf.json.new"

# package.json uses tabs; jq emits spaces. Convert leading 2-space indents
# back to tabs so we match the existing file exactly.
jq --indent 2 --arg v "$VERSION" '.version = $v' "$UI_PKG" \
    | sed -E 's/^(  )+/&/; :a; s/^(\t*)  /\1\t/; t a' \
        >"$WORK_DIR/package.json.new"

show_diff() {
    diff -u "$1" "$2" || true
}

echo "── Diff: $CARGO_TOML"
show_diff "$CARGO_TOML" "$WORK_DIR/Cargo.toml.new"
echo "── Diff: $TAURI_JSON"
show_diff "$TAURI_JSON" "$WORK_DIR/tauri.conf.json.new"
echo "── Diff: $UI_PKG"
show_diff "$UI_PKG" "$WORK_DIR/package.json.new"

if ((DRY_RUN == 1)); then
    echo
    echo "Dry run — no files written, no commit, no tag."
    exit 0
fi

# ─── Apply ──────────────────────────────────────────────────────────────────
cp "$WORK_DIR/Cargo.toml.new" "$CARGO_TOML"
cp "$WORK_DIR/tauri.conf.json.new" "$TAURI_JSON"
cp "$WORK_DIR/package.json.new" "$UI_PKG"

# Best-effort refresh of Cargo.lock so the new workspace-member version is
# recorded. `cargo check -p parish` is the only reliable way to update a
# local-only crate's lockfile entry; if deps aren't available offline it
# may fail, in which case CI's release build (no --locked) will refresh it.
if command -v cargo >/dev/null 2>&1; then
    (cd parish && cargo check -p parish --offline -q 2>/dev/null) \
        || (cd parish && cargo check -p parish -q 2>/dev/null) \
        || echo "warning: could not refresh Cargo.lock for parish $VERSION (continuing)" >&2
fi

git add "$CARGO_TOML" "$TAURI_JSON" "$UI_PKG" parish/Cargo.lock 2>/dev/null || true
git commit -m "chore(release): v$VERSION" >/dev/null
echo "Committed: chore(release): v$VERSION"

if ((NO_TAG == 0)); then
    git tag -a "v$VERSION" -m "Rundale v$VERSION"
    echo "Tagged:    v$VERSION"
    echo
    echo "Next steps:"
    echo "  git push origin <branch>     # e.g. main"
    echo "  git push origin v$VERSION    # triggers .github/workflows/release.yml"
else
    echo "Skipping tag (--no-tag)."
fi
