#!/usr/bin/env bash
#
# PR proof gate for agent-assisted changes.
#
# This script is intentionally self-contained: CI can run it before installing
# Rust, Node, or `just`, and local agents can run the same check while their
# work is still unstaged.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

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
relevant="$tmpdir/relevant"
evidence="$tmpdir/evidence"
judges="$tmpdir/judges"

{
    git diff --name-only "$base"...HEAD
    git diff --cached --name-only
    git diff --name-only
    git ls-files --others --exclude-standard
} | sed '/^[[:space:]]*$/d' | sort -u > "$changed"

: > "$relevant"
: > "$evidence"
: > "$judges"

is_proof_relevant() {
    local file="$1"
    case "$file" in
        docs/proofs/*)
            return 1
            ;;
        AGENTS.md|CLAUDE.md|justfile|parish/justfile|docs/agent/*|\
        .agents/*|.claude/*|\
        .github/workflows/*|.github/pull_request_template.md|\
        parish/Cargo.toml|parish/Cargo.lock|\
        parish/scripts/*|parish/crates/*|parish/apps/*|parish/testing/*|\
        mods/*|deploy/*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

is_evidence_file() {
    local file="$1"
    case "$file" in
        docs/proofs/*/judge.md|docs/proofs/README.md)
            return 1
            ;;
        docs/proofs/*/*.md|docs/proofs/*/*.txt|docs/proofs/*/*.png|\
        docs/proofs/*/*.jpg|docs/proofs/*/*.jpeg|docs/proofs/*/*.gif)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

is_judge_file() {
    case "$1" in
        docs/proofs/*/judge.md)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

validate_evidence_file() {
    local file="$1"
    case "$file" in
        *.png|*.jpg|*.jpeg|*.gif)
            return 0
            ;;
        *.md|*.txt)
            if grep -Eiq '^Evidence type:[[:space:]]*(gameplay transcript|screenshot|gif)[[:space:]]*$' "$file"; then
                return 0
            fi
            echo "agent-check FAILED: $file must declare 'Evidence type: gameplay transcript', 'screenshot', or 'gif'." >&2
            return 1
            ;;
        *)
            echo "agent-check FAILED: $file is not an accepted proof artifact type." >&2
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
    if is_proof_relevant "$file"; then
        echo "$file" >> "$relevant"
    fi
    if [[ -f "$file" ]] && is_evidence_file "$file"; then
        echo "$file" >> "$evidence"
    fi
    if [[ -f "$file" ]] && is_judge_file "$file"; then
        echo "$file" >> "$judges"
    fi
done < "$changed"

changed_count="$(wc -l < "$changed" | tr -d ' ')"
relevant_count="$(wc -l < "$relevant" | tr -d ' ')"
evidence_count="$(wc -l < "$evidence" | tr -d ' ')"
judge_count="$(wc -l < "$judges" | tr -d ' ')"

echo "agent-check: comparing $changed_count changed file(s) against $base_ref."

failed=0

if [[ "$relevant_count" -gt 0 ]]; then
    echo "agent-check: $relevant_count proof-relevant file(s) changed."

    if [[ "$evidence_count" -eq 0 ]]; then
        echo "agent-check FAILED: proof-relevant changes require a changed artifact under docs/proofs/<proof-id>/." >&2
        echo "Accepted evidence forms: gameplay transcript (.md or .txt), screenshot (.png/.jpg/.jpeg), or gif (.gif)." >&2
        failed=1
    else
        while IFS= read -r file; do
            validate_evidence_file "$file" || failed=1
        done < "$evidence"
    fi

    if [[ "$judge_count" -eq 0 ]]; then
        echo "agent-check FAILED: proof-relevant changes require docs/proofs/<proof-id>/judge.md." >&2
        echo "The judge file must include 'Verdict: sufficient' and 'Technical debt: clear'." >&2
        failed=1
    else
        while IFS= read -r file; do
            if ! grep -Eiq '^Verdict:[[:space:]]*sufficient([[:space:]]|$)' "$file"; then
                echo "agent-check FAILED: $file must include 'Verdict: sufficient'." >&2
                failed=1
            fi
            if ! grep -Eiq '^Technical debt:[[:space:]]*clear([[:space:]]|$)' "$file"; then
                echo "agent-check FAILED: $file must include 'Technical debt: clear'." >&2
                failed=1
            fi
        done < "$judges"
    fi
else
    echo "agent-check: no proof-relevant changes; proof bundle not required."
fi

debt_found=0
while IFS= read -r file; do
    # Skip scanning the check tools and docs themselves to avoid matching the regex patterns they contain
    [[ "$file" == "parish/scripts/agent-check.sh" ]] && continue
    [[ "$file" == "parish/justfile" ]] && continue
    [[ "$file" == "docs/agent/witness.md" ]] && continue
    if scan_for_debt_markers "$file"; then
        debt_found=1
    fi
done < "$changed"

if [[ "$debt_found" -eq 1 ]]; then
    echo "agent-check FAILED: placeholder-like debt markers found in changed files." >&2
    failed=1
fi

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi

if [[ "$relevant_count" -gt 0 ]]; then
    echo "agent-check passed: proof evidence and judge verdict are present; no placeholder debt markers found."
else
    echo "agent-check passed: no proof needed; no placeholder debt markers found."
fi
