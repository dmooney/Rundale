#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
checker="$repo_root/parish/scripts/check-repository-artifacts.sh"
test_repo="$(mktemp -d)"
trap 'rm -rf "$test_repo"' EXIT

cd "$test_repo"
git init -q
git config user.name "Repository Artifact Test"
git config user.email "repository-artifact-test@example.invalid"

mkdir -p docs/screenshots
printf '[Current screenshot](docs/screenshots/current.png)\n' >README.md
printf 'current image\n' >docs/screenshots/current.png
printf '# kind|path|bytes|sha256|owner|purpose\n' >policy.txt
git add README.md docs/screenshots/current.png policy.txt

run_checker() {
    REPOSITORY_ARTIFACT_ROOT="$test_repo" \
        REPOSITORY_ARTIFACT_POLICY="$test_repo/policy.txt" \
        bash "$checker"
}

expect_failure() {
    local expected="$1"
    local output="$test_repo/check-output.txt"
    if run_checker >"$output" 2>&1; then
        echo "repository artifact check unexpectedly passed: $expected" >&2
        exit 1
    fi
    if ! grep -Fq "$expected" "$output"; then
        echo "repository artifact failure did not mention: $expected" >&2
        sed -n '1,120p' "$output" >&2
        exit 1
    fi
}

run_checker >/dev/null

mkdir -p nested/graphify-out
printf '{}\n' >nested/graphify-out/graph.json
git add nested/graphify-out/graph.json
expect_failure "Graphify output is generated"
git rm -q -f nested/graphify-out/graph.json

mkdir -p docs/graphics-v2/pipeline-experiments/nested
printf 'direct experiment image\n' \
    >docs/graphics-v2/pipeline-experiments/direct.png
printf 'nested experiment image\n' \
    >docs/graphics-v2/pipeline-experiments/nested/control.png
git add docs/graphics-v2/pipeline-experiments/direct.png \
    docs/graphics-v2/pipeline-experiments/nested/control.png
expect_failure "Graphics V2 experiment PNGs are archived output"
git rm -q -f docs/graphics-v2/pipeline-experiments/direct.png \
    docs/graphics-v2/pipeline-experiments/nested/control.png

mkdir -p parish/docs/screenshots
printf 'legacy\n' >parish/docs/screenshots/gui-morning.png
git add parish/docs/screenshots/gui-morning.png
expect_failure "legacy parish/docs screenshots are retired"
git rm -q -f parish/docs/screenshots/gui-morning.png

mkdir -p parish/apps/ui/static/rundale/notebook-ui
printf '{"scene":"scene-kilteevan-village.png"}\n' \
    >parish/apps/ui/static/rundale/notebook-ui/asset-manifest.json
git add parish/apps/ui/static/rundale/notebook-ui/asset-manifest.json
expect_failure "manifest still references retired scene plate"
printf '{"assets":{}}\n' \
    >parish/apps/ui/static/rundale/notebook-ui/asset-manifest.json
git add parish/apps/ui/static/rundale/notebook-ui/asset-manifest.json
run_checker >/dev/null

printf 'orphan\n' >docs/screenshots/orphan.png
git add docs/screenshots/orphan.png
expect_failure "documentation screenshot is not referenced"
printf '[Orphan promoted](docs/screenshots/orphan.png)\n' >>README.md
git add README.md
run_checker >/dev/null

dd if=/dev/zero of=large.bin bs=1048576 count=9 >/dev/null 2>&1
git add large.bin
expect_failure "require an exact reviewed exception"

large_size="$(wc -c <large.bin | tr -d '[:space:]')"
if command -v sha256sum >/dev/null 2>&1; then
    large_sha="$(sha256sum large.bin | awk '{print $1}')"
else
    large_sha="$(shasum -a 256 large.bin | awk '{print $1}')"
fi
printf 'large|large.bin|%s|%s|test-owner|Large-file gate fixture.\n' \
    "$large_size" "$large_sha" >>policy.txt
git add policy.txt
run_checker >/dev/null

printf 'tamper\n' >>large.bin
git add large.bin
expect_failure "exception size/hash drifted"

echo "repository artifact policy tests passed"
