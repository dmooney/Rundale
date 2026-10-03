#!/usr/bin/env bash
#
# PR proof gate for agent-assisted changes.
#
# Two source modes:
#   --source=local              (default) Validate proof bundles in .proofs/ on
#                               disk. Used by `just agent-check` and the Stop
#                               hook before push.
#   --source=pr <number>        Validate proof bundles posted to a PR as
#                               structured comments. Used by CI's agent-check
#                               job. Requires `gh` and read access to the PR.
#
# In both modes the script:
#   - Diffs the working tree against the base ref.
#   - Categorises changed files into proof-relevant / runtime-shipping.
#   - Validates that proof artifacts (acceptance criteria and evidence)
#     exist and contain the required header lines. A legacy judge.md is
#     ignored; the implementing agent no longer grades its own work.
#   - Rejects placeholder debt markers in changed files.
#   - Rejects any `.proofs/` path appearing in the diff (those files are
#     meant to live in PR comments only — see `just attach-proof`).
#
# The script is intentionally self-contained: CI can run it before installing
# Rust, Node, or `just`, and local agents can run the same check while their
# work is still unstaged.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

source_mode="local"
pr_number=""
bundle_filter=""
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
        --bundle)
            bundle_filter="${2:-}"
            if [[ -z "$bundle_filter" ]]; then
                echo "agent-check: --bundle requires a task-id." >&2
                exit 2
            fi
            shift 2
            ;;
        --bundle=*)
            bundle_filter="${1#--bundle=}"
            shift
            ;;
        *)
            echo "agent-check: unknown argument: $1" >&2
            echo "Usage: agent-check.sh [--source=local | --source=pr <number>] [--bundle <task-id>]" >&2
            exit 2
            ;;
    esac
done

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
ac_files="$tmpdir/ac_files"
test_only="$tmpdir/test_only"

{
    git diff --name-only "$base"...HEAD
    git diff --cached --name-only
    git diff --name-only
    git ls-files --others --exclude-standard
} | sed '/^[[:space:]]*$/d' | sort -u >"$changed"

: >"$relevant"
: >"$runtime"
: >"$evidence"
: >"$ac_files"
: >"$test_only"

is_proof_relevant() {
    local file="$1"
    case "$file" in
        # Proof bundles themselves are never the trigger.
        docs/proofs/* | .proofs/*)
            return 1
            ;;
        # Graphify outputs are generated documentation, even when the graph
        # describes files under a runtime-owned tree such as mods/.
        graphify-out/* | */graphify-out/*)
            return 1
            ;;
        # Documentation, agent instructions, build config, CI workflows,
        # and check tooling require proof only when paired with a runtime
        # code change. On their own, they have no gameplay behavior to
        # prove (truthful test automation, docs/agent/test-tooling-rules.md).
        # (*.md already covers AGENTS.md and README.md.)
        *.md | *.txt | \
            justfile | limerick/justfile | \
            docs/* | .agents/* | .claude/* | \
            .github/* | \
            limerick/scripts/*)
            return 1
            ;;
        # Source / runtime paths.
        limerick/Cargo.toml | limerick/Cargo.lock | \
            limerick/crates/* | limerick/apps/* | limerick/testing/* | \
            mods/* | deploy/*)
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
# (limerick-config, limerick-types, limerick-palette, limerick-persistence) are
# excluded — their behaviour is fully covered by `cargo test`. See
# truthful test automation in docs/agent/test-tooling-rules.md.
# Test code: unit/integration/e2e tests and the test runner's setup file.
# Still proof-relevant (a bundle is required), but it never ships into a
# live process, so it is excluded from the runtime tier below and a test
# run is honest evidence for it.
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

# Print package.json with its top-level devDependencies block removed, so
# two versions compare equal exactly when only devDependencies changed.
strip_dev_dependencies() {
    awk '
        function count(s, ch,   t) { t = s; return gsub(ch, "", t) }
        !skip && /^[[:space:]]*"devDependencies":[[:space:]]*\{/ { skip = 1; depth = 0 }
        skip {
            depth += count($0, "{") - count($0, "}")
            if (depth <= 0) skip = 0
            next
        }
        { print }
    '
}

# Print one record per package-lock.json "packages" entry:
# key <FS> dev-flag <FS> entry text. Only "dev": true counts as dev-only;
# "devOptional" can still reach a production optional dependency.
lock_entries() {
    awk '
        BEGIN { fs = sprintf("%c", 28); join = sprintf("%c", 29) }
        function count(s, ch,   t) { t = s; return gsub(ch, "", t) }
        {
            line = $0
            if (in_entry) {
                body = body join line
                if (depth == 3 && line ~ /^[[:space:]]*"dev":[[:space:]]*true/) dev = 1
            } else if (in_packages && depth == 2 && line ~ /^[[:space:]]*"[^"]*":[[:space:]]*\{/) {
                key = line
                sub(/^[[:space:]]*"/, "", key)
                sub(/".*$/, "", key)
                in_entry = 1
                body = line
                dev = 0
            } else if (depth == 1 && line ~ /^[[:space:]]*"packages":[[:space:]]*\{/) {
                in_packages = 1
            }
            depth += count(line, "{") - count(line, "}")
            if (in_entry && depth == 2) {
                print key fs dev fs body
                in_entry = 0
            }
            if (in_packages && depth == 1) in_packages = 0
        }
    '
}

# True when every lockfile entry added, removed, or changed between two
# lockfiles is flagged dev-only. The root entry ("") mirrors package.json,
# which is checked separately.
lock_changes_dev_only() {
    awk '
        BEGIN { FS = sprintf("%c", 28) }
        NR == FNR { old_body[$1] = $3; old_dev[$1] = $2; next }
        { new_body[$1] = $3; new_dev[$1] = $2 }
        END {
            for (k in new_body) {
                if (k == "") continue
                if (!(k in old_body) || new_body[k] != old_body[k]) {
                    if (new_dev[k] != 1) exit 1
                    if ((k in old_body) && old_dev[k] != 1) exit 1
                }
            }
            for (k in old_body) {
                if (k == "") continue
                if (!(k in new_body) && old_dev[k] != 1) exit 1
            }
            exit 0
        }
    ' <(git show "$base:$1" 2>/dev/null | lock_entries) <(if [[ -f "$1" ]]; then lock_entries <"$1"; fi)
}

# A UI package manifest change confined to development tooling: package.json
# differs only in devDependencies, and every package-lock.json entry it moves
# is dev-only. Such a change ships nothing into a live process.
is_dev_only_node_manifest() {
    local file="$1"
    case "$file" in
        limerick/apps/*/package.json)
            [[ -f "$file" ]] || return 1
            diff -q \
                <(git show "$base:$file" 2>/dev/null | strip_dev_dependencies) \
                <(strip_dev_dependencies <"$file") >/dev/null
            ;;
        limerick/apps/*/package-lock.json)
            [[ -f "$file" ]] || return 1
            is_dev_only_node_manifest "${file%package-lock.json}package.json" \
                && lock_changes_dev_only "$file"
            ;;
        *)
            return 1
            ;;
    esac
}

is_runtime_path() {
    local file="$1"
    if is_test_path "$file"; then
        return 1
    fi
    case "$file" in
        graphify-out/* | */graphify-out/*)
            return 1
            ;;
        # Markdown under a runtime tree (mods/AGENTS.md, a mod README) is
        # documentation, not shipped behavior. .txt stays runtime: mods ship
        # prompt templates as .txt.
        *.md)
            return 1
            ;;
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
            mods/* | \
            .claude/hooks/* | \
            .claude/skills/*)
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
        *.png | *.jpg | *.jpeg | *.gif)
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
        *.md | *)
            # Accept the optional `live ` prefix that the runtime-path
            # tier (truthful test automation) requires for proofs of changes touching
            # the Tauri/server/CLI/UI/mod seams. Plain
            # `Evidence type: gameplay transcript` remains valid for
            # non-runtime proof-relevant changes. `game-loop integration
            # test` is the real-loop tier for deterministic guards that
            # cannot be triggered live on demand (its runtime-path
            # acceptance below additionally requires an `execute_via_real_loop`
            # reference, so it cannot be stamped over plain unit tests).
            if grep -Eiq '^Evidence type:[[:space:]]*((live[[:space:]]+)?(gameplay transcript|screenshot|gif)|game-loop integration test)[[:space:]]*$' "$file"; then
                return 0
            fi
            # `test run` is honest evidence only when every proof-relevant
            # file is test code or dev-only tooling: there is no gameplay
            # or live process for such a change to show.
            if [[ "$test_only_change" -eq 1 ]] \
                && grep -Eiq '^Evidence type:[[:space:]]*test run[[:space:]]*$' "$file"; then
                return 0
            fi
            echo "agent-check FAILED: $file must declare 'Evidence type: [live ](gameplay transcript|screenshot|gif)' or 'Evidence type: game-loop integration test'." >&2
            if [[ "$test_only_change" -eq 1 ]]; then
                echo "This change touches only test code and dev tooling, so 'Evidence type: test run' is also accepted." >&2
            fi
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

# ── Relevance & runtime classification (both modes share this) ───────────
while IFS= read -r file; do
    if is_proof_relevant "$file"; then
        echo "$file" >>"$relevant"
        if is_test_path "$file" || is_dev_only_node_manifest "$file"; then
            echo "$file" >>"$test_only"
        fi
    fi
    if is_runtime_path "$file"; then
        echo "$file" >>"$runtime"
    fi
done <"$changed"

# Lint: `.proofs/` must never appear in the diff. Bundles are gitignored
# and posted to the PR via `just attach-proof`. A leaked `.proofs/` file
# means someone ran `git add -f` or removed the gitignore entry.
if grep -E '^\.proofs/' "$changed" >/dev/null 2>&1; then
    echo "agent-check FAILED: .proofs/ paths appear in the diff:" >&2
    grep -E '^\.proofs/' "$changed" | head -5 | sed 's/^/  - /' >&2
    echo "Proof bundles are gitignored. Use 'just attach-proof <task-id>' to post them to the PR." >&2
    exit 1
fi

# ── Bundle discovery (mode-specific) ─────────────────────────────────────
gather_bundles_local() {
    [[ -d .proofs ]] || return 0
    while IFS= read -r f; do
        # If --bundle <id> was passed, scope to just that bundle.
        if [[ -n "$bundle_filter" ]]; then
            [[ "$f" == ".proofs/${bundle_filter}/"* ]] || continue
        fi
        case "$f" in
            .proofs/*/judge.md)
                # Legacy self-judged verdict file: accepted but ignored.
                ;;
            .proofs/*/acceptance-criteria.md)
                echo "$f" >>"$ac_files"
                ;;
            .proofs/*/*.md | .proofs/*/*.txt | .proofs/*/*.png | .proofs/*/*.jpg | .proofs/*/*.jpeg | .proofs/*/*.gif)
                echo "$f" >>"$evidence"
                ;;
        esac
    done < <(find .proofs -mindepth 2 -maxdepth 2 -type f 2>/dev/null || true)
}

gather_bundles_pr() {
    if ! command -v gh >/dev/null 2>&1; then
        echo "agent-check FAILED: --source=pr requires 'gh' to be installed." >&2
        return 1
    fi

    # Only ingest content authored by trusted users: the PR author. The
    # PR body is always trusted (only the PR author can edit it). Comments
    # are filtered by author.login == PR.author.login. This prevents
    # third-party commenters on public repos from forging or spoofing a
    # bundle that satisfies or breaks the gate.
    local pr_author
    pr_author="$(gh pr view "$pr_number" --json author --jq '.author.login // empty' 2>/dev/null || true)"
    if [[ -z "$pr_author" ]]; then
        echo "agent-check FAILED: could not resolve PR #$pr_number author." >&2
        return 1
    fi
    echo "agent-check: PR #$pr_number author is '$pr_author'; trusting only that login for proof comments."

    local raw="$tmpdir/comments_and_body.txt"
    : >"$raw"
    # The PR body is the primary, race-free carrier: it is present on the
    # very first gate run (`pull_request.opened`), whereas a comment posted
    # after the PR opens loses that race (#1177). Read the body first, then
    # author-filtered comments, and extract the fenced bundle from either.
    gh pr view "$pr_number" --json body --jq '.body // empty' >>"$raw" 2>/dev/null \
        || {
            echo "agent-check FAILED: could not fetch PR #$pr_number." >&2
            return 1
        }
    printf '\n' >>"$raw"
    # Filter comments by login server-side so untrusted bodies never reach
    # the extractor.
    gh pr view "$pr_number" --json comments \
        --jq ".comments[] | select(.author.login == \"$pr_author\") | .body // empty" \
        >>"$raw" 2>/dev/null \
        || {
            echo "agent-check FAILED: could not fetch PR #$pr_number comments." >&2
            return 1
        }

    # Extract each `<!-- limerick-proof-bundle:ID v=N -->` ... `<!-- /limerick-proof-bundle:ID -->`
    # block into a per-bundle file. Opener and closer must be on their
    # own line so that inline-code prose (e.g. PR descriptions that
    # mention the fence as a literal example) doesn't trigger false
    # extractions.
    awk -v outdir="$tmpdir" '
        match($0, /^[[:space:]]*<!--[[:space:]]+limerick-proof-bundle:[^[:space:]]+[[:space:]]+v=[0-9]+[[:space:]]*-->[[:space:]]*$/) {
            id = $0
            sub(/^[[:space:]]*<!--[[:space:]]+limerick-proof-bundle:/, "", id)
            sub(/[[:space:]]+v=.*$/, "", id)
            sanitised = id
            gsub(/[^A-Za-z0-9_.-]/, "_", sanitised)
            block_file = outdir "/pr_block_" sanitised ".md"
            in_block = 1
            print "" > block_file
            next
        }
        /^[[:space:]]*<!--[[:space:]]+\/limerick-proof-bundle:[^[:space:]]+[[:space:]]*-->[[:space:]]*$/ {
            if (in_block) { close(block_file); in_block = 0 }
            next
        }
        in_block { print >> block_file }
    ' "$raw"

    # Register every extracted block as evidence + AC. The validators
    # below independently confirm each required header line is present.
    # AC section detection is intentionally strict: it requires a real
    # `## Acceptance criteria` heading. The status line
    # `Acceptance criteria: met` alone does NOT count, so a bundle that
    # only contains that line cannot stand in for written criteria.
    for block_file in "$tmpdir"/pr_block_*.md; do
        [[ -f "$block_file" ]] || continue
        echo "$block_file" >>"$evidence"
        if grep -Eiq '^##+[[:space:]]+Acceptance criteria' "$block_file"; then
            echo "$block_file" >>"$ac_files"
        fi
    done
}

if [[ "$source_mode" == "local" ]]; then
    gather_bundles_local
else
    gather_bundles_pr || exit 1
fi

changed_count="$(wc -l <"$changed" | tr -d ' ')"
relevant_count="$(wc -l <"$relevant" | tr -d ' ')"
runtime_count="$(wc -l <"$runtime" | tr -d ' ')"
evidence_count="$(wc -l <"$evidence" | tr -d ' ')"
ac_count="$(wc -l <"$ac_files" | tr -d ' ')"
test_only_count="$(wc -l <"$test_only" | tr -d ' ')"
test_only_change=0
if [[ "$relevant_count" -gt 0 && "$test_only_count" -eq "$relevant_count" ]]; then
    test_only_change=1
fi

echo "agent-check: source=$source_mode; comparing $changed_count changed file(s) against $base_ref."

failed=0

if [[ "$relevant_count" -gt 0 ]]; then
    echo "agent-check: $relevant_count proof-relevant file(s) changed."
    if [[ "$test_only_change" -eq 1 ]]; then
        echo "agent-check: all are test code or dev-only tooling; 'Evidence type: test run' accepted."
    fi

    if [[ "$evidence_count" -eq 0 ]]; then
        if [[ "$source_mode" == "pr" ]]; then
            echo "agent-check FAILED: PR #$pr_number has no limerick-proof-bundle block in its body or comments." >&2
            echo "Run 'just attach-proof <task-id> $pr_number' to write the bundle into the PR body." >&2
        else
            echo "agent-check FAILED: proof-relevant changes require a bundle under .proofs/<task-id>/." >&2
            echo "Accepted evidence forms: gameplay transcript (.md or .txt), screenshot (.png/.jpg/.jpeg), or gif (.gif)." >&2
        fi
        failed=1
    else
        while IFS= read -r file; do
            validate_evidence_file "$file" || failed=1
        done <"$evidence"
    fi

    if [[ "$ac_count" -gt 0 ]]; then
        echo "agent-check: $ac_count acceptance-criteria artifact(s) present."
    fi

    # Runtime-path tier: when the diff touches a path that only fires in
    # a real process, the evidence must show the change was actually run
    # live. Accepted live signals:
    #   - any binary artifact (screenshot .png/.jpg/.jpeg, gif .gif)
    #   - an artifact that declares 'Evidence type: live ...'
    #   - an artifact that declares 'Evidence type: game-loop integration test'
    #     AND references `execute_via_real_loop` (real game-loop wiring, mock
    #     LLM) — for deterministic guards that can't be triggered live on demand.
    # In PR mode the block content carries the header. In local mode the
    # `.md` summary file declares it; `.txt` transcripts are exempt from
    # the header requirement (literal program output may match the regex
    # by accident).
    if [[ "$runtime_count" -gt 0 ]]; then
        echo "agent-check: $runtime_count runtime-shipping file(s) changed; live proof required."
        live_found=0
        if [[ "$evidence_count" -gt 0 ]]; then
            while IFS= read -r file; do
                case "$file" in
                    *.png | *.jpg | *.jpeg | *.gif)
                        live_found=1
                        ;;
                    *.txt)
                        # Exempt from header grep — see comment above.
                        ;;
                    *)
                        if grep -Eiq '^Evidence type:[[:space:]]*live[[:space:]]+(gameplay transcript|screenshot|gif)[[:space:]]*$' "$file"; then
                            live_found=1
                        elif grep -Eiq '^Evidence type:[[:space:]]*game-loop integration test[[:space:]]*$' "$file" \
                            && grep -q 'execute_via_real_loop' "$file"; then
                            # Real-loop integration tier (truthful test automation). Some runtime
                            # behaviours — deterministic guards whose ONLY trigger is
                            # intermittent large-model output (e.g. the 14B
                            # spontaneously impersonating another NPC, or looping a
                            # phrase to the token cap) — cannot be reproduced on demand
                            # in a live process. The honest, strongest proof is a Rust
                            # integration test that drives the REAL `game_loop`
                            # (`handle_game_input` -> `run_npc_turn`) via
                            # `GameTestHarness::execute_via_real_loop`, mocking only the
                            # LLM boundary. That exercises the exact production wiring —
                            # the gate's actual concern — so it counts as runtime proof.
                            # Requiring the `execute_via_real_loop` mention in the same
                            # evidence file ties the claim to the real mechanism, so the
                            # tier cannot be stamped over plain unit tests.
                            live_found=1
                        fi
                        ;;
                esac
            done <"$evidence"
        fi
        if [[ "$live_found" -eq 0 ]]; then
            echo "agent-check FAILED: runtime-shipping changes require evidence from a live process." >&2
            echo "Provide a screenshot/gif in the bundle, or include 'Evidence type: live gameplay transcript'" >&2
            echo "in the evidence section. The word 'live' asserts the change was exercised in a real" >&2
            echo "Tauri / server / CLI / browser, not just in unit tests." >&2
            echo "For a deterministic guard that cannot be triggered live on demand, declare" >&2
            echo "'Evidence type: game-loop integration test' and reference execute_via_real_loop in" >&2
            echo "the evidence — a test that drives the real game_loop wiring (mock LLM boundary)." >&2
            failed=1
        fi
    fi
else
    echo "agent-check: no proof-relevant changes; proof bundle not required."
fi

# Per-bundle completeness: every bundle that exists at all must contain
# evidence + acceptance criteria. Catches the case where bundle A is
# complete and bundle B has only an evidence file — the aggregate counts
# above would have silently allowed B to pass. A real acceptance-criteria
# section is required; the status line alone does not count.
if [[ "$evidence_count" -gt 0 || "$ac_count" -gt 0 ]]; then
    if [[ "$source_mode" == "local" ]]; then
        # Collect every bundle dir touched.
        while IFS= read -r bundle_dir; do
            [[ -z "$bundle_dir" ]] && continue
            # An evidence file means any of: evidence.md, transcript.{md,txt}, or
            # a binary artifact in the bundle dir.
            local_has_evidence=0
            for ev in "$bundle_dir"/*.md "$bundle_dir"/*.txt "$bundle_dir"/*.png "$bundle_dir"/*.jpg "$bundle_dir"/*.jpeg "$bundle_dir"/*.gif; do
                [[ -f "$ev" ]] || continue
                case "$ev" in
                    "$bundle_dir/judge.md" | "$bundle_dir/acceptance-criteria.md") ;;
                    *)
                        local_has_evidence=1
                        break
                        ;;
                esac
            done
            if [[ "$local_has_evidence" -eq 0 ]]; then
                echo "agent-check FAILED: bundle '$bundle_dir/' has no evidence file (.md, .txt, image)." >&2
                failed=1
            fi
            if [[ ! -f "$bundle_dir/acceptance-criteria.md" ]]; then
                echo "agent-check FAILED: bundle '$bundle_dir/' is missing acceptance-criteria.md." >&2
                echo "Write the acceptance criteria before implementing; see docs/agent/agent-check.md." >&2
                failed=1
                continue
            fi
            # The evidence must state that every criterion was met. A legacy
            # judge.md carrying the line still counts so older bundles pass.
            met_found=0
            for ev in "$bundle_dir"/*.md; do
                [[ -f "$ev" ]] || continue
                [[ "$ev" == "$bundle_dir/acceptance-criteria.md" ]] && continue
                if grep -Eiq '^Acceptance criteria:[[:space:]]*met([[:space:]]|$)' "$ev"; then
                    met_found=1
                    break
                fi
            done
            if [[ "$met_found" -eq 0 ]]; then
                echo "agent-check FAILED: bundle '$bundle_dir/' must include 'Acceptance criteria: met' in evidence.md." >&2
                echo "evidence.md maps every criterion in acceptance-criteria.md to the transcript lines that show it." >&2
                failed=1
            fi
        done < <(cat "$evidence" "$ac_files" 2>/dev/null | grep -E '^\.proofs/[^/]+/' | sed 's|/[^/]*$||' | sort -u)
    else
        # PR mode: every block must contain a real `## Acceptance criteria`
        # heading and the 'Acceptance criteria: met' status line.
        while IFS= read -r block_file; do
            bundle_id="$(basename "$block_file" .md | sed 's/^pr_block_//')"
            if ! grep -Eiq '^##+[[:space:]]+Acceptance criteria' "$block_file"; then
                echo "agent-check FAILED: PR bundle '$bundle_id' has no '## Acceptance criteria' section." >&2
                echo "The status line 'Acceptance criteria: met' alone does not stand in for written criteria." >&2
                failed=1
            elif ! grep -Eiq '^Acceptance criteria:[[:space:]]*met([[:space:]]|$)' "$block_file"; then
                echo "agent-check FAILED: PR bundle '$bundle_id' must include 'Acceptance criteria: met' in its evidence." >&2
                failed=1
            fi
        done < <(sort -u "$evidence")
    fi
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

if [[ "$relevant_count" -gt 0 ]]; then
    echo "agent-check passed: acceptance criteria and proof evidence are present; no placeholder debt markers found."
else
    echo "agent-check passed: no proof needed; no placeholder debt markers found."
fi
