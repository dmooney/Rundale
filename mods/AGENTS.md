# mods/ — agent scope

Root mod registry for the Limerick engine. Contains game-world mods (`kind = "base"`) and provider-registration mods (`kind = "providers"`), each in its own subdirectory with a `mod.toml` manifest. The active mod is selected by `mod-list.toml`. See root [`AGENTS.md`](../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo test -p limerick-mod                            # manifest, discovery, and load
cargo test -p limerick-core --test mod_artefact_malformed_input  # malformed mod files fail cleanly
cargo run  -p limerick-client -- --script ...        # live gameplay against active mod
```

## Local gotchas

- **`mod-list.toml` controls the active mod.** `active_setting = "rundale"` selects Rundale. Switching a `base` mod changes the world, NPC catalog, prompts, and save root.
- **Two mod kinds.** `kind = "base"` for game worlds (`rundale`, `testbed`); `kind = "providers"` for LLM provider registrations.
- **`save_root` controls per-user data directory resolution ([runtime paths](../docs/agent/persistence-and-session-rules.md#runtime-paths)).** The `save_root` field in each base mod's `mod.toml` becomes the app name for `limerick_persistence::paths::resolve_user_data_dir()`. Changing it silently relocates existing saves. Provider mods omit `save_root`.
- **Provider mod naming convention.** Each provider is `<name>-provider/` with a `mod.toml` and a `providers/<name>.toml` config defining `id`, `display_name`, `default_base_url`, `api_key_env_var`, `requires_api_key`, and `[[presets]]` with per-model-tier keys (`recommended`, `budget`, `mini`).
- **Provider configs follow OpenAI-compat schema.** Non-OpenAI providers use `kind = "anthropic"` or `kind = "openai-compat"`. The `featured` boolean gates visibility in the UI picker.
- **`mod.toml` schema is additive only.** Adding fields is safe; renaming or removing existing fields breaks deserialization for saves that store the schema.
- **Mod loading pipeline lives in `limerick-core/src/loading.rs`.** Adding a new mod kind requires changes there.
- **`rundale/` has its own `AGENTS.md`** at `mods/rundale/AGENTS.md` with game-content-specific gotchas (coordinate rules, NPC catalogue size, prompt variable casing).

## What belongs here

**1 game mod:** `rundale/` — Kilteevan, 1820: the canonical tiny world (product spec §14), three locations and three NPCs. Kind = `base`. `world-sheet.txt` is a test oracle checked by `limerick-engine/tests/world_sheet.rs`; change the sheet with any intentional world change. The earlier large world is test data at `limerick/testing/fixtures/mods/rundale-legacy/`, used by `GameTestHarness::new()` and the `test_*.txt` fixtures.

**1 test mod:** `testbed/` — minimal engine test harness (5-location grid). Kind = `base`. Used by integration tests; pig Latin code-switch for dialogue testing.

**Provider mods:** every `mods/*-provider/` directory. Each is `kind = "providers"` with a `mod.toml` and `providers/*.toml` config.

**Registry file:** `mod-list.toml` — selects the active mod setting via `active_setting`.
