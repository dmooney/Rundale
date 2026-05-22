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

# ── Render (to a staging file so we can size-check before stdout) ──────
# GitHub caps PR comment bodies at 65536 chars. Renderer keeps output
# under 60000 chars to leave headroom for the closing fence and a few
# attachment lines added at the tail.
COMMENT_BYTE_CAP=60000
staged="$(mktemp)"
trap 'rm -f "$staged"' EXIT

{
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
} > "$staged"

# Cap individual section bodies so that even huge AC/evidence/judge
# files can't bust the comment ceiling. Each section is hard-truncated
# to 15 000 bytes individually; when applied alongside the omitted
# transcript section that leaves headroom under the 60 000 ceiling for
# fences and the attachments list. The truncation marker is shown so
# reviewers know the file was clipped.
SECTION_BYTE_CAP=15000
truncate_section_body() {
    local file="$1"
    local bytes
    bytes=$(wc -c < "$file" | tr -d ' ')
    if [[ "$bytes" -le "$SECTION_BYTE_CAP" ]]; then
        cat "$file"
    else
        head -c "$SECTION_BYTE_CAP" "$file"
        printf '\n\n_... [section truncated; %s bytes total — see attached file] ..._\n' "$bytes"
    fi
}

emit_truncated_render() {
    cat <<EOF
<!-- parish-proof-bundle:${task_id} v=1 -->

## Proof: ${task_id}

### Acceptance criteria

$(truncate_section_body "$ac")

### Evidence

$(truncate_section_body "$evidence")

### Transcript

_omitted from inline comment — exceeds GitHub comment-body byte cap. Drop the file referenced below into this PR comment via the GitHub UI to share the full capture._

### Judge

$(truncate_section_body "$judge")
EOF
    emit_attachments_section
    cat <<EOF

<!-- /parish-proof-bundle:${task_id} -->
EOF
}

staged_bytes=$(wc -c < "$staged" | tr -d ' ')
if [[ "$staged_bytes" -gt "$COMMENT_BYTE_CAP" ]]; then
    # First fallback: drop the inline transcript section. The transcript
    # file is still listed under "Artifacts" for upload via the GitHub
    # UI. Re-check size after the rewrite — if AC/evidence/judge themselves
    # are huge, hard-truncate each section so the comment always fits.
    emit_truncated_render > "$staged"
    staged_bytes=$(wc -c < "$staged" | tr -d ' ')
    if [[ "$staged_bytes" -gt "$COMMENT_BYTE_CAP" ]]; then
        echo "render-proof-comment: ERROR — rendered body $staged_bytes bytes exceeds $COMMENT_BYTE_CAP even after section truncation. Reduce the bundle's source files manually." >&2
        exit 1
    fi
fi

cat "$staged"
