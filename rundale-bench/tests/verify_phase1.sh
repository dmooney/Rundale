#!/usr/bin/env bash
#
# Phase 1 end-to-end verification for the eval-redesign foundations.
# Hermetic: no network, no model calls. Run from the repo root:
#
#   bash rundale-bench/tests/verify_phase1.sh
#
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

PY=python3
BENCH=rundale-bench/rundale_bench.py

echo "==================================================================="
echo " Phase 1 verification — rundale-bench eval redesign"
echo "==================================================================="

echo
echo "### Criterion 1 — catalog parses; cheapest provider resolves"
$PY "$BENCH" catalog --show

echo
echo "### Criterion 2 — Sonnet judge config pinned (rubric_sha256 integrity)"
$PY "$BENCH" judge --verify sonnet

echo
echo "### Criteria 3-9 — cache key, bundle/queue, ingest, failures, skills"
$PY rundale-bench/test_phase1.py

echo
echo "### Regression — legacy graders unaffected"
$PY rundale-bench/test_grade.py | tail -1

echo
echo "==================================================================="
echo " Phase 1 verification PASSED"
echo "==================================================================="
