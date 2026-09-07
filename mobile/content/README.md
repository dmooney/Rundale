# Phase 2 authored content

`phase2-crossroads.json` is the bounded Phase 2 content bundle. It is based on
the Phase 1 `crossroads` scene and `Peig` fixture, but deliberately contains
one playable location and one interactive NPC. The stable content IDs are
separate from the labels shown to players: `crossroads` / `The crossroads` and
`npc-peig` / `Peig`.

The bundle is authored definition data only. `schema_version` describes this
JSON shape; `content_version` identifies the authored facts and labels. The
engine numeric IDs (`engine_location_id: 1` for the existing Crossroads and
`engine_npc_id: 22` for Peig Hannigan) let a Rust loader construct typed
`parish_types::LocationId` and `parish_types::NpcId` values while retaining the
stable string IDs used by content references.

## Shape for a Rust consumer

The intended `serde` model is equivalent to:

```rust
struct ContentBundle {
    schema_version: u32,
    content_version: u32,
    content_id: String,
    locations: Vec<LocationDefinition>,
    npcs: Vec<NpcDefinition>,
}

struct LocationDefinition {
    id: String,
    engine_location_id: u32, // parish_types::LocationId(engine_location_id)
    display_name: String,
    playable: bool,
    opening_description: String,
    commands: CommandOutputs, // look, people, exits: String
    initial_npc_ids: Vec<String>,
    exits: Vec<ExitDefinition>,
}

struct ExitDefinition {
    id: String,
    direction: String,
    display_name: String,
    description: String,
    destination_id: Option<String>,
    playable: bool,
}

struct NpcDefinition {
    id: String,
    engine_npc_id: u32, // parish_types::NpcId(engine_npc_id)
    display_name: String,
    interactive: bool,
    aliases: Vec<String>,
    initial_location_id: String,
    home_location_id: String,
    role: String,
    personality: Vec<String>,
    known_people: Vec<KnownPerson>,
    known_places: Vec<KnownPlace>,
    known_facts: Vec<KnownFact>,
}
```

`KnownPerson` has `id`, `display_name`, and `relationship`. `KnownPlace` has
`id`, `display_name`, `description`, and `playable`. `KnownFact` has `id`,
`statement`, `known_people`, `known_places`, and `source`. A loader should
validate unique IDs, resolve every string reference, require exactly one
location and one NPC for this bundle, and reject unknown fields. The single
scenic east exit has `destination_id: null` and `playable: false`; it gives
`/exits` a grounded response without adding a second playable location.

The `opening_description` and `commands` strings are immutable authored
outputs for the opening scene and `/look`, `/people`, and `/exits`. Runtime
location, NPC presence, game time, memories, conversation history, task
progress, saves, Endpoint settings, and credentials do not belong in this
bundle; those are supplied by the engine or persistence layers.
