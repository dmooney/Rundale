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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDERS_DIR="$SCRIPT_DIR/../crates/parish-config/providers"

usage() {
  cat <<'EOF'
Reset Parish/Rundale first-run onboarding.

Default: removes only the .onboarded sentinel so the BYOK wizard fires on
next launch but the saved provider/model and stored API key remain.

Flags:
  --config   Also remove parish.toml (forgets provider/model/base_url choice).
  --keys     Also delete OS keychain entries for every known provider.
  --all      Implies --config --keys.
  --dry-run  Print actions without performing them.
  -h | --help
EOF
}

remove_config=0
remove_keys=0
dry_run=0

for arg in "$@"; do
  case "$arg" in
    --config)  remove_config=1 ;;
    --keys)    remove_keys=1 ;;
    --all)     remove_config=1; remove_keys=1 ;;
    --dry-run) dry_run=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown flag: $arg" >&2; usage >&2; exit 1 ;;
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

# Read provider IDs from parish-config/providers/*.toml so the list stays in
# sync with the data-driven provider registry (#968).
load_provider_ids() {
  if [[ ! -d "$PROVIDERS_DIR" ]]; then
    echo "warn: provider registry not found at $PROVIDERS_DIR; using built-in fallback list." >&2
    printf '%s\n' \
      ollama lmstudio openrouter vllmmlx openai google groq xai mistral \
      deepseek together nvidia-nim anthropic custom simulator
    return
  fi
  local f
  for f in "$PROVIDERS_DIR"/*.toml; do
    [[ -e "$f" ]] || continue
    awk -F'"' '/^id = "/ { print $2; exit }' "$f"
  done
}

remove_path "$config_dir/.onboarded"

if (( remove_config )); then
  remove_path "$config_dir/parish.toml"
fi

if (( remove_keys )); then
  # Service must match parish-tauri/src/keychain.rs (`SERVICE` constant).
  service="com.parish.rundale"
  providers=()
  while IFS= read -r line; do
    [[ -n "$line" ]] && providers+=("$line")
  done < <(load_provider_ids)
  case "$(uname -s)" in
    Darwin)
      for p in "${providers[@]}"; do
        account="provider:$p"
        if security find-generic-password -s "$service" -a "$account" >/dev/null 2>&1; then
          run security delete-generic-password -s "$service" -a "$account"
        fi
      done
      ;;
    Linux)
      # keyring v3 secret-service backend stores BYOK keys under attributes
      # `service` + `username` (not `account`). See keyring-3.x docs:
      # https://docs.rs/keyring/3/keyring/#linux
      if command -v secret-tool >/dev/null 2>&1; then
        for p in "${providers[@]}"; do
          run secret-tool clear service "$service" username "provider:$p" || true
        done
      else
        echo "warn: secret-tool not installed; clear keys via your wallet UI." >&2
      fi
      ;;
    MINGW*|MSYS*|CYGWIN*)
      # keyring v3 windows-native backend names credentials `<user>.<service>`,
      # where user = `provider:<id>`. See keyring-3.x src/windows.rs.
      for p in "${providers[@]}"; do
        target="provider:$p.$service"
        run cmdkey "/delete:$target" || true
      done
      ;;
  esac
fi

echo "done. relaunch Parish to re-enter onboarding."
