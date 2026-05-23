#!/usr/bin/env bash
#
# Phase 3 verification — perf-by-provider. Hermetic.
# Run from repo root:  bash rundale-bench/tests/verify_phase3.sh
#
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
PY=python3

echo "==================================================================="
echo " Phase 3 verification — rundale-bench perf-by-provider"
echo "==================================================================="

echo
echo "### Criteria 1-8 — perf aggregation, sweep, schema (hermetic)"
$PY rundale-bench/test_phase3.py

echo
echo "### Criterion 9 — Phase 1/2 + legacy graders unaffected"
$PY rundale-bench/test_phase1.py | tail -1
$PY rundale-bench/test_phase2.py | tail -1
$PY rundale-bench/test_grade.py | tail -1

echo
echo "==================================================================="
echo " Phase 3 verification PASSED"
echo "==================================================================="
