#!/usr/bin/env bash
# Stop hook: blocks "done" if code changed in this session but no proof
# was captured. Proof = a tool invocation that exercises the change.
#
# Accepted proof signals (case-insensitive match on transcript JSONL):
#   - mcp__parish__*           : Parish MCP (drives the live Tauri/web backend)
#   - mcp__claude-in-chrome__* : Chrome MCP (drives the browser UI)
#   - cargo test / cargo nextest
#   - npm test / npm run test / npm run check / npm run e2e
#   - npx playwright
#   - just check / verify / agent-check / ui-test / ui-e2e
#   - /prove, /check, /verify, /play, /rubric, /chrome-test, /demo
#
# Bypass: include "[skip-proof-hook]" in your most recent assistant
# message, or set CLAUDE_SKIP_PROOF_HOOK=1.
#
# Block protocol: emits `{"decision":"block","reason":"..."}` on stdout
# (per Claude Code Stop-hook spec). All diagnostic chatter goes to stderr.

set -euo pipefail

INPUT="$(cat)"

# Diagnostics → stderr so they appear in --debug logs without affecting
# the JSON block payload on stdout.
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

# Resolve to repo top-level. If not a git repo, bail (the project hook
# only applies inside this checkout).
if ! ROOT="$(git -C "$CWD" rev-parse --show-toplevel 2>/dev/null)"; then
  exit 0
fi

CODE_REGEX='\.(rs|svelte|ts|tsx|js|mjs|cjs|py|go|java|kt|swift|c|h|cc|cpp|hpp|rb)$'

# Tracked changes vs HEAD + untracked, filtered to code extensions.
CHANGED="$(
  {
    git -C "$ROOT" diff --name-only HEAD 2>/dev/null || true
    git -C "$ROOT" ls-files --others --exclude-standard 2>/dev/null || true
  } | grep -E "$CODE_REGEX" || true
)"

if [ -z "$CHANGED" ]; then
  exit 0
fi

# Honor sentinel in the most-recent assistant message. Grep the whole
# transcript rather than picking the last record (cheap, and any recent
# bypass directive should still apply).
# macOS has no `tac`; use a portable pattern.
if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  if grep -E '"type"[[:space:]]*:[[:space:]]*"assistant"' "$TRANSCRIPT" 2>/dev/null | grep -q '\[skip-proof-hook\]'; then
    log "bypass: [skip-proof-hook] sentinel in assistant message"
    exit 0
  fi
fi

PROOF_PATTERNS='mcp__parish__|mcp__claude-in-chrome__|cargo[[:space:]]+(test|nextest)|npm[[:space:]]+(test|run[[:space:]]+(test|check|e2e))|npx[[:space:]]+playwright|just[[:space:]]+(check|verify|agent-check|ui-test|ui-e2e)|/prove|/check |/verify|/play|/rubric|/chrome-test|/demo'

PROOF=""
if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  PROOF="$(grep -E -i "$PROOF_PATTERNS" "$TRANSCRIPT" 2>/dev/null | head -1 || true)"
fi

if [ -n "$PROOF" ]; then
  log "proof found: $(printf '%s' "$PROOF" | head -c 120)"
  exit 0
fi

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
