#!/usr/bin/env bash
#
# Render a proof bundle in .proofs/<task-id>/ as a structured Markdown
# comment for a GitHub pull request. The output goes to stdout.
#
# The rendered block is fenced with HTML comments so that
# parish/scripts/agent-check.sh --source=pr can extract and validate it:
#
#     <!-- parish-proof-bundle:<task-id> v=1 -->
#     ...content...
#     <!-- /parish-proof-bundle:<task-id> -->
#
# Required files in the bundle:
#   acceptance-criteria.md
#   evidence.md          (with `Evidence type:` header)
#   judge.md             (with Verdict / Technical debt / Acceptance criteria
#                        verdict lines)
# Optional:
#   transcript.txt       (truncated inline; full file expected as a PR
#                        attachment)
#   *.png / *.jpg / *.jpeg / *.gif  (referenced inline as PR attachments)
#
# Usage: render-proof-comment.sh <task-id>
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: render-proof-comment.sh <task-id>" >&2
    exit 2
fi

task_id="$1"
cd "$(git rev-parse --show-toplevel)"

bundle=".proofs/$task_id"
if [[ ! -d "$bundle" ]]; then
    echo "render-proof-comment: bundle directory '$bundle' does not exist." >&2
    exit 1
fi

ac="$bundle/acceptance-criteria.md"
evidence="$bundle/evidence.md"
judge="$bundle/judge.md"
transcript="$bundle/transcript.txt"

for required in "$ac" "$evidence" "$judge"; do
    if [[ ! -f "$required" ]]; then
        echo "render-proof-comment: missing required file '$required'." >&2
        exit 1
    fi
done

# Transcript: include the first 80 lines and the last 200 lines if the
# file is bigger than ~300 lines. Keeps the comment under GitHub's 64 KiB
# body cap on typical multi-hour runs while still showing context.
emit_transcript_inline() {
    [[ -f "$transcript" ]] || return 0
    local total
    total=$(wc -l < "$transcript" | tr -d ' ')
    echo
    echo '### Transcript'
    echo
    echo '```text'
    if [[ "$total" -gt 300 ]]; then
        head -n 80 "$transcript"
        echo
        echo "... [truncated; full transcript has $total lines — attach the .txt file to this PR for the complete capture] ..."
        echo
        tail -n 200 "$transcript"
    else
        cat "$transcript"
    fi
    echo '```'
}

emit_attachments_section() {
    local found=0
    local f
    for f in "$bundle"/*.png "$bundle"/*.jpg "$bundle"/*.jpeg "$bundle"/*.gif; do
        [[ -f "$f" ]] || continue
        if [[ "$found" -eq 0 ]]; then
            echo
            echo '### Artifacts'
            echo
            echo 'Drag these files into this PR comment in the GitHub UI to attach them:'
            echo
            found=1
        fi
        echo "- \`$(basename "$f")\`"
    done
    if [[ -f "$transcript" ]]; then
        if [[ "$found" -eq 0 ]]; then
            echo
            echo '### Artifacts'
            echo
            found=1
        fi
        echo "- \`$(basename "$transcript")\` (full transcript)"
    fi
}

# ── Render ──────────────────────────────────────────────────────────────
cat <<EOF
<!-- parish-proof-bundle:${task_id} v=1 -->

## Proof: ${task_id}

### Acceptance criteria

$(cat "$ac")

### Evidence

$(cat "$evidence")
EOF

emit_transcript_inline

cat <<EOF

### Judge

$(cat "$judge")
EOF

emit_attachments_section

cat <<EOF

<!-- /parish-proof-bundle:${task_id} -->
EOF
