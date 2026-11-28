#!/usr/bin/env bash
#
# Unit test for agent-check.sh's evidence-link gate. Pure: builds a
# throwaway git repo and a `gh` stub, never touches the network. Run
# directly or via CI (shell-quality job).
#
# Asserts, in PR mode:
#   1. A runtime diff (mods/, engine crate, mobile app, mobile UI test)
#      without the evidence link fails.
#   2. The same diff passes when the body links pr/<this PR>/.
#   3. A link to another PR's page fails.
#   4. A docs/tooling/CI/test-only diff passes with no link.
#   5. Markdown and generated graphs under mods/ need no link.
# And in both modes:
#   6. A .proofs/ path in the diff fails.
#   7. A placeholder debt marker in a changed file fails.
# And in PR mode with no AGENT_CHECK_BASE_REF:
#   8. A stacked PR is compared against its own base branch, so the lower
#      PR's runtime files need no link here; an unreadable base falls back
#      to origin/main.
set -euo pipefail

scripts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$scripts_dir/agent-check.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
check() {
    local desc="$1" actual="$2" expected="$3"
    if [[ "$actual" == "$expected" ]]; then
        echo "ok   - $desc"
    else
        echo "FAIL - $desc (got '$actual', want '$expected')" >&2
        sed 's/^/       /' "$tmp/out.txt" >&2
        fails=$((fails + 1))
    fi
}

repo="$tmp/repo"
stub_dir="$tmp/bin"
mkdir -p "$repo" "$stub_dir"
export PR_BODY_FILE="$tmp/body.md"

# The gh stub serves the PR body from $PR_BODY_FILE and the base branch
# from $PR_BASE_REF (empty: none reported).
export PR_BASE_REF=""
cat >"$stub_dir/gh" <<'STUB'
#!/usr/bin/env bash
case "$*" in
    *"--json body"*) cat "$PR_BODY_FILE" ;;
    *"--json baseRefName"*) printf '%s\n' "$PR_BASE_REF" ;;
    *) exit 1 ;;
esac
STUB
chmod +x "$stub_dir/gh"

# Run the checker against the last commit; print 'pass' or 'fail'.
run_gate() {
    if AGENT_CHECK_BASE_REF=HEAD^ PATH="$stub_dir:$PATH" bash "$checker" "$@" >"$tmp/out.txt" 2>&1; then
        echo pass
    else
        echo fail
    fi
}

body() {
    printf 'Summary of the change.\n\n%s\n' "$1" >"$PR_BODY_FILE"
}

# Commit one change touching the given paths, starting from a clean base.
commit_change() {
    git reset -q --hard base
    local path
    for path in "$@"; do
        mkdir -p "$(dirname "$path")"
        printf 'change\n' >"$path"
    done
    git add -A
    git commit -qm "test: change"
}

link="https://dmooney.github.io/rundale-pages/pr/42/"

cd "$repo"
git init -q
git config user.email t@t.test
git config user.name test
printf 'fixture\n' >README.md
git add README.md
git commit -qm init
git tag base

# 1-3: runtime diffs.
for path in \
    mods/rundale/world.json \
    mods/rundale/prompts/dialogue.txt \
    limerick/crates/limerick-engine/src/lib.rs \
    mobile/Rundale/ContentView.swift \
    mobile/RundaleKit/Sources/RundaleKit/Turn.swift \
    mobile/RundaleUITests/RundalePhase1UITests.swift; do
    commit_change "$path"
    body "No link here."
    check "pr: $path without a link fails" "$(run_gate --source=pr 42)" "fail"
    body "Evidence: $link"
    check "pr: $path with pr/42/ link passes" "$(run_gate --source=pr 42)" "pass"
    body "Evidence: https://dmooney.github.io/rundale-pages/pr/41/"
    check "pr: $path linking another PR fails" "$(run_gate --source=pr 42)" "fail"
done

commit_change mods/rundale/world.json
body "Evidence: https://dmooney.github.io/rundale-pages/pr/420/"
check "pr: pr/420/ does not satisfy PR 42" "$(run_gate --source=pr 42)" "fail"
body "Evidence: [recording](${link}index.html)"
check "pr: a link to the page's index.html passes" "$(run_gate --source=pr 42)" "pass"
check "local: runtime diff passes and reports the requirement" "$(run_gate)" "pass"
grep -Fq 'evidence page link is required' "$tmp/out.txt" && said=yes || said=no
check "local: names the link requirement" "$said" "yes"

# 4-5: diffs that need no link.
body "No link here."
for path in \
    docs/agent/agent-check.md \
    limerick/scripts/agent-check.sh \
    .github/workflows/ci.yml \
    .agents/skills/gatekeeper/SKILL.md \
    .claude/hooks/hook.sh \
    justfile \
    mobile/scripts/verify.py \
    mobile/AGENTS.md \
    mods/rundale/AGENTS.md \
    mods/graphify-out/graph.json \
    limerick/crates/limerick-engine/tests/fixture.rs \
    limerick/apps/ui/src/lib/thing.test.ts \
    mobile/RundaleTests/Phase1AuditVolumeTests.swift \
    mobile/RundaleKit/Tests/RundaleKitTests/TurnTests.swift \
    mobile/LimerickEndpointKit/Tests/LimerickEndpointKitTests/ClientTests.swift \
    mobile/coverage-baseline.json \
    limerick/crates/limerick-config/src/lib.rs; do
    commit_change "$path"
    check "pr: $path needs no link" "$(run_gate --source=pr 42)" "pass"
done

# 4b: Rust test-only hunks inside runtime-path files (hunk-aware gate).
rt=limerick/crates/limerick-core/src/game_loop/staged.rs
rt_mod=limerick/crates/limerick-core/src/game_loop/mod.rs
fixture_base='pub fn run() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_runs() {
        assert_eq!(super::run(), 1); // "}" in a string and a comment }
    }
}
'
# Reset to base, commit the fixture file, run the edit function given as
# arguments, and commit that as the change under test (the gate diffs HEAD^).
rs_change() {
    git reset -q --hard base
    mkdir -p "$(dirname "$rt")"
    printf '%s' "$fixture_base" >"$rt"
    git add -A
    git commit -qm "test: fixture"
    "$@"
    git add -A
    git commit -qm "test: edit"
}
edit_test_body() { sed -i.bak 's/assert_eq!(super::run(), 1);/assert_eq!(super::run(), 1); let _x = 2;/' "$rt" && rm -f "$rt.bak"; }
edit_runtime_body() { sed -i.bak 's/^    1$/    2/' "$rt" && rm -f "$rt.bak"; }
edit_both() {
    edit_test_body
    edit_runtime_body
}
add_test_mod() { printf '\n#[cfg(test)]\nmod more {\n    #[test]\n    fn extra() {}\n}\n' >>"$rt"; }
add_plain_mod() { printf '\nmod more {\n    pub fn extra() {}\n}\n' >>"$rt"; }
add_not_test_mod() { printf '\n#[cfg(not(test))]\nmod more {\n    pub fn extra() {}\n}\n' >>"$rt"; }
delete_test_mod() { printf 'pub fn run() -> u32 {\n    1\n}\n' >"$rt"; }
add_runtime_after_tests() { printf 'pub fn late() {}\n' >>"$rt"; }
body "No link here."
rs_change edit_test_body
check "pr: edit confined to a cfg(test) module needs no link" "$(run_gate --source=pr 42)" "pass"
grep -Fq 'no runtime-shipping changes' "$tmp/out.txt" && said=yes || said=no
check "pr: test-only hunk is reported as no runtime change" "$said" "yes"
rs_change add_test_mod
check "pr: adding a cfg(test) module needs no link" "$(run_gate --source=pr 42)" "pass"
rs_change delete_test_mod
check "pr: deleting the cfg(test) module needs no link" "$(run_gate --source=pr 42)" "pass"
rs_change edit_runtime_body
check "pr: runtime hunk in the same file needs a link" "$(run_gate --source=pr 42)" "fail"
rs_change edit_both
check "pr: mixed test and runtime hunks need a link" "$(run_gate --source=pr 42)" "fail"
rs_change add_plain_mod
check "pr: a new non-test module needs a link" "$(run_gate --source=pr 42)" "fail"
rs_change add_not_test_mod
check "pr: cfg(not(test)) is not test-only" "$(run_gate --source=pr 42)" "fail"
rs_change add_runtime_after_tests
check "pr: runtime code appended after the test module needs a link" "$(run_gate --source=pr 42)" "fail"

# Whole-file gating through the parent's `#[cfg(test)] mod name;`.
gated_file=limerick/crates/limerick-core/src/game_loop/contract.rs
gated_setup() {
    git reset -q --hard base
    mkdir -p "$(dirname "$gated_file")"
    printf '%s\n' "$1" >"$rt_mod"
    printf 'pub fn a() {}\n' >"$gated_file"
    git add -A
    git commit -qm "test: fixture"
    printf 'pub fn b() {}\n' >>"$gated_file"
    git add -A
    git commit -qm "test: edit"
}
gated_setup '#[cfg(test)]
pub(crate) mod contract;'
check "pr: file declared under #[cfg(test)] in mod.rs needs no link" "$(run_gate --source=pr 42)" "pass"
gated_setup 'pub(crate) mod contract;'
check "pr: ungated module file needs a link" "$(run_gate --source=pr 42)" "fail"
gated_setup '#[cfg(not(test))]
pub(crate) mod contract;'
check "pr: file under cfg(not(test)) needs a link" "$(run_gate --source=pr 42)" "fail"
gated_setup '#[cfg(test)]
pub(crate) mod other;
pub(crate) mod contract;'
check "pr: gate on a different module does not cover the file" "$(run_gate --source=pr 42)" "fail"
git reset -q --hard base
mkdir -p "$(dirname "$gated_file")"
printf '#![cfg(test)]\npub fn a() {}\n' >"$gated_file"
git add -A
git commit -qm "test: inner cfg(test) file"
check "pr: new file starting with #![cfg(test)] needs no link" "$(run_gate --source=pr 42)" "pass"

# 6: a leaked .proofs/ file fails in both modes, even with the link.
commit_change docs/note.md
mkdir -p .proofs/demo
printf 'Evidence type: gameplay transcript\n' >.proofs/demo/evidence.md
git add -f .proofs/demo/evidence.md
git commit -qm "test: leak a bundle"
body "Evidence: $link"
check "local: .proofs/ path fails" "$(run_gate)" "fail"
check "pr: .proofs/ path fails" "$(run_gate --source=pr 42)" "fail"

# 7: placeholder debt markers fail; the marker is assembled at run time so
# this file does not trip the scan itself.
git reset -q --hard base
mkdir -p limerick/crates/limerick-config/src
printf 'pub fn f() { %s!() }\n' todo >limerick/crates/limerick-config/src/lib.rs
git add -A
git commit -qm "test: placeholder"
check "local: placeholder debt marker fails" "$(run_gate)" "fail"
grep -Fq 'placeholder-like debt markers' "$tmp/out.txt" && said=yes || said=no
check "local: failure names the debt markers" "$said" "yes"

# 8: a stacked PR. origin/stack holds the lower PR's runtime change; this PR
# adds only tooling on top. No AGENT_CHECK_BASE_REF, as in CI.
run_gate_pr_base() {
    if PATH="$stub_dir:$PATH" bash "$checker" --source=pr 42 >"$tmp/out.txt" 2>&1; then
        echo pass
    else
        echo fail
    fi
}
commit_change mobile/RundaleSession/Sources/RundaleSession/Presenter.swift
git update-ref refs/remotes/origin/stack HEAD
git update-ref refs/remotes/origin/main base
mkdir -p tools/cli
printf 'change\n' >tools/cli/main.swift
git add -A
git commit -qm "test: stacked tooling change"
body "No link here."
PR_BASE_REF=stack
check "pr: stacked PR is compared against its base branch" "$(run_gate_pr_base)" "pass"
grep -Fq 'against origin/stack' "$tmp/out.txt" && said=yes || said=no
check "pr: names the base branch it compared against" "$said" "yes"
PR_BASE_REF=main
check "pr: the same commits against main need the link" "$(run_gate_pr_base)" "fail"
PR_BASE_REF=gone
check "pr: a base branch missing locally falls back to origin/main" "$(run_gate_pr_base)" "fail"
PR_BASE_REF=""
check "pr: no reported base falls back to origin/main" "$(run_gate_pr_base)" "fail"

if [[ "$fails" -ne 0 ]]; then
    echo "agent-check-evidence-link.test.sh: $fails assertion(s) failed." >&2
    exit 1
fi
echo "agent-check-evidence-link.test.sh: all assertions passed."
