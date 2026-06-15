# parish-world — agent scope

World graph, movement, weather, and environment state for the Parish engine. Backend-agnostic leaf crate. Owns the location graph, player movement, weather generation and effects, environmental descriptions, transport, travel encounters, wayfarers (travelling NPCs), and geographic coordinate resolution. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-world                       # unit tests
cargo test -p parish-world -- --nocapture        # with stdout (weather seeds, coordinate resolution)
```

Integration tests reference `../../testing/fixtures/...` and `../../../mods/rundale/...` — cwd = crate root.

## Local gotchas

- **Leaf-crate dependency rule (rule #1).** Depends only on `parish-types` and `parish-config`. Never depend on `parish-core` or any runtime crate (tauri, axum, engine).
- **World graph loads from `world.json` in the active mod.** Deserialised at session start; structural changes to graph types (nodes, edges, paths) must be reflected in `mods/rundale/world.json`.
- **Coordinate resolution has three tiers.** Absolute (lat/lon) first, then relative (`relative_to` parent offset), then graph-delta fallback (inferred from topology). Test each tier separately when adding a new location type.
- **Weather generation is deterministic from a seed.** Same seed + same time-of-day produces identical weather. Do not use `rand::thread_rng()` — the seed derives from the world clock. Tests asserting weather snapshots must fix the clock.
- **`strsim` fuzzy matching for location names.** Player input is resolved via Jaro-Winkler distance. The threshold is tuned in `graph/` — lower increases false positives, higher increases false negatives for colloquial names.
- **Movement edges may have transport restrictions.** Some edges require a specific transport type (horse, ferry); walking is the default fallback. New transport variants require updating both `transport.rs` and edge validation in `movement/`.
- **`description.rs` generates player-facing "look" output.** Composes static description, weather, time-of-day lighting, nearby NPCs (via `parish-npc`), and available exits. Changes here affect every entry point's `look` command.
- **Wayfarers are NPCs on roads, not at nodes.** `wayfarers.rs` manages transient encounters between settlements — they appear on edges during travel. Separate from the NPC tier system in `parish-npc`.
- **Encounters are random events during travel.** `encounter.rs` defines tables per-region and per-transport-type. Outcomes resolved by `parish-npc` and `parish-inference` — this crate only triggers and classifies them.

## Module map

`graph/` location graph (nodes, edges, paths), `movement/` player movement, `session.rs` world session state, `weather.rs` weather generation + state, `weather_travel.rs` weather effects on travel, `description.rs` location description prose, `transport.rs` transport type definitions, `encounter.rs` random travel encounters, `wayfarers.rs` travelling NPCs on roads, `geo.rs` geographic coordinate resolution.
