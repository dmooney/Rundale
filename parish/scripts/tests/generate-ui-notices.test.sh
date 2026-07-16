#!/usr/bin/env bash
# Unit tests for atomic UI notice generation (#1735). Uses throwaway package
# fixtures and command stubs; never touches the repository notice or network.
set -euo pipefail

scripts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
generator="$scripts_dir/generate-ui-notices.sh"

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
ui_dir="$tmp/ui"
destination="$tmp/THIRD_PARTY_NOTICES.ui.md"
bin_dir="$tmp/bin"
calls="$tmp/calls.log"
mkdir -p "$ui_dir" "$bin_dir"

cat >"$ui_dir/package.json" <<'JSON'
{"name":"notice-fixture","private":true,"dependencies":{"alpha":"1.0.0","beta":"3.0.0"}}
JSON
printf '{}\n' >"$ui_dir/package-lock.json"
printf '{}\n' >"$ui_dir/license-clarifications.json"

cat >"$bin_dir/npm" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
echo "npm $*" >>"$TEST_CALLS"
case "${1:-}" in
    ci)
        if [[ "${TEST_CI_FAIL:-0}" == "1" ]]; then
            exit 41
        fi
        mkdir -p "$PARISH_UI_NOTICES_UI_DIR/node_modules"
        ;;
    ls)
        printf '%s\n' "$TEST_DEPENDENCY_TREE"
        ;;
    *)
        echo "unexpected npm invocation: $*" >&2
        exit 42
        ;;
esac
SH

cat >"$bin_dir/npx" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
echo "npx $*" >>"$TEST_CALLS"
output=""
while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--out" ]]; then
        output="$2"
        break
    fi
    shift
done
if [[ -z "$output" ]]; then
    echo "npx stub did not receive --out" >&2
    exit 43
fi
printf '%s' "${TEST_NOTICE_OUTPUT:-partial output}" >"$output"
if [[ "${TEST_GENERATOR_FAIL:-0}" == "1" ]]; then
    exit 44
fi
SH
chmod +x "$bin_dir/npm" "$bin_dir/npx"

dependency_tree='{"dependencies":{"alpha":{"version":"1.0.0","dependencies":{"optional-absent":{},"transitive":{"version":"2.0.0"}}},"beta":{"version":"3.0.0"}}}'
complete_notice='- [alpha@1.0.0](https://example.test/alpha) - MIT
- [beta@3.0.0](https://example.test/beta) - ISC
- [transitive@2.0.0](https://example.test/transitive) - Apache-2.0
'
incomplete_notice='- [alpha@1.0.0](https://example.test/alpha) - MIT
- [beta@3.0.0](https://example.test/beta) - ISC
'
unexpected_notice="${complete_notice}- [gamma@4.0.0](https://example.test/gamma) - MIT
"

run_generator() {
    PARISH_UI_NOTICES_UI_DIR="$ui_dir" \
        PARISH_UI_NOTICES_DESTINATION="$destination" \
        PARISH_UI_NOTICES_NPM_BIN="$bin_dir/npm" \
        PARISH_UI_NOTICES_NPX_BIN="$bin_dir/npx" \
        TEST_CALLS="$calls" \
        TEST_DEPENDENCY_TREE="$dependency_tree" \
        TEST_CI_FAIL="${TEST_CI_FAIL:-0}" \
        TEST_GENERATOR_FAIL="${TEST_GENERATOR_FAIL:-0}" \
        TEST_NOTICE_OUTPUT="${TEST_NOTICE_OUTPUT:-$complete_notice}" \
        bash "$generator" >/dev/null 2>&1
}

original='existing attribution must survive'
printf '%s\n' "$original" >"$destination"

# Missing prerequisites: a failed npm ci must stop before invoking the
# generator or touching the destination.
: >"$calls"
rm -rf "$ui_dir/node_modules"
TEST_CI_FAIL=1
if run_generator; then
    check "missing prerequisite exits nonzero" "0" "nonzero"
else
    check "missing prerequisite exits nonzero" "nonzero" "nonzero"
fi
unset TEST_CI_FAIL
check "npm ci failure preserves destination" "$(cat "$destination")" "$original"
check "generator skipped after npm ci failure" "$(grep -c '^npx ' "$calls" || true)" "0"

# Generator failure may write a partial candidate, but never the destination.
: >"$calls"
TEST_GENERATOR_FAIL=1
if run_generator; then
    check "generator failure exits nonzero" "0" "nonzero"
else
    check "generator failure exits nonzero" "nonzero" "nonzero"
fi
unset TEST_GENERATOR_FAIL
check "generator failure preserves destination" "$(cat "$destination")" "$original"

# Exit 0 with blank output is rejected before the destination is replaced.
TEST_NOTICE_OUTPUT=$' \n'
if run_generator; then
    check "blank output exits nonzero" "0" "nonzero"
else
    check "blank output exits nonzero" "nonzero" "nonzero"
fi
unset TEST_NOTICE_OUTPUT
check "blank output preserves destination" "$(cat "$destination")" "$original"

# Exit 0 with structurally incomplete output is still rejected atomically.
TEST_NOTICE_OUTPUT="$incomplete_notice"
if run_generator; then
    check "partial output exits nonzero" "0" "nonzero"
else
    check "partial output exits nonzero" "nonzero" "nonzero"
fi
unset TEST_NOTICE_OUTPUT
check "partial output preserves destination" "$(cat "$destination")" "$original"

# The notice must describe the exact installed production tree, not dev or
# stale packages left behind by another installation.
TEST_NOTICE_OUTPUT="$unexpected_notice"
if run_generator; then
    check "unexpected package exits nonzero" "0" "nonzero"
else
    check "unexpected package exits nonzero" "nonzero" "nonzero"
fi
unset TEST_NOTICE_OUTPUT
check "unexpected package preserves destination" "$(cat "$destination")" "$original"

# A clean fixture establishes node_modules, validates every installed package,
# and atomically replaces the destination. Repetition must be byte-identical.
rm -rf "$ui_dir/node_modules"
run_generator
first_hash="$(shasum -a 256 "$destination" | awk '{print $1}')"
check "clean success establishes node_modules" "$([[ -d "$ui_dir/node_modules" ]] && echo yes || echo no)" "yes"
check "successful output replaces destination" "$(cat "$destination")" "${complete_notice%$'\n'}"
run_generator
second_hash="$(shasum -a 256 "$destination" | awk '{print $1}')"
check "consecutive successful generations are identical" "$second_hash" "$first_hash"

if [[ "$fails" -ne 0 ]]; then
    echo "generate-ui-notices.test.sh: $fails assertion(s) failed." >&2
    exit 1
fi
echo "generate-ui-notices.test.sh: all assertions passed."
