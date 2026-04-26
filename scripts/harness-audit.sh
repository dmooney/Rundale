#!/usr/bin/env bash
#
# Read-only audit: where does the gameplay harness have coverage gaps?
#
# Cross-references three sources:
#   1. testing/fixtures/*.txt    — what we exercise
#   2. testing/evals/baselines/  — what we lock against drift
#   3. docs/requirements/roadmap.md — what's been shipped
#
# Plus a curated "core subsystem" matrix that maps named gameplay features
# (weather, persistence, banshee, etc.) to the fixtures expected to cover
# them. New core subsystems should be added to the matrix as they ship.
#
# This script is **descriptive, not enforcing** — it prints a report and
# exits 0. It's a planning aid, not a CI gate.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# ─── Inventory ───────────────────────────────────────────────────────────────

fixtures_total=$(find testing/fixtures -name '*.txt' -type f | wc -l)
fixtures_test=$(find testing/fixtures -name 'test_*.txt' -type f | wc -l)
fixtures_play=$(find testing/fixtures -name 'play_*.txt' -type f | wc -l)
fixtures_banshee=$(find testing/fixtures -name 'banshee_*.txt' -type f | wc -l)
fixtures_other=$(( fixtures_total - fixtures_test - fixtures_play - fixtures_banshee ))

baselines=0
if [[ -d testing/evals/baselines ]]; then
    baselines=$(find testing/evals/baselines -name '*.json' -type f | wc -l)
fi

echo "===== HARNESS COVERAGE AUDIT ====="
echo
echo "Script-harness fixtures:  ${fixtures_total}"
echo "  test_*    (regression): ${fixtures_test}"
echo "  play_*    (feature):    ${fixtures_play}"
echo "  banshee_* (feature):    ${fixtures_banshee}"
echo "  other:                  ${fixtures_other}"
echo
echo "Eval baselines (drift sensors): ${baselines}"
if (( baselines > 0 )); then
    while IFS= read -r f; do
        echo "  $(basename "$f" .json)"
    done < <(find testing/evals/baselines -name '*.json' -type f | sort)
fi
echo

# ─── Roadmap progress per phase ──────────────────────────────────────────────

echo "===== ROADMAP PROGRESS ====="
awk '
    /^## Phase/        { current = $0; phases[++n] = current; next }
    /^- \[x\]/         { done[current]++; total[current]++ }
    /^- \[~\]/         { prog[current]++; total[current]++ }
    /^- \[ \]/         { todo[current]++; total[current]++ }
    END {
        for (i = 1; i <= n; i++) {
            p = phases[i]
            d = done[p] + 0; w = prog[p] + 0; t = todo[p] + 0; sum = total[p] + 0
            printf "  %-44s %2d / %2d done", p, d, sum
            if (w > 0) printf "  (%d in progress)", w
            if (t > 0) printf "  (%d not started)", t
            print ""
        }
    }
' docs/requirements/roadmap.md
echo

# ─── Curated subsystem-coverage matrix ───────────────────────────────────────
#
# Format: "subsystem|fixture-name-keyword|roadmap-keyword"
# A subsystem is "covered" if any fixture name contains the keyword.
# A subsystem is "shipped" if the roadmap has any `- [x]` containing the
# keyword (case-insensitive).

SUBSYSTEMS=(
    "Movement|movement|movement"
    "Look / descriptions|look|descriptions"
    "World graph|all_locations|world graph"
    "Multi-hop pathfinding|multi_hop|pathfinding"
    "Time progression|time|GameClock"
    "Speed presets|speed|speed"
    "Pause / resume|pause|pause"
    "Persistence|persistence|persistence"
    "Anachronism detection|anachronism|anachronism"
    "Weather|weather|weather"
    "Banshee / death|banshee|banshee"
    "Frontier (sparse-tier)|frontier|frontier"
    "Feature flags|flags|feature flag"
    "Aliases|aliases|alias"
    "Fuzzy name matching|fuzzy|fuzzy"
    "Debug commands|debug|debug"
    "Festivals|festival|festival"
    "Encounters|encounter|encounter"
    "Memory / overhear|overhear|memory"
    "Schedules|schedule|schedule"
)

echo "===== SUBSYSTEM COVERAGE ====="
echo
printf "  %-26s %-9s %-9s %s\n" "Subsystem" "Fixture" "Roadmap" "Notes"
printf "  %-26s %-9s %-9s %s\n" "─────────" "───────" "───────" "─────"
gaps=0
for entry in "${SUBSYSTEMS[@]}"; do
    IFS='|' read -r name fixture_kw roadmap_kw <<< "$entry"
    if find testing/fixtures -iname "*${fixture_kw}*" | grep -q .; then
        fix_status="yes"
    else
        fix_status="MISSING"
    fi
    if grep -iqE "^- \[x\].*${roadmap_kw}" docs/requirements/roadmap.md; then
        rm_status="shipped"
    elif grep -iqE "^- \[~\].*${roadmap_kw}" docs/requirements/roadmap.md; then
        rm_status="WIP"
    elif grep -iqE "^- \[ \].*${roadmap_kw}" docs/requirements/roadmap.md; then
        rm_status="planned"
    else
        rm_status="-"
    fi
    note=""
    if [[ "$fix_status" == "MISSING" && "$rm_status" == "shipped" ]]; then
        note="← gap: shipped without fixture"
        gaps=$((gaps + 1))
    fi
    printf "  %-26s %-9s %-9s %s\n" "$name" "$fix_status" "$rm_status" "$note"
done
echo

echo "===== SUMMARY ====="
echo "  Subsystems with shipped roadmap items but no fixture: ${gaps}"
echo
echo "Add a fixture for any flagged gap with /feature-scaffold or by hand:"
echo "  testing/fixtures/play_<subsystem>.txt"
echo "and consider adding it to BASELINED_FIXTURES in"
echo "  crates/parish-cli/tests/eval_baselines.rs"
echo "for drift-sensor coverage."
