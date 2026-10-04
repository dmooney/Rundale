#!/usr/bin/env bash
# Docs/data formatting gate: Prettier + markdownlint over *tracked* files.
#
# Mirrors the "Docs/data formatting (Prettier + markdownlint)" CI job. CI runs
# on a clean checkout, so only tracked files are checked: feeding `git ls-files`
# (instead of the package.json globs) keeps git-ignored local files, such as
# `.claude/worktrees/**` copies of other branches, from tripping a check CI
# never sees. Prettier and markdownlint still apply their own ignore configs.
#
# Fails when the root dev tooling is missing (run `npm ci` at the repo root);
# a gate that cannot run has not passed. Used by `just check`, `just verify`,
# and `.githooks/pre-push`.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [ ! -x node_modules/.bin/prettier ] || [ ! -x node_modules/.bin/markdownlint-cli2 ]; then
    echo "docs-format: root dev tooling not installed; run 'npm ci' at the repo root." >&2
    exit 1
fi

# NUL-delimited read loop (works on macOS's bash 3.2, which lacks `mapfile`).
# Skip symlinks: CI's glob silently skips them, but passing them explicitly
# makes Prettier error. markdownlint takes `--no-globs`, or the config's
# `globs` would add every Markdown file on disk back to the list.
fmt_files=()
while IFS= read -r -d '' f; do [ -L "$f" ] || fmt_files+=("$f"); done \
    < <(git ls-files -z -- '*.md' '*.json' '*.jsonc' '*.yml' '*.yaml')
md_files=()
while IFS= read -r -d '' f; do [ -L "$f" ] || md_files+=("$f"); done \
    < <(git ls-files -z -- '*.md')

if [ "${#fmt_files[@]}" -gt 0 ]; then
    if ! node_modules/.bin/prettier --check --log-level warn "${fmt_files[@]}"; then
        echo "" >&2
        echo "docs-format: Prettier found unformatted docs. Fix with: just fmt-docs (or npm run format)" >&2
        exit 1
    fi
fi
if [ "${#md_files[@]}" -gt 0 ]; then
    if ! node_modules/.bin/markdownlint-cli2 --no-globs "${md_files[@]}"; then
        echo "" >&2
        echo "docs-format: markdownlint failed. See errors above." >&2
        exit 1
    fi
fi
echo "docs-format: OK."
