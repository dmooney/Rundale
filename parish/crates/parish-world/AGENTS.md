# parish-world — agent scope

World graph, movement, weather, and environment state for the Parish engine. Backend-agnostic leaf crate. Owns the location graph (nodes + edges), player movement through the world, weather generation and effects, environmental descriptions, transport between locations, encounters during travel, wayfarers (travelling NPCs), and geographic coordinate resolution. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-world                       # unit tests
cargo test -p parish-world -- --nocapture        # with stdout (weather seeds, coordinate resolution)
```

Integration tests reference `../../testing/fixtures/...` and `../../../mods/rundale/...` — cwd = crate root.

## Local gotchas

- **Leaf-crate dependency rule.** Depends only on `parish-types` and `parish-config`. Never take a dependency on `parish-core` or any runtime crate (tauri, axum, engine). Adding a dep on a non-leaf crate violates the architecture-fitness test (rule #1).
- **World graph is loaded from `world.json` in the active mod's directory.** The graph is deserialised at session start; any structural change to graph types (nodes, edges, paths) must be reflected in mod fixture files under `mods/rundale/world.json`.
- **Coordinate resolution has three tiers.** Absolute (lat/lon) is tried first, then relative (`relative_to` parent offset), then graph-delta fallback (inferring position from graph topology). Always test each tier separately when adding a new location type.
- **Weather generation is deterministic from a seed.** Same seed + same time-of-day produces identical weather. Do not use `rand::thread_rng()` — the seed is derived from the world clock. Tests asserting weather snapshots must fix the clock.
- **`strsim` is used for fuzzy matching of location names.** Player input like "go to the churh" is resolved via Jaro-Winkler distance. The similarity threshold is tuned in `graph.rs` — lowering it increases false positives, raising it increases false negatives for colloquial names.
- **Movement edges may have transport restrictions.** Some edges require a specific transport type (e.g., horse, ferry). Walking is the default fallback. Adding a new transport variant requires updating both `transport.rs` and the edge validation in `movement.rs`.
- **`description.rs` generates the "you see..." prose for each location.** This is the player-facing "look" output. It composes the location's static description, current weather, time-of-day lighting, nearby NPCs (via `parish-npc`), and available exits. Changes here directly affect every entry point's `look` command output.
- **Wayfarers are NPCs travelling on roads, not stationary NPCs.** `wayfarers.rs` manages transient encounters between settlements — they appear on edges during travel, not at nodes. Wayfarers are separate from the NPC tier system in `parish-npc`.
- **Encounters are random events during travel between locations.** `encounter.rs` defines encounter tables per-region and per-transport-type. Encounter outcomes are resolved by `parish-npc` and `parish-inference` — this crate only triggers and classifies them.

## Module map

`graph.rs` location graph (nodes, edges, paths), `movement.rs` player movement, `session.rs` world session state, `weather.rs` weather generation + state, `weather_travel.rs` weather effects on travel, `description.rs` location description prose, `transport.rs` transport type definitions, `encounter.rs` random travel encounters, `wayfarers.rs` travelling NPCs on roads, `geo.rs` geographic coordinate resolution.
