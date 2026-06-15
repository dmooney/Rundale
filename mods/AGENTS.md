# mods/ — agent scope

Root mod registry for the Parish engine. Contains game-world mods (`kind = "base"`) and provider-registration mods (`kind = "providers"`), each in its own subdirectory with a `mod.toml` manifest. The active mod is selected by `mod-list.toml`. See root [`AGENTS.md`](../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo test -p parish-core --test mod_loading       # schema validation + load round-trip
cargo run  -p parish-client -- --script ...        # live gameplay against active mod
parish --list-mods                                  # available mods (if exposed)
```

## Local gotchas

- **`mod-list.toml` controls the active mod.** `active_setting = "rundale"` selects Rundale. Switching a `base` mod changes the world, NPC catalog, prompts, and save root.
- **Two mod kinds.** `kind = "base"` for game worlds (`rundale`, `testbed`); `kind = "providers"` for LLM provider registrations.
- **`save_root` controls per-user data directory resolution (rule #9).** The `save_root` field in each base mod's `mod.toml` becomes the app name for `parish_persistence::paths::resolve_user_data_dir()`. Changing it silently relocates existing saves. Provider mods omit `save_root`.
- **Provider mod naming convention.** Each provider is `<name>-provider/` with a `mod.toml` and a `providers/<name>.toml` config defining `id`, `display_name`, `default_base_url`, `api_key_env_var`, `requires_api_key`, and `[[presets]]` with per-model-tier keys (`recommended`, `budget`, `mini`).
- **Provider configs follow OpenAI-compat schema.** Non-OpenAI providers use `kind = "anthropic"` or `kind = "openai-compat"`. The `featured` boolean gates visibility in the UI picker.
- **`mod.toml` schema is additive only.** Adding fields is safe; renaming or removing existing fields breaks deserialization for saves that store the schema.
- **Mod loading pipeline lives in `parish-core/src/loading.rs`.** Adding a new mod kind requires changes there.
- **`rundale/` has its own `AGENTS.md`** at `mods/rundale/AGENTS.md` with game-content-specific gotchas (coordinate rules, NPC catalogue size, prompt variable casing).

## What belongs here

**1 game mod:** `rundale/` — Irish living world, 1820. Kind = `base`. Full game content (world, NPCs, prompts, schedules, festivals, encounters, transport).

**1 test mod:** `testbed/` — minimal engine test harness (5-location grid). Kind = `base`. Used by integration tests; pig Latin code-switch for dialogue testing.

**21 provider mods:** `anthropic-provider/`, `cohere-provider/`, `deepseek-provider/`, `github_models-provider/`, `google-provider/`, `groq-provider/`, `lmstudio-provider/`, `mistral-provider/`, `moonshot-provider/`, `nvidia-nim-provider/`, `openai-provider/`, `opencode-provider/`, `openrouter-provider/`, `qwen-provider/`, `scaleway-provider/`, `siliconflow-provider/`, `together-provider/`, `vercel-ai-provider/`, `xai-provider/`, `zhipu-provider/`, and others. Each is `kind = "providers"` with a `mod.toml` and `providers/*.toml` config.

**Registry file:** `mod-list.toml` — selects the active mod setting via `active_setting`.
