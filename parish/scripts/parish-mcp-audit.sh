#!/usr/bin/env bash
#
# parish-mcp-audit.sh — strict Init / Execute / Validate / Teardown audit loop
# for a live parish-server command session (#1331).
#
# Despite the historical filename, this script calls HTTP routes directly. It
# does not exercise the stdio parish-mcp server or a player-visible UI. Its
# purpose is backend session/state validation with reliable teardown.
#
#   Init      Clear local caches, boot the backend, verify the MCP link is live
#             (/api/health AND /api/engine-state both answer).
#   Execute   Drive a sequence of turns against the live backend.
#   Validate  Assert structural and optional expected-scene invariants against
#             the canonical state returned by GET /api/engine-state.
#   Teardown  On detected failure, file a bug (screenshot + logs + state +
#             diagnostic payload) via POST /api/submit-bug-report. Always kill
#             the backend cleanly and reset, whether the run passed or failed.
#
# Usage:
#   bash parish/scripts/parish-mcp-audit.sh run [SEQUENCE_FILE]
#   bash parish/scripts/parish-mcp-audit.sh --help
#
# SEQUENCE_FILE is an optional newline-delimited list of player inputs (one per
# line, '#' comments allowed). Defaults to a built-in look/move/look probe.
#
# Environment:
#   PARISH_MCP_BACKEND_PORT   backend port (default 3030; honoured by the helper)
#   PARISH_AUDIT_EXPECT_SCENE if set, Validate asserts the engine's active scene
#                             contains this substring; a mismatch triggers Teardown.
#                             Leave unset to only assert structural integrity.
#   PARISH_BUG_REPORT_DRY_RUN forwarded to the backend; '1' writes the bug
#                             bundle to disk instead of filing on GitHub.
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required (sudo apt-get install jq)" >&2
    exit 1
fi

REPO="$(git rev-parse --show-toplevel)"
PORT="${PARISH_MCP_BACKEND_PORT:-3030}"
BASE="http://127.0.0.1:${PORT}"
BACKEND="$REPO/parish/scripts/parish-mcp-backend.sh"
COOKIE_JAR="$(mktemp)"

# ── Logging helpers ───────────────────────────────────────────────────────────
phase() { printf '\n=== %s ===\n' "$1"; }
info() { printf '  %s\n' "$1"; }
fail() { printf '  FAIL: %s\n' "$1" >&2; }

usage() {
    sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

# ── Teardown — runs on EVERY exit path (trap) ─────────────────────────────────
# Guarantees: no orphaned backend process, pid/log files cleaned up.
teardown() {
    local rc=$?
    phase "Teardown"
    if [ "${AUDIT_FAILED:-0}" = "1" ]; then
        info "failure detected — filing a bug report with the full diagnostic payload"
        file_bug "${AUDIT_FAIL_REASON:-audit loop detected a UI/Engine mismatch}" \
            || fail "bug filing failed (continuing teardown)"
    fi
    info "stopping backend"
    bash "$BACKEND" stop >/dev/null 2>&1 || true
    rm -f "$COOKIE_JAR"
    info "teardown complete"
    return "$rc"
}

# ── HTTP helpers ──────────────────────────────────────────────────────────────
engine_state() { curl -fsS -b "$COOKIE_JAR" -c "$COOKIE_JAR" "$BASE/api/engine-state"; }

# Submit one player input through the synchronous thin-client command surface.
submit() {
    curl -fsS -b "$COOKIE_JAR" -c "$COOKIE_JAR" -X POST "$BASE/api/command" \
        -H 'content-type: application/json' \
        -d "$(printf '{"text":%s}' "$(json_str "$1")")"
}

# File a bug via the shared bug-report route. The backend auto-appends the
# diagnostic payload (engine state + LLM history + last user intent, #1331).
file_bug() {
    curl -fsS -b "$COOKIE_JAR" -c "$COOKIE_JAR" -X POST "$BASE/api/submit-bug-report" \
        -H 'content-type: application/json' \
        -d "$(printf '{"title":%s,"description":%s}' \
            "$(json_str "Backend HTTP audit failure")" "$(json_str "$1")")" || return 1
}

# Minimal JSON string escaper (quotes + backslashes + newlines).
json_str() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    printf '"%s"' "$s"
}

# Extract the active-scene location name from an engine-state JSON blob.
# jq is a hard requirement (#1366 §7) — the old grep/sed fallback silently
# mis-parsed nested/escaped JSON, which is worse than failing fast.
scene_name() {
    printf '%s' "$1" | jq -r '.active_scene.location_name // empty'
}

# ── Init ──────────────────────────────────────────────────────────────────────
do_init() {
    phase "Init"
    info "clearing local audit caches"
    rm -f "$REPO/parish/.parish-mcp-backend.log" "$REPO/parish/.parish-mcp-backend.pid" \
        2>/dev/null || true

    info "booting backend on port $PORT"
    bash "$BACKEND" start

    info "verifying backend session routes (/api/health + /api/engine-state)"
    curl -fsS "$BASE/api/health" >/dev/null || {
        fail "/api/health did not answer"
        exit 1
    }
    engine_state >/dev/null || {
        fail "/api/engine-state did not answer — get_engine_state not wired?"
        exit 1
    }
    info "backend session verified"
}

# ── Execute ───────────────────────────────────────────────────────────────────
do_execute() {
    phase "Execute"
    local seq_file="${1:-}"
    local -a inputs=()
    if [ -n "$seq_file" ] && [ -f "$seq_file" ]; then
        info "running sequence from $seq_file"
        while IFS= read -r line; do
            case "$line" in
                '' | \#*) continue ;;
                *) inputs+=("$line") ;;
            esac
        done <"$seq_file"
    else
        info "running built-in probe sequence (look / move / look)"
        inputs=("look" "go to the church" "look")
    fi
    for input in "${inputs[@]}"; do
        info "turn: $input"
        submit "$input" >/dev/null || {
            AUDIT_FAILED=1
            AUDIT_FAIL_REASON="turn '$input' was rejected by the backend"
            fail "$AUDIT_FAIL_REASON"
            return 0
        }
    done
}

# ── Validate ──────────────────────────────────────────────────────────────────
# Compare the canonical engine state against the expected/observed UI state.
do_validate() {
    phase "Validate"
    local es scene
    es="$(engine_state)" || {
        AUDIT_FAILED=1
        AUDIT_FAIL_REASON="engine-state read failed during validation"
        fail "$AUDIT_FAIL_REASON"
        return 0
    }
    scene="$(scene_name "$es")"
    info "engine active_scene: ${scene:-<unknown>}"

    if [ -z "$scene" ]; then
        AUDIT_FAILED=1
        AUDIT_FAIL_REASON="engine-state returned no active_scene.location_name"
        fail "$AUDIT_FAIL_REASON"
        return 0
    fi

    if [ -n "${PARISH_AUDIT_EXPECT_SCENE:-}" ]; then
        if [[ "$scene" == *"$PARISH_AUDIT_EXPECT_SCENE"* ]]; then
            info "scene matches expected substring '$PARISH_AUDIT_EXPECT_SCENE'"
        else
            AUDIT_FAILED=1
            AUDIT_FAIL_REASON="scene mismatch: expected '$PARISH_AUDIT_EXPECT_SCENE' but engine reports '$scene'"
            fail "$AUDIT_FAIL_REASON"
            return 0
        fi
    fi
    info "validation passed"
}

# ── Main ──────────────────────────────────────────────────────────────────────
main() {
    case "${1:-run}" in
        -h | --help | help) usage 0 ;;
        run) ;;
        *)
            fail "unknown command: ${1:-}"
            usage 1
            ;;
    esac
    shift || true

    AUDIT_FAILED=0
    AUDIT_FAIL_REASON=""
    trap teardown EXIT

    do_init
    do_execute "${1:-}"
    do_validate

    if [ "$AUDIT_FAILED" = "1" ]; then
        fail "audit run FAILED — see teardown bug report"
        exit 1
    fi
    phase "Result"
    info "audit run PASSED"
}

main "$@"
