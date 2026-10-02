#!/usr/bin/env bash
#
# Unit test for agent-check.sh's proof-bundle rules after judge.md was
# retired (#2119). Pure: builds throwaway git repos and a `gh` stub, never
# touches the network. Run directly or via CI (shell-quality job).
#
# Asserts, in local mode:
#   1. A bundle with acceptance criteria and evidence (no judge.md) passes.
#   2. A legacy bundle whose status line lives in judge.md still passes.
#   3. A bundle without an evidence file fails.
#   4. A bundle without 'Acceptance criteria: met' fails.
#   5. A bundle without acceptance-criteria.md fails.
# And in PR mode, against a body composed by compose-proof-body.sh:
#   6. A bundle without judge.md passes.
#   7. A legacy bundle with judge.md passes.
#   8. A bundle without 'Acceptance criteria: met' fails.
set -euo pipefail

scripts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$scripts_dir/agent-check.sh"
compose="$scripts_dir/compose-proof-body.sh"

fails=0
check() {
    local desc="$1" actual="$2" expected="$3"
    if [[ "$actual" == "$expected" ]]; then
        echo "ok   - $desc"
    else
        echo "FAIL - $desc (got '$actual', want '$expected')" >&2
        fails=$((fails + 1))
    fi
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

repo="$tmp/repo"
stub_dir="$tmp/bin"
mkdir -p "$repo" "$stub_dir"

# The gh stub serves a canned PR: author 'alice', body from $PR_BODY_FILE,
# no comments.
cat >"$stub_dir/gh" <<'STUB'
#!/usr/bin/env bash
case "$*" in
    *"--json author"*) echo alice ;;
    *"--json body"*) cat "$PR_BODY_FILE" ;;
    *"--json comments"*) ;;
    *) exit 1 ;;
esac
STUB
chmod +x "$stub_dir/gh"

# Run the checker; print 'pass' or 'fail'.
run_gate() {
    if AGENT_CHECK_BASE_REF=HEAD^ PATH="$stub_dir:$PATH" bash "$checker" "$@" >"$tmp/out.txt" 2>&1; then
        echo pass
    else
        echo fail
    fi
}

write_bundle() {
    local id="$1" met_in="$2"
    rm -rf .proofs
    mkdir -p ".proofs/$id"
    printf '## Acceptance criteria\n\n- A: thing happens\n' >".proofs/$id/acceptance-criteria.md"
    printf 'Evidence type: gameplay transcript\n\nA happened on line 7.\n' >".proofs/$id/evidence.md"
    case "$met_in" in
        evidence) printf '\nAcceptance criteria: met\n' >>".proofs/$id/evidence.md" ;;
        judge) printf 'Verdict: sufficient\nTechnical debt: clear\nAcceptance criteria: met\n' >".proofs/$id/judge.md" ;;
        none) ;;
    esac
}

result=0
(
    cd "$repo"
    git init -q
    git config user.email t@t.test
    git config user.name test
    printf '.proofs/\n' >.gitignore
    git add .gitignore
    git commit -qm init
    # A proof-relevant (non-runtime) change so the gate demands a bundle.
    mkdir -p limerick/crates/limerick-config/src
    printf 'pub fn f() {}\n' >limerick/crates/limerick-config/src/lib.rs
    git add limerick
    git commit -qm "test: proof-relevant change"

    write_bundle demo evidence
    check "local: bundle without judge.md passes" "$(run_gate)" "pass"

    write_bundle demo judge
    check "local: legacy bundle with judge.md passes" "$(run_gate)" "pass"

    write_bundle demo evidence
    rm ".proofs/demo/evidence.md"
    check "local: bundle without evidence fails" "$(run_gate)" "fail"

    write_bundle demo none
    check "local: bundle without met line fails" "$(run_gate)" "fail"
    grep -q "Acceptance criteria: met" "$tmp/out.txt" && met_msg=yes || met_msg=no
    check "local: failure names the met line" "$met_msg" "yes"

    write_bundle demo evidence
    rm ".proofs/demo/acceptance-criteria.md"
    check "local: bundle without acceptance criteria fails" "$(run_gate)" "fail"

    # PR mode: compose the body exactly as `just attach-proof` would.
    export PR_BODY_FILE="$tmp/body.md"

    write_bundle demo evidence
    printf 'Change summary.\n' | bash "$compose" demo >"$PR_BODY_FILE"
    rm -rf .proofs
    check "pr: bundle without judge.md passes" "$(run_gate --source=pr 7)" "pass"

    write_bundle demo judge
    printf 'Change summary.\n' | bash "$compose" demo >"$PR_BODY_FILE"
    rm -rf .proofs
    check "pr: legacy bundle with judge.md passes" "$(run_gate --source=pr 7)" "pass"

    write_bundle demo none
    printf 'Change summary.\n' | bash "$compose" demo >"$PR_BODY_FILE"
    rm -rf .proofs
    check "pr: bundle without met line fails" "$(run_gate --source=pr 7)" "fail"

    exit "$fails"
) || result=$?

if [[ "$result" -ne 0 ]]; then
    echo "agent-check-proof-bundle.test.sh: $result assertion(s) failed." >&2
    exit 1
fi
echo "agent-check-proof-bundle.test.sh: all assertions passed."
