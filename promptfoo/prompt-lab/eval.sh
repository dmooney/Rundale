#!/usr/bin/env bash
# Runs `promptfoo eval` for the lab, loading API keys from the repository's
# .env when it exists (keys already in the environment win). Pass promptfoo's
# eval arguments straight through; run it from anywhere.
#   bash promptfoo/prompt-lab/eval.sh -c promptfooconfig.yaml --no-cache -o output/round-01.json
# Exits 3 when any reply is an outage (a failed request or judge, such as an
# exhausted credit balance), so a chained batch stops at the first void run
# instead of paying for the rest.
# Before calling anything it checks the OpenRouter balance and exits 4 when less
# than LAB_MIN_CREDIT_USD (default 8, about one training round) is left, so a
# round is not voided halfway by "402 Payment Required".
set -euo pipefail
cd "$(dirname "$0")"
export PROMPTFOO_DISABLE_TELEMETRY=1
# The GPT judge at xhigh effort can take over promptfoo's default 300 s per
# request; a timeout is an outage that voids the whole round.
export REQUEST_TIMEOUT_MS="${REQUEST_TIMEOUT_MS:-900000}"
env_args=()
[ -f ../../.env ] && env_args=(--env-file ../../.env)
key="${OPENROUTER_API_KEY:-}"
if [ -z "$key" ] && [ -f ../../.env ]; then
    key=$(sed -n 's/^OPENROUTER_API_KEY=//p' ../../.env | head -1 | tr -d "\"'")
fi
if [ -n "$key" ] && [ "${LAB_SKIP_CREDIT_CHECK:-}" != 1 ]; then
    left=$(curl -fsS --max-time 20 https://openrouter.ai/api/v1/credits -H "Authorization: Bearer $key" \
        | jq -r '.data.total_credits - .data.total_usage') || left=""
    if [ -z "$left" ]; then
        echo "eval.sh: could not read the OpenRouter balance; set LAB_SKIP_CREDIT_CHECK=1 to run anyway" >&2
        exit 4
    fi
    if jq -en --argjson l "$left" --argjson m "${LAB_MIN_CREDIT_USD:-8}" '$l < $m' >/dev/null; then
        printf 'eval.sh: only $%.2f of OpenRouter credit left (need $%s); add credit or lower LAB_MIN_CREDIT_USD\n' "$left" "${LAB_MIN_CREDIT_USD:-8}" >&2
        exit 4
    fi
fi
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
