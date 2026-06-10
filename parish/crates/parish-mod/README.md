# parish-mod

Content-mod loader for the Parish engine. A "mod" is a directory containing a
`mod.toml` manifest plus data files (world graph, NPCs, encounters, prompts, UI
theme, pronunciations, transport, reactions). The engine loads a `GameMod` at
startup and queries it for all game-specific content at runtime.

This crate was extracted from `parish-core/src/game_mod/` so the loader has a
single, backend-agnostic owner. `parish-core` re-exports it as
`parish_core::game_mod`, so every historical consumer path keeps compiling
unchanged.

## Surface

- `GameMod` / `GameMod::load` — load + validate a mod directory.
- `manifest` — `ModManifest`, `ModMeta`, `ModKind`, `SettingConfig`,
  `FileRefs`, `PromptRefs`.
- `types` — runtime data: `PromptTemplates`, `AnachronismData`, `FestivalDef`,
  `EncounterTable`, `LoadingConfig`, `UiConfig`, `ThemeConfig`,
  `ThemePaletteConfig`, `PronunciationEntry`, plus `default_theme_palette()`.
- `discovery` — `discover_mods`, `discover_mods_in`, `find_default_mod`,
  `find_mods_root`, `DiscoveredMods`.
- `world::world_state_from_mod` — bridge to `parish_world::WorldState`.
- `register_provider_mods_once` / `load_providers_from_mod` — LLM provider
  catalog mods.
- `app_name_from_mod` — per-user data-folder resolver (saves + tile cache).

## Dependencies

Leaf crates only — `parish-types`, `parish-config`, `parish-world`,
`parish-palette`, `parish-npc`, `parish-persistence`. No runtime
(`tauri`/`axum`) deps; enforced by the `BACKEND_AGNOSTIC` list in
`parish-core/tests/architecture_fitness.rs`.

The `ThemePalette` type lives in `parish-types` (the zero-dep leaf); the
`From<RawPalette>` hex conversion lives in `parish-palette`.

## Scoped commands

```sh
cargo build -p parish-mod
cargo test  -p parish-mod
```
