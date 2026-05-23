#!/usr/bin/env bash
#
# Phase 2 verification — tier funnel + differential re-judge. Hermetic.
# Run from repo root:  bash rundale-bench/tests/verify_phase2.sh
#
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
PY=python3
BENCH=rundale-bench/rundale_bench.py

echo "==================================================================="
echo " Phase 2 verification — rundale-bench tier funnel + re-judge"
echo "==================================================================="

echo
echo "### Criterion 1 — tier prompt sets resolve"
$PY "$BENCH" tiers --show

echo
echo "### Criteria 2-8 — run/promote/rejudge (hermetic)"
$PY rundale-bench/test_phase2.py

echo
echo "### Criterion 9 — Phase 1 + legacy graders unaffected"
$PY rundale-bench/test_phase1.py | tail -1
$PY rundale-bench/test_grade.py | tail -1

echo
echo "==================================================================="
echo " Phase 2 verification PASSED"
echo "==================================================================="
