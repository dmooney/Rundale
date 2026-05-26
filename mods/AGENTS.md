# mods/ — agent scope

Root mod registry for the Parish engine. Contains game-world mods (`kind = "base"`) and provider-registration mods (`kind = "providers"`), each in its own subdirectory with a `mod.toml` manifest. The active mod is selected by `mod-list.toml`. See root [`AGENTS.md`](../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo test -p parish-core --test mod_loading       # schema validation + load round-trip
cargo run  -p parish-client -- --script ...        # live gameplay against active mod
parish --list-mods                                  # available mods (if exposed)
```

## Local gotchas

- **`mod-list.toml` controls the active mod.** `active_setting = "rundale"` selects the Rundale game world. Switching to a different `base` mod changes the world, NPC catalog, prompts, and save root.
- **Two mod kinds.** `kind = "base"` for game worlds (`rundale`, `testbed`); `kind = "providers"` for LLM provider registrations. The kind is set in `mod.toml` and distinguishes game content from provider configuration.
- **`save_root` controls per-user data directory resolution (rule #9).** The `save_root` field in each base mod's `mod.toml` becomes the app name for `parish_persistence::paths::resolve_user_data_dir()`. Existing saves live under `<save_root>` — changing it silently relocates saves. Provider mods omit `save_root`.
- **Provider mod naming convention.** Each provider is `<name>-provider/` containing a `mod.toml` manifest and a `providers/<name>.toml` config. The provider config defines `id`, `display_name`, `default_base_url`, `api_key_env_var`, `requires_api_key`, and `[[presets]]` entries with per-model-tier keys (`recommended`, `budget`, `mini`).
- **Provider configs follow OpenAI-compat schema.** Even Anthropic and other non-OpenAI providers use the same TOML layout with `kind = "anthropic"` or `kind = "openai-compat"`. The `featured` boolean gates visibility in the UI picker.
- **`mod.toml` schema is additive only.** Adding new fields is safe; renaming or removing existing fields breaks deserialisation for saves that store the schema. Base mod `mod.toml` carries `[mod]`, `[setting]`, `[files]`, and `[prompts]` sections; provider mods only carry `[mod]`.
- **Mod loading pipeline lives in `parish-core::loading`.** See `parish/crates/parish-core/src/loading/` for the `ModLoader`, schema validation, provider registry, and mod resolution logic. Adding a new mod kind requires changes in the loading pipeline.
- **`rundale/` has its own `AGENTS.md`** at `mods/rundale/AGENTS.md` with game-content-specific gotchas (coordinate rules, NPC catalogue size, prompt variable casing).

## What belongs here

**1 game mod:** `rundale/` — Irish living world, 1820. Kind = `base`. Full game content (world, NPCs, prompts, schedules, festivals, encounters, transport).

**1 test mod:** `testbed/` — minimal engine test harness (5-location grid). Kind = `base`. Used by integration tests; pig Latin code-switch for dialogue testing.

**20 provider mods:** `anthropic-provider/`, `cohere-provider/`, `deepseek-provider/`, `github_models-provider/`, `google-provider/`, `groq-provider/`, `lmstudio-provider/`, `mistral-provider/`, `moonshot-provider/`, `nvidia-nim-provider/`, `openai-provider/`, `openrouter-provider/`, `qwen-provider/`, `scaleway-provider/`, `siliconflow-provider/`, `together-provider/`, `vercel-ai-provider/`, `xai-provider/`, `zhipu-provider/`, and others. Each is `kind = "providers"` with a `mod.toml` manifest and `providers/*.toml` config file.

**Registry file:** `mod-list.toml` — selects the active mod setting via `active_setting`.
