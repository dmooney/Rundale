# parish-mod — agent scope

Backend-agnostic leaf crate that owns the content-mod loader: `mod.toml` manifest parsing, mod discovery, all runtime data types loaded from JSON/TOML, the `app_name_from_mod` resolver, and the `world_state_from_mod` bridge from a loaded mod to `parish_world::WorldState`. Re-exported by `parish-core` as `parish_core::game_mod`. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-mod                    # unit tests (manifest, discovery, assets, types)
cargo test -p parish-mod -- --nocapture     # with stdout for debugging
```

## Gotchas

- **Leaf-crate dependency rule (rule #1).** Depends only on `parish-types`, `parish-config`, `parish-world`, `parish-palette`, `parish-npc`, `parish-persistence`. Never depend on `parish-core`, `tauri`, `axum`, or any entry-point crate.
- **Use `app_name_from_mod`, not `ModMeta::app_name` directly (rule #9).** `ModMeta::app_name()` returns the raw manifest string without path sanitisation. `app_name_from_mod(&Option<GameMod>)` in `lib.rs` applies basename stripping and traversal-guard so a `save_root = "../../etc"` can't redirect saves outside the per-user root.
- **`find_mods_root` cwd-walk is a dev fallback only.** Production builds must set `PARISH_MODS_DIR`; the cwd-walk is forbidden in packaged or daemonised contexts (rule #9). Use `discover_mods_in(explicit_path)` in tests.
- **Directory-traversal guard on all file loads.** `GameMod::load` canonicalises every path and checks it still starts with `mod_dir` before reading. Asset paths must also live under `assets/` (enforced in `assets.rs`). Malicious `mod.toml` entries are rejected with `ParishError::Config` (#741).
- **`register_provider_mods_once` is idempotent via `OnceLock`.** Safe to call from both the Tauri-sync path and a later server reload; second call returns `Ok(0)`. Provider TOML ids must be unique within a mod — duplicates are a fatal error at startup.
- **Dependency declarations are parsed but not enforced.** `ModMeta.dependencies`, `optional_dependencies`, and `conflicts` are stored but the resolver is not yet implemented.

## Module map

`lib.rs` — `GameMod` struct + `GameMod::load` + `app_name_from_mod` + `register_provider_mods_once` + `load_providers_from_mod`; `manifest.rs` — `ModManifest`, `ModMeta`, `ModKind`, `SettingConfig`, `FileRefs`, `PromptRefs` parsed from `mod.toml`; `types.rs` — runtime data types: `PromptTemplates`, `AnachronismData`, `FestivalDef`, `EncounterTable`, `LoadingConfig`, `UiConfig`/`ThemeConfig`/`BrandingConfig`, `PronunciationEntry` + palette conversion helpers; `discovery.rs` — `discover_mods`, `discover_mods_in`, `find_mods_root`, `find_default_mod`, `DiscoveredMods`/`DiscoveredMod`; `world.rs` — `world_state_from_mod` bridge to `parish_world::WorldState::from_mod_params`; `assets.rs` — `canonical_mod_asset_path` + `validate_optional_asset_ref` path-safety helpers (crate-internal).
