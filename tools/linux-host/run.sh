#!/usr/bin/env bash
# Builds the engine's Linux static library and the Linux host, then plays the
# app's game as Markdown frames (README.md). `--test` runs the golden-frame
# tests instead; any other arguments go to the host.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
package="$root/tools/linux-host"

target_dir="$(cd "$root/limerick" && cargo metadata --format-version 1 --no-deps \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
(cd "$root/limerick" && cargo build --quiet -p limerick-mobile-ffi)
export RUNDALE_FFI_LIB_DIR="$target_dir/debug"

if [[ "${1:-}" == "--test" ]]; then
    shift
    exec swift test --package-path "$package" -Xswiftc -warnings-as-errors "$@"
fi

swift build --package-path "$package" --quiet
defaults=()
[[ " $* " == *" --mod-dir "* ]] || defaults+=(--mod-dir "$root/mods/rundale")
[[ " $* " == *" --save-dir "* ]] || defaults+=(--save-dir "$package/.build/save")
exec "$package/.build/debug/rundale-linux-host" "${defaults[@]}" "$@"
