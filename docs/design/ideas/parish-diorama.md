# Interactive Parish Diorama — Scene-Based Graphical Direction

> Status: Proposed · Updated: 2026-06-12 · [Docs Index](../../index.md)

> [Docs Index](../../index.md) · [Architecture Overview](../overview.md) · [GUI Design](../gui-design.md) · Supersedes: [Graphical World View (pixel scenes)](graphical-world-view.md)

## Vision

The graphical version of Rundale is an **interactive parish diorama**: a retro,
scene-based village simulation with Stardew-like visual readability, Myst-like
click navigation, and the existing living-world NPC simulation underneath.

Rather than a continuous tile-based open world, the player sees richly composed
pixel-art views of meaningful locations in and around one Irish village (1820).
They click paths, doors, people, and objects to move through the world graph,
speak with villagers, inspect places, and gradually understand the social life
of the parish.

> You do not build the village. You learn it.

Rundale is not about farming, crafting, combat, or puzzle gating. It is about
entering a living rural community and discovering how people, places, rumors,
obligations, grudges, and memories connect. The promise is not "explore a huge
open world" — it is "**understand a small place deeply**."

### Visual style

- Top-down 3/4 perspective, 16-bit pixel-art fidelity
- Readable and cozy, but grounded — not cartoonish, not cottagecore
- Muted greens, browns, greys, straw yellows, whitewash, peat-dark accents
- Muddy paths, puddles, hedges, low stone walls, small fields, streams,
  thatched cottages; handmade and irregular, never clean or fantasy-medieval
- Inviting but slightly melancholy — a specific Irish place with memory,
  poverty, weather, gossip, and social pressure

### Why scene nodes fit Rundale

A free-walking world needs large tile maps, collision, pathfinding, many
animation states, and a lot of empty traversable space. A scene-node approach
gives fewer, more meaningful locations; atmospheric composition; a far lower
asset burden; and an NPC simulation without an open-world renderer. The
important map is social, not spatial: who lives where, who avoids whom, where
gossip travels, what histories attach to each place. The engine already models
exactly that.

## What the engine already provides

Everything below exists today and is reused **unchanged**:

| Capability           | Where                                                                                                                                          |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Location graph       | 22 locations with connections, `indoor` flag, description templates — `mods/rundale/world.json`, `parish-world/src/graph/schema.rs`            |
| NPC roster           | 23 NPCs with homes, workplaces, hour-by-hour seasonal schedules — `mods/rundale/npcs.json`, `parish-npc/src/npc.rs`                            |
| Presence             | `NpcManager::npcs_at(location)` (`parish-npc/src/manager/lookup.rs`)                                                                           |
| Time & weather       | `GameClock`, `TimeOfDay`, `Season`, 7-state `Weather` engine (`parish-types`)                                                                  |
| Mood palette         | `parish-palette::compute_palette(hour, minute, season, weather)`                                                                               |
| Frontend transport   | unified `command()` seam for Tauri IPC and HTTP/WS (`apps/ui/src/lib/ipc/transport.ts`), reactive stores (`worldState`, `npcsHere`, `mapData`) |
| Click-to-travel      | map clicks already submit `go to <Location Name>` (`MapPanel.svelte`)                                                                          |
| Mod asset validation | traversal-guarded asset paths under `mods/<mod>/assets/` (`parish-mod/src/assets.rs`)                                                          |

The diorama is a **presentation layer over the existing simulation** — no
world-graph, schedule, or save-format changes anywhere in this design.

## Decisions

1. **Scene-first layout.** The plate becomes the main viewport; the dialogue
   panel and text input remain alongside (text input keeps full parity).
   Gated by a `diorama` feature flag: opt-in (`flags.is_enabled`) during the
   build-out, flipped to a default-on kill-switch (`!flags.is_disabled`) once
   curated art exists — satisfying AGENTS rule 6 at the point the feature is
   actually shippable, and keeping Playwright baselines stable until the
   deliberate flip.
2. **AI-curated static assets, generated developer-side only.** Background
   plates and per-NPC sprites are generated with OpenAI (`gpt-image-1`) or
   Google (Imagen 3), curated by a human, post-processed, and checked into
   `mods/rundale/assets/scenes/`. No image generation at runtime. This
   supersedes the procedural `parish-sprite` approach in
   [graphical-world-view.md](graphical-world-view.md); that RFC's serving
   route shapes and scene-panel concept are kept.
3. **Rust CLI art tool** (`parish-art-tool`), following the
   `parish-geo-tool` / `parish-npc-tool` precedent.
4. **Reuse the existing 22 nodes.** One plate per location; the `indoor` flag
   picks interior vs exterior composition. Interior/exterior node splits
   (Pub Exterior vs Pub Interior) are a possible later content change, not
   part of the MVP.

## Scene data model

A mod declares scenes in one index file, mirroring the `world.json` /
`npcs.json` one-file-per-domain pattern. `mod.toml` `[files]` gains an
optional `scenes = "scenes.json"`; mods without it load unchanged.

```json
{
  "scenes": [
    {
      "location_id": 2,
      "slug": "darcys-pub",
      "plate": "assets/scenes/darcys-pub/plate.png",
      "variants": { "night": "assets/scenes/darcys-pub/plate_night.png" },
      "hotspots": [
        {
          "id": "door",
          "shape": { "rect": [82.0, 38.0, 14.0, 50.0] },
          "label": "Out to the Crossroads",
          "action": { "travel_to": 1 }
        },
        {
          "id": "hearth",
          "shape": { "rect": [5.0, 30.0, 18.0, 40.0] },
          "label": "The hearth",
          "action": { "inspect": "A turf fire smoulders in the wide hearth." }
        }
      ],
      "slots": [
        {
          "id": "behind-bar",
          "x": 48.0,
          "y": 55.0,
          "scale": 1.0,
          "prefer_npc": 1
        },
        { "id": "bench-left", "x": 22.0, "y": 68.0, "scale": 1.1 }
      ]
    }
  ],
  "sprites": [
    { "npc_id": 1, "image": "assets/scenes/sprites/padraig-darcy.png" }
  ],
  "fallback_sprites": {
    "default": "assets/scenes/sprites/generic-villager.png"
  }
}
```

- **Coordinates are percentages (0–100) of the plate's native dimensions.**
  `x,y` is a sprite's foot-anchor; rects are `[x, y, w, h]`. Percentage
  coordinates keep hotspots, sprites, and the plate congruent at any display
  size.
- `HotspotAction = TravelTo(location_id) | TalkTo(npc_id) | Inspect(text)`.
  A `polygon` shape variant is reserved in the enum for later.
- Slots are NPC anchor positions; `prefer_npc` pins a host to their station
  (publican behind the bar, smith at the anvil, priest at the altar).
- Schema lives in a new `parish/crates/parish-mod/src/scenes.rs` module
  (`SceneIndex::load` / `scene_for` / `sprite_for`) — `parish-mod` is already
  backend-agnostic, already owns asset-path validation, and ~300 lines does
  not justify a new crate. Every asset reference passes
  `assets::canonical_mod_asset_path` at load; cross-validation (location and
  NPC ids exist, coords in range) logs warnings without failing the load,
  matching other optional mod files.

## Layered scene composition

Each location view is composed at runtime from five layers:

1. **Background plate** — curated PNG (one per location, optional `night` and
   later weather variants). 480×270 native, displayed with
   `image-rendering: pixelated`. No baked UI, labels, or characters.
2. **Hotspot layer** — invisible clickable regions from `scenes.json`
   (exits, doors, NPCs, objects, signs).
3. **Character layer** — NPC sprites placed dynamically from
   `npcs_at(player_location)` and the slot list. Who is present comes from the
   live schedule simulation, never from the art.
4. **State overlay layer** — palette tint from `parish-palette` (dawn / dusk /
   night / weather mood) plus optional variant-plate swaps and a CSS rain
   effect; lit windows and chimney smoke arrive as night-variant art.
5. **UI layer** — the existing StatusBar, dialogue panel, and input field.

## Backend design

Per AGENTS rule 12, the orchestration is written once in `parish-core`
(`src/ipc/scene.rs`) and adapted by thin wiring in each entry point:

```rust
pub fn build_scene_state(
    world: &World, npcs: &NpcManager, scenes: &SceneIndex, flags: &FeatureFlags,
    asset_url: &dyn Fn(&str) -> Option<String>,   // the runtime seam
) -> Option<SceneState>;

pub struct SceneState {
    pub location_id: u32,
    pub plate_url: String,
    pub variant: String,                 // "day" | "night" | ...
    pub hotspots: Vec<SceneHotspotView>,
    pub npcs: Vec<SceneNpcView>,         // name, real_name, mood_emoji, sprite_url, x, y, scale, flip
    pub overflow_npcs: Vec<String>,      // present beyond slot capacity
}
```

- Returns `None` when the flag is off or the location has no scene — the
  frontend falls back to the existing text+map layout. The flag is checked
  backend-side on every fetch (single source of truth, no stale-at-mount).
- **Deterministic slot assignment:** pass 1 seats `prefer_npc` occupants;
  pass 2 fills remaining slots with present NPCs sorted by id, in slot
  declaration order; leftovers are listed in `overflow_npcs`. Pure function,
  directly unit-testable.
- Variant selection by clock hour (Dusk/Night/Midnight → `night` if the
  variant exists); weather variants reserved.

Runtime wiring:

- **Server** (`parish-server/src/routes/scene.rs`): `GET /api/scene-state`
  (state-lock pattern of `get_npcs_here`), and `GET /api/scene-asset/{*rel}`
  serving plate/sprite bytes — re-validated through
  `canonical_mod_asset_path` (promoted `pub(crate)` → `pub`), restricted to
  `assets/scenes/`, served like `serve_mod_icon`
  (`routes/world.rs:163`) with `Cache-Control: immutable` plus `?v=<mtime>`
  cache-busting.
- **Tauri** (`parish-tauri/src/commands/scene.rs`): `get_scene_state` with
  `asset_url` mapping to data URLs via the existing `mod_asset_data_url`
  helper (`parish-tauri/src/lib.rs:39`). Data URLs sidestep the
  `assetProtocol` gotcha (its scope is build-time-fixed while the mods dir is
  runtime-resolved). The frontend caches data URLs by `(slug, variant)`.
- **Headless CLI**: a `/scene` debug command prints scene id, variant, and
  slot assignments as text, so all three modes exercise the shared handler
  and the script harness can assert it (mode parity, rule 2).
- **MCP exposure** (`parish-mcp`): a `parish_scene_state` tool bridging
  `GET /api/scene-state` — a thin passthrough like the other bridge tools.
  Rationale: the repo added `parish_engine_state` so auto-QA agents can
  assert the UI against canonical engine state (#1331); the diorama
  introduces a new class of drift — the _rendered scene_ vs the simulation
  (an NPC seated in a slot who is not present, a stale night variant, a
  hotspot pointing at a non-adjacent location) — and QA agents need to
  assert plate/variant/hotspots/slot assignments structurally rather than
  by reading pixels from screenshots. The demo-audit skills gain a scene
  assertion step once the tool exists.

## Frontend design

New component tree under `parish/apps/ui/src/components/diorama/`:

```text
DioramaView.svelte      // aspect-ratio 480/270 wrapper: layer stack + fallback
├── ScenePlate.svelte   // <img>, image-rendering: pixelated, cross-fade on change
├── HotspotLayer.svelte // SVG viewBox="0 0 100 100" preserveAspectRatio="none"
├── NpcSpriteLayer.svelte // left:{x}%; bottom:{100-y}%; foot-anchored, tooltip
└── SceneOverlay.svelte // multiply-blend tint from the existing palette store
```

One CSS `aspect-ratio` wrapper sizes all layers, so percentage coordinates in
the SVG hotspot layer and the absolutely-positioned sprites align exactly with
the plate at any viewport size.

Wiring:

- `src/stores/scene.ts` (`sceneState` writable) and `src/lib/ipc/scene.ts`
  (`getSceneState()` through the existing `command()` seam).
- `page-controller.ts` fetches scene-state at mount and on every
  `world-update`, alongside `getMap()` / `getNpcsHere()`.
- `+page.svelte`: `$sceneState !== null` renders the scene-first grid
  (DioramaView primary; ChatPanel + InputField below; right column unchanged);
  `null` renders the existing layout — which doubles as the graceful
  per-location fallback.
- `src/lib/scene-actions.ts` maps clicks onto existing input paths:
  `travel_to` → `submitInput("go to <name>")` (the map-click path);
  `talk_to` / sprite click → focus the input with `addressed_to` set (the
  existing mention mechanism); `inspect` → local system entry in the text log.

## The core loop this enables

1. Arrive at Kilteevan Main Street; the plate shows the village at the current
   hour and weather, villagers placed by their real schedules.
2. Click a villager → dialogue (existing inference loop, `addressed_to`).
3. They mention a rumor; click the lane hotspot → travel to the Crossroads.
4. Time advances; schedules move people; the grapevine carries what you said.
5. Return later — the scene has changed: night variant, different faces,
   someone conspicuously absent.

The game rewards attention: noticing who is present, who is absent, where
people live, and how information moves — all of which the simulation already
models and the diorama now makes _visible_.

## `parish-art-tool` — developer-side asset pipeline

A new workspace binary crate, `parish/crates/parish-art-tool/`, mirroring the
clap structure of `parish-npc-tool`:

```sh
parish-art-tool init                      # art/style-bible.md skeleton + empty manifest
parish-art-tool gen-plate <location-id>  [--provider openai|google] [--ref IMG ...] [--n 3]
parish-art-tool gen-sprite <npc-id>      [--provider ...] [--ref IMG ...]
parish-art-tool gen-variant <location-id> --variant night --ref <accepted-day-plate>
parish-art-tool list [--pending|--accepted]
parish-art-tool review <asset-id>         # prints path + prompt + history
parish-art-tool accept <asset-id> [--note "cleaned stray pixels"]
parish-art-tool reject <asset-id> --reason "..."
```

- **Providers:** `ImageProvider` trait with `openai.rs` (`gpt-image-1`
  generations + edits, transparent backgrounds for sprites) and `google.rs`
  (Imagen 3 via the Gemini API). API keys come from `OPENAI_API_KEY` /
  `GEMINI_API_KEY` env vars directly — the `parish.toml` provider registry is
  text-inference plumbing (chat endpoints, streaming, model catalogs) and
  buys nothing for a dev-only image tool.
- **Prompts** are built from real engine data — `LocationData.name`,
  `description_template`, `indoor`, `mythological_significance`;
  `Npc.name/age/occupation/brief_description` — prefixed by the committed
  style bible (fixed framing: "1820 rural Irish parish, 16-bit pixel art,
  top-down 3/4 view, 480×270 plate, muted earth palette…", plus the negative
  rules: no UI text, no readable labels, no baked characters, no fantasy
  drift, no clean cottagecore).
- **Consistency lever:** the first accepted plate and sprite are flagged
  `anchor: true`; every subsequent generation passes anchors as reference
  images to the edit/img2img endpoints. Curate aggressively; the human
  `accept` gate is part of the pipeline, not an afterthought.
- **Tracking:** `art/manifest.json` (committed) records per asset: id, kind
  (plate/sprite/variant), target id, provider, model, prompt, reference
  images, created, status (pending/accepted/rejected), anchor flag, cleanup
  notes, output path. Candidate PNGs in `art/` stay gitignored; only
  accepted, post-processed assets land in `mods/rundale/assets/scenes/`.
- **Post-processing:** generate large (1536×1024 plates / 1024×1024 sprites),
  downscale to 480×270 plates and 48×72 transparent sprites (`image` crate);
  display scaling is CSS `image-rendering: pixelated`.

## MVP content scope

**8 plates** covering the core social loop: Kilteevan Village (15, start),
The Crossroads (1), Darcy's Pub (2), St. Brigid's Church (3), The Forge (16),
The Holy Well (17), Murphy's Farm (9), The Bog Road (12). Interior
compositions where `indoor: true`; night variants for the pub and village
first (where evenings happen).

**~12 sprites:** the NPCs whose homes, workplaces, or schedules put them in
those locations — Padraig and Niamh Darcy, Fr. Declan Tierney, the Gallaghers,
Siobhán and Liam Murphy, Aoife Brennan, Mick Flanagan, Brigid Ní Fhátharta,
Seán Ruadh Kelly — plus a committed generic-villager fallback for everyone
else who wanders in.

**Hotspot authoring:** hand-written against the final plates. Every
`world.json` connection from a covered location gets a `travel_to` hotspot,
including to non-plated neighbors — travel always works; arrival at an
unplated location simply falls back to the text view.

## Implementation roadmap

> Detailed task breakdown (subagent assignments, model/effort flags, automated
> test plan): [Implementation Plan](../../plans/parish-diorama-implementation.md).

```text
M1 ──► M2 ──► M3 ──► M5 ──► M6
  └──► M4 ─────────────┘        (M4 parallel with M2/M3; M5a plates ∥ M5b sprites)
```

| Milestone            | Delivers                                                                                                                | Key tests / proof                                                                                                            |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| **M1** Scene schema  | `parish-mod/src/scenes.rs`, `FileRefs.scenes`, loader + validation, `mods/rundale/scenes.json` with placeholder plates  | serde roundtrip, traversal rejection, unknown-id warnings; headless log shows `scenes.json loaded`                           |
| **M2** Backend       | `parish-core/src/ipc/scene.rs` shared handler; server routes `scene-state` + `scene-asset`; Tauri command; CLI `/scene` | slot determinism, variant-by-hour, flag-off → `None`, asset-route traversal 4xx; live `curl` proof                           |
| **M3** Frontend      | `components/diorama/` tree, `scene-actions.ts`, scene-first layout behind the flag, fallback path                       | vitest action mapping + geometry; new `e2e/diorama.spec.ts` (click hotspot → location changes); flag-off baselines untouched |
| **M4** Art tool      | `parish-art-tool` crate, providers, manifest, post-process, export                                                      | golden prompts, manifest roundtrip, postprocess dims, payload-size caps (rule 16); live gen→accept transcript                |
| **M5** Content       | 8 curated plates, ~12 sprites, authored hotspots/slots                                                                  | loader validation clean; scripted walk of all 8 locations with screenshots, day + night pub                                  |
| **M6** Polish & flip | tint/rain tuning, fade travel transition, flag → default-on, Playwright baseline regen, README + docs                   | before/after gif; e2e green; this RFC graduates to `docs/design/`                                                            |

Each milestone is an independently landable PR following the repo's
acceptance-criteria-first workflow (`/task-start`, live-proof bundle in the
PR body, `just agent-check`).

End-to-end MVP check: new game → Kilteevan plate renders → click lane hotspot
→ arrive at the Crossroads plate → click Padraig's sprite → dialogue with
`addressed_to` set → wait to evening → pub plate swaps to its night variant
under the palette tint.

## Risks & mitigations

- **Style drift across generations** — style bible + anchor references on
  every call + the human accept gate; the manifest records each asset's
  anchor lineage so regeneration is reproducible.
- **Tauri asset serving** — data URLs over IPC (existing precedent) avoid the
  build-time `assetProtocol` scope; plates at 480×270 are ~50–200 KB; cached
  by `(slug, variant)`.
- **Layer coordinate skew** — one aspect-ratio wrapper + percentage
  coordinates everywhere + a geometry unit test.
- **Baseline churn** — the flag stays off until M6's deliberate, documented
  flip.
- **Save compatibility** — none at risk; no save-schema or world-graph
  changes anywhere in the design.
