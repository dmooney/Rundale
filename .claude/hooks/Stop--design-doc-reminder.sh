#!/usr/bin/env bash
set -euo pipefail
exec >&2

# Stop hook: enforce that design docs are updated alongside any non-trivial
# code change. Blocks (exit 2) if code files were modified on this branch
# but no docs/design/ or CLAUDE.md files were updated in any commit.
#
# "Non-trivial" = more than a handful of changed lines across code files.
# Trivial threshold: <= 5 net added lines across all code files.

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Code file extensions to track
CODE_PATTERNS=('*.rs' '*.ts' '*.svelte' '*.js' '*.json' '*.toml')
# Exclude patterns (lockfiles, generated files, test fixtures)
EXCLUDE_PATTERNS=('package-lock.json' 'Cargo.lock' '*.snap' 'tests/fixtures/*')

# Determine the merge base with main/master to scope the check to this branch
BASE_BRANCH="main"
if ! git rev-parse --verify "$BASE_BRANCH" &>/dev/null; then
    BASE_BRANCH="master"
fi
if ! git rev-parse --verify "$BASE_BRANCH" &>/dev/null; then
    BASE_BRANCH=""
fi

# Build git diff path specs
PATHSPECS=()
for p in "${CODE_PATTERNS[@]}"; do
    PATHSPECS+=("$p")
done

EXCLUDES=()
for p in "${EXCLUDE_PATTERNS[@]}"; do
    EXCLUDES+=(":(exclude)$p")
done

# Collect code changes: committed on branch + uncommitted + unstaged
COMMITTED_STAT=""
if [[ -n "$BASE_BRANCH" ]]; then
    MERGE_BASE=$(git merge-base "$BASE_BRANCH" HEAD 2>/dev/null || echo "")
    if [[ -n "$MERGE_BASE" ]]; then
        COMMITTED_STAT=$(git diff "$MERGE_BASE"..HEAD --stat -- "${PATHSPECS[@]}" "${EXCLUDES[@]}" 2>/dev/null || true)
    fi
fi

UNCOMMITTED_STAT=$(git diff HEAD --stat -- "${PATHSPECS[@]}" "${EXCLUDES[@]}" 2>/dev/null || true)
UNSTAGED_STAT=$(git diff --stat -- "${PATHSPECS[@]}" "${EXCLUDES[@]}" 2>/dev/null || true)

ALL_STAT="${COMMITTED_STAT}${UNCOMMITTED_STAT}${UNSTAGED_STAT}"

# No code changes at all — nothing to check
if [[ -z "$ALL_STAT" ]]; then
    exit 0
fi

# Count net added lines to gauge significance
COMMITTED_ADDS=0
if [[ -n "$BASE_BRANCH" && -n "${MERGE_BASE:-}" ]]; then
    COMMITTED_ADDS=$(git diff "$MERGE_BASE"..HEAD --numstat -- "${PATHSPECS[@]}" "${EXCLUDES[@]}" 2>/dev/null \
        | awk '{s+=$1} END {print s+0}' || echo 0)
fi

UNCOMMITTED_ADDS=$(git diff HEAD --numstat -- "${PATHSPECS[@]}" "${EXCLUDES[@]}" 2>/dev/null \
    | awk '{s+=$1} END {print s+0}' || echo 0)

UNSTAGED_ADDS=$(git diff --numstat -- "${PATHSPECS[@]}" "${EXCLUDES[@]}" 2>/dev/null \
    | awk '{s+=$1} END {print s+0}' || echo 0)

TOTAL_ADDS=$(( COMMITTED_ADDS + UNCOMMITTED_ADDS + UNSTAGED_ADDS ))

# Trivial threshold: skip if <= 5 net added lines
if [[ "$TOTAL_ADDS" -le 5 ]]; then
    exit 0
fi

# Check if any design docs were updated in commits on this branch
DESIGN_COMMITTED=""
if [[ -n "$BASE_BRANCH" && -n "${MERGE_BASE:-}" ]]; then
    DESIGN_COMMITTED=$(git diff --name-only "$MERGE_BASE"..HEAD 2>/dev/null | grep -E 'docs/design/|CLAUDE\.md' || true)
fi

# Also check uncommitted/unstaged design doc changes
DESIGN_UNCOMMITTED=$(git diff --name-only HEAD 2>/dev/null | grep -E 'docs/design/|CLAUDE\.md' || true)
DESIGN_UNSTAGED=$(git diff --name-only 2>/dev/null | grep -E 'docs/design/|CLAUDE\.md' || true)

if [[ -n "$DESIGN_COMMITTED" || -n "$DESIGN_UNCOMMITTED" || -n "$DESIGN_UNSTAGED" ]]; then
    exit 0
fi

echo "=== Design Doc Update Required ==="
echo "Non-trivial code changes detected (~${TOTAL_ADDS} lines added) but"
echo "no docs/design/ or CLAUDE.md files were updated in the commits."
echo ""
echo "Before declaring done, update and commit the relevant design doc(s):"
echo "  - Data structures and their fields"
echo "  - Architecture (data flow, shared state, IPC)"
echo "  - Capacity/limits and design rationale"
echo ""
echo "Key design docs: inference-pipeline.md, debug-ui.md, npc-system.md,"
echo "  cognitive-lod.md, gui-design.md, overview.md"
echo "=================================="

exit 2
