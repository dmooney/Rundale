#!/usr/bin/env bash
#
# Post (or edit) a structured PR comment that carries a proof bundle.
#
# Bundles live in .proofs/<task-id>/ (gitignored). The comment body is
# rendered by render-proof-comment.sh and fenced with HTML comments so
# parish/scripts/agent-check.sh --source=pr can extract and validate it.
#
# Idempotent: re-running on the same task edits the existing
# parish-proof-bundle:<task-id> comment instead of appending a new one.
#
# Usage: attach-proof.sh <task-id> [<pr-number>]
#   <pr-number> defaults to the PR for the current branch (gh pr view).
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: attach-proof.sh <task-id> [<pr-number>]" >&2
    exit 2
fi

task_id="$1"
pr_number="${2:-}"

cd "$(git rev-parse --show-toplevel)"

if ! command -v gh >/dev/null 2>&1; then
    echo "attach-proof: 'gh' is required. Install from https://cli.github.com/." >&2
    exit 1
fi

bundle=".proofs/$task_id"
if [[ ! -d "$bundle" ]]; then
    echo "attach-proof: bundle '$bundle' does not exist." >&2
    exit 1
fi

# Validate the bundle locally before publishing — exact same gate the
# author would hit on push. Skip the debt-marker scan here; the local
# `just agent-check` invocation handles that as part of the normal
# pre-push workflow. We only need to confirm the bundle is well-formed.
echo "attach-proof: validating .proofs/$task_id/ against the local proof gate..."
if ! bash parish/scripts/agent-check.sh --source=local >/dev/null; then
    echo "attach-proof FAILED: local agent-check rejected the bundle. Fix the issues and re-run." >&2
    exit 1
fi

# Resolve PR number if not given.
if [[ -z "$pr_number" ]]; then
    pr_number="$(gh pr view --json number --jq .number 2>/dev/null || true)"
    if [[ -z "$pr_number" ]]; then
        echo "attach-proof: no PR detected for the current branch — pass the PR number explicitly." >&2
        exit 1
    fi
fi

echo "attach-proof: target PR is #$pr_number."

# Render the comment body.
body_file="$(mktemp)"
trap 'rm -f "$body_file"' EXIT
bash parish/scripts/render-proof-comment.sh "$task_id" > "$body_file"

# Resolve owner/repo for REST calls.
repo_full="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"

# Look for an existing comment with this bundle's fence. REST returns
# integer `id`s suitable for PATCH; GraphQL node IDs would require a
# different mutation path.
fence="<!-- parish-proof-bundle:${task_id} "
existing_id="$(
    gh api --paginate "repos/${repo_full}/issues/${pr_number}/comments" \
        --jq ".[] | select(.body | contains(\"${fence}\")) | .id" \
        2>/dev/null | head -n 1 || true
)"

if [[ -n "$existing_id" ]]; then
    echo "attach-proof: editing existing comment $existing_id on PR #$pr_number."
    gh api --method PATCH \
        "repos/${repo_full}/issues/comments/${existing_id}" \
        --field body=@"$body_file" >/dev/null
else
    echo "attach-proof: posting new comment on PR #$pr_number."
    gh pr comment "$pr_number" --body-file "$body_file" >/dev/null
fi

echo "attach-proof: done. View: $(gh pr view "$pr_number" --json url --jq .url)"

# Reminder: images/transcripts are referenced by name in the comment but
# the actual binary files have to be uploaded by drag-drop in the GitHub
# UI. The runtime-shipping tier of the gate is satisfied by the
# 'Evidence type: live ...' header in the bundle's evidence.md, so the
# images are for human reviewers — not the CI gate.
if compgen -G "$bundle/*.png" >/dev/null \
    || compgen -G "$bundle/*.jpg" >/dev/null \
    || compgen -G "$bundle/*.jpeg" >/dev/null \
    || compgen -G "$bundle/*.gif" >/dev/null \
    || [[ -f "$bundle/transcript.txt" ]]; then
    echo
    echo "attach-proof: bundle has binary artifacts. Drag the following files into the PR comment in the GitHub UI:"
    for f in "$bundle"/*.png "$bundle"/*.jpg "$bundle"/*.jpeg "$bundle"/*.gif "$bundle/transcript.txt"; do
        [[ -f "$f" ]] && echo "  - $f"
    done
fi
