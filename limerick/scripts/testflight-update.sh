#!/usr/bin/env bash
# Run `release.py testflight` with the App Store Connect API key taken from the
# repository's gitignored .env when the environment does not already set it.
# Only ASC_KEY_PATH, ASC_KEY_ID and ASC_ISSUER_ID are read; nothing else in
# .env is exported. RUNDALE_DOTENV names another file (for tests).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dotenv="${RUNDALE_DOTENV:-$repo_root/.env}"

if [[ -f "$dotenv" ]]; then
    # `|| [[ -n "$name" ]]` keeps a final line that has no trailing newline.
    while IFS='=' read -r name value || [[ -n "$name" ]]; do
        case "$name" in
            ASC_KEY_PATH | ASC_KEY_ID | ASC_ISSUER_ID)
                if [[ -z "${!name:-}" ]]; then
                    value="${value%$'\r'}"
                    value="${value%\"}"
                    value="${value#\"}"
                    value="${value%\'}"
                    value="${value#\'}"
                    export "$name=${value/#\~/$HOME}"
                fi
                ;;
        esac
    done <"$dotenv"
fi

exec python3 "$repo_root/mobile/scripts/release.py" testflight "$@"
