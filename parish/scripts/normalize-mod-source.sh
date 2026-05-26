#!/usr/bin/env bash
set -euo pipefail

# normalize-mod-source.sh — rewrite a mod's JSON files through the editor
# save path. Destructive operation — prompts for confirmation.
#
# Usage: bash parish/scripts/normalize-mod-source.sh <mod-dir>
# Example: bash parish/scripts/normalize-mod-source.sh mods/rundale

MOD_DIR="${1:-}"
if [[ -z "$MOD_DIR" ]]; then
    echo "Usage: $0 <mod-dir>" >&2
    exit 1
fi

if [[ ! -d "$MOD_DIR" ]]; then
    echo "ERROR: $MOD_DIR does not exist or is not a directory" >&2
    exit 1
fi

echo "WARNING: This will REWRITE all JSON files in: $MOD_DIR"
echo "  - npcs.json"
echo "  - world.json"
echo "  - festivals.json"
echo "  - encounters.json"
echo "  - anachronisms.json"
echo ""
read -r -p "Type the full path to confirm: " CONFIRM

if [[ "$CONFIRM" != "$MOD_DIR" ]]; then
    echo "Confirmation mismatch. Aborting."
    exit 1
fi

cd "$(git rev-parse --show-toplevel)"
export PARISH_NORMALIZE_MOD_DIR="$MOD_DIR"
cargo test -p parish-core -- normalize_mod_source_integration --ignored --nocapture
