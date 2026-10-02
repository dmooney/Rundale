#!/usr/bin/env bash
#
# Unit test for agent-check.sh's handling of test-only and dev-tooling
# changes. Pure: builds a throwaway git repo, never touches the network.
# Run directly or via CI (shell-quality job).
#
# Asserts, in local mode:
#   1. Test code under limerick/apps/ui/src (test-setup.ts, *.test.ts) is
#      not runtime-shipping, still needs a bundle, and passes with
#      'Evidence type: test run'.
#   2. A devDependency bump (package.json + dev-only lockfile entries)
#      passes with a test-run bundle.
#   3. A production dependency change in package.json rejects test run.
#   4. A lockfile change to a non-dev entry rejects test run.
#   5. A new non-dev lockfile entry rejects test run.
#   6. A real UI source change rejects test run and requires live proof.
set -euo pipefail

scripts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$scripts_dir/agent-check.sh"

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

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
repo="$tmp/repo"
mkdir -p "$repo"
cd "$repo"
git init -q
git config user.email t@t.test
git config user.name t
printf '.proofs/\n' >.gitignore

ui=limerick/apps/ui
mkdir -p "$ui/src"

write_manifest() {
    local prod="$1" dev="$2"
    cat >"$ui/package.json" <<EOF
{
	"name": "ui",
	"dependencies": {
		"maplibre-gl": "$prod"
	},
	"devDependencies": {
		"vitest": "$dev"
	}
}
EOF
}

# Args: prod version, dev version, extra entry ("" or "dev" or "prod").
write_lock() {
    local prod="$1" dev="$2" extra="$3"
    {
        printf '{\n\t"name": "ui",\n\t"lockfileVersion": 3,\n\t"packages": {\n'
        printf '\t\t"": {\n\t\t\t"name": "ui"\n\t\t},\n'
        printf '\t\t"node_modules/maplibre-gl": {\n\t\t\t"version": "%s"\n\t\t},\n' "$prod"
        case "$extra" in
            dev) printf '\t\t"node_modules/tinyspy": {\n\t\t\t"version": "5.0.0",\n\t\t\t"dev": true\n\t\t},\n' ;;
            prod) printf '\t\t"node_modules/earcut": {\n\t\t\t"version": "3.0.0"\n\t\t},\n' ;;
        esac
        printf '\t\t"node_modules/vitest": {\n\t\t\t"version": "%s",\n\t\t\t"dev": true,\n\t\t\t"engines": {\n\t\t\t\t"node": ">=24"\n\t\t\t}\n\t\t}\n' "$dev"
        printf '\t}\n}\n'
    } >"$ui/package-lock.json"
}

write_manifest 5.0.0 4.1.11
write_lock 5.0.0 4.1.11 ""
printf 'import "x";\n' >"$ui/src/test-setup.ts"
printf '<p>hi</p>\n' >"$ui/src/App.svelte"
git add -A
git commit -qm base
git branch -q base

write_bundle() {
    rm -rf .proofs
    mkdir -p .proofs/t
    printf '## Acceptance criteria\n\n- A: tests pass\n' >.proofs/t/acceptance-criteria.md
    printf 'Evidence type: test run\n\nA: 946 tests passed.\n\nAcceptance criteria: met\n' >.proofs/t/evidence.md
}

# Commit the working-tree change on a fresh branch from base, then run the
# gate against HEAD^. Prints 'pass' or 'fail'.
run_case() {
    local name="$1" bundle="$2"
    git add -A -- limerick
    git commit -qm "$name"
    rm -rf .proofs
    [[ "$bundle" == "bundle" ]] && write_bundle
    if AGENT_CHECK_BASE_REF=HEAD^ bash "$checker" >"$tmp/out.txt" 2>&1; then
        echo pass
    else
        echo fail
    fi
}

start_case() {
    git checkout -q -B "$1" base
}

# 1. Test code only.
start_case test-code
printf 'import "y";\n' >"$ui/src/test-setup.ts"
printf 'test("x", () => {});\n' >"$ui/src/App.test.ts"
check "test code without a bundle fails" "$(run_case test-code-nobundle none)" fail
grep -Fq 'require a bundle' "$tmp/out.txt" || {
    echo "FAIL - test code must still require a bundle" >&2
    fails=$((fails + 1))
}
git checkout -q -B test-code base
printf 'import "y";\n' >"$ui/src/test-setup.ts"
printf 'test("x", () => {});\n' >"$ui/src/App.test.ts"
check "test code passes with a test-run bundle" "$(run_case test-code bundle)" pass
if grep -Fq 'live proof required' "$tmp/out.txt"; then
    echo "FAIL - test code must not require live proof" >&2
    fails=$((fails + 1))
fi

# 2. devDependency bump with dev-only lockfile movement.
start_case dev-bump
write_manifest 5.0.0 5.0.3
write_lock 5.0.0 5.0.3 dev
check "devDependency bump passes with a test-run bundle" "$(run_case dev-bump bundle)" pass

# 3. Production dependency change.
start_case prod-bump
write_manifest 5.1.0 4.1.11
write_lock 5.1.0 4.1.11 ""
check "production dependency bump rejects test run" "$(run_case prod-bump bundle)" fail

# 4. Dev-only package.json, but a non-dev lockfile entry moves.
start_case lock-prod-entry
write_manifest 5.0.0 5.0.3
write_lock 5.1.0 5.0.3 ""
check "non-dev lockfile entry change rejects test run" "$(run_case lock-prod-entry bundle)" fail

# 5. A new non-dev lockfile entry.
start_case lock-prod-added
write_manifest 5.0.0 5.0.3
write_lock 5.0.0 5.0.3 prod
check "new non-dev lockfile entry rejects test run" "$(run_case lock-prod-added bundle)" fail

# 6. Real UI source change.
start_case ui-source
printf '<p>hello</p>\n' >"$ui/src/App.svelte"
check "UI source change rejects test run" "$(run_case ui-source bundle)" fail
grep -Fq 'live proof required' "$tmp/out.txt" || {
    echo "FAIL - UI source change must require live proof" >&2
    fails=$((fails + 1))
}

if [[ "$fails" -ne 0 ]]; then
    echo "$fails agent-check test-only case(s) failed" >&2
    exit 1
fi
echo "agent-check test-only change test passed"
