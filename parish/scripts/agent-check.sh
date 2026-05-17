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
runtime="$tmpdir/runtime"
evidence="$tmpdir/evidence"
judges="$tmpdir/judges"
ac_files="$tmpdir/ac_files"

{
    git diff --name-only "$base"...HEAD
    git diff --cached --name-only
    git diff --name-only
    git ls-files --others --exclude-standard
} | sed '/^[[:space:]]*$/d' | sort -u > "$changed"

: > "$relevant"
: > "$runtime"
: > "$evidence"
: > "$judges"
: > "$ac_files"

is_proof_relevant() {
    local file="$1"
    case "$file" in
        # Proof bundles themselves are never the trigger.
        docs/proofs/*)
            return 1
            ;;
        # Documentation, agent instructions, build config, CI workflows,
        # and check tooling require proof only when paired with a runtime
        # code change. On their own, they have no gameplay behavior to
        # prove. Per rule 10 in AGENTS.md.
        *.md|*.txt|\
        AGENTS.md|CLAUDE.md|README.md|\
        justfile|parish/justfile|\
        docs/*|.agents/*|.claude/*|\
        .github/*|\
        parish/scripts/*)
            return 1
            ;;
        # Source / runtime paths.
        parish/Cargo.toml|parish/Cargo.lock|\
        parish/crates/*|parish/apps/*|parish/testing/*|\
        mods/*|deploy/*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

# A subset of proof-relevant paths that ship runtime behavior into a real
# process (Tauri desktop, axum web server, headless CLI, UI). Changes
# under these prefixes can only be proven by exercising the code in a
# live process — unit tests alone don't fire startup wiring, IPC
# handlers, or browser-mounted Svelte components. Pure logic crates
# (parish-config, parish-types, parish-palette, parish-persistence) are
# excluded — their behaviour is fully covered by `cargo test`. Per rule
# #10 in AGENTS.md.
is_runtime_path() {
    local file="$1"
    case "$file" in
        parish/crates/parish-tauri/*|\
        parish/crates/parish-server/*|\
        parish/crates/parish-cli/*|\
        parish/crates/parish-core/src/game_loop/*|\
        parish/crates/parish-core/src/game_session/*|\
        parish/crates/parish-core/src/ipc/*|\
        parish/crates/parish-inference/src/setup.rs|\
        parish/crates/parish-inference/src/client.rs|\
        parish/crates/parish-npc/src/ticks.rs|\
        parish/crates/parish-npc/src/manager.rs|\
        parish/crates/parish-npc/src/reactions/*|\
        parish/crates/parish-npc/src/autonomous/*|\
        parish/crates/parish-world/*|\
        parish/crates/parish-input/*|\
        parish/apps/ui/src/*|\
        mods/*|\
        .claude/hooks/*|\
        .claude/skills/*)
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
        docs/proofs/*/judge.md|docs/proofs/README.md|docs/proofs/*/acceptance-criteria.md)
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

is_acceptance_criteria_file() {
    case "$1" in
        docs/proofs/*/acceptance-criteria.md)
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
        # Raw transcripts (.txt) carry literal program output — they
        # don't need a typed header, only the markdown summary file
        # alongside them does. Pairing pattern: write a transcript
        # `*.txt` capturing real output, and a sibling `evidence.md`
        # whose `Evidence type:` header declares the run kind.
        *.txt)
            return 0
            ;;
        *.md)
            # Accept the optional `live ` prefix that the runtime-path
            # tier (rule #10) requires for proofs of changes touching
            # the Tauri/server/CLI/UI/mod seams. Plain
            # `Evidence type: gameplay transcript` remains valid for
            # non-runtime proof-relevant changes.
            if grep -Eiq '^Evidence type:[[:space:]]*(live[[:space:]]+)?(gameplay transcript|screenshot|gif)[[:space:]]*$' "$file"; then
                return 0
            fi
            echo "agent-check FAILED: $file must declare 'Evidence type: [live ](gameplay transcript|screenshot|gif)'." >&2
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
    if is_runtime_path "$file"; then
        echo "$file" >> "$runtime"
    fi
    if [[ -f "$file" ]] && is_evidence_file "$file"; then
        echo "$file" >> "$evidence"
    fi
    if [[ -f "$file" ]] && is_judge_file "$file"; then
        echo "$file" >> "$judges"
    fi
    if [[ -f "$file" ]] && is_acceptance_criteria_file "$file"; then
        echo "$file" >> "$ac_files"
    fi
done < "$changed"

changed_count="$(wc -l < "$changed" | tr -d ' ')"
relevant_count="$(wc -l < "$relevant" | tr -d ' ')"
runtime_count="$(wc -l < "$runtime" | tr -d ' ')"
evidence_count="$(wc -l < "$evidence" | tr -d ' ')"
judge_count="$(wc -l < "$judges" | tr -d ' ')"
ac_count="$(wc -l < "$ac_files" | tr -d ' ')"

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
            # When the same proof bundle has an acceptance-criteria.md, the
            # judge must also confirm every criterion was verified against the
            # game log. Only enforced for bundles that opted into the
            # AC workflow (i.e. have the sibling file).
            bundle_dir="$(dirname "$file")"
            if [[ -f "$bundle_dir/acceptance-criteria.md" ]]; then
                if ! grep -Eiq '^Acceptance criteria:[[:space:]]*met([[:space:]]|$)' "$file"; then
                    echo "agent-check FAILED: $file must include 'Acceptance criteria: met' (bundle has acceptance-criteria.md)." >&2
                    echo "The judge must verify every criterion from acceptance-criteria.md against the game log." >&2
                    failed=1
                fi
            fi
        done < "$judges"
    fi

    # Proof bundles added in this diff must include acceptance-criteria.md.
    # We detect new bundles by finding evidence/judge files that are new
    # (present in working tree but not in base). Only new bundles are
    # checked — existing proofs on main are not retroactively broken.
    if [[ "$evidence_count" -gt 0 || "$judge_count" -gt 0 ]]; then
        while IFS= read -r file; do
            bundle_dir="$(dirname "$file")"
            ac_path="$bundle_dir/acceptance-criteria.md"
            # Check if this specific artifact (evidence/judge) is new in base.
            # Using the file path directly avoids the false-negative where a bundle
            # dir pre-existed (e.g. with notes) but evidence/judge are new.
            if ! git show "$base:$file" >/dev/null 2>&1; then
                # New bundle: acceptance-criteria.md must be present.
                if [[ ! -f "$ac_path" ]]; then
                    echo "agent-check FAILED: new proof bundle '$bundle_dir/' is missing acceptance-criteria.md." >&2
                    echo "Write acceptance criteria BEFORE coding using /task-start <task-id>." >&2
                    echo "See rule 13 in AGENTS.md." >&2
                    failed=1
                fi
            fi
        done < <(cat "$evidence" "$judges" 2>/dev/null | sort -u)
        if [[ "$ac_count" -gt 0 ]]; then
            echo "agent-check: $ac_count acceptance-criteria file(s) present."
        fi
    fi

    # Runtime-path tier: when the diff touches a path that only fires in
    # a real process (Tauri startup, server routes, CLI bootstrap,
    # NPC tick loop, mod content, UI components), the evidence must
    # show the change was actually run live. Accepted live signals:
    #   - any binary artifact (screenshot .png/.jpg/.jpeg, gif .gif) —
    #     these can't be produced without running the app, and
    #   - an `.md` summary file that declares
    #     'Evidence type: live gameplay transcript'.
    # A plain 'Evidence type: gameplay transcript' is not enough — that
    # phrasing is used today for analysis-only writeups that never
    # touch a live process. The added word "live" is the explicit
    # author affirmation that the run happened (#NNN).
    #
    # `.txt` transcripts carry raw program output and are exempt from
    # the header requirement under `validate_evidence_file` — grepping
    # them here would risk a false-positive live signal from literal
    # output containing the regex pattern. The `.md` is where the
    # author makes the live claim; the `.txt` is the corroborating
    # evidence body.
    if [[ "$runtime_count" -gt 0 ]]; then
        echo "agent-check: $runtime_count runtime-shipping file(s) changed; live proof required."
        live_found=0
        if [[ "$evidence_count" -gt 0 ]]; then
            while IFS= read -r file; do
                case "$file" in
                    *.png|*.jpg|*.jpeg|*.gif)
                        live_found=1
                        ;;
                    *.md)
                        if grep -Eiq '^Evidence type:[[:space:]]*live[[:space:]]+(gameplay transcript|screenshot|gif)[[:space:]]*$' "$file"; then
                            live_found=1
                        fi
                        ;;
                esac
            done < "$evidence"
        fi
        if [[ "$live_found" -eq 0 ]]; then
            echo "agent-check FAILED: runtime-shipping changes require evidence from a live process." >&2
            echo "Provide a screenshot/gif under docs/proofs/<proof-id>/, or a transcript whose" >&2
            echo "header declares 'Evidence type: live gameplay transcript' (the literal word 'live'" >&2
            echo "asserts the change was exercised in a real Tauri / server / CLI / browser, not just" >&2
            echo "in unit tests)." >&2
            failed=1
        fi
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
    [[ "$file" == ".agents/skills/rundale-ci-pitfalls/SKILL.md" ]] && continue
    [[ "$file" == ".agents/skills/task-start/SKILL.md" ]] && continue
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
