#!/usr/bin/env bash
# Stop hook: remind to update LEARNINGS.md if the session touched
# non-trivial source code. Non-blocking — prints to stderr.
#
# Rationale: LEARNINGS.md is a brief, hand-curated index of gotchas
# that aren't obvious from reading code or git history (e.g. seam
# locations, surprising default behaviours, package-name aliases).
# Most sessions will have nothing to add; this hook just nudges
# Claude to ask "did I learn anything worth saving here?" before
# closing out.
set -euo pipefail
trap 'rc=$?; printf "Stop hook %s failed (exit=%d) at line %d\n" "${BASH_SOURCE[0]##*/}" "$rc" "$LINENO" >&2' ERR
exec >&2

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# Source-code change detection: tracked diff vs HEAD AND untracked
# source files. Match the same source extensions other Stop hooks
# treat as "real code" — Rust, Svelte, TS/JS, Python, shell.
SRC_RE='\.(rs|svelte|ts|tsx|js|jsx|py|sh)$'

CHANGED_RS=$(git diff --name-only HEAD 2>/dev/null | grep -E "$SRC_RE" || true)
UNSTAGED_RS=$(git diff --name-only 2>/dev/null | grep -E "$SRC_RE" || true)
UNTRACKED_RS=$(git ls-files --others --exclude-standard 2>/dev/null | grep -E "$SRC_RE" || true)

# Count touched source files (deduped).
# `grep -v '^$'` exits 1 when every input line is blank (all three vars empty);
# under `pipefail` that would silently fail the script. Swallow with `|| true`.
TOUCHED=$(printf '%s\n%s\n%s\n' "$CHANGED_RS" "$UNSTAGED_RS" "$UNTRACKED_RS" \
    | { grep -v '^$' || true; } | sort -u | wc -l | tr -d ' ')

# Fire only when the session did real work (>= 3 source files
# touched). One-or-two-file fixes rarely yield generalisable
# lessons; this avoids reminder fatigue on small changes.
if [[ "${TOUCHED:-0}" -ge 3 ]]; then
    cat <<'EOF'
=== LEARNINGS.md reminder ===
Session touched non-trivial source. Did you discover anything a
future agent would benefit from knowing?  Examples worth recording:

  - A seam or invariant that isn't obvious from the code
  - A package-name alias or env-var quirk you tripped over
  - A test/harness gotcha (defaults that pollute, parallelism
    races, etc.)
  - A pre-existing bug you noticed but left for a separate PR

Append a bullet to LEARNINGS.md at the repo root. Keep it brief —
2-3 lines max per entry. Skip if nothing worth saving.
=============================
EOF
fi

exit 0
