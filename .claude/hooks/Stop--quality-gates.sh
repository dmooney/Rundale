#!/usr/bin/env bash
# Stop hook: runs fmt + clippy + test when Rust files changed in the session.
#
# Failure protocol: emits `{"decision":"block","reason":"..."}` on stdout so
# Claude is re-prompted with the failure log and must address the gate
# before stopping (Claude Code Stop-hook spec). Human-readable diagnostics
# go to stderr.
#
# Bypass (one of):
#   - `[skip-quality-hook]` sentinel in the most recent assistant message
#   - `CLAUDE_SKIP_QUALITY_HOOK=1` in the environment
#   - `stop_hook_active=true` on the incoming Stop event (anti-loop guard
#     after one block-and-retry — matches Stop--proof-required.sh).

set -euo pipefail

INPUT="$(cat 2>/dev/null || true)"

log() { echo "[Stop--quality-gates] $*" >&2; }

STOP_ACTIVE="$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false' 2>/dev/null || echo false)"
if [ "$STOP_ACTIVE" = "true" ]; then
  log "bypass: stop_hook_active=true (already retried once)"
  exit 0
fi

if [ "${CLAUDE_SKIP_QUALITY_HOOK:-0}" = "1" ]; then
  log "bypass: CLAUDE_SKIP_QUALITY_HOOK=1"
  exit 0
fi

TRANSCRIPT="$(printf '%s' "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null || true)"
CWD="$(printf '%s' "$INPUT" | jq -r '.cwd // empty' 2>/dev/null || true)"
[ -z "$CWD" ] && CWD="$PWD"

if ! ROOT="$(git -C "$CWD" rev-parse --show-toplevel 2>/dev/null)"; then
  exit 0
fi

# Sentinel bypass — only the most-recent assistant message counts, so a
# stale sentinel from an earlier turn doesn't leak forward.
if [ -n "$TRANSCRIPT" ] && [ -r "$TRANSCRIPT" ]; then
  LAST_ASSISTANT_TEXT="$(
    jq -srj '
      [.[] | select(.type == "assistant")] | last // {} |
      (.message.content // [])
      | map(select(.type == "text") | .text)
      | join("\n")
    ' "$TRANSCRIPT" 2>/dev/null || true
  )"
  if printf '%s' "$LAST_ASSISTANT_TEXT" | grep -q '\[skip-quality-hook\]'; then
    log "bypass: [skip-quality-hook] sentinel in most-recent assistant message"
    exit 0
  fi
fi

cd "$ROOT"

CHANGED_RS="$(git diff --name-only HEAD 2>/dev/null | grep '\.rs$' || true)"
UNSTAGED_RS="$(git diff --name-only 2>/dev/null | grep '\.rs$' || true)"
UNTRACKED_RS="$(git ls-files --others --exclude-standard 2>/dev/null | grep '\.rs$' || true)"

if [[ -z "$CHANGED_RS" && -z "$UNSTAGED_RS" && -z "$UNTRACKED_RS" ]]; then
  exit 0
fi

# Cargo workspace lives under parish/ since the 855f2a9 relocation refactor.
if [ ! -f "parish/Cargo.toml" ]; then
  log "no parish/Cargo.toml at $ROOT; skipping"
  exit 0
fi
cd parish

LOG_FILE="$(mktemp -t parish-quality-gates.XXXXXX.log)"
trap 'rm -f "$LOG_FILE"' EXIT

run_step() {
  local label="$1"; shift
  printf '\n--- %s ---\n' "$label" | tee -a "$LOG_FILE" >&2
  if "$@" >>"$LOG_FILE" 2>&1; then
    printf 'PASS: %s\n' "$label" | tee -a "$LOG_FILE" >&2
    return 0
  else
    printf 'FAIL: %s\n' "$label" | tee -a "$LOG_FILE" >&2
    return 1
  fi
}

{
  echo "=== Parish Quality Gates ==="
  echo "Rust files changed -- running checks at $(date -u +%FT%TZ)"
} >&2

FAILED=0
run_step "cargo fmt --check" cargo fmt --check          || FAILED=1
run_step "cargo clippy"      cargo clippy -- -D warnings || FAILED=1
run_step "cargo test"        cargo test                 || FAILED=1

if [[ $FAILED -eq 0 ]]; then
  log "all quality gates passed"
  exit 0
fi

# Truncate captured log to its tail (clippy/test errors are at the end) so
# the block reason stays under any size cap and still gives Claude useful
# context to act on.
MAX_BYTES=12000
LOG_TAIL="$(tail -c "$MAX_BYTES" "$LOG_FILE" 2>/dev/null || cat "$LOG_FILE")"

REASON="$(printf '%s\n' \
  "Parish quality gates failed (cargo fmt / clippy / test). Address the" \
  "failure(s) below before stopping. Re-run \`cd parish && cargo fmt &&" \
  "cargo clippy -- -D warnings && cargo test\` after fixing." \
  "" \
  "Bypass (only for unfixable env issues): include [skip-quality-hook] in" \
  "your next message, or set CLAUDE_SKIP_QUALITY_HOOK=1." \
  "" \
  "----- log tail (last ${MAX_BYTES} bytes) -----" \
  "$LOG_TAIL")"

jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason}'
exit 0
