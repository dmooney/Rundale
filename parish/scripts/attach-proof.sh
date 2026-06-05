#!/usr/bin/env bash
#
# Attach a proof bundle to a PR.
#
# By default the bundle is written into the PR **body** (a fenced
# `<!-- parish-proof-bundle:<id> v=N -->` … `<!-- /… -->` region). Carrying
# it in the body is race-free: the body is present on the very FIRST
# `agent-check --source=pr` run, so the gate goes green immediately. The old
# behaviour — posting the bundle as a comment AFTER the PR opens — loses a
# race with the `pull_request.opened` gate run and made the first check fail
# spuriously until a re-push (#1177). For a brand-new PR, create it with the
# bundle already in the body (the gate then passes on run #1):
#
#   gh pr create --body-file <(printf '%s\n' "$description" \
#       | bash parish/scripts/compose-proof-body.sh <task-id>)
#
# `attach-proof` itself edits the body of an EXISTING PR, so use it to
# (re-)inject the bundle after fixing it. Both paths share
# compose-proof-body.sh and are idempotent.
#
# Bundles live in .proofs/<task-id>/ (gitignored). The block is rendered by
# render-proof-comment.sh and fenced with HTML comments so
# parish/scripts/agent-check.sh --source=pr can extract and validate it.
#
# Usage: attach-proof.sh <task-id> [<pr-number>] [--as-comment | --via-mcp]
#   <pr-number>   defaults to the PR for the current branch (gh pr view).
#   --as-comment  legacy: post/edit a PR comment instead of the body. Kept
#                 for back-compat; subject to the opened-event race above.
#   --via-mcp     no-gh sandbox path (#1178): validate the bundle locally and
#                 print the rendered block to stdout (no network). Drop it
#                 into the PR body when creating/editing the PR through the
#                 GitHub MCP (race-free), or post it with add_issue_comment.
set -euo pipefail

# Resolve sibling scripts by location, not by CWD, so attach-proof works from
# any directory (and is unit-testable against a throwaway repo).
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

mode="body"
positional=()
for arg in "$@"; do
    case "$arg" in
        --as-comment) mode="comment" ;;
        --to-body) mode="body" ;;
        --via-mcp) mode="via-mcp" ;;
        -*)
            echo "attach-proof: unknown flag '$arg'" >&2
            exit 2
            ;;
        *) positional+=("$arg") ;;
    esac
done
set -- ${positional[@]+"${positional[@]}"}

if [[ $# -lt 1 ]]; then
    echo "Usage: attach-proof.sh <task-id> [<pr-number>] [--as-comment | --via-mcp]" >&2
    exit 2
fi

task_id="$1"
pr_number="${2:-}"

cd "$(git rev-parse --show-toplevel)"

bundle=".proofs/$task_id"
if [[ ! -d "$bundle" ]]; then
    echo "attach-proof: bundle '$bundle' does not exist." >&2
    exit 1
fi

# Validate the bundle locally before publishing — exact same gate the
# author would hit on push. Skip the debt-marker scan here; the local
# `just agent-check` invocation handles that as part of the normal
# pre-push workflow. We only need to confirm the bundle is well-formed.
echo "attach-proof: validating .proofs/$task_id/ against the local proof gate..." >&2
if ! bash "$script_dir/agent-check.sh" --source=local --bundle "$task_id" >/dev/null; then
    echo "attach-proof FAILED: local agent-check rejected the bundle. Fix the issues and re-run." >&2
    exit 1
fi

# --via-mcp: no gh. Print the rendered block to STDOUT (clean, pipeable) and
# all guidance to stderr. The caller posts it through the GitHub MCP — into
# the PR body when creating/editing the PR (race-free, green on run #1), or
# as a comment via add_issue_comment (the gate reads both). For sandboxes
# where gh is absent (#1178).
if [[ "$mode" == "via-mcp" ]]; then
    echo "attach-proof: bundle validated. The fenced block is on stdout." >&2
    echo "attach-proof: preferred — set it as the PR body via the GitHub MCP (create_pull_request / update body) so the gate is green on the first run." >&2
    echo "attach-proof: alternative — post it with the GitHub MCP add_issue_comment tool." >&2
    if compgen -G "$bundle/*.png" >/dev/null \
        || compgen -G "$bundle/*.jpg" >/dev/null \
        || compgen -G "$bundle/*.jpeg" >/dev/null \
        || compgen -G "$bundle/*.gif" >/dev/null \
        || [[ -f "$bundle/transcript.txt" ]]; then
        echo "attach-proof: bundle has binary artifacts — upload them in the GitHub UI; the CI gate only needs the text block." >&2
    fi
    bash "$script_dir/render-proof-comment.sh" "$task_id"
    exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
    echo "attach-proof: 'gh' is required for body/comment mode. Install from https://cli.github.com/," >&2
    echo "attach-proof: or use '--via-mcp' in a no-gh sandbox to emit the block for the GitHub MCP." >&2
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

echo "attach-proof: target PR is #$pr_number (mode: $mode)."

# Resolve the PR's base repository (where the PR lives), NOT the local
# repo. Fork contributors clone their fork as origin, so `gh repo view`
# returns the wrong nameWithOwner. The PR `url` field is the base repo by
# construction (github.com/<owner>/<repo>/pull/<n>) and is available across
# gh versions, unlike the `baseRepository` JSON field.
pr_url="$(gh pr view "$pr_number" --json url --jq '.url // empty')"
repo_full="$(printf '%s\n' "$pr_url" | sed -E 's#^https?://[^/]+/([^/]+/[^/]+)/pull/.*#\1#')"
if [[ -z "$repo_full" || "$repo_full" == "$pr_url" ]]; then
    echo "attach-proof: could not resolve base repository for PR #$pr_number." >&2
    exit 1
fi

if [[ "$mode" == "body" ]]; then
    # Merge the bundle into the PR body: fetch the current body, swap in a
    # freshly rendered region (compose-proof-body strips any prior region
    # for this task id), and write the whole body back. Idempotent.
    body_file="$(mktemp)"
    trap 'rm -f "$body_file"' EXIT
    current_body="$(gh pr view "$pr_number" --repo "$repo_full" --json body --jq '.body // empty')"
    printf '%s' "$current_body" | bash "$script_dir/compose-proof-body.sh" "$task_id" >"$body_file"
    echo "attach-proof: writing bundle into the body of PR #$pr_number."
    gh pr edit "$pr_number" --repo "$repo_full" --body-file "$body_file" >/dev/null
else
    # Legacy comment path. Render the comment body.
    body_file="$(mktemp)"
    trap 'rm -f "$body_file"' EXIT
    bash "$script_dir/render-proof-comment.sh" "$task_id" >"$body_file"

    # Resolve the authenticated user so we can filter for our own prior
    # comments. A reviewer who quoted the fence in their review must not
    # become the target of the PATCH.
    self_login="$(gh api user --jq '.login // empty')"
    if [[ -z "$self_login" ]]; then
        echo "attach-proof: could not resolve authenticated gh user." >&2
        exit 1
    fi

    # Look for an existing comment with this bundle's fence AUTHORED BY the
    # current user. REST returns integer `id`s suitable for PATCH; GraphQL
    # node IDs would require a different mutation path.
    fence="<!-- parish-proof-bundle:${task_id} "
    existing_id="$(
        gh api --paginate "repos/${repo_full}/issues/${pr_number}/comments" \
            --jq ".[] | select(.user.login == \"$self_login\") | select(.body | contains(\"${fence}\")) | .id" \
            2>/dev/null | head -n 1 || true
    )"

    if [[ -n "$existing_id" ]]; then
        echo "attach-proof: editing existing comment $existing_id on PR #$pr_number (author $self_login)."
        gh api --method PATCH \
            "repos/${repo_full}/issues/comments/${existing_id}" \
            --field body=@"$body_file" >/dev/null
    else
        echo "attach-proof: posting new comment on PR #$pr_number as $self_login."
        gh pr comment "$pr_number" --repo "$repo_full" --body-file "$body_file" >/dev/null
    fi
fi

echo "attach-proof: done. View: $(gh pr view "$pr_number" --repo "$repo_full" --json url --jq .url)"

# Reminder: images/transcripts are referenced by name in the bundle but the
# actual binary files have to be uploaded by drag-drop in the GitHub UI. The
# runtime-shipping tier of the gate is satisfied by the
# 'Evidence type: live ...' header in the bundle's evidence.md, so the images
# are for human reviewers — not the CI gate.
if compgen -G "$bundle/*.png" >/dev/null \
    || compgen -G "$bundle/*.jpg" >/dev/null \
    || compgen -G "$bundle/*.jpeg" >/dev/null \
    || compgen -G "$bundle/*.gif" >/dev/null \
    || [[ -f "$bundle/transcript.txt" ]]; then
    echo
    echo "attach-proof: bundle has binary artifacts. Drag the following files into the PR in the GitHub UI:"
    for f in "$bundle"/*.png "$bundle"/*.jpg "$bundle"/*.jpeg "$bundle"/*.gif "$bundle/transcript.txt"; do
        [[ -f "$f" ]] && echo "  - $f"
    done
fi
