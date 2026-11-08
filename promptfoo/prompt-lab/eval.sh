#!/usr/bin/env bash
# Runs `promptfoo eval` for the lab, loading API keys from the repository's
# .env when it exists (keys already in the environment win). Pass promptfoo's
# eval arguments straight through; run it from anywhere.
#   bash promptfoo/prompt-lab/eval.sh -c promptfooconfig.yaml --no-cache -o output/round-01.json
# Exits 3 when any reply is an outage (a failed request or judge, such as an
# exhausted credit balance), so a chained batch stops at the first void run
# instead of paying for the rest.
set -euo pipefail
cd "$(dirname "$0")"
export PROMPTFOO_DISABLE_TELEMETRY=1
env_args=()
[ -f ../../.env ] && env_args=(--env-file ../../.env)
out=""
prev=""
for a in "$@"; do
    [ "$prev" = "-o" ] || [ "$prev" = "--output" ] && out=$a
    prev=$a
done
status=0
../node_modules/.bin/promptfoo eval "${env_args[@]}" "$@" || status=$?
if [ -n "$out" ] && [ -f "$out" ]; then
    outages=$(jq -f summary.jq "$out" | jq -r '[.prompts[].all_models.outages[]] | length, (group_by(.why) | map("  \(length)x \(.[0].why)") | .[])')
    if [ "$(head -1 <<<"$outages")" != "0" ]; then
        echo "eval.sh: $(head -1 <<<"$outages") outages; this run is void and must be rerun whole:" >&2
        tail -n +2 <<<"$outages" >&2
        exit 3
    fi
fi
# promptfoo exits 100 when any assertion fails, which is normal for a scored run.
[ "$status" = 0 ] || [ "$status" = 100 ] || exit "$status"
