#!/usr/bin/env bash
# Reset Parish/Rundale first-run onboarding.
#
# Default: removes only the `.onboarded` sentinel so the BYOK wizard fires on
# next launch but the saved provider/model and stored API key remain.
#
# Flags:
#   --config   Also remove parish.toml (forgets provider/model/base_url choice).
#   --keys     Also delete OS keychain entries for every known provider.
#   --all      Implies --config --keys.
#   --dry-run  Print actions without performing them.
#   -h | --help
set -euo pipefail

remove_config=0
remove_keys=0
dry_run=0

usage() {
  sed -n '2,12p' "$0"
  exit "${1:-0}"
}

for arg in "$@"; do
  case "$arg" in
    --config)  remove_config=1 ;;
    --keys)    remove_keys=1 ;;
    --all)     remove_config=1; remove_keys=1 ;;
    --dry-run) dry_run=1 ;;
    -h|--help) usage 0 ;;
    *) echo "unknown flag: $arg" >&2; usage 1 ;;
  esac
done

# Resolve user-config dir (matches parish-core::user_paths resolution order).
if [[ -n "${PARISH_USER_CONFIG_DIR:-}" ]]; then
  config_dir="$PARISH_USER_CONFIG_DIR"
else
  case "$(uname -s)" in
    Darwin) config_dir="$HOME/Library/Application Support/Parish" ;;
    Linux)  config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/parish" ;;
    MINGW*|MSYS*|CYGWIN*) config_dir="${APPDATA:-$HOME/AppData/Roaming}/Parish" ;;
    *) echo "unsupported platform: $(uname -s)" >&2; exit 2 ;;
  esac
fi

run() {
  if (( dry_run )); then
    echo "DRY: $*"
  else
    echo "+ $*"
    "$@"
  fi
}

remove_path() {
  local p="$1"
  if [[ -e "$p" ]]; then
    run rm -f "$p"
  else
    echo "skip (missing): $p"
  fi
}

remove_path "$config_dir/.onboarded"

if (( remove_config )); then
  remove_path "$config_dir/parish.toml"
fi

if (( remove_keys )); then
  # Service + accounts must match parish-tauri/src/keychain.rs and
  # parish-config/src/provider.rs::Provider::id().
  service="com.parish.rundale"
  providers=(
    ollama lmstudio openrouter vllmmlx openai google groq xai
    mistral deepseek together nvidia-nim anthropic custom simulator
  )
  case "$(uname -s)" in
    Darwin)
      for p in "${providers[@]}"; do
        account="provider:$p"
        if security find-generic-password -s "$service" -a "$account" >/dev/null 2>&1; then
          run security delete-generic-password -s "$service" -a "$account" >/dev/null
        fi
      done
      ;;
    Linux)
      if command -v secret-tool >/dev/null 2>&1; then
        for p in "${providers[@]}"; do
          run secret-tool clear service "$service" account "provider:$p" || true
        done
      else
        echo "warn: secret-tool not installed; clear keys via your wallet UI." >&2
      fi
      ;;
    MINGW*|MSYS*|CYGWIN*)
      for p in "${providers[@]}"; do
        target="$service/provider:$p"
        run cmdkey /delete:"$target" >/dev/null 2>&1 || true
      done
      ;;
  esac
fi

echo "done. relaunch Parish to re-enter onboarding."
