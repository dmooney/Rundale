#!/usr/bin/env bash
#
# Phase 4 verification — results website. Run from repo root:
#   bash rundale-bench/tests/verify_phase4.sh
#
# Requires `bench-site` deps installed (pnpm install) for the build step.
#
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
PY=python3

echo "==================================================================="
echo " Phase 4 verification — rundale-bench results website"
echo "==================================================================="

echo
echo "### Criteria 1-3 — data bridge (hermetic)"
$PY rundale-bench/test_phase4.py

echo
echo "### Regenerate bench.json from committed artifacts"
$PY rundale-bench/build_site_data.py

echo
echo "### Criterion 4 — Astro builds static dist/"
( cd bench-site && pnpm build >/dev/null )
test -f bench-site/dist/index.html && echo "OK dist/index.html"
ls bench-site/dist/models/*/index.html >/dev/null 2>&1 && echo "OK dist/models/<id>/index.html"
test -f bench-site/dist/perf/index.html && echo "OK dist/perf/index.html"

echo
echo "### Criterion 8 — publish workflow wired"
wf=.github/workflows/publish-bench-site.yml
for needle in "configure-pages" "upload-pages-artifact" "deploy-pages" "bench-site/dist" "pages: write"; do
  grep -q "$needle" "$wf" && echo "OK workflow has: $needle"
done

echo
echo "### Criterion 9 — Phase 1/2/3 + graders unaffected"
$PY rundale-bench/test_phase1.py | tail -1
$PY rundale-bench/test_phase2.py | tail -1
$PY rundale-bench/test_phase3.py | tail -1
$PY rundale-bench/test_grade.py | tail -1

echo
echo "==================================================================="
echo " Phase 4 verification PASSED"
echo "==================================================================="
