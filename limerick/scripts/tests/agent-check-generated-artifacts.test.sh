#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
checker="$repo_root/limerick/scripts/agent-check.sh"
test_repo="$(mktemp -d)"
trap 'rm -rf "$test_repo"' EXIT

cd "$test_repo"
git init -q
git config user.name "Agent Check Test"
git config user.email "agent-check@example.invalid"

printf 'fixture\n' >README.md
git add README.md
git commit -qm "test: initialize fixture"

mkdir -p mods/graphify-out
printf '{}\n' >mods/graphify-out/graph.json
git add mods/graphify-out/graph.json
git commit -qm "test: add generated graph"

generated_output="$(AGENT_CHECK_BASE_REF=HEAD^ bash "$checker")"
grep -Fq 'no proof-relevant changes' <<<"$generated_output"

# Markdown under mods/ is documentation: paired with a non-runtime code
# change (an integration test), it must not demand live-process proof.
mkdir -p mods/rundale limerick/crates/limerick-core/tests
printf '# Mod notes\n' >mods/rundale/AGENTS.md
printf '#[test]\nfn fixture() {}\n' >limerick/crates/limerick-core/tests/fixture.rs
git add mods/rundale/AGENTS.md limerick/crates/limerick-core/tests/fixture.rs
git commit -qm "test: add mod documentation and a core test"

doc_output="$(AGENT_CHECK_BASE_REF=HEAD^ bash "$checker" 2>&1 || true)"
grep -Fq '1 proof-relevant file(s) changed' <<<"$doc_output"
if grep -Fq 'live proof required' <<<"$doc_output"; then
    echo "expected Markdown under mods/ not to require live proof" >&2
    exit 1
fi

printf '{}\n' >mods/rundale/world.json
git add mods/rundale/world.json
git commit -qm "test: add runtime mod source"

runtime_output="$test_repo/runtime-output.txt"
if AGENT_CHECK_BASE_REF=HEAD^ bash "$checker" >"$runtime_output" 2>&1; then
    echo "expected a real mod source change to require proof" >&2
    exit 1
fi
grep -Fq 'proof-relevant changes require a bundle' "$runtime_output"

echo "agent-check generated-artifact classification test passed"
