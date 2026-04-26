#!/usr/bin/env bash
#
# Harness sensor: fail if any filesystem path cited inside a backtick-quoted
# token in docs/agent/*.md, AGENTS.md, or CLAUDE.md doesn't exist on disk.
#
# Rationale: OpenAI's harness-engineering post recommends mechanically
# enforcing cross-linked design docs so agents can trust the repo as their
# authoritative map. Without this, docs drift — e.g., architecture.md can
# describe a `parish-core/src/world/` subtree that hasn't lived at that path
# for months — and every agent reading the doc starts with a wrong model.
#
# Scope:
#  - Matches backtick-delimited tokens that begin with one of the known
#    repo roots (crates/, apps/, docs/, mods/, testing/, deploy/, assets/,
#    scripts/, .skills/).
#  - Skips globs (*), template vars ({...}), URLs, and the ../../ relative
#    fragments used in code snippets.
#  - Treats trailing-slash directory refs (`crates/parish-core/`) the same
#    as file refs.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

ROOT_ALT='(crates|apps|docs|mods|testing|deploy|assets|scripts|\.agents|\.claude|\.codex)'

# Source docs: docs/agent/*.md plus the repo-root agent files (CLAUDE.md is a
# symlink to AGENTS.md so we deduplicate by checking it isn't a symlink).
mapfile -t sources < <(
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

if (( missing > 0 )); then
    echo "" >&2
    echo "FAIL: $missing cited path(s) missing (checked $checked)." >&2
    echo "Either update the doc to reflect the repo, or create the path." >&2
    exit 1
fi

echo "OK: every cited path exists ($checked checked across ${#sources[@]} file(s))."
