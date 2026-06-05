#!/usr/bin/env bash
#
# Unit test for `attach-proof.sh --via-mcp` — the no-gh sandbox path (#1178).
# Proves the contract: validate the bundle locally and print the fenced block
# to stdout WITHOUT ever invoking `gh`. Pure: throwaway git repo + a `gh`
# stub that fails loudly if called.
#
# Asserts:
#   1. Exit 0 on a well-formed bundle.
#   2. The fenced bundle block (opener + closer) is on stdout.
#   3. `gh` is never invoked (a failing stub on PATH is never hit).
#   4. stdout carries ONLY the block — no progress chatter leaks in (so the
#      output can be piped straight to a GitHub MCP call).
set -euo pipefail

scripts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
attach="$scripts_dir/attach-proof.sh"

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

# A gh stub that records every invocation and fails — if --via-mcp touches
# gh at all, the marker file appears and/or the script errors out.
stub_dir="$tmp/bin"
mkdir -p "$stub_dir"
cat >"$stub_dir/gh" <<EOF
#!/usr/bin/env bash
echo "GH_STUB_INVOKED \$*" >>"$tmp/gh-calls.log"
exit 1
EOF
chmod +x "$stub_dir/gh"

# `|| result=$?` keeps `set -e` from aborting the parent at the subshell's
# non-zero exit, so the failure summary below still prints.
result=0
(
    cd "$tmp"
    git init -q
    git config user.email t@t.test
    git config user.name test
    # Mirror the real repo: .proofs/ is gitignored, so the bundle never shows
    # in the diff. A committed HEAD gives agent-check a base ref; diffing HEAD
    # yields no proof-relevant changes, so local validation passes and
    # --via-mcp proceeds to emit. (AGENT_CHECK_BASE_REF=HEAD.)
    printf '.proofs/\n' >.gitignore
    git add .gitignore
    git commit -q -m init
    id="demo-1178"
    mkdir -p ".proofs/$id"
    printf '## Acceptance criteria\n\n- A: thing happens\n' >".proofs/$id/acceptance-criteria.md"
    printf 'Evidence type: live gameplay transcript\n\nA happened on line 7.\n' >".proofs/$id/evidence.md"
    printf 'Verdict: sufficient\nTechnical debt: clear\nAcceptance criteria: met\n' >".proofs/$id/judge.md"

    # Run with the gh stub FIRST on PATH. stdout -> out, stderr discarded.
    set +e
    out="$(AGENT_CHECK_BASE_REF=HEAD PATH="$stub_dir:$PATH" bash "$attach" "$id" --via-mcp 2>/dev/null)"
    rc=$?
    set -e

    check "exit 0" "$rc" "0"
    check "one opener on stdout" "$(grep -c '^<!-- parish-proof-bundle:' <<<"$out" || true)" "1"
    check "one closer on stdout" "$(grep -c '^<!-- /parish-proof-bundle:' <<<"$out" || true)" "1"
    check "gh never invoked" "$([[ -f "$tmp/gh-calls.log" ]] && echo called || echo no)" "no"
    # First non-empty stdout line must be the opener fence (no chatter above).
    first_line="$(grep -m1 . <<<"$out" || true)"
    check "stdout starts with the fence" "${first_line:0:24}" "<!-- parish-proof-bundle"

    exit "$fails"
) || result=$?

if [[ "$result" -ne 0 ]]; then
    echo "attach-proof-via-mcp.test.sh: $result assertion(s) failed." >&2
    exit 1
fi
echo "attach-proof-via-mcp.test.sh: all assertions passed."
