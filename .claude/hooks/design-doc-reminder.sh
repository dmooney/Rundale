#!/usr/bin/env bash
set -euo pipefail

# Stop hook: enforce that design docs are updated when significant code changes
# are committed. Blocks (exit 2) if new public structs, functions, or modules
# appear in committed .rs changes but no docs/design/ files were included in
# any commit on the current branch.

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Determine the merge base with main/master to scope the check to this branch
BASE_BRANCH="main"
if ! git rev-parse --verify "$BASE_BRANCH" &>/dev/null; then
    BASE_BRANCH="master"
fi
if ! git rev-parse --verify "$BASE_BRANCH" &>/dev/null; then
    # No main/master — fall back to checking only uncommitted changes
    BASE_BRANCH=""
fi

# Collect the .rs diff: committed changes on this branch + any uncommitted changes
if [[ -n "$BASE_BRANCH" ]]; then
    MERGE_BASE=$(git merge-base "$BASE_BRANCH" HEAD 2>/dev/null || echo "")
    if [[ -n "$MERGE_BASE" ]]; then
        COMMITTED_DIFF=$(git diff "$MERGE_BASE"..HEAD --unified=0 -- '*.rs' 2>/dev/null || true)
    else
        COMMITTED_DIFF=""
    fi
else
    COMMITTED_DIFF=""
fi

UNCOMMITTED_DIFF=$(git diff HEAD --unified=0 -- '*.rs' 2>/dev/null || true)
UNSTAGED_DIFF=$(git diff --unified=0 -- '*.rs' 2>/dev/null || true)
ALL_DIFF="${COMMITTED_DIFF}${UNCOMMITTED_DIFF}${UNSTAGED_DIFF}"

if [[ -z "$ALL_DIFF" ]]; then
    exit 0
fi

# Look for signals of significant architectural changes:
#   - New pub struct/enum/type definitions
#   - New pub fn signatures
#   - New module declarations (pub mod)
SIGNIFICANT=$(echo "$ALL_DIFF" | grep -E '^\+.*(pub struct |pub enum |pub type |pub fn |pub mod |pub const )' || true)

if [[ -z "$SIGNIFICANT" ]]; then
    exit 0
fi

# Check if any design docs were updated in commits on this branch
if [[ -n "$BASE_BRANCH" && -n "${MERGE_BASE:-}" ]]; then
    DESIGN_COMMITTED=$(git diff --name-only "$MERGE_BASE"..HEAD 2>/dev/null | grep -E 'docs/design/|CLAUDE\.md' || true)
else
    DESIGN_COMMITTED=""
fi

# Also check uncommitted/unstaged design doc changes
DESIGN_UNCOMMITTED=$(git diff --name-only HEAD 2>/dev/null | grep -E 'docs/design/|CLAUDE\.md' || true)
DESIGN_UNSTAGED=$(git diff --name-only 2>/dev/null | grep -E 'docs/design/|CLAUDE\.md' || true)

if [[ -n "$DESIGN_COMMITTED" || -n "$DESIGN_UNCOMMITTED" || -n "$DESIGN_UNSTAGED" ]]; then
    exit 0
fi

echo "=== Design Doc Update Required ==="
echo "Significant new public API detected (structs, functions, modules) but"
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
