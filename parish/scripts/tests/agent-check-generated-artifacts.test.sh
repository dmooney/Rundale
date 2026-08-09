#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
checker="$repo_root/parish/scripts/agent-check.sh"
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

mkdir -p mods/rundale
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
