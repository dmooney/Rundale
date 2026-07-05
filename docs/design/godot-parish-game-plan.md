# Godot-Based Rundale Built on the Parish Engine

## Executive Summary

Build a fresh Godot game client for Rundale, but keep Parish as the authoritative
simulation, persistence, dialogue, schedule, world-state, and inference engine.
Godot should own presentation: painted scenes, actors, animation, navigation
feel, scene hotspots, UI, audio, lighting, and player input. Parish should own
truth: who is where, what time it is, what NPCs know, what commands mean, what
dialogue is produced, what events happened, and what is saved.

The recommended architecture is a **Godot presentation client + Parish Rust
sidecar**. During development, Godot talks to a running `parish-server` over
HTTP/WebSocket. For packaged builds, Godot launches a bundled Parish sidecar and
communicates with it over localhost or stdio using the same protocol.

Do not port Parish into GDScript as the first move. That would throw away the
strongest part of the current project and create years of parity bugs.

## Goals

- Preserve the existing Parish simulation and Rundale content investment.
- Make the game feel like the concept art: hand-painted, low 3/4,
  historical-Irish, readable doors, lived-in yards, and notebook-like UI.
- Replace raw text-first presentation with explorable visual scenes.
- Support AI-assisted art production without trusting AI for map interpretation,
  geometry, or final animation consistency.
- Keep Godot iteration fast: one scene, one NPC group, one interaction loop
  should be testable without rebuilding Parish.
- Keep Parish testability: engine behavior remains covered by existing Rust
  tests and harnesses.

## Non-Goals

- No full custom engine.
- No one-shot AI-generated map-to-scene pipeline as a production dependency.
- No immediate rewrite of Parish simulation in Godot.
- No strict isometric requirement if it fights the concept art. Use stable
  sprite scale and Y-sorting, with painterly low 3/4 backgrounds.
- No attempt to make every historical exterior at once. Start with one vertical
  slice location cluster.

## Core Architecture

```text
Godot client
  painted scene renderer
  player/NPC sprites
  navigation feel
  hotspots and scene UI
  notebook/dialogue interface
  audio/weather/lighting
          |
          | Visual Client Protocol
          | HTTP + WebSocket in dev
          | localhost/stdio sidecar in packaged builds
          v
Parish runtime
  parish-core + leaf crates
  world graph
  NPC schedules and memory
  input parsing
  dialogue/inference
  persistence
  event bus and snapshots
```

Parish remains the authoritative state machine. Godot may predict local walking
inside a scene, but any consequential state change goes through Parish.

## Recommended Tech Choices

- **Godot version:** Godot 4.6.x stable until a later branch proves 4.7+ is
  worth adopting.
- **Godot language:** GDScript first. It is fastest for scene wiring, editor
  tools, UI, and animation glue. Use C# only if a specific tooling or type
  system need appears.
- **Parish integration:** sidecar process, not GDExtension first. GDExtension
  can be reconsidered after the protocol is stable.
- **Scene art:** fixed-background 2D scenes with separated layers, masks,
  hotspots, and actor anchors.
- **Characters:** `CharacterBody2D` or `Node2D` actors driven by
  `NavigationAgent2D` for local movement; Parish validates high-level movement
  and NPC state.
- **Scene navigation:** `NavigationRegion2D` polygons authored per scene.
- **Animation:** `AnimatedSprite2D` sprite sheets for early prototypes;
  consider `Skeleton2D` cutout rigs for reusable body/clothing systems later.
- **Data format:** JSON for shared protocol and generated scene metadata;
  Godot `.tres` / `.tscn` resources for editor-native scene composition.

## Repository Layout

Recommended eventual layout:

```text
clients/godot-rundale/
  project.godot
  scenes/
    boot/
    world/
    ui/
    actors/
  scripts/
    parish/
    scene/
    ui/
  assets/
    backgrounds/
    actors/
    props/
    textures/
    audio/
  data/
    visual_scenes/
    actor_visuals/

parish/crates/parish-visual-protocol/
  shared DTOs for Godot-facing state, events, and commands

parish/crates/parish-visual-server/
  optional sidecar binary or routes if `parish-server` should not expose every
  visual-client endpoint directly

docs/design/godot-parish-game-plan.md
docs/godot/
  art-pipeline.md
  scene-authoring.md
  protocol.md
```

If the first prototype should stay outside the Rust workspace, start with
`clients/godot-rundale/` and communicate with the existing `parish-server`.
Only add new Parish crates once the prototype proves the protocol shape.

## Parish Runtime Responsibilities

Parish owns:

- world graph and location IDs
- current player location
- clock, weather, market days, and future calendar systems
- NPC current locations, schedules, states, memories, and dialogue history
- input parsing and intent classification
- system commands such as save/load/wait/status
- inference provider setup and model routing
- save branches and persistence
- debug snapshots and harness proof tooling
- event stream: movement, dialogue, schedule changes, weather changes, time
  advancement, save/load, branch switching

Godot should never fork this logic into its own local truth. It can cache a
snapshot for rendering, but must discard the cache when Parish sends a newer
authoritative event.

## Godot Client Responsibilities

Godot owns:

- launching/connectivity UI for the Parish runtime
- loading visual scene resources for a Parish location
- rendering background, midground, foreground, actors, props, weather, lighting,
  and UI
- local pathfinding inside the current scene
- pointer, keyboard, controller, and touch input
- hover/click affordances for exits, doors, props, and NPCs
- actor animation state from movement/emotion/activity
- dialogue and notebook UI presentation
- audio ambience and positional sound
- accessibility and display settings
- screenshot capture for visual QA

Godot may implement "feel" locally: walking interpolation, idle fidgets, door
opening animations, hover labels, transition fades. Parish only needs to know
the meaningful command or event.

## Visual Client Protocol

Start with a small protocol rather than exposing the whole existing web UI API.
The Godot client needs stable, game-shaped messages.

### Session Lifecycle

- `POST /visual/session/new`
- `POST /visual/session/load`
- `POST /visual/session/save`
- `GET /visual/session/snapshot`
- `GET /visual/session/events` or WebSocket `/visual/ws`

### Player Commands

- `POST /visual/command`
  - `look`
  - `wait`
  - `talk`
  - `move_to_location`
  - `interact`
  - raw text fallback

### Scene Data Queries

- `GET /visual/location/{location_id}`

  - Parish location metadata
  - connected exits
  - current NPCs
  - visible schedule/state hints
  - recommended visual scene ID

- `GET /visual/npc/{npc_id}`
  - name, pronouns, state, mood, visible activity
  - current location
  - portrait/sprite mapping key

### Event Messages

The Godot client should subscribe to events such as:

- `world_snapshot`
- `player_location_changed`
- `npc_location_changed`
- `npc_activity_changed`
- `dialogue_started`
- `dialogue_line`
- `dialogue_ended`
- `clock_changed`
- `weather_changed`
- `save_completed`
- `branch_loaded`
- `error`

Each event should include a monotonic sequence number so Godot can ignore stale
updates after reconnects.

## Scene Data Model

Each visual scene should be a Godot scene plus a metadata file. The metadata is
the bridge between Parish world IDs and Godot art.

Example conceptual schema:

```json
{
  "scene_id": "grove_exterior",
  "parish_location_ids": ["grove"],
  "background": "res://assets/backgrounds/grove_exterior/base.png",
  "layers": {
    "background": "res://assets/backgrounds/grove_exterior/background.png",
    "midground": "res://assets/backgrounds/grove_exterior/midground.png",
    "foreground": "res://assets/backgrounds/grove_exterior/foreground.png",
    "occlusion_mask": "res://assets/backgrounds/grove_exterior/occlusion.png"
  },
  "navigation": {
    "walkable_polygons": [],
    "blocked_polygons": []
  },
  "anchors": [
    {
      "id": "front_door",
      "kind": "door",
      "parish_target": "grove_house",
      "position": [650, 420],
      "facing": "south"
    }
  ],
  "exits": [
    {
      "id": "road_east",
      "target_location_id": "kilteevan_road",
      "position": [1130, 520],
      "arrival_anchor": "road_west"
    }
  ],
  "depth": {
    "sort_axis": "y",
    "baseline_y": 0,
    "actor_scale": 1.0
  },
  "sockets": [
    {
      "id": "chimney_smoke_1",
      "kind": "vfx",
      "position": [710, 255]
    }
  ]
}
```

The schema should be boring and auditable. Human-authored geometry wins over
generated art. Historical map annotations can feed this metadata, but should not
overwrite it without review.

## Art Direction Pipeline

### Exterior Scenes

1. Human reads historical map crop.
2. Map annotation JSON captures buildings, roads, paths, physical boundaries,
   administrative boundaries, trees, rough vegetation/bog, labels, and uncertain
   marks.
3. Designer translates the map into a playable scene plan.
4. Artist creates a blockout in the correct camera style.
5. Background plate is painted or AI-assisted, then manually corrected.
6. Layers are separated:
   - background ground
   - buildings and fixed objects
   - foreground occluders
   - tree canopies
   - shadows
   - lighting/weather masks
   - optional object sockets
7. Godot scene receives navigation polygons, hotspots, actor anchors, and exits.
8. QA compares:
   - historical map read
   - scene metadata
   - Godot scene
   - in-game screenshot

### AI Role

AI is useful for:

- rough scene mood boards
- texture studies
- prop variants
- tree/hedge/wall material sheets
- character costume concepts
- portrait ideation
- background paintover options

AI should not be final authority for:

- building count or footprint
- road/path topology
- door placement
- walkable space
- historical map interpretation
- sprite sheet consistency
- collision/navigation masks

The production rule: AI can propose pixels; humans approve geometry.

## Character and Sprite Pipeline

Start simple:

- one player sprite set
- one male adult, one female adult, one child, one elder base
- 4-direction or 8-direction walk cycles depending on camera readability
- idle variants by mood/activity
- separate portrait art for dialogue/notebook UI

Recommended early format:

```text
actor_id/
  sprite_sheet.png
  sprite_frames.tres
  portrait_neutral.png
  portrait_worried.png
  portrait_angry.png
  actor_visual.json
```

`actor_visual.json` maps Parish NPC IDs or visual archetypes to Godot assets:

```json
{
  "npc_id": "roisin_connolly",
  "archetype": "adult_woman",
  "sprite": "res://assets/actors/roisin/sprite_frames.tres",
  "portrait_set": "res://assets/actors/roisin/portraits/",
  "scale": 1.0,
  "default_facing": "south",
  "palette_tags": ["earth", "linen", "dark_shawl"]
}
```

Avoid depth-based sprite scaling in the first version. Keep scale constant and
use Y-sorting/occluders for depth, because variable scale would recreate the
same problem the image experiments exposed.

## UI Plan

The Godot UI should inherit the notebook concept, but become game-native:

- left or bottom notebook panel for dialogue/history
- selected NPC card with name, mood, visible activity, known relationship
- compact command/input area
- scene hover labels
- subtle time/weather strip
- optional map/notebook tabs
- debug overlay hidden behind developer shortcut

The UI should not feel like a generic RPG HUD. It should feel like a field
notebook layered over an illustrated parish scene.

## Gameplay Loop

Target loop for the first playable prototype:

1. Player enters a painted exterior scene.
2. Parish sends current location, time, weather, present NPCs, and recent log.
3. Godot places NPC actors at authored anchors or schedule-derived activity
   anchors.
4. Player clicks ground to walk locally.
5. Player clicks an NPC, door, road exit, or prop.
6. Godot sends the meaningful command to Parish.
7. Parish validates and updates world state.
8. Godot receives events and animates the result.
9. Dialogue appears in the notebook UI.
10. Time/weather/schedules continue to advance according to Parish rules.

## Development Phases

### Phase 0: Prototype Setup

Deliverables:

- create `clients/godot-rundale/`
- Godot project launches
- Godot can connect to an already-running Parish server
- Godot can fetch a snapshot and display:
  - location name
  - clock
  - weather
  - player status
  - NPCs at current location

Acceptance:

- no Godot-side mock world truth except fallback demo data
- connection failures are visible and recoverable

### Phase 1: Single Scene Walkable Prototype

Use Grove or another small exterior.

Deliverables:

- placeholder painted/background image
- `NavigationRegion2D`
- player actor walks on click
- hotspots for one door, one road exit, one NPC
- local hover labels

Acceptance:

- player cannot walk through blocked buildings
- clicking an exit sends a Parish movement command
- Parish response changes the current location or rejects the move

### Phase 2: Parish Event Sync

Deliverables:

- WebSocket or event polling from Parish
- event sequence numbers
- actor presence updates from NPC location changes
- clock/weather updates
- reconnect behavior

Acceptance:

- `/wait` or time advancement in Parish visibly updates Godot
- NPCs appear/disappear when their Parish location changes
- stale events do not rewind Godot state

### Phase 3: Dialogue and Notebook UI

Deliverables:

- click NPC to select
- talk command reaches Parish
- streaming or completed dialogue appears in Godot UI
- conversation log visible
- typed fallback input still available

Acceptance:

- dialogue uses the same Parish prompt/response path as current Rundale
- addressed NPC is respected
- absent NPC feedback still works

### Phase 4: Authoring Pipeline

Deliverables:

- visual scene metadata schema
- converter from map annotator JSON to initial scene-plan draft
- Godot editor helper for anchors/hotspots
- screenshot review checklist

Acceptance:

- a human can create a new exterior scene without editing code
- exported metadata round-trips through Godot and Parish IDs

### Phase 5: Art Production Slice

Deliverables:

- one final-quality exterior scene
- one interior scene
- 3-5 NPC sprites with portraits
- prop and texture mini-kit
- day/night/weather overlays
- audio ambience pass

Acceptance:

- scene matches concept-art direction more than the AI-generated map plates did
- doors, roads, exits, and walkable areas are readable
- NPC sprites feel native to the background

### Phase 6: Packaging

Deliverables:

- Godot launches Parish sidecar automatically
- user-data paths configured explicitly
- setup flow for inference provider/API key
- save/load UI
- crash/error reporting path

Acceptance:

- packaged app starts without a manually running server
- save/load survives app restart
- Parish logs remain inspectable for debugging

### Phase 7: Vertical Slice

Deliverables:

- 3-5 connected exterior scenes
- 1-2 interiors
- one meaningful NPC conversation chain
- schedule movement visible in scenes
- time/weather overlay
- save/load
- QA harness or scripted smoke route

Acceptance:

- player can walk, inspect, talk, wait, save, reload, and see NPC schedule
  effects without leaving Godot
- Parish tests still pass
- Godot screenshot review passes visual and interaction criteria

## Sidecar Packaging Strategy

Development:

- run `parish-server` normally
- Godot connects to `http://127.0.0.1:<port>`
- use live logs and existing Rust tooling

Packaged prototype:

- Godot includes a Parish runtime binary
- Godot starts the sidecar on launch
- sidecar binds to `127.0.0.1` on an available port or uses stdio
- Godot waits for `/health`
- Godot shuts the sidecar down on quit

Later hardening:

- signed sidecar binary
- fixed protocol version handshake
- crash recovery and log bundle export
- local-only binding enforcement
- no secret leakage in logs

## Testing Strategy

Parish remains tested by:

- Rust unit/integration tests
- existing harness fixtures
- mode parity tests
- benchmark/eval tooling where relevant

Godot should add:

- smoke test scene that connects to a test Parish server
- screenshot baseline for the first scene
- protocol contract tests using recorded Parish snapshots/events
- navigation tests for each authored scene
- manual QA checklist for:
  - hotspot readability
  - door placement
  - collision boundaries
  - actor occlusion
  - dialogue flow
  - save/load

Important: do not claim a Godot feature works because the Rust engine works.
The visual client needs its own proof: screenshots, recorded inputs, and logs
showing the Parish event that drove the visual result.

## Main Risks

- **State divergence:** Godot invents local truth. Mitigation: Parish owns all
  consequential state; Godot caches only render state.
- **Protocol sprawl:** Godot reaches into old web UI endpoints and depends on
  accidental shapes. Mitigation: define a small visual protocol.
- **Packaging complexity:** Godot plus Rust sidecar creates launch and path
  bugs. Mitigation: keep sidecar simple, explicit paths, health checks, clear
  logs.
- **AI asset inconsistency:** generated sprites/backgrounds do not match across
  scenes. Mitigation: AI for concepts and texture sheets; human cleanup and
  style bible for final assets.
- **Authoring overhead:** every location needs too much hand work. Mitigation:
  build a reusable kit and a scene metadata editor after the first two scenes,
  not before.
- **Losing the concept-art feel:** strict isometric/carto accuracy makes scenes
  stiff. Mitigation: lock gameplay geometry separately from painterly final art.

## First Vertical Slice Recommendation

Use a tiny slice:

- one exterior: Grove or Murphy Farm
- one interior: a cottage room or shop
- one road/exit transition
- two NPCs
- one conversation
- one wait/schedule change
- one save/load proof

Success looks like this:

1. Godot launches.
2. Parish sidecar starts or connects.
3. The player sees a painted scene.
4. The player clicks to walk.
5. The player talks to an NPC using Parish dialogue.
6. The player waits.
7. The NPC schedule/time/weather changes come from Parish and update the scene.
8. The player saves, quits, reloads, and lands back in the same world state.

That is the smallest honest proof that Godot can carry the visual game while
Parish remains the engine.

## Decision Log

- Use Godot for the new visual game client.
- Keep Parish as authoritative simulation/runtime.
- Use a sidecar protocol first, not a Rust GDExtension.
- Use fixed-background, layered 2D scenes rather than generated full-map scenes.
- Use AI art only where human review can enforce geometry, style, and continuity.
- Start with one vertical slice before building generalized scene tooling.
