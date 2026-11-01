#!/usr/bin/env bash
# Runs `promptfoo eval` for the lab, loading API keys from the repository's
# .env when it exists (keys already in the environment win). Pass promptfoo's
# eval arguments straight through; run it from anywhere.
#   bash promptfoo/prompt-lab/eval.sh -c promptfooconfig.yaml --no-cache -o output/round-01.json
set -euo pipefail
cd "$(dirname "$0")"
export PROMPTFOO_DISABLE_TELEMETRY=1
env_args=()
[ -f ../../.env ] && env_args=(--env-file ../../.env)
exec ../node_modules/.bin/promptfoo eval "${env_args[@]}" "$@"
