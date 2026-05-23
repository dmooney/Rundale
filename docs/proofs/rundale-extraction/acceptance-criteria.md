# Acceptance Criteria: rundale-extraction

## Overview

Rundale-specific content (Irish language text, UI palette settings, dialogue/prompt templates) is extracted from the engine core into mods. The engine carries no Rundale defaults and can host multiple base mods with distinct configuration, language, and NPC behavior.

## Acceptance Criteria

1. **ModKind::Base terminology**: All internal APIs and configuration files refer to "Base" mods, not "Setting" mods. The term "Setting" is reserved for historical-period configuration and is no longer overloaded.

   - `ModKind::Base` enum variant exists
   - `mods/rundale/mod.toml` and `mods/testbed/mod.toml` declare `kind = "base"`
   - `mods/mod-list.toml` references `active_base = "rundale"` or `active_base = "testbed"`
   - No `ModKind::Setting` or `active_setting` in code or toml

2. **Engine carries no Rundale defaults**: The same game script run under two different base mods (testbed, rundale) produces divergent output proving the engine adapted to each mod's configuration.

   - Running `play_rundale-extraction.txt` under `testbed` produces NPC dialogue with pig-Latin suffixes (`-ay`, `-way`)
   - Running the same fixture under `rundale` produces NPC dialogue with Hiberno-English markers (`ye`, `'tis`, `Dia dhuit`, `mo chara`)
   - Splash text, start location names reflect mod-specific data
   - `testbed` transcript matches `Irish|Roscommon|1820` zero times (engine correctly picks testbed content)

3. **UI palette extracted to mod**: The mod selector overlay and game UI colors are configurable per base mod.

   - `ModSelectorOverlay.svelte` component loads `base_mod_required: bool` from UI config
   - Colors, button styling, theme can vary per mod without engine code changes

4. **Base mod selection is enforced**: If no base mod is active, the UI enforces mod selection before gameplay.

   - On startup with no active base mod, `UiConfigSnapshot.base_mod_required = true`
   - The mod selector overlay is shown and cannot be dismissed (no X, no Escape, no Cancel)
   - After selecting a mod and confirming, UI shows "Restart the server, then reload the page"
   - All 6 UI config construction sites (Tauri init, server auth, server game-session, tests) propagate `base_mod_required` correctly

5. **All tests pass**: Full workspace test suite including new/modified unit and integration tests passes.

   - `cargo test --workspace` succeeds with all tests passing
   - `cargo clippy --all` clean with no warnings
   - Frontend tests pass (if applicable)

## Evidence Gathering

1. **Terminology verification**: Read commits ef736cb5 and dependencies, verify no `Setting` remains.
2. **Multi-mod fixture**: Run `parish/testing/fixtures/play_rundale-extraction.txt` twice:
   - Once with `active_base = "testbed"` in `mods/mod-list.toml` → `transcript-testbed.txt`
   - Once with `active_base = "rundale"` in `mods/mod-list.toml` → `transcript-rundale.txt`
   - Diff transcripts to show NPC language/behavior divergence
3. **UI forced selection**: Run the server/Tauri with no base mod selected, verify overlay is non-dismissable and forces selection.
4. **Test coverage**: Run full test suite to verify all criteria.

## Success Condition

- Multi-mod transcripts show clear divergence in NPC dialogue and location names
- Testbed transcript contains zero matches for "Irish", "Roscommon", or "1820"
- No "Setting" terminology remains in code or config files
- Full test suite passes
- UI shows enforced base mod selection when no mod is active
