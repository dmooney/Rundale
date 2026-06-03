#!/usr/bin/env bash
# UserPromptSubmit hook: remind Claude to write acceptance criteria before coding.
#
# Fires on every prompt that looks like an implementation task (contains an
# action verb aimed at a code artifact). Non-blocking — outputs to stdout so
# Claude Code injects it as pre-context before generating a response.
set -euo pipefail

INPUT=$(cat)
PROMPT=$(printf '%s' "$INPUT" | jq -r '.prompt // ""')

# Only act on implementation-task prompts. Match common verbs that precede code.
if ! printf '%s' "$PROMPT" | grep -qiE \
    '\b(add|fix|implement|create|build|refactor|update|change|make|write|port|migrate|remove|delete|rename|extract|split|merge|enable|disable|wire|integrate)\b'; then
    exit 0
fi

# Skip if this looks like a pure doc / config / question prompt.
if printf '%s' "$PROMPT" | grep -qiE \
    '\b(document|explain|describe|what|how|why|list|show|read|check|review|analyse|analyze)\b' \
    && ! printf '%s' "$PROMPT" | grep -qiE \
        '\b(implement|create|build|fix|add|refactor|update|change|make)\b'; then
    exit 0
fi

cat <<'REMINDER'
=== ACCEPTANCE-CRITERIA-FIRST ===
This looks like an implementation task. Before writing any code:

  1. Run /task-start <task-id>
       → writes docs/proofs/<task-id>/acceptance-criteria.md
       → writes parish/testing/fixtures/play_<task-id>.txt
       → stops for human review before any code is written

  2. After approval: implement, then run the verification script and
     capture docs/proofs/<task-id>/transcript.txt

  3. Write evidence.md (Evidence type: live gameplay transcript) mapping
     each criterion to transcript lines that prove it

  4. Write judge.md with all three required lines:
         Verdict: sufficient
         Technical debt: clear
         Acceptance criteria: met

  5. Run: just agent-check

The Stop hook blocks session-end if code was changed without an
acceptance-criteria.md in the proof bundle.
=================================
REMINDER

exit 0
