#!/usr/bin/env bash
# Stop hook: catches lazy-out phrases in the last assistant message and
# forces the agent to find an automated path instead.
#
# Blocked phrases: "no display", "requires manual testing", "out of scope",
# "cannot test/verify", "deferred", "unable to", etc.
#
# Block protocol: emits {"decision":"block","reason":"..."} on stdout.
# Diagnostic chatter goes to stderr.
#
# Bypass: include '[skip-no-excuses]' in your message when you have a
# genuine hard blocker that truly cannot be automated (rare).

set -euo pipefail
trap 'rc=$?; printf "Stop hook %s failed (exit=%d) at line %d\n" "${BASH_SOURCE[0]##*/}" "$rc" "$LINENO" >&2' ERR

INPUT="$(cat)"

log() { echo "[Stop--no-excuses] $*" >&2; }

# Guard against infinite block loops
STOP_ACTIVE="$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false' 2>/dev/null || echo false)"
if [ "$STOP_ACTIVE" = "true" ]; then
    exit 0
fi

TRANSCRIPT="$(printf '%s' "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null || true)"
if [ -z "$TRANSCRIPT" ] || [ ! -r "$TRANSCRIPT" ]; then
    exit 0
fi

# Extract text from the last assistant message only
LAST_TEXT="$(
    jq -srj '
    [.[] | select(.type == "assistant")] | last // {} |
    (.message.content // [])
    | map(select(.type == "text") | .text)
    | join("\n")
  ' "$TRANSCRIPT" 2>/dev/null || true
)"

if [ -z "$LAST_TEXT" ]; then
    exit 0
fi

# Sentinel bypass — only checked in the last assistant message
if printf '%s' "$LAST_TEXT" | grep -q '\[skip-no-excuses\]'; then
    log "bypass: [skip-no-excuses] sentinel found"
    exit 0
fi

# Lazy-out patterns (case-insensitive)
# Ordered from specific to general to reduce false positives.
LAZY_PATTERN='no (graphical |physical )?(display|screen|monitor|window|gui)\b'
LAZY_PATTERN="$LAZY_PATTERN"'|without a (display|screen|monitor|graphical)'
LAZY_PATTERN="$LAZY_PATTERN"'|headless (environment|mode|context|container|server) (does not|doesn.t|cannot|can.t) (support|allow|show|render|display)'
LAZY_PATTERN="$LAZY_PATTERN"'|requires? (manual|human) (testing|verification|review|inspection|intervention)'
LAZY_PATTERN="$LAZY_PATTERN"'|manual(ly)? (test|verif|inspect|review)'
LAZY_PATTERN="$LAZY_PATTERN"'|cannot (be )?(tested|verified|confirmed|demonstrated|shown|automated) (here|in this|without|from)'
LAZY_PATTERN="$LAZY_PATTERN"'|can.t (be )?(tested|verified|confirmed|demonstrated|shown|automated) (here|in this|without|from)'
LAZY_PATTERN="$LAZY_PATTERN"'|not (possible|feasible) to (test|verify|demonstrate|automate) (this|here|in this|without)'
LAZY_PATTERN="$LAZY_PATTERN"'|out of scope'
LAZY_PATTERN="$LAZY_PATTERN"'|beyond (the )?(scope|capabilities|ability)'
LAZY_PATTERN="$LAZY_PATTERN"'|(this|that) (would need|needs|requires?) to be (done|tested|verified|handled) manually'
LAZY_PATTERN="$LAZY_PATTERN"'|left (as an exercise|for (the )?user|for manual)'
LAZY_PATTERN="$LAZY_PATTERN"'|deferred (to|for|until)'
LAZY_PATTERN="$LAZY_PATTERN"'|i (cannot|can.t|am unable to) (test|verify|demonstrate|show|access|run|interact with|open|launch|display)'
LAZY_PATTERN="$LAZY_PATTERN"'|unable to (test|verify|demonstrate|show|access|run|interact with|open|launch|display)'
LAZY_PATTERN="$LAZY_PATTERN"'|no (automated )?way to (test|verify|demonstrate|run|check|interact)'
LAZY_PATTERN="$LAZY_PATTERN"'|testing (is )?not (possible|feasible|available) (here|in this|without)'
LAZY_PATTERN="$LAZY_PATTERN"'|this (step|task|part|feature|check) (is )?beyond'
LAZY_PATTERN="$LAZY_PATTERN"'|would need (a|an) (actual|real|physical|live) (browser|display|screen|window|user|human)'

MATCHED="$(printf '%s' "$LAST_TEXT" | grep -oiE "$LAZY_PATTERN" | head -1 || true)"

if [ -z "$MATCHED" ]; then
    exit 0
fi

log "lazy phrase detected: '$MATCHED'"

REASON="ACTION REQUIRED — do not stop, do not summarize this back to the user. Replace the lazy-out phrase with concrete action (run the automated path below) and continue. The Stop gate will re-fire until it clears.

Stop blocked by .claude/hooks/Stop--no-excuses.sh

Lazy-out phrase detected in your response: \"$MATCHED\"

Anything a human can do manually, an agent can automate. No excuses.
Here are the automated paths for the most common cases:

  'No display / no graphical environment / headless'
    → Start the Parish headless backend (no display needed):
        bash parish/scripts/parish-mcp-backend.sh start
    → Drive the game via mcp__parish__* tools
    → Screenshot via mcp__parish__parish_latest_screenshot (press F2 in desktop)
    → Use cargo test / just check / just verify for backend logic
    → Use npx playwright test --headless for UI behavior

  'Requires manual testing / verification'
    → Write a test that encodes what you'd check manually
    → Use Bash to exercise the code path and assert the output
    → Use cargo nextest, npm run test, just ui-test, or just ui-e2e
    → Drive through the Parish MCP bridge if it's gameplay behavior

  'Cannot test / unable to verify / I can't demonstrate'
    → Think harder: what tool can observe this behavior?
    → cargo test, parish MCP, playwright, just verify — pick one
    → If you're missing a tool, say exactly which one and ask

  'Out of scope / deferred / beyond scope'
    → That's the user's call, not yours
    → Complete what you can; ask a specific question about the rest
    → Don't silently drop work because it's difficult

Fix the work or ask a specific question. Do not give up.

Genuine hard blocker? Include '[skip-no-excuses]' in your reply with a
one-sentence explanation of why automation is truly impossible here."

jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason}'
