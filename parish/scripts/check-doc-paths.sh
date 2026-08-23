#!/usr/bin/env bash
#
# Harness sensor: fail if a stable path cited by an agent doc, or a relative
# Markdown link in an active project document, doesn't exist on disk.
#
# Rationale: OpenAI's harness-engineering post recommends mechanically
# enforcing cross-linked design docs so agents can trust the repo as their
# authoritative map. Without this, docs drift — e.g., architecture.md can
# describe a `parish-core/src/world/` subtree that hasn't lived at that path
# for months — and every agent reading the doc starts with a wrong model.
#
# Scope:
#  - Matches backtick-delimited tokens in agent docs that begin with one of the
#    known repo roots (parish/, crates/, apps/, docs/, mods/, testing/, deploy/,
#    assets/, scripts/, .skills/).
#  - Checks ordinary relative Markdown links in every tracked Markdown document.
#  - Skips globs (*), template vars ({...}), URLs, anchors, and fenced code
#    examples. Trailing-slash directory refs work as expected.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

ROOT_ALT='(parish|crates|apps|docs|mods|testing|deploy|assets|scripts|\.agents|\.claude|\.codex)'

# Source docs: docs/agent/*.md plus the repo-root agent files (CLAUDE.md is a
# symlink to AGENTS.md so we deduplicate by checking it isn't a symlink).
sources=()
while IFS= read -r line; do
    sources+=("$line")
done < <(
    find docs/agent -type f -name '*.md' 2>/dev/null
    [[ -f AGENTS.md ]] && echo AGENTS.md
    [[ -f CLAUDE.md && ! -L CLAUDE.md ]] && echo CLAUDE.md
)

missing=0
checked=0

for doc in "${sources[@]}"; do
    # Pull every backtick-quoted token that starts with a known repo root.
    while IFS= read -r path; do
        [[ -z "$path" ]] && continue
        # Skip globs, templates, URLs, code fragments.
        [[ "$path" == *'*'* ]] && continue
        [[ "$path" == *'{'* ]] && continue
        [[ "$path" == http* ]] && continue
        [[ "$path" == *'...'* ]] && continue
        # Skip gitignored paths (build outputs like `parish/target/...`):
        # docs may legitimately cite them, but they never exist on a fresh
        # checkout, so existence is not a meaningful sensor for them.
        # Query both `path` and `path/`: dir-only ignore patterns (trailing
        # slash, e.g. `parish/target/`) never match a slashless NONEXISTENT
        # path, and check-ignore exits 0 if any given path is ignored.
        # (output silenced instead of -q: --quiet is fatal with more than one
        # pathspec. stderr too: check-ignore is fatal-but-harmless on paths
        # that traverse a symlink, e.g. `.claude/skills/...`; those fall
        # through to the normal existence check.)
        git check-ignore -- "$path" "$path/" >/dev/null 2>&1 && continue
        # Normalise: drop trailing slash so directory refs match `test -e`.
        path="${path%/}"

        checked=$((checked + 1))
        if [[ ! -e "$path" ]]; then
            echo "::error file=$doc::cited path does not exist: $path" >&2
            missing=$((missing + 1))
        fi
    done < <(
        grep -oE "\`${ROOT_ALT}/[A-Za-z0-9_./+-]+\`" "$doc" \
            | tr -d '`' \
            | sort -u
    )
done

# Then validate ordinary Markdown links across active project documentation.
# `git ls-files` keeps the scan deterministic and avoids generated local docs.
markdown_checked=0
while IFS= read -r doc; do
    # Skip fenced examples: a documentation example may intentionally use a
    # placeholder link, while prose links should always resolve in the checkout.
    while IFS= read -r path; do
        [[ -z "$path" ]] && continue
        path="${path#<}"
        path="${path%>}"
        [[ "$path" == \#* ]] && continue
        [[ "$path" == http:* || "$path" == https:* || "$path" == mailto:* || "$path" == tel:* || "$path" == data:* || "$path" == git+* ]] && continue
        [[ "$path" =~ ^[0-9]+$ ]] && continue

        # A local fragment points at a valid file even if this sensor does not
        # validate its heading. Query strings are likewise irrelevant to the
        # filesystem path.
        path="${path%%\#*}"
        path="${path%%\?*}"
        [[ -z "$path" ]] && continue
        path="${path//%20/ }"

        markdown_checked=$((markdown_checked + 1))
        if [[ ! -e "$(dirname "$doc")/$path" ]]; then
            echo "::error file=$doc::Markdown link target does not exist: $path" >&2
            missing=$((missing + 1))
        fi
    done < <(
        awk '
            /^```|^~~~/ { fenced = !fenced; next }
            !fenced {
                line = $0
                while (match(line, /`[^`]*`/)) {
                    line = substr(line, 1, RSTART - 1) substr(line, RSTART + RLENGTH)
                }
                print line
            }
        ' "$doc" \
            | grep -oE '\]\((<[^>]+>|[^ )]+)' \
            | sed -E 's/^\]\(//; s/^<//; s/>$//' \
            || true
    )
done < <(git ls-files '*.md')

if ((missing > 0)); then
    echo "" >&2
    echo "FAIL: $missing cited path(s) missing (checked $checked)." >&2
    echo "Either update the doc to reflect the repo, or create the path." >&2
    exit 1
fi

echo "OK: every cited agent path and active Markdown link exists ($checked agent paths; $markdown_checked Markdown links)."
