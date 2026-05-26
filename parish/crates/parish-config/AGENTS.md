# parish-config — agent scope

Engine and LLM-provider configuration loader for the Parish engine. Backend-agnostic leaf crate — depends only on `parish-types`. Loads and validates `parish.toml`, manages provider configs (API keys, base URLs, model names), feature flags, per-user config, and built-in provider definitions. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-config                       # unit + integration
cargo test -p parish-config -- --nocapture        # with stdout (feature-flag persistence, env-serialised tests)
```

## Local gotchas

- **Leaf crate constraint.** `parish-config` may never depend on any non-leaf parish crate. Adding a dependency on `parish-core`, `parish-inference`, etc. violates the architecture-fitness test (rule #1).
- **Config merging order (4-layer priority).** Compiled default → TOML file → env var → CLI flag. Each layer overrides the previous. The `PARISH_OLLAMA_URL` env var is deprecated in favour of `PARISH_BASE_URL`.
- **Feature flags are default-on.** `config.flags.is_enabled("feature")` returns `false` for unknown flags (opt-in). `config.flags.is_disabled("feature")` returns `true` only when the flag has been explicitly set to `false` — use `is_disabled` for kill-switch features that ship enabled by default.
- **`Provider::recommended_for_platform()` is platform-aware.** macOS: vllm-mlx with two-slot Qwen loadout (needs >=16 GB unified memory; below that falls back to simulator). Linux/Windows: vllm. Cross-platform code that picks a provider at startup should use this rather than hardcoding an id.
- **`MapConfig::apply_defaults()` prevents tile-source wipe.** serde replaces the whole `BTreeMap` on deserialisation, so a partial `[engine.map.tile_sources.osm]` override in `parish.toml` silently drops the `historic` source. Always call `apply_defaults()` after loading to fold baked-in defaults back.
- **Provider registry is write-once after bootstrap.** Builtins (`simulator`, `ollama`, `vllm`, `vllmmlx`, `custom`) are compiled in via `include_str!` TOMLs. Cloud providers are loaded from `mods/<id>/providers/<id>.toml` via `register_mod_providers` after `discover_mods`. After bootstrap the registry is effectively read-only; existing `Arc<ProviderMod>` handles keep pointing at their snapshot.
- **`UserConfig` must never store an `api_key` field.** Secrets belong in the OS keychain (`parish_core::secret_store`). The `save_does_not_write_api_key_field` test guards against accidental inclusion.
- **`#[serial(provider_registry)]` and `#[serial(parish_env)]` test attributes.** Tests that mutate the global provider registry or environment variables are annotated with `serial_test` to prevent inter-test poisoning. Adding a new test of that kind must include the matching `#[serial(...)]`.

## Module map

`engine.rs` engine-level structs (inference, NPC, encounter, palette, world, map, session tuning), `user_config.rs` per-user BYOK prefs + onboarding marker, `provider.rs` `ProviderKind`/`Provider`/`ProviderRegistry` + 4-layer resolution, `flags.rs` runtime feature flags persistence, `builtin_providers/` five built-in provider TOMLs loaded at compile time.
