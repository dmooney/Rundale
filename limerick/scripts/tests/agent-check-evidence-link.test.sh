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

# The gh stub serves the PR body from $PR_BODY_FILE.
cat >"$stub_dir/gh" <<'STUB'
#!/usr/bin/env bash
case "$*" in
    *"--json body"*) cat "$PR_BODY_FILE" ;;
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
    limerick/crates/limerick-config/src/lib.rs; do
    commit_change "$path"
    check "pr: $path needs no link" "$(run_gate --source=pr 42)" "pass"
done

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

if [[ "$fails" -ne 0 ]]; then
    echo "agent-check-evidence-link.test.sh: $fails assertion(s) failed." >&2
    exit 1
fi
echo "agent-check-evidence-link.test.sh: all assertions passed."
