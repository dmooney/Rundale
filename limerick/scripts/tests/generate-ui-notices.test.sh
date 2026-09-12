#!/usr/bin/env bash
# CI entry point; the implementation and regression suite are Node so the
# supported native Windows workflow does not require Bash.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec node "$script_dir/generate-ui-notices.test.mjs"
