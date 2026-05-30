# mods/rundale/prompts — agent scope

Game dialogue prompt templates for the Rundale mod. Loaded by `parish-core::prompts` during mod loading — paths are declared in `mods/rundale/mod.toml` under `[prompts]`, and the engine reads the files at startup through `GameMod::load_prompts()`. See root [`AGENTS.md`](../../../AGENTS.md), [`mods/AGENTS.md`](../../AGENTS.md), and [`mods/rundale/AGENTS.md`](../AGENTS.md) for non-negotiable rules.

These are **plain-text Jinja-like templates** (not `.prompt.yml`), using `{variable}` syntax with case-sensitive names. Each variable is substituted by the NPC dialogue manager at runtime (`parish-npc::manager`).

## Scoped commands

```sh
cargo test -p parish-core                                # unit + arch fitness (prompt loading)
cargo test -p parish-npc                                 # NPC manager + anachronism tests
cargo run  -p parish-engine -- --script ...              # live gameplay with this mod's prompts
```

## Local gotchas

- **Variable names are case-sensitive.** `{player_name}` works, `{PlayerName}` does not. A misspelled or wrong-case variable passes through verbatim in the rendered prompt — the engine does not warn about unknown placeholders in plain-text templates.
- **Tier 1 NPCs get the full system + context prompt assembled by `parish-npc::manager`.** `tier1_system.txt` is the character definition; `tier1_context.txt` is appended with runtime scene data (location, time, weather, the player's last action). Tier 2 NPCs receive only the system prompt (`tier2_system.txt`) — no context injection.
- **`tier1_system.txt` expects a JSON metadata block** after `---` on every response. The format is `{"action": "...", "mood": "...", "internal_thought": "...", "language_hints": [...]}`. The dialogue manager parses this; malformed blocks will be rejected or silently dropped.
- **Keep prompt templates in sync with `parish-npc::manager`.** Adding a new variable to a template requires a corresponding substitution in `parish-npc::manager::build_tier1_system_prompt` and friends. Adding a new tier requires a new `[prompts]` entry in `mod.toml` and loading logic in `parish-core::loading::game_mod`.
- **The anachronism subsystem (`parish-npc::anachronism`) watches NPC dialogue output.** If a generated response contains a term from `anachronisms.json`, the system flags it and injects a corrective context alert into the next prompt. Editing `tier1_system.txt` to add new period language guidance should be cross-checked against the anachronism word list.
- **Testbed mod (`mods/testbed/prompts/`) demonstrates the mod override pattern.** Any base mod can supply its own `prompts/` directory with the same filenames; the engine loads whichever the active mod provides. Testbed uses pig Latin versions to make the override visible in tests.
- **These are plain-text files, not `.prompt.yml`.** The engine reads them via `std::fs::read_to_string()` (not `include_str!`), so they can vary per mod without recompilation. This means a missing file at startup produces a runtime error (`PromptsLoadError`), not a compile-time panic.
- **The `{improv_section}` variable** in `tier1_system.txt` is pre-rendered by the NPC manager and may be empty — the template author must account for that (it is already appended with no conditional guard in the template).

## Prompt index

Three template files, all declared in `mods/rundale/mod.toml`:

### `tier1_system.txt` — Tier 1 NPC system prompt

Character definition for high-fidelity LLM-driven NPCs. Contains the role assignment (`name`, `age`, `occupation`), world-fact grounding (1820 Roscommon history), cultural and language guidelines (Hiberno-English dialect, permitted Irish phrases, banned anachronisms), personality injection (`personality`, `intel_guidance`, `tone_guidance`, `mood`), and output format specification (dialogue + JSON metadata block).

**Variables:** `{name}`, `{age}`, `{occupation}`, `{personality}`, `{intel_guidance}`, `{tone_guidance}`, `{mood}`, `{improv_section}`.

### `tier1_context.txt` — Tier 1 NPC scene context

Runtime context injected into the Tier 1 prompt after the system message. Provides the NPC with awareness of their current location, the time of day, season, weather, and whatever the player just said or did. This is the only prompt template that includes player-action context.

**Variables:** `{location_name}`, `{location_description}`, `{time}`, `{season}`, `{weather}`, `{scene_context}`, `{player_action}`.

### `tier2_system.txt` — Tier 2 NPC system prompt

System prompt for medium-fidelity NPC background conversations. Instructs the model to generate a brief summary of ambient character interactions (1-2 sentences) plus structured JSON output for mood and relationship changes between background NPCs. Does not receive player-action context.

**Variables:** `{location}`, `{time}`, `{weather}`, `{characters}`.
