#!/usr/bin/env bash
#
# Read-only audit: where does the gameplay harness have coverage gaps?
#
# Cross-references four sources:
#   1. parish/testing/scenarios/*.yaml  — real-loop asserted regressions
#   2. parish/testing/fixtures/test_*.txt — legacy asserted regressions
#   3. parish/testing/proofs/*.txt      — one-off exploratory proofs
#   4. parish/testing/evals/baselines/  — legacy output drift locks
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

fixtures_test=$(find parish/testing/fixtures -name 'test_*.txt' -type f | wc -l)
scenarios=$(find parish/testing/scenarios -name '*.yaml' -type f | wc -l)
proofs=$(find parish/testing/proofs -name '*.txt' -type f | wc -l)

baselines=0
if [[ -d parish/testing/evals/baselines ]]; then
    baselines=$(find parish/testing/evals/baselines -name '*.json' -type f | wc -l)
fi

echo "===== HARNESS COVERAGE AUDIT ====="
echo
echo "Asserted real-loop scenarios: ${scenarios}"
echo "Legacy regression fixtures:  ${fixtures_test}"
echo "Exploratory proof scripts:    ${proofs}"
echo
echo "Eval baselines (drift sensors): ${baselines}"
if ((baselines > 0)); then
    while IFS= read -r f; do
        echo "  $(basename "$f" .json)"
    done < <(find parish/testing/evals/baselines -name '*.json' -type f | sort)
fi
echo

# ─── Roadmap portfolio status ──────────────────────────────────────────────

echo "===== ROADMAP STATUS ====="
awk -F'|' '
    function trim(value) {
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
        return value
    }
    /^\|/ {
        feature = trim($2)
        status = trim($3)
        if (feature == "Subsystem / Feature" || feature ~ /^-+$/ || status == "") next
        counts[status]++
    }
    END {
        printf "  %-12s %2d\n", "Implemented", counts["Implemented"] + 0
        printf "  %-12s %2d\n", "Partial", counts["Partial"] + 0
        printf "  %-12s %2d\n", "In progress", counts["In progress"] + 0
        printf "  %-12s %2d\n", "Proposed", counts["Proposed"] + 0
        printf "  %-12s %2d\n", "Planned", counts["Planned"] + 0
    }
' docs/requirements/roadmap.md
echo

roadmap_status() {
    local keyword="$1"
    awk -F'|' -v keyword="$keyword" '
        function trim(value) {
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
            return value
        }
        function priority(status) {
            if (status == "Implemented") return 3
            if (status == "Partial" || status == "In progress") return 2
            if (status == "Proposed" || status == "Planned") return 1
            return 0
        }
        BEGIN {
            target = tolower(keyword)
            best = 0
            result = "-"
        }
        /^\|/ {
            feature = trim($2)
            status = trim($3)
            rank = priority(status)
            if (index(tolower(feature), target) && rank > best) {
                best = rank
                if (rank == 3) result = "shipped"
                else if (rank == 2) result = "WIP"
                else if (rank == 1) result = "planned"
            }
        }
        END { print result }
    ' docs/requirements/roadmap.md
}

# ─── Curated subsystem-coverage matrix ───────────────────────────────────────
#
# Format: "subsystem|fixture-name-keyword|roadmap-keyword"
# A subsystem is "covered" if any fixture name contains the keyword.
# A subsystem's status comes from the authoritative roadmap feature-status
# table. When multiple rows match, the most complete status wins.

SUBSYSTEMS=(
    "Movement|movement|movement"
    "Look / descriptions|look|descriptions"
    "World graph|all_locations|graph-based world"
    "Multi-hop pathfinding|multi_hop|pathfinding"
    "Time progression|time|time"
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
    IFS='|' read -r name fixture_kw roadmap_kw <<<"$entry"
    if find parish/testing/scenarios parish/testing/fixtures -iname "*${fixture_kw}*" | grep -q .; then
        fix_status="yes"
    else
        fix_status="MISSING"
    fi
    rm_status=$(roadmap_status "$roadmap_kw")
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
echo "Add a machine-asserted real-loop scenario for any flagged gap:"
echo "  parish/testing/scenarios/<subsystem>.yaml"
echo "One-off demonstrations belong under parish/testing/proofs/, not fixtures/."
