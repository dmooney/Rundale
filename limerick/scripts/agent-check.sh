#!/usr/bin/env bash
#
# PR evidence gate for agent-assisted changes.
#
# Two source modes:
#   --source=local              (default) Classify the working tree's changes
#                               and run the lints. Used by `just agent-check`
#                               and `just check`. It cannot see a PR body, so
#                               it only reports whether a link will be needed.
#   --source=pr <number>        Also require the evidence-page link in the PR
#                               body. Used by CI's agent-check job. Requires
#                               `gh` and read access to the PR.
#
# In both modes the script:
#   - Diffs the working tree against the base ref.
#   - Classifies changed files as runtime-shipping or not.
#   - Rejects any `.proofs/` path appearing in the diff (the retired proof
#     bundles were never meant to be committed).
#   - Rejects placeholder debt markers in changed files.
#
# In PR mode, a diff that touches a runtime-shipping path must link its
# evidence page, https://dmooney.github.io/rundale-pages/pr/<number>/, in
# the PR body. Acceptance criteria and the verification actually run are
# ordinary PR prose; the gate does not parse them. See
# docs/agent/agent-check.md.
#
# The script is intentionally self-contained: CI can run it before installing
# Rust, Node, or `just`, and local agents can run the same check while their
# work is still unstaged.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

evidence_base_url="https://dmooney.github.io/rundale-pages/pr"

source_mode="local"
pr_number=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --source=local)
            source_mode="local"
            shift
            ;;
        --source=pr)
            source_mode="pr"
            pr_number="${2:-}"
            if [[ -z "$pr_number" ]]; then
                echo "agent-check: --source=pr requires a PR number." >&2
                exit 2
            fi
            shift 2
            ;;
        --source=pr=*)
            source_mode="pr"
            pr_number="${1#--source=pr=}"
            shift
            ;;
        *)
            echo "agent-check: unknown argument: $1" >&2
            echo "Usage: agent-check.sh [--source=local | --source=pr <number>]" >&2
            exit 2
            ;;
    esac
done

if [[ "$source_mode" == "pr" && ! "$pr_number" =~ ^[0-9]+$ ]]; then
    echo "agent-check: PR number must be numeric, got '$pr_number'." >&2
    exit 2
fi

base_ref="${AGENT_CHECK_BASE_REF:-}"
if [[ -z "$base_ref" ]]; then
    if git rev-parse --verify --quiet origin/main >/dev/null; then
        base_ref="origin/main"
    else
        base_ref="main"
    fi
fi

if ! git rev-parse --verify --quiet "$base_ref" >/dev/null; then
    echo "agent-check FAILED: base ref '$base_ref' does not exist." >&2
    echo "Set AGENT_CHECK_BASE_REF to the branch or commit this change should be compared against." >&2
    exit 2
fi

base="$(git merge-base "$base_ref" HEAD 2>/dev/null || git rev-parse "$base_ref")"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

changed="$tmpdir/changed"
runtime="$tmpdir/runtime"

{
    git diff --name-only "$base"...HEAD
    git diff --cached --name-only
    git diff --name-only
    git ls-files --others --exclude-standard
} | sed '/^[[:space:]]*$/d' | sort -u >"$changed"

: >"$runtime"

# Rust, UI, and Swift unit-test code never ships into a live process; a
# test run is honest evidence for it. Mobile UI tests are handled below.
is_test_path() {
    case "$1" in
        *.test.* | *.spec.* | */tests/* | */e2e/* | limerick/apps/ui/src/test-setup.ts)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

# Runtime-shipping paths: code that runs in the iPhone app or a real engine
# process (server, headless CLI, Tauri, UI), plus the game content those
# processes load. A change here needs an evidence page showing it working.
is_runtime_path() {
    local file="$1"
    case "$file" in
        # Generated documentation, even under a runtime tree such as mods/.
        graphify-out/* | */graphify-out/*)
            return 1
            ;;
        # Markdown anywhere is documentation, not shipped behavior. .txt
        # stays runtime under mods/: mods ship prompt templates as .txt.
        *.md)
            return 1
            ;;
        # Docs, CI, agent instructions, check tooling, build recipes, and
        # the iPhone app's verification/release scripts ship nothing.
        docs/* | .github/* | .agents/* | .claude/* | \
            limerick/scripts/* | justfile | limerick/justfile | \
            mobile/scripts/*)
            return 1
            ;;
        # Swift unit tests (the app's and each Swift package's), like Rust
        # and UI tests above.
        mobile/RundaleTests/* | mobile/*/Tests/*)
            return 1
            ;;
        # The iPhone app, its Swift packages, endpoint fixtures, and its UI
        # tests. Mobile UI tests drive the app itself, so they count.
        mobile/*)
            return 0
            ;;
    esac
    if is_test_path "$file"; then
        return 1
    fi
    case "$file" in
        limerick/crates/limerick-tauri/* | \
            limerick/crates/limerick-server/* | \
            limerick/crates/limerick-engine/* | \
            limerick/crates/limerick-core/src/game_loop/* | \
            limerick/crates/limerick-core/src/game_session/* | \
            limerick/crates/limerick-core/src/ipc/* | \
            limerick/crates/limerick-inference/src/setup.rs | \
            limerick/crates/limerick-inference/src/client.rs | \
            limerick/crates/limerick-npc/src/ticks.rs | \
            limerick/crates/limerick-npc/src/manager.rs | \
            limerick/crates/limerick-npc/src/reactions/* | \
            limerick/crates/limerick-npc/src/autonomous/* | \
            limerick/crates/limerick-world/* | \
            limerick/crates/limerick-input/* | \
            limerick/apps/ui/src/* | \
            mods/*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

scan_for_debt_markers() {
    local file="$1"
    [[ -f "$file" ]] || return 1   # file deleted/absent — no debt to find
    grep -Iq . "$file" || return 1 # binary file — skip

    grep -En \
        -e '//[[:space:]]*unchanged' \
        -e '//[[:space:]]*existing' \
        -e '//[[:space:]]*[.][.][.]([[:space:]]*rest of the function)?' \
        -e '/[*][[:space:]]*[.][.][.][[:space:]]*[*]/' \
        -e 'pass[[:space:]]*#[[:space:]]*TODO' \
        -e 'return nil[[:space:]]*//[[:space:]]*placeholder' \
        -e 'todo!\(' \
        -e 'unimplemented!\(' \
        -e 'unreachable!\([[:space:]]*\)' \
        -e 'panic!\("[Nn]ot implemented' \
        -e 'panic!\("[Tt]odo' \
        -- "$file"
}

while IFS= read -r file; do
    if is_runtime_path "$file"; then
        echo "$file" >>"$runtime"
    fi
done <"$changed"

changed_count="$(wc -l <"$changed" | tr -d ' ')"
runtime_count="$(wc -l <"$runtime" | tr -d ' ')"

echo "agent-check: source=$source_mode; comparing $changed_count changed file(s) against $base_ref."

failed=0

# Lint: `.proofs/` must never appear in the diff. It held the retired proof
# bundles and stays gitignored; a tracked file there is a leftover.
if grep -E '^\.proofs/' "$changed" >/dev/null 2>&1; then
    echo "agent-check FAILED: .proofs/ paths appear in the diff:" >&2
    grep -E '^\.proofs/' "$changed" | head -5 | sed 's/^/  - /' >&2
    echo "Proof bundles are retired. Put evidence on the PR's evidence page and its verification in the PR body." >&2
    failed=1
fi

if [[ "$runtime_count" -gt 0 ]]; then
    echo "agent-check: $runtime_count runtime-shipping file(s) changed; an evidence page link is required:"
    head -5 "$runtime" | sed 's/^/  - /'
    if [[ "$source_mode" == "pr" ]]; then
        if ! command -v gh >/dev/null 2>&1; then
            echo "agent-check FAILED: --source=pr requires 'gh' to be installed." >&2
            exit 1
        fi
        body="$tmpdir/body.md"
        # Only the PR author and maintainers can edit the body, so comments
        # are never read: a third-party comment cannot satisfy the gate.
        if ! gh pr view "$pr_number" --json body --jq '.body // empty' >"$body" 2>/dev/null; then
            echo "agent-check FAILED: could not fetch PR #$pr_number." >&2
            exit 1
        fi
        expected="$evidence_base_url/$pr_number/"
        if grep -Fq -- "$expected" "$body"; then
            echo "agent-check: PR body links $expected"
        else
            echo "agent-check FAILED: PR #$pr_number changes runtime-shipping code but its body does not link $expected" >&2
            if grep -Eq "$evidence_base_url/[0-9]+/" "$body"; then
                echo "The body links another PR's evidence page; the link must name this PR's number." >&2
            fi
            echo "Publish the recording with 'bash limerick/scripts/publish-pr-page.sh $pr_number <dir>' and link the page in the PR body." >&2
            echo "Editing the body does not re-run the job; re-run it or push a commit." >&2
            failed=1
        fi
    else
        echo "agent-check: local mode cannot read the PR body; CI requires $evidence_base_url/<number>/ there."
    fi
else
    echo "agent-check: no runtime-shipping changes; no evidence page required."
fi

debt_found=0
while IFS= read -r file; do
    # The debt scanner hunts for stubbed-out *code* an agent left behind.
    # Documentation (Markdown) legitimately contains illustrative, deliberately
    # incomplete code snippets (`// ... existing`, `unimplemented!()`, etc.) as
    # examples — scanning prose for these is a false positive. Skip all docs;
    # also skip the check tooling, which embeds the marker regexes themselves.
    [[ "$file" == *.md ]] && continue
    [[ "$file" == "limerick/scripts/agent-check.sh" ]] && continue
    [[ "$file" == "limerick/justfile" ]] && continue
    if scan_for_debt_markers "$file"; then
        debt_found=1
    fi
done <"$changed"

if [[ "$debt_found" -eq 1 ]]; then
    echo "agent-check FAILED: placeholder-like debt markers found in changed files." >&2
    failed=1
fi

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi

echo "agent-check passed: no .proofs/ paths; no placeholder debt markers found."
