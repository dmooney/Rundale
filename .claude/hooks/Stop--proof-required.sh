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

# ── Sentinel bypass — most-recent assistant message only ──────────────
# Checked first so [skip-proof-hook] bypasses both the proof gate and
# the acceptance-criteria check below.
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

CODE_REGEX='\.(rs|svelte|ts|tsx|js|mjs|cjs|py|go|java|kt|swift|c|h|cc|cpp|hpp|rb)$'

# Subset of CODE_REGEX paths that ship runtime behavior. When any changed
# file is under one of these prefixes, unit-test signals (cargo test /
# just check / npm run check) are no longer sufficient on their own —
# the agent must additionally exercise the change at runtime
# (mcp__parish__*, mcp__claude-in-chrome__*, Skill prove/play/demo/
# chrome-test, or a Bash invocation of just demo / just play / just run
# / cargo tauri dev / cargo run -p parish-*). This closes the gap where
# unit tests are green but the runtime-only seam is never touched
# (e.g. Tauri startup paths that only fire in `just demo`).
# Per-branch trailing tokens (each branch carries its own `/` or `.rs`
# suffix). A single trailing `/` outside the group would force every
# alternative to be a directory, mis-matching the `.rs` file leaves
# (`setup.rs`, `client.rs`, `ticks.rs`, `manager.rs`).
RUNTIME_PATH_REGEX='^(parish/crates/parish-(tauri/|server/|cli/|core/src/(game_loop|game_session|ipc)/|inference/src/(setup|client)\.rs|npc/src/(ticks|manager)\.rs|npc/src/(reactions|autonomous)/|world/|input/)|parish/apps/ui/src/|mods/|\.claude/hooks/|\.claude/skills/)'

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

# ── Acceptance-criteria detection ─────────────────────────────────────
# Computed here (before the early no-code-change exit) so proof-only
# sessions (proof files written without any code edits) also get the
# AC gate.
#
# Proof bundles live in `.proofs/<task-id>/` (gitignored — they are
# attached to the PR via `just attach-proof`, not committed). Detection
# sources:
#   a) transcript Write/Edit/MultiEdit tool_use entries
#   b) transcript Bash commands — heredoc/redirect/tee writes
# The git-diff source from the old `docs/proofs/` scheme is gone:
# `.proofs/` is gitignored, so `git diff` / `ls-files --exclude-standard`
# don't see it. Disk-walk would over-trigger across stale bundles from
# prior branches; we scope detection to this session via the transcript.
# Then check on disk whether each detected bundle has an
# acceptance-criteria.md (prior-session files count too). Per-bundle:
# `.proofs/A/judge.md` + `.proofs/B/ac.md` does NOT satisfy bundle A.

_proof_bundle_dirs() {
  {
    if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Write" or .name == "Edit" or .name == "MultiEdit")
        | .input.file_path // empty
      ' "$TRANSCRIPT" 2>/dev/null \
        | sed "s|^$ROOT/||" \
        | grep -E '\.proofs/.*/(evidence|judge)\.md$' \
        | sed 's|/[^/]*$||' || true
      # Bash scan: any command that names a .proofs/<id>/(evidence|judge).md
      # target is treated as session work. Covers redirect/tee plus
      # cp/mv/install/python/curl/etc. (the original git-diff fallback is
      # gone since .proofs/ is gitignored). False positives from
      # read-only commands like `cat .proofs/x/judge.md` are harmless —
      # the AC check that follows passes if the dir has an AC file
      # alongside.
      #
      # Disk walk of .proofs/ is intentionally NOT used as a fallback:
      # .proofs/ persists across branches and sessions, so stale bundles
      # from prior work would block unrelated sessions (regresses the
      # session-scoped gate).
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Bash")
        | .input.command // empty
      ' "$TRANSCRIPT" 2>/dev/null \
        | grep -oE '\.proofs/[^/[:space:]\"]+/(evidence|judge)\.md' \
        | sed 's|/[^/]*$||' || true
    fi
  } | grep -v '^$' | sort -u || true
}

PROOF_BUNDLE_DIRS="$(_proof_bundle_dirs)"

# For each bundle written this session, verify acceptance-criteria.md exists
# on disk (current session or pre-existing from a prior commit). Bundles
# that no longer exist on disk (e.g. scratch dirs deleted later in the
# same session) are skipped — there's nothing to validate.
BUNDLE_MISSING_AC=""
if [ -n "$PROOF_BUNDLE_DIRS" ]; then
  while IFS= read -r bundle_dir; do
    [ -z "$bundle_dir" ] && continue
    [ -d "$ROOT/$bundle_dir" ] || continue
    if [ ! -f "$ROOT/$bundle_dir/acceptance-criteria.md" ]; then
      BUNDLE_MISSING_AC="$bundle_dir"
      break
    fi
  done <<< "$PROOF_BUNDLE_DIRS"
fi

if [ -z "$CHANGED" ]; then
  # No code changes — still block if a proof bundle written this session
  # is missing its acceptance-criteria.md.
  if [ -n "$BUNDLE_MISSING_AC" ]; then
    jq -n --arg reason "Stop blocked by .claude/hooks/Stop--proof-required.sh:
ACCEPTANCE CRITERIA MISSING: proof bundle '$BUNDLE_MISSING_AC/' has no
acceptance-criteria.md (checked on disk — prior-session files count too).

Acceptance criteria must be written BEFORE coding (rule 13 in AGENTS.md).
Run /task-start <task-id> to create the file, then add it to your proof bundle.

The judge.md must also include:
  Acceptance criteria: met
with each criterion verified against the game log.

Intentional bypass: include '[skip-proof-hook]' in your message or set
CLAUDE_SKIP_PROOF_HOOK=1." '{"decision":"block","reason":$reason}'
    exit 0
  fi
  exit 0
fi

# Did any runtime-shipping path change? Drives the proof-tier decision
# below: a "yes" upgrades the requirement from "any test signal" to
# "live runtime signal".
RUNTIME_CHANGED="$(printf '%s\n' "$CHANGED" | grep -E "$RUNTIME_PATH_REGEX" || true)"

# ── Proof detection (tool_use entries only) ───────────────────────────
#
# Two tiers:
#
#   LIVE proof  = the change actually ran in a real process.
#     - mcp__parish__* (drives a live backend)
#     - mcp__claude-in-chrome__* (drives a live browser)
#     - Skill prove / play / demo / rubric / chrome-test (live harness)
#     - Bash matching LIVE_BASH_PATTERN (just demo|play|run|run-headless
#       |web; cargo tauri dev; cargo run -p parish-cli|parish-tauri
#       |parish-server)
#
#   TEST proof  = static / unit / integration tests passed.
#     - Skill check / verify
#     - Bash matching TEST_BASH_PATTERN (cargo test|nextest; npm test|
#       run check|run e2e; npx playwright; just check|verify|
#       agent-check|ui-test|ui-e2e)
#
# When RUNTIME_CHANGED is non-empty, only LIVE proof clears the gate
# (TEST proof on its own is rejected — unit tests don't exercise the
# Tauri/server/CLI seam and won't catch startup-only regressions like
# the parish.toml category_overrides drop). When RUNTIME_CHANGED is
# empty (e.g. only parish-config / parish-types touched), either tier
# is accepted.
LIVE_PROOF=""
TEST_PROOF=""

if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  # LIVE-1: direct MCP tool invocations.
  LIVE_PROOF="$(
    jq -rc '
      (.message.content // [])[]?
      | select(.type == "tool_use")
      | select(.name | test("^mcp__(parish|claude-in-chrome)__"))
      | .name
    ' "$TRANSCRIPT" 2>/dev/null | head -1 || true
  )"

  # LIVE-2: live-harness skill invocations.
  if [ -z "$LIVE_PROOF" ]; then
    LIVE_PROOF="$(
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Skill")
        | select(.input.skill | IN("prove","play","demo","rubric","chrome-test"))
        | "skill: \(.input.skill)"
      ' "$TRANSCRIPT" 2>/dev/null | head -1 || true
    )"
  fi

  # LIVE-3: Bash calls that boot a real process against the workspace.
  if [ -z "$LIVE_PROOF" ]; then
    LIVE_BASH_PATTERN='just[[:space:]]+(demo|play|run|run-headless|web)\b|cargo[[:space:]]+tauri[[:space:]]+dev|cargo[[:space:]]+run[[:space:]]+(--manifest-path[[:space:]]+\S+[[:space:]]+)?-p[[:space:]]+parish-(cli|tauri|server)\b|parish-mcp-backend\.sh[[:space:]]+start'
    LIVE_PROOF="$(
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Bash")
        | .input.command // empty
      ' "$TRANSCRIPT" 2>/dev/null \
        | grep -E -m1 "$LIVE_BASH_PATTERN" \
        | head -c 160 || true
    )"
  fi

  # TEST-1: static/unit-test skills.
  TEST_PROOF="$(
    jq -rc '
      (.message.content // [])[]?
      | select(.type == "tool_use")
      | select(.name == "Skill")
      | select(.input.skill | IN("check","verify"))
      | "skill: \(.input.skill)"
    ' "$TRANSCRIPT" 2>/dev/null | head -1 || true
  )"

  # TEST-2: Bash test/check commands.
  if [ -z "$TEST_PROOF" ]; then
    TEST_BASH_PATTERN='cargo[[:space:]]+(test|nextest)|npm[[:space:]]+(test|run[[:space:]]+(test|check|e2e))|npx[[:space:]]+playwright|just[[:space:]]+(check|verify|agent-check|ui-test|ui-e2e)'
    TEST_PROOF="$(
      jq -rc '
        (.message.content // [])[]?
        | select(.type == "tool_use")
        | select(.name == "Bash")
        | .input.command // empty
      ' "$TRANSCRIPT" 2>/dev/null \
        | grep -E -m1 "$TEST_BASH_PATTERN" \
        | head -c 160 || true
    )"
  fi
fi

# Decision matrix.
if [ -n "$LIVE_PROOF" ]; then
  log "live proof found: $LIVE_PROOF"
  # Even with live proof, block if any proof bundle is missing its AC file.
  if [ -n "$BUNDLE_MISSING_AC" ]; then
    jq -n --arg reason "Stop blocked by .claude/hooks/Stop--proof-required.sh:
ACCEPTANCE CRITERIA MISSING: proof bundle '$BUNDLE_MISSING_AC/' has no
acceptance-criteria.md (checked on disk — prior-session files count too).

Acceptance criteria must be written BEFORE coding (rule 13 in AGENTS.md).
Run /task-start <task-id> to create the file, then add it to your proof bundle.

The judge.md must also include:
  Acceptance criteria: met
with each criterion verified against the game log.

Intentional bypass: include '[skip-proof-hook]' in your message or set
CLAUDE_SKIP_PROOF_HOOK=1." '{"decision":"block","reason":$reason}'
    exit 0
  fi
  exit 0
fi

if [ -z "$RUNTIME_CHANGED" ] && [ -n "$TEST_PROOF" ]; then
  log "test proof accepted (no runtime paths touched): $TEST_PROOF"
  if [ -n "$BUNDLE_MISSING_AC" ]; then
    jq -n --arg reason "Stop blocked by .claude/hooks/Stop--proof-required.sh:
ACCEPTANCE CRITERIA MISSING: proof bundle '$BUNDLE_MISSING_AC/' has no
acceptance-criteria.md (checked on disk — prior-session files count too).

Acceptance criteria must be written BEFORE coding (rule 13 in AGENTS.md).
Run /task-start <task-id> to create the file, then add it to your proof bundle.

The judge.md must also include:
  Acceptance criteria: met
with each criterion verified against the game log.

Intentional bypass: include '[skip-proof-hook]' in your message or set
CLAUDE_SKIP_PROOF_HOOK=1." '{"decision":"block","reason":$reason}'
    exit 0
  fi
  exit 0
fi

# ── Block ──────────────────────────────────────────────────────────────
# Append a structured record of the block to .claude/logs/proof-blocks.jsonl
# for the self-improving learn loop (parish/scripts/learn/collect_stop_blocks.py).
# Best-effort: log failures must never block the hook itself.
{
  LOG_DIR="$ROOT/.claude/logs"
  mkdir -p "$LOG_DIR" 2>/dev/null || true
  LOG_TS="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo "")"
  if [ -n "$RUNTIME_CHANGED" ]; then
    LOG_TIER="live"
  else
    LOG_TIER="test"
  fi
  CHANGED_JSON="$(printf '%s\n' "$CHANGED" | jq -R . | jq -sc . 2>/dev/null || echo '[]')"
  jq -nc \
    --arg ts "$LOG_TS" \
    --arg tier "$LOG_TIER" \
    --argjson paths "$CHANGED_JSON" \
    '{ts:$ts, evidence_type:$tier, paths_touched:$paths, reason:"proof-required"}' \
    >> "$LOG_DIR/proof-blocks.jsonl" 2>/dev/null || true
} || true

FILES_PREVIEW="$(printf '%s\n' "$CHANGED" | head -8 | sed 's/^/  - /')"
EXTRA="$(printf '%s\n' "$CHANGED" | wc -l | tr -d ' ')"
TRAIL=""
[ "$EXTRA" -gt 8 ] && TRAIL=$'\n  - ...'

if [ -n "$RUNTIME_CHANGED" ]; then
  RUNTIME_PREVIEW="$(printf '%s\n' "$RUNTIME_CHANGED" | head -6 | sed 's/^/  - /')"
  TIER_BANNER="LIVE proof required: a runtime-shipping path changed this session."
  TIER_NOTE="Runtime-shipping files touched:
${RUNTIME_PREVIEW}

Unit tests (cargo test, just check) are NOT sufficient on their own —
they don't exercise the Tauri / server / CLI startup seams. You must
additionally drive the change through a real process before claiming
done."
  EXAMPLES="  Backend (Rust, gameplay, Tauri)
    - bash parish/scripts/parish-mcp-backend.sh start  (then mcp__parish__*)
    - cargo run -p parish-tauri  (live desktop window)
    - cargo run -p parish-cli -- --headless  (REPL)
    - just demo  /  just play  /  just run
    - /prove <feature>  /  /play

  Frontend (parish/apps/ui)
    - mcp__claude-in-chrome__* against vite dev server
    - /chrome-test
    - npx playwright e2e/<spec>.spec.ts  (real browser, not just type-check)"
else
  TIER_BANNER="TEST proof required: code changed this session."
  TIER_NOTE=""
  EXAMPLES="  - cargo test / cargo nextest
  - npm run check / npm run e2e / npx playwright
  - just check / just verify / just agent-check
  - /check or /verify
  - mcp__parish__* or mcp__claude-in-chrome__* (these also satisfy)"
fi

REASON="Stop blocked by .claude/hooks/Stop--proof-required.sh:
${TIER_BANNER}

Changed files:
${FILES_PREVIEW}${TRAIL}
${TIER_NOTE:+
$TIER_NOTE}

Before claiming done, exercise the change:

${EXAMPLES}

Also required (rule 13): write .proofs/<task-id>/acceptance-criteria.md
BEFORE coding, run the game, capture the transcript, and include
'Acceptance criteria: met' in judge.md. Use /task-start <task-id>.
After the bundle is complete, run `just attach-proof <task-id>` to post it
to the PR — bundles are not committed (`.proofs/` is gitignored).

Then restate in your message what you exercised and what the result was.
Type-checking and svelte-check are not proof of behavior — they catch
shape, not feature.

Intentional bypass: include '[skip-proof-hook]' in your message (e.g.
doc-only edit, user explicitly waived testing) or set
CLAUDE_SKIP_PROOF_HOOK=1 in the environment."

jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason}'
