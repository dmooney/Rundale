# Plan: Engine / Game Data Separation

## Context

Parish is a living-world text adventure set in 1820 rural Ireland. The engine (world graph, NPC cognition tiers, time system, LLM inference, persistence) is largely generic and reusable, but game-specific content (Irish place names, 1820 historical context, anachronism dictionary, festivals, loading phrases, system prompts) is hardcoded throughout the Rust source. The goal is to separate these cleanly — like Factorio's engine vs. base-game mod — so the engine knows nothing about Ireland or 1820, and all setting-specific content lives in a loadable data package ("mod").

## Current State Assessment

### Already well-separated (engine-quality):
- World graph data structure + BFS pathfinding (`crates/parish-core/src/world/graph.rs`)
- Time system mechanics: GameClock, TimeOfDay, Season, GameSpeed (`world/time.rs` — except festivals)
- Movement resolution (`world/movement.rs`)
- Description template rendering (`world/description.rs`)
- Color palette interpolation (`world/palette.rs`)
- NPC types, manager, memory, cognitive tiers (`npc/types.rs`, `manager.rs`, `memory.rs`)
- Input parsing + intent classification (`input/mod.rs`)
- Config/provider resolution (`config.rs`)
- Inference pipeline — LLM client, Ollama bootstrap, GPU detection (`inference/`)
- Persistence layer (`persistence/`)
- All Tauri IPC commands + Svelte UI components (fully generic)

### Already externalized as data files:
- `data/parish.json` — 15 locations with names, description templates, connections, mythological significance
- `data/npcs.json` — 8 NPCs with names, personalities, schedules, relationships, knowledge

### Hardcoded game content that needs extraction:

| Content | Location | Lines |
|---------|----------|-------|
| Tier 1 system prompt (1820, County Roscommon, Acts of Union, Catholic Emancipation, cultural guidelines, Irish language instructions) | `crates/parish-core/src/npc/mod.rs` | 336-391 |
| Tier 2 system prompt ("Irish parish in 1820") | `crates/parish-core/src/npc/ticks.rs` | 159-186 |
| Anachronism dictionary (~60+ terms with origin years) | `src/npc/anachronism.rs` | 69-441 |
| Irish festival definitions (Imbolc, Bealtaine, Lughnasa, Samhain) | `crates/parish-core/src/world/time.rs` | 89-127 |
| Loading phrases (24 Irish-themed strings) | `crates/parish-core/src/loading.rs` | 19-44 |
| Spinner frames (Celtic crosses) | `crates/parish-core/src/loading.rs` | 10 |
| Spinner colors (Irish palette) | `crates/parish-core/src/loading.rs` | 47-54 |
| Encounter flavor text (rural Irish encounters) | `crates/parish-core/src/world/encounter.rs` | 40-52 |
| Start date (1820-03-20 08:00) | `crates/parish-core/src/world/mod.rs` | 120, 159 |
| Default location ("The Crossroads" with Irish description) | `crates/parish-core/src/world/mod.rs` | 104-115 |
| Test NPC ("Padraig O'Brien") | `crates/parish-core/src/npc/mod.rs` | 142-168 |
| `IrishWordHint` struct name | `crates/parish-core/src/npc/mod.rs` | 22-35 |
| "Focail (Irish Words)" UI label | `ui/src/components/Sidebar.svelte` |
| geo_tool (entire binary is Ireland-specific) | `src/bin/geo_tool/` |

## Recommended Approach

### Mod Data Package Structure

A mod is a directory with a `mod.toml` manifest and data files:

```
mods/
└── rundale/
    ├── mod.toml                # Manifest: name, version, start_date, start_location, etc.
    ├── world.json              # Locations, connections (currently data/parish.json)
    ├── npcs.json               # NPC definitions (currently data/npcs.json)
    ├── prompts/
    │   ├── tier1_system.txt    # Tier 1 system prompt template with {name}, {age}, etc.
    │   ├── tier1_context.txt   # Tier 1 context template
    │   └── tier2_system.txt    # Tier 2 background simulation prompt template
    ├── anachronisms.json       # Period enforcement dictionary
    ├── festivals.json          # Calendar events [{name, month, day, description}]
    ├── encounters.json         # Encounter flavor text keyed by time-of-day
    ├── loading.toml            # Loading phrases, spinner frames, spinner colors
    └── ui.toml                 # UI customization: sidebar labels, hint field name, theme colors
```

### `mod.toml` Manifest

```toml
[mod]
name = "Parish: 1820 Ireland"
id = "rundale"
version = "1.0.0"
description = "A small parish in County Roscommon, Ireland, in the year 1820"

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820          # Used by anachronism checker as cutoff

[files]
world = "world.json"
npcs = "npcs.json"
anachronisms = "anachronisms.json"
festivals = "festivals.json"
encounters = "encounters.json"
loading = "loading.toml"
ui = "ui.toml"

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
```

### Engine-Side Changes

#### 1. New `GameMod` struct in parish-core

A `GameMod` struct that loads and holds all mod data. This replaces scattered hardcoded content with a single loaded data source.

```rust
// crates/parish-core/src/game_mod.rs
pub struct GameMod {
    pub manifest: ModManifest,
    pub prompt_templates: PromptTemplates,
    pub anachronisms: Vec<AnachronismEntry>,  // generic, not "Irish"
    pub festivals: Vec<FestivalDef>,
    pub encounters: EncounterTable,
    pub loading: LoadingConfig,
    pub ui: UiConfig,
}
```

Key design decisions:
- **Load at startup, immutable thereafter** — mods are read once and passed by reference
- **No trait-based plugin system** — too complex for this stage. A data-driven approach (JSON/TOML files + prompt templates) gives 95% of the benefit
- **Prompt templates use simple `{placeholder}` interpolation** — same pattern already used in description templates

#### 2. Generalize hardcoded Festival enum → data-driven

Replace the `Festival` enum with a data-driven list loaded from `festivals.json`:

```rust
pub struct FestivalDef {
    pub name: String,
    pub month: u32,
    pub day: u32,
    pub description: Option<String>,
}
```

`GameClock::festival()` changes from pattern-matching an enum to checking the loaded festival list.

#### 3. Generalize anachronism system → "period enforcement"

- Rename `Anachronism*` types to `PeriodViolation*` (or keep the name, it's descriptive)
- Move the dictionary from a `const` array to a loaded JSON file
- The cutoff year comes from `mod.toml` `period_year` field
- The `AnachronismCategory` enum stays in the engine (Technology/Slang/Concept/Material/Measurement are generic categories)
- The checker function takes `&[AnachronismEntry]` instead of using a static

#### 4. Extract system prompts to template files

The Tier 1 system prompt (`build_tier1_system_prompt`) becomes:

```
You are {name}, a {age}-year-old {occupation} in {setting_description}.

{historical_context}

{cultural_guidelines}

Personality: {personality}

Current mood: {mood}

{response_format_instructions}
```

The engine provides `{name}`, `{age}`, `{occupation}`, `{personality}`, `{mood}` from NPC data. The mod provides `{setting_description}`, `{historical_context}`, `{cultural_guidelines}`, and `{response_format_instructions}` (or a default).

The `IrishWordHint` struct becomes `LanguageHint` — the concept of "NPCs use a secondary language with pronunciation guides" is engine-generic. The mod's prompt template instructs the LLM to produce these hints.

#### 5. Extract encounter text and loading config

- Encounters: `check_encounter()` takes an `&EncounterTable` parameter instead of using hardcoded strings
- Loading: `LoadingAnimation::new()` takes `&LoadingConfig` with phrases, spinner frames, colors

#### 6. WorldState initialization takes mod config

`WorldState::from_mod()` replaces both `WorldState::new()` and `WorldState::from_parish_file()`:
- Start date from `mod.toml`
- World graph from `world.json`
- Start location from `mod.toml`

#### 7. UI customization

`ui.toml` provides:
```toml
[sidebar]
hints_label = "Focail (Irish Words)"
hints_field = "language_hints"

[theme]
default_accent = "#c4a35a"
```

Passed to the frontend via a new IPC command `get_ui_config()`.

### What Stays in the Engine (Unchanged)

- World graph algorithms, BFS, fuzzy matching
- GameClock mechanics (tick, pause, speed)
- TimeOfDay, Season enums (generic)
- Weather enum (generic)
- Movement resolution
- Description template rendering
- NPC cognitive tier system
- NPC memory ring buffer
- NPC manager + tier assignment
- Input parsing + intent classification
- Config/provider resolution
- Inference pipeline (LLM client)
- Persistence layer
- All Tauri IPC commands
- All Svelte UI components (they already use generic props)

### What Moves to the Mod

- `data/parish.json` → `mods/rundale/world.json`
- `data/npcs.json` → `mods/rundale/npcs.json`
- System prompt text → `mods/rundale/prompts/`
- Anachronism dictionary → `mods/rundale/anachronisms.json`
- Festival definitions → `mods/rundale/festivals.json`
- Encounter flavor text → `mods/rundale/encounters.json`
- Loading phrases/colors/spinners → `mods/rundale/loading.toml`
- UI labels → `mods/rundale/ui.toml`
- `geo_tool` stays as a separate binary (it's a development tool for generating mod content, not part of the engine or mod runtime)

### Migration Path

**Phase 1: Define mod structure + GameMod loader**
- Create `mods/rundale/` directory with `mod.toml`
- Add `GameMod` struct and loader to `parish-core`
- Move `data/*.json` to the mod directory
- No behavior changes yet — just loading from new paths

**Phase 2: Extract prompt templates**
- Move system prompt strings to template files
- Rename `IrishWordHint` → `LanguageHint`
- `build_tier1_system_prompt()` reads template from `GameMod`
- `build_tier2_prompt()` reads template from `GameMod`

**Phase 3: Extract hardcoded data**
- Festivals: enum → data-driven from `festivals.json`
- Anachronisms: static dict → loaded from `anachronisms.json`
- Encounters: hardcoded text → loaded from `encounters.json`
- Loading: hardcoded phrases/colors → loaded from `loading.toml`

**Phase 4: Wire through WorldState + App**
- `WorldState` constructor takes `&GameMod`
- `App` holds `GameMod` and passes references where needed
- Start date + start location from mod manifest
- Remove `WorldState::new()` fallback (or make it load a built-in default mod)

**Phase 5: UI customization**
- Add `get_ui_config` IPC command
- Frontend reads labels/theme from config instead of hardcoding
- Rename "Focail" sidebar label to come from mod config

## Files to Modify

### New files:
- `mods/rundale/mod.toml`
- `mods/rundale/prompts/tier1_system.txt`
- `mods/rundale/prompts/tier1_context.txt`
- `mods/rundale/prompts/tier2_system.txt`
- `mods/rundale/anachronisms.json`
- `mods/rundale/festivals.json`
- `mods/rundale/encounters.json`
- `mods/rundale/loading.toml`
- `mods/rundale/ui.toml`
- `crates/parish-core/src/game_mod.rs` — GameMod struct + loader

### Move:
- `data/parish.json` → `mods/rundale/world.json`
- `data/npcs.json` → `mods/rundale/npcs.json`

### Modify:
- `crates/parish-core/src/lib.rs` — add `game_mod` module
- `crates/parish-core/src/npc/mod.rs` — `build_tier1_system_prompt()` uses template, rename `IrishWordHint` → `LanguageHint`
- `crates/parish-core/src/npc/ticks.rs` — `build_tier2_prompt()` uses template
- `crates/parish-core/src/world/time.rs` — `Festival` enum → data-driven `FestivalDef`
- `crates/parish-core/src/world/encounter.rs` — parameterize encounter text
- `crates/parish-core/src/world/mod.rs` — `WorldState` constructor takes mod config
- `crates/parish-core/src/loading.rs` — `LoadingAnimation` takes `LoadingConfig`
- `src/npc/anachronism.rs` — load dictionary from file, cutoff year from config
- `src/app.rs` — `App` holds `GameMod`
- `src/main.rs` — `--mod` CLI flag to select mod directory
- `src-tauri/src/commands.rs` — add `get_ui_config` command
- `ui/src/components/Sidebar.svelte` — read label from config
- Various test files — update to load mod or use test mod fixtures

## Verification

1. `cargo build` — ensure all crates compile
2. `cargo test` — all existing tests pass (may need test mod fixture)
3. `cargo clippy -- -D warnings` — no warnings
4. `cargo run -- --mod mods/rundale --script tests/fixtures/test_walkthrough.txt` — game runs identically to before
5. Confirm no Ireland/1820-specific strings remain in `crates/parish-core/src/` (grep test)
6. Confirm a hypothetical empty mod with minimal `mod.toml` loads without panic (engine doesn't assume Irish content)
