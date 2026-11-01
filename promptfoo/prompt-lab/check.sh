#!/usr/bin/env bash
# Offline check of the prompt lab: no model or judge calls, no keys needed.
# Renders every case under both prompts with promptfoo's echo provider, checks
# the anachronism switch, and checks summary.jq against a fixture.
set -euo pipefail
cd "$(dirname "$0")"
pf=../node_modules/.bin/promptfoo
export PROMPTFOO_DISABLE_TELEMETRY=1 PROMPTFOO_DISABLE_UPDATE=1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

$pf validate -c promptfooconfig.yaml >/dev/null

# Every case renders to the production shape under both prompts. The echo
# output is not the schema's JSON, so the transform fails before any judge runs.
for split in train heldout; do
    cases=$(grep -c '^- description:' "cases/$split.yaml")
    $pf eval -c promptfooconfig.yaml -r echo -t "cases/$split.yaml" --no-cache --no-progress-bar -o "$tmp/$split.json" >/dev/null 2>&1 || true
    jq -e --argjson want "$((cases * 2))" '
    .results.results as $r
    | ($r | length) == $want
    and ([$r[] | .response.tokenUsage.assertions.numRequests // 0] | add) == 0
    and all($r[]; .prompt.raw | fromjson
      | .[0].role == "system" and .[1].role == "user"
      and (.[1].content | startswith("Endpoint input values:\n"))
      and (.[1].content | split("\n")[1:] | join("\n") | fromjson | type == "object"))
    and all($r[] | select(.prompt.label == "brief"); .prompt.raw | fromjson | .[1].content
      | split("\n")[1:] | join("\n") | fromjson | keys == ["aboutYou", "commonKnowledge", "theVillage", "thisMoment", "whatTheStrangerSays"])
  ' "$tmp/$split.json" >/dev/null || {
        echo "check: $split cases did not all render" >&2
        exit 1
    }
done

# The notice follows the case's anachronism value, and --var noflag=true drops it.
notice='not a thing you have ever heard of'
jq -e --arg n "$notice" '[.results.results[] | select(.prompt.label == "brief" and .vars.anachronism) | .prompt.raw | test($n)] | length > 0 and all' "$tmp/train.json" >/dev/null \
    || {
        echo "check: flagged cases lack the anachronism notice" >&2
        exit 1
    }
jq -e --arg n "$notice" '[.results.results[] | select(.prompt.label == "brief" and (.vars.anachronism | not)) | .prompt.raw | test($n)] | any | not' "$tmp/train.json" >/dev/null \
    || {
        echo "check: unflagged cases carry the anachronism notice" >&2
        exit 1
    }
$pf eval -c promptfooconfig.yaml -r echo --filter-prompts '^brief$' --var noflag=true --no-cache --no-progress-bar -o "$tmp/noflag.json" >/dev/null 2>&1 || true
jq -e --arg n "$notice" '[.results.results[] | .prompt.raw | test($n)] | any | not' "$tmp/noflag.json" >/dev/null \
    || {
        echo "check: noflag=true kept the anachronism notice" >&2
        exit 1
    }

# summary.jq: a malformed reply scores 0, request and judge failures are left
# out and reported, splits over 20 points are flagged, spend is priced per model.
jq -f summary.jq test/fixture-results.json | jq -e '
  .prompts[0].all_models as $a
  | $a.scored == 2 and $a.malformed == 1 and ($a.outages | length) == 2
  and $a.mean == 43.3
  and $a.by_requirement == {"B1": 80, "B3": 65, "B5": 0}
  and ($a.judges_split_over_20 | map(.case)) == ["scored-split"]
  and $a.spend_usd == 0.0275
  and ($a.worst | map(.points)) == [0, 50, 80]
  and .prompts[0].by_model["flash-lite"].mean == 40
' >/dev/null || {
    echo "check: summary.jq gave the wrong summary for test/fixture-results.json" >&2
    jq -f summary.jq test/fixture-results.json >&2
    exit 1
}

echo "prompt-lab check passed"
