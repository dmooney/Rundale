#!/usr/bin/env bash
# Stop hook: blocks "done" if code changed in this session but no proof was
# captured. Proof = a tool invocation that exercises the change.
#
# Accepted proof signals (read from the transcript's tool_use entries, not
# from raw conversational text — bot review feedback #973):
#   - Tool name `mcp__parish__*`           : Parish MCP
#   - Tool name `mcp__claude-in-chrome__*` : Chrome MCP
#   - Tool name `Bash` with command containing one of:
#       cargo test, cargo nextest, npm test, npm run (test|check|e2e),
#       npx playwright, just (check|verify|agent-check|ui-test|ui-e2e)
#   - Tool name `Skill` with input.skill in
#       {prove, check, verify, play, rubric, chrome-test, demo}
#
# Code-change detection (covers the "agent committed before stopping"
# bypass — bot review feedback #973):
#   - Tracked diff vs HEAD AND untracked files (extension filter), OR
#   - Edit / Write / NotebookEdit / MultiEdit tool_use entries in the
#     transcript whose target path matches the same extension filter.
#
# Bypass (checked against the MOST RECENT assistant message only — bot
# review feedback #973):
#   - "[skip-proof-hook]" sentinel
#   - `CLAUDE_SKIP_PROOF_HOOK=1` env
#
# Block protocol: emits `{"decision":"block","reason":"..."}` on stdout
# (Claude Code Stop-hook spec). Diagnostic chatter goes to stderr.

set -euo pipefail

INPUT="$(cat)"

log() { echo "[Stop--proof-required] $*" >&2; }

STOP_ACTIVE="$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false' 2>/dev/null || echo false)"
if [ "$STOP_ACTIVE" = "true" ]; then
  exit 0
fi

if [ "${CLAUDE_SKIP_PROOF_HOOK:-0}" = "1" ]; then
  log "bypass: CLAUDE_SKIP_PROOF_HOOK=1"
  exit 0
fi

TRANSCRIPT="$(printf '%s' "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null || true)"
CWD="$(printf '%s' "$INPUT" | jq -r '.cwd // empty' 2>/dev/null || true)"
[ -z "$CWD" ] && CWD="$PWD"

if ! ROOT="$(git -C "$CWD" rev-parse --show-toplevel 2>/dev/null)"; then
  exit 0
fi

CODE_REGEX='\.(rs|svelte|ts|tsx|js|mjs|cjs|py|go|java|kt|swift|c|h|cc|cpp|hpp|rb)$'

# ── Code-change detection ──────────────────────────────────────────────
# Source 1: tracked diff vs HEAD + untracked code files.
DIFF_CHANGED="$(
  {
    git -C "$ROOT" diff --name-only HEAD 2>/dev/null || true
    git -C "$ROOT" ls-files --others --exclude-standard 2>/dev/null || true
  } | grep -E "$CODE_REGEX" || true
)"

# Source 2: transcript tool_use entries that edited code files. Survives
# the case where the agent committed mid-session and the worktree is
# clean by the time Stop fires.
TRANSCRIPT_EDITED=""
if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  TRANSCRIPT_EDITED="$(
    jq -rc '
      (.message.content // [])[]?
      | select(.type == "tool_use")
      | select(.name == "Edit" or .name == "Write" or .name == "MultiEdit" or .name == "NotebookEdit")
      | .input.file_path // .input.notebook_path // empty
    ' "$TRANSCRIPT" 2>/dev/null | grep -E "$CODE_REGEX" || true
  )"
fi

CHANGED="$(printf '%s\n%s\n' "$DIFF_CHANGED" "$TRANSCRIPT_EDITED" | grep -v '^$' | sort -u || true)"

if [ -z "$CHANGED" ]; then
  exit 0
fi

# ── Sentinel bypass — most-recent assistant message only ──────────────
# `jq -s` slurps the whole JSONL into an array so we can pick the very
# last assistant entry and grep only its text blocks. A sentinel left in
# an earlier assistant message no longer leaks bypass authority forward
# to later stops.
if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  LAST_ASSISTANT_TEXT="$(
    jq -srj '
      [.[] | select(.type == "assistant")] | last // {} |
      (.message.content // [])
      | map(select(.type == "text") | .text)
      | join("\n")
    ' "$TRANSCRIPT" 2>/dev/null || true
  )"
  if printf '%s' "$LAST_ASSISTANT_TEXT" | grep -q '\[skip-proof-hook\]'; then
    log "bypass: [skip-proof-hook] sentinel in most-recent assistant message"
    exit 0
  fi
fi

# ── Proof detection (tool_use entries only) ───────────────────────────
PROOF=""
if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  # 1. Direct MCP tool invocations.
  PROOF="$(
    jq -rc '
      (.message.content // [])[]?
      | select(.type == "tool_use")
      | select(.name | test("^mcp__(parish|claude-in-chrome)__"))
      | .name
    ' "$TRANSCRIPT" 2>/dev/null | head -1 || true
  )"

  # 2. Skill invocations.
  if [ -z "$PROOF" ]; then
    PROOF="$(
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Skill")
        | select(.input.skill | IN("prove","check","verify","play","rubric","chrome-test","demo"))
        | "skill: \(.input.skill)"
      ' "$TRANSCRIPT" 2>/dev/null | head -1 || true
    )"
  fi

  # 3. Bash tool calls that ran a real test / check command.
  if [ -z "$PROOF" ]; then
    BASH_PATTERN='cargo[[:space:]]+(test|nextest)|npm[[:space:]]+(test|run[[:space:]]+(test|check|e2e))|npx[[:space:]]+playwright|just[[:space:]]+(check|verify|agent-check|ui-test|ui-e2e)'
    PROOF="$(
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Bash")
        | .input.command // empty
      ' "$TRANSCRIPT" 2>/dev/null \
        | grep -E -m1 "$BASH_PATTERN" \
        | head -c 160 || true
    )"
  fi
fi

if [ -n "$PROOF" ]; then
  log "proof found: $PROOF"
  exit 0
fi

# ── Block ──────────────────────────────────────────────────────────────
FILES_PREVIEW="$(printf '%s\n' "$CHANGED" | head -8 | sed 's/^/  - /')"
EXTRA="$(printf '%s\n' "$CHANGED" | wc -l | tr -d ' ')"
TRAIL=""
[ "$EXTRA" -gt 8 ] && TRAIL=$'\n  - ...'

REASON="Stop blocked by .claude/hooks/Stop--proof-required.sh:
code changed but no proof captured this session.

Changed files:
${FILES_PREVIEW}${TRAIL}

Before claiming done, exercise the change. Pick the right tool for the
layer you touched:

  Tauri / backend / gameplay
    - mcp__parish__* (start backend first: bash parish/scripts/parish-mcp-backend.sh start)
    - cargo test / cargo nextest
    - /prove <feature> for gameplay features
    - /check or /verify

  Frontend (parish/apps/ui)
    - mcp__claude-in-chrome__* against the live dev server
    - npm run check / npm run e2e / npx playwright e2e/...
    - /chrome-test for browser walkthroughs

Then restate in your message what you exercised and what the result was.
Type-checking and svelte-check are not proof of behavior — they catch
shape, not feature.

Intentional bypass: include '[skip-proof-hook]' in your message (e.g.
doc-only edit, user explicitly waived testing) or set
CLAUDE_SKIP_PROOF_HOOK=1 in the environment."

jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason}'
