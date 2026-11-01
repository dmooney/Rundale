#!/usr/bin/env bash
# Offline check of the prompt lab: no model or judge calls, no keys needed.
# Renders every case under both prompts with promptfoo's echo provider, checks
# the anachronism switch, and checks summary.jq on real promptfoo output and on
# a fixture.
set -euo pipefail
cd "$(dirname "$0")"
pf="$(cd .. && pwd)/node_modules/.bin/promptfoo"
export PROMPTFOO_DISABLE_TELEMETRY=1 PROMPTFOO_DISABLE_UPDATE=1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

$pf validate -c promptfooconfig.yaml >/dev/null

# The same config with the judges removed, so rendering every case costs
# nothing. The echo reply is not the output schema's JSON, so every row must
# come out malformed.
node -e '
  const yaml = require(require.resolve("js-yaml", { paths: ["../node_modules"] }));
  const fs = require("fs");
  const cfg = yaml.load(fs.readFileSync("promptfooconfig.yaml", "utf8"));
  cfg.defaultTest.assert = cfg.defaultTest.assert.filter((a) => a.metric === "schema");
  fs.writeFileSync(process.argv[1], yaml.dump(cfg));
' "$tmp/nojudge.yaml"
cp -r prompts cases content "$tmp/"

brief_keys='["aboutYou","commonKnowledge","theVillage","thisMoment","whatTheStrangerSays"]'
for split in train heldout; do
    cases=$(grep -c '^- description:' "cases/$split.yaml")
    (cd "$tmp" && $pf eval -c nojudge.yaml -r echo -t "cases/$split.yaml" --no-cache --no-progress-bar -o "$split.json" >/dev/null 2>&1) || true
    jq -e --argjson want "$((cases * 2))" --argjson keys "$brief_keys" '
    .results.results as $r
    | ($r | length) == $want
    and all($r[]; .prompt.raw | fromjson
      | .[0].role == "system" and .[1].role == "user"
      and (.[1].content | startswith("Endpoint input values:\n"))
      and (.[1].content | split("\n")[1:] | join("\n") | fromjson | type == "object"))
    and all($r[] | select(.prompt.label == "brief"); .prompt.raw | fromjson | .[1].content
      | split("\n")[1:] | join("\n") | fromjson | keys == $keys)
  ' "$tmp/$split.json" >/dev/null || {
        echo "check: $split cases did not all render" >&2
        exit 1
    }
    jq -f summary.jq "$tmp/$split.json" | jq -e --argjson want "$((cases * 2))" '
    .unpriced_models == ["echo"] and ([.prompts[].all_models] | (map(.malformed) | add) == $want and (map(.outages | length) | add) == 0)
  ' >/dev/null || {
        echo "check: summary.jq did not score every echo reply as malformed" >&2
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
(cd "$tmp" && $pf eval -c nojudge.yaml -r echo --filter-prompts '^brief$' --var noflag=true --no-cache --no-progress-bar -o noflag.json >/dev/null 2>&1) || true
jq -e --arg n "$notice" '[.results.results[] | .prompt.raw | test($n)] | any | not' "$tmp/noflag.json" >/dev/null \
    || {
        echo "check: noflag=true kept the anachronism notice" >&2
        exit 1
    }

# A malformed reply stays malformed when the judges fail too. Fake keys and a
# closed local port make both judges fail at once, without touching a network.
PROMPTFOO_REQUEST_BACKOFF_MS=1 ANTHROPIC_API_KEY=offline OPENAI_API_KEY=offline \
    ANTHROPIC_BASE_URL=http://127.0.0.1:9 OPENAI_BASE_URL=http://127.0.0.1:9/v1 \
    $pf eval -c promptfooconfig.yaml -r echo --filter-pattern '^t07-' --filter-prompts '^brief$' \
    --no-cache --no-progress-bar -o "$tmp/offline.json" >/dev/null 2>&1 || true
jq -f summary.jq "$tmp/offline.json" | jq -e '.prompts[0].all_models | .malformed == 1 and (.outages | length) == 0' >/dev/null || {
    echo "check: a malformed reply with failed judges was not scored as malformed" >&2
    exit 1
}

# summary.jq on the fixture: one row of each kind, with reasoning tokens for
# every provider (added for Gemini only).
jq -f summary.jq test/fixture-results.json | jq -e '
  .prompts[0].all_models as $a
  | $a.scored == 2 and $a.malformed == 1 and ($a.outages | map(.case)) == ["request-failed", "judge-failed"]
  and $a.mean == 43.3
  and $a.by_requirement == {"B1": 80, "B3": 65, "B5": 0}
  and ($a.judges_split_over_20 | map(.case)) == ["scored-split"]
  and ($a.spend_usd * 10000 | round) == 277
  and ($a.worst | map(.points)) == [0, 50, 80]
  and .prompts[0].by_model["flash-lite"].mean == 40
  and .unpriced_models == []
' >/dev/null || {
    echo "check: summary.jq gave the wrong summary for test/fixture-results.json" >&2
    jq -f summary.jq test/fixture-results.json >&2
    exit 1
}

echo "prompt-lab check passed"
