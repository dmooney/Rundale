#!/usr/bin/env bash
# check-docs-format.sh checks tracked Markdown, ignores git-ignored files, and
# fails (not skips) when the root dev tooling is missing.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
checker="$repo_root/limerick/scripts/check-docs-format.sh"
test_repo="$(mktemp -d)"
trap 'rm -rf "$test_repo"' EXIT

cd "$test_repo"
git init -q
cp "$repo_root/.markdownlint-cli2.jsonc" "$repo_root/prettier.config.js" "$repo_root/.prettierignore" .
# shellcheck disable=SC2016 # literal Markdown backticks
printf '# Good\n\nTry `/debug` here.\n' >good.md
printf 'ignored/\nnode_modules\n' >.gitignore
git add good.md .gitignore .markdownlint-cli2.jsonc prettier.config.js .prettierignore

if bash "$checker" >out.txt 2>&1; then
    echo "passed without node_modules" >&2
    exit 1
fi
grep -q "npm ci" out.txt || {
    cat out.txt >&2
    exit 1
}

ln -s "$repo_root/node_modules" node_modules
bash "$checker" >out.txt 2>&1 || {
    cat out.txt >&2
    echo "clean tracked docs failed" >&2
    exit 1
}

# An ignored file with a lint error (a worktree copy, say) is not checked.
mkdir ignored
# shellcheck disable=SC2016 # literal Markdown backticks
printf '# Bad\n\nTry `/debug ` here.\n' >ignored/bad.md
bash "$checker" >out.txt 2>&1 || {
    cat out.txt >&2
    echo "ignored file was linted" >&2
    exit 1
}

# The same error in a tracked file fails.
cp ignored/bad.md bad.md
git add bad.md
if bash "$checker" >out.txt 2>&1; then
    echo "tracked MD038 error passed" >&2
    exit 1
fi
grep -q "MD038" out.txt || {
    cat out.txt >&2
    exit 1
}
echo "check-docs-format: ok"
