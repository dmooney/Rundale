#!/usr/bin/env bash
set -euo pipefail

# Stop hook: remind to update design docs when significant code changes are made
# Triggers when new structs, public functions, or module-level changes appear
# in .rs files but no docs/design/ files were touched.

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Collect staged + unstaged .rs changes
RS_DIFF=$(git diff HEAD --unified=0 -- '*.rs' 2>/dev/null || true)
RS_UNSTAGED=$(git diff --unified=0 -- '*.rs' 2>/dev/null || true)
ALL_DIFF="${RS_DIFF}${RS_UNSTAGED}"

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

# Check if any design docs were also updated
DESIGN_CHANGED=$(git diff --name-only HEAD 2>/dev/null | grep 'docs/design/' || true)
DESIGN_UNSTAGED=$(git diff --name-only 2>/dev/null | grep 'docs/design/' || true)

if [[ -n "$DESIGN_CHANGED" || -n "$DESIGN_UNSTAGED" ]]; then
    exit 0
fi

echo "=== Design Doc Reminder ==="
echo "Significant new public API detected (structs, functions, modules) but"
echo "no docs/design/ files were updated."
echo ""
echo "Before finishing, update the relevant design doc(s) in docs/design/ with:"
echo "  - Data structures and their fields"
echo "  - Architecture (data flow, shared state, IPC)"
echo "  - Capacity/limits and design rationale"
echo ""
echo "Key design docs: inference-pipeline.md, debug-ui.md, npc-system.md,"
echo "  cognitive-lod.md, gui-design.md, overview.md"
echo "============================="

exit 0
