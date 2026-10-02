#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
script="$repo_root/limerick/scripts/publish-pr-page.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Pin HEAD to main as GitHub does; a runner's git may default to master,
# and a clone would then check out nothing.
git init -q --bare --initial-branch=main "$work/remote.git"
export RUNDALE_PAGES_REMOTE="$work/remote.git"
export RUNDALE_PAGES_CHECKOUT="$work/checkout"
export RUNDALE_PAGES_BASE_URL="https://pages.example.invalid"
export GIT_AUTHOR_NAME="Pages Test" GIT_AUTHOR_EMAIL="pages-test@example.invalid"
export GIT_COMMITTER_NAME="Pages Test" GIT_COMMITTER_EMAIL="pages-test@example.invalid"

fail() {
    echo "publish-pr-page test: $*" >&2
    exit 1
}

page() {
    mkdir -p "$1"
    printf '<!doctype html><title>%s</title><p>%s</p>\n' "$2" "$2" >"$1/index.html"
}

# First publish into an empty repository creates main, the page, and the listing.
page "$work/a" "First page"
printf 'video\n' >"$work/a/clip.mp4"
url="$(bash "$script" 12 "$work/a")"
[[ "$url" == "https://pages.example.invalid/pr/12/" ]] || fail "unexpected URL: $url"
git clone -q "$work/remote.git" "$work/view"
[[ -f "$work/view/pr/12/clip.mp4" ]] || fail "video was not published"
[[ -f "$work/view/.nojekyll" ]] || fail ".nojekyll was not written"
grep -q 'href="pr/12/">PR #12</a> · First page' "$work/view/index.html" || fail "listing lacks PR 12"

# A second PR is listed first; republishing replaces the directory's files.
page "$work/b" "Second page"
bash "$script" 9 "$work/b" >/dev/null
page "$work/c" "First page, revised"
bash "$script" 12 "$work/c" >/dev/null
git -C "$work/view" pull -q
[[ ! -e "$work/view/pr/12/clip.mp4" ]] || fail "republish kept a removed file"
grep -q 'First page, revised' "$work/view/pr/12/index.html" || fail "republish did not replace the page"
first="$(grep -o 'pr/[0-9]*/' "$work/view/index.html" | head -n 1)"
[[ "$first" == "pr/12/" ]] || fail "listing is not newest first: $first"

# Publishing identical content makes no commit.
before="$(git -C "$work/view" rev-parse HEAD)"
out="$(bash "$script" 12 "$work/c")"
grep -q 'already up to date' <<<"$out" || fail "identical publish was not reported as up to date"
git -C "$work/view" pull -q
[[ "$(git -C "$work/view" rev-parse HEAD)" == "$before" ]] || fail "identical publish made a commit"

# Bad input is rejected before anything is pushed.
if bash "$script" abc "$work/c" 2>/dev/null; then fail "accepted a non-numeric PR"; fi
mkdir -p "$work/empty"
if bash "$script" 3 "$work/empty" 2>/dev/null; then fail "accepted a directory without index.html"; fi

echo "publish-pr-page tests passed"
