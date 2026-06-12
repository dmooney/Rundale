# mods/rundale/prompts — agent scope

Game dialogue prompt templates for the Rundale mod. Loaded by `parish-core::prompts`; paths declared in `mods/rundale/mod.toml` under `[prompts]`, read at startup via `GameMod::load_prompts()`. See root [`AGENTS.md`](../../../AGENTS.md), [`mods/AGENTS.md`](../../AGENTS.md), and [`mods/rundale/AGENTS.md`](../AGENTS.md) for non-negotiable rules.

**Plain-text Jinja-like templates** (not `.prompt.yml`), `{variable}` syntax, case-sensitive names. Variables substituted by `parish-npc::manager` at runtime.

## Scoped commands

```sh
cargo test -p parish-core                                # unit + arch fitness (prompt loading)
cargo test -p parish-npc                                 # NPC manager + anachronism tests
cargo run  -p parish-engine -- --script ...              # live gameplay with this mod's prompts
```

## Local gotchas

- **Variable names are case-sensitive.** `{player_name}` works; `{PlayerName}` does not. Misspelled variables pass through verbatim — the engine does not warn about unknown placeholders.
- **Tier 1 NPCs receive both prompts.** `tier1_system.txt` is the character definition; `tier1_context.txt` is appended with runtime scene data (location, time, weather, player action). Tier 2 NPCs receive only `tier2_system.txt` — no context injection.
- **`tier1_system.txt` expects a JSON metadata block** after `---` on every response: `{"action": "...", "mood": "...", "internal_thought": "...", "language_hints": [...]}`. Malformed blocks are rejected or silently dropped.
- **Adding a variable requires a matching substitution** in `parish-npc::manager::build_tier1_system_prompt`. Adding a tier requires a new `[prompts]` entry in `mod.toml` and loading logic in `parish-core::loading::game_mod`.
- **Anachronism subsystem (`parish-npc::anachronism`) watches output.** Terms from `anachronisms.json` trigger a corrective context alert into the next prompt. Cross-check the word list when editing period-language guidance in `tier1_system.txt`.
- **Testbed mod (`mods/testbed/prompts/`) demonstrates the override pattern** — pig Latin versions make the override visible in tests.
- **Missing files produce `PromptsLoadError` at startup** (read via `std::fs::read_to_string`, not `include_str!`), not a compile-time panic.
- **`{improv_section}` in `tier1_system.txt`** is pre-rendered by the NPC manager and may be empty; the template appends it with no conditional guard.

## Prompt index

Three template files, all declared in `mods/rundale/mod.toml`:

### `tier1_system.txt` — Tier 1 NPC system prompt

Character definition for high-fidelity LLM-driven NPCs: role assignment, world-fact grounding (1820 Roscommon), Hiberno-English dialect guidelines, personality injection, and output format (dialogue + JSON metadata block).

**Variables:** `{name}`, `{age}`, `{occupation}`, `{personality}`, `{intel_guidance}`, `{tone_guidance}`, `{mood}`, `{improv_section}`.

### `tier1_context.txt` — Tier 1 NPC scene context

Runtime context appended after the system message. The only template that includes player-action context (location, time, season, weather, player action).

**Variables:** `{location_name}`, `{location_description}`, `{time}`, `{season}`, `{weather}`, `{scene_context}`, `{player_action}`.

### `tier2_system.txt` — Tier 2 NPC system prompt

Medium-fidelity background NPC prompt. Generates a 1-2 sentence ambient summary plus structured JSON for mood/relationship changes. No player-action context.

**Variables:** `{location}`, `{time}`, `{weather}`, `{characters}`.
