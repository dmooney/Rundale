#!/usr/bin/env bash
# Publish a PR's evidence page (video, screenshots, run summaries) to the
# public rundale-pages repository, served by GitHub Pages at
# https://dmooney.github.io/rundale-pages/pr/<number>/.
#
#   bash limerick/scripts/publish-pr-page.sh <pr-number> <directory>
#
# <directory> must contain an index.html; its files replace pr/<number>/.
# The root index.html, listing every PR page, is regenerated on each publish.
# Videos are .mp4 (H.264) so they play in the browser; a short .gif can be
# embedded inline in the PR body from the published URL.
#
# Environment overrides (tests use them):
#   RUNDALE_PAGES_REMOTE    git remote (default: github.com/dmooney/rundale-pages)
#   RUNDALE_PAGES_CHECKOUT  local clone (default: ~/.cache/limerick/rundale-pages)
#   RUNDALE_PAGES_BASE_URL  public URL (default: https://dmooney.github.io/rundale-pages)
set -euo pipefail

remote="${RUNDALE_PAGES_REMOTE:-https://github.com/dmooney/rundale-pages.git}"
checkout="${RUNDALE_PAGES_CHECKOUT:-${XDG_CACHE_HOME:-$HOME/.cache}/limerick/rundale-pages}"
base_url="${RUNDALE_PAGES_BASE_URL:-https://dmooney.github.io/rundale-pages}"
# GitHub rejects files over 100 MB; keep well under it.
max_file_bytes=$((50 * 1024 * 1024))

usage() {
    echo "usage: publish-pr-page.sh <pr-number> <directory-with-index.html>" >&2
    exit 2
}

[[ $# -eq 2 ]] || usage
pr="$1"
source_dir="$2"
[[ "$pr" =~ ^[1-9][0-9]*$ ]] || {
    echo "publish-pr-page: PR number must be a positive integer: $pr" >&2
    exit 2
}
[[ -f "$source_dir/index.html" ]] || {
    echo "publish-pr-page: $source_dir/index.html is missing" >&2
    exit 2
}
while IFS= read -r -d '' file; do
    size="$(wc -c <"$file" | tr -d ' ')"
    if ((size > max_file_bytes)); then
        echo "publish-pr-page: $file is $size bytes; re-encode it under 50 MB" >&2
        exit 1
    fi
done < <(find "$source_dir" -type f -print0)

if [[ -d "$checkout/.git" ]]; then
    git -C "$checkout" fetch -q origin
    # A brand-new repository has no main branch yet.
    if git -C "$checkout" rev-parse -q --verify origin/main >/dev/null; then
        git -C "$checkout" checkout -q -B main origin/main
    fi
else
    mkdir -p "$(dirname "$checkout")"
    git clone -q "$remote" "$checkout"
fi

target="$checkout/pr/$pr"
rm -rf "$target"
mkdir -p "$target"
cp -R "$source_dir"/. "$target"/
# Serve files as-is (no Jekyll processing of underscores or Markdown).
touch "$checkout/.nojekyll"

# Regenerate the root listing, newest PR first, titled from each page.
{
    cat <<'HTML'
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rundale pages</title>
<style>
:root { --bg: #f6f7f4; --fg: #1d2420; --muted: #5d6862; --accent: #2f6b4f; }
@media (prefers-color-scheme: dark) { :root { --bg: #141815; --fg: #e4e9e4; --muted: #9aa69f; --accent: #7cc4a0; color-scheme: dark; } }
body { background: var(--bg); color: var(--fg); font: 16px/1.5 system-ui, sans-serif; margin: 0; }
main { max-width: 760px; margin: 0 auto; padding: 32px 20px; }
h1 { font: 600 1.6rem/1.2 Georgia, serif; }
li { margin: 6px 0; }
a { color: var(--accent); }
.muted { color: var(--muted); }
</style>
</head>
<body>
<main>
<h1>Rundale pages</h1>
<p class="muted">Evidence for <a href="https://github.com/dmooney/Rundale">Rundale</a> pull requests: recordings and run summaries.</p>
<ul>
HTML
    for dir in $(find "$checkout/pr" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort -rn); do
        page_title="$(sed -n 's:.*<title>\(.*\)</title>.*:\1:p' "$checkout/pr/$dir/index.html" | head -n 1)"
        printf '<li><a href="pr/%s/">PR #%s</a> %s</li>\n' "$dir" "$dir" "${page_title:+· $page_title}"
    done
    cat <<'HTML'
</ul>
</main>
</body>
</html>
HTML
} >"$checkout/index.html"

git -C "$checkout" add -A
if git -C "$checkout" diff --cached --quiet; then
    echo "publish-pr-page: pr/$pr is already up to date"
else
    git -C "$checkout" commit -q -m "pr/$pr: publish evidence page"
    git -C "$checkout" push -q origin HEAD:main
fi
echo "$base_url/pr/$pr/"
