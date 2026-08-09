# parish-config — agent scope

Backend-agnostic leaf crate — depends only on `parish-types`. Loads and validates `parish.toml`, manages provider configs, feature flags, per-user config, and built-in provider definitions. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-config                       # unit + integration
cargo test -p parish-config -- --nocapture        # with stdout (feature-flag persistence, env-serialised tests)
```

## Local gotchas

- **Leaf crate constraint (enforced, rule #1).** Never depend on `parish-core`, `parish-inference`, or any non-leaf crate.
- **Config merging order: 4 layers.** Compiled default → TOML file → env var → CLI flag. `PARISH_OLLAMA_URL` is deprecated; use `PARISH_BASE_URL`.
- **Feature flag semantics.** `config.flags.is_enabled("feature")` returns `false` for unknown flags. Use `is_disabled` for kill-switch features that ship enabled by default — it returns `true` only when the flag is explicitly `false`.
- **Backend availability is not dialogue qualification.** `Provider::recommended_for_platform()` chooses a runnable backend (macOS vllm-mlx above the memory floor; Linux/Windows vLLM). It does not certify prose quality. Only profiles in `local_dialogue::QUALIFIED_LOCAL_DIALOGUE_PROFILES`, backed by a passing promotion receipt, may be labeled qualified; the registry is currently empty.
- **`MapConfig::apply_defaults()` is mandatory after loading.** serde replaces the whole `BTreeMap` on deserialisation, so a partial `[engine.map.tile_sources.osm]` entry in `parish.toml` silently drops the `historic` source. Always call `apply_defaults()` to fold baked-in defaults back.
- **Provider registry is write-once after bootstrap.** Builtins (`simulator`, `ollama`, `vllm`, `vllmmlx`, `custom`) are compiled in via `include_str!` TOMLs in `builtin_providers/`. Cloud providers load from `mods/<id>/providers/<id>.toml` via `register_mod_providers` after `discover_mods`. Existing `Arc<ProviderMod>` handles retain their snapshot.
- **`UserConfig` must never store an `api_key` field.** Secrets belong in the OS keychain (`parish_core::secret_store`). `save_does_not_write_api_key_field` test guards this.
- **Serial test attributes required.** Tests that mutate the global provider registry or env vars must carry `#[serial(provider_registry)]` or `#[serial(parish_env)]` to prevent inter-test poisoning.

## Module map

`engine/` engine-level structs (inference, NPC, encounter, palette, world, map, session tuning), `user_config.rs` per-user BYOK prefs + onboarding marker, `provider/` `ProviderKind`/`Provider`/`ProviderRegistry` + 4-layer resolution, `flags.rs` runtime feature flags persistence, `builtin_providers/` five built-in provider TOMLs loaded at compile time.
