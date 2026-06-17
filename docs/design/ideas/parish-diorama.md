# Interactive Parish Diorama — Runtime-Composed Graphical Direction

> Status: Proposed · Updated: 2026-06-17 · [Docs Index](../../index.md)

> [Docs Index](../../index.md) · [Architecture Overview](../overview.md) · [GUI Design](../gui-design.md) · Incorporates: [Graphical World View (pixel scenes)](graphical-world-view.md)

## Vision

The graphical version of Rundale is an **interactive parish diorama**: a
high-fidelity, retro, scene-based village simulation with Stardew-like visual
readability, Myst-like click navigation, and the existing living-world NPC
simulation underneath.

Rather than a continuous tile-based open world, the player sees richly composed
views of meaningful locations in and around one Irish village (1820). They
click paths, doors, people, and objects to move through the world graph, speak
with villagers, inspect places, and gradually understand the social life of
the parish.

> You do not build the village. You learn it.

Rundale is not about farming, crafting, combat, or puzzle gating. It is about
entering a living rural community and discovering how people, places, rumors,
obligations, grudges, and memories connect. The promise is not "explore a huge
open world" — it is "**understand a small place deeply**."

## Visual Target

The strongest art reference so far is the user-provided ChatGPT sample:
<https://chatgpt.com/s/m_6a2b4e7a5f188191b62e44e48d3372c0>. It is not
content-canonical — its date, text labels, road signs, and exact geography are
reference-only — but its **style and fidelity are the target**:

- Top-down / isometric 3/4 perspective with a readable village-stage layout.
- Dense, high-fidelity pixel art: small plants, puddles, stone walls, thatch,
  chimneys, paths, fences, carts, doorways, bridges, and water edges all have
  hand-placed texture.
- Grounded Irish rural material culture: whitewashed cottages, straw thatch,
  peat smoke, low drystone walls, muddy lanes, small fields, hedgerows, wells,
  carts, white hand-painted wayfinding signs, bridges, streams, and uneven
  handmade surfaces.
- Muted earth palette: moss greens, mud browns, straw yellows, greys, whitewash,
  peat-dark shadows, and small warm-light accents.
- Cozy but not sanitized: inviting, lived-in, damp, slightly melancholy, and
  specific to an Irish parish rather than generic fantasy village art.

The sample also demonstrates what **not** to make runtime-dependent: UI title
plaques, route names, dates, and labels should not be baked into canonical art.
If a signpost, plaque, or label-like prop appears in scene art, it must be
treated as a prop and reviewed manually for spelling and setting correctness.

## Representation Pivot

Earlier drafts centered on **AI-curated full-scene backplates**: one composed
PNG per location, with hotspots and NPCs layered over it. Experiments showed
that this asks too much of a single generated bitmap. Full scenes often look
beautiful at a glance but fail in semantically important ways: disconnected
rivers, broken chimneys, impossible walls, inconsistent roads, baked-in
characters, wrong signs, or day/night variants whose geometry no longer
matches.

The updated direction is a **Factorio-style runtime sprite compositor**:

- The engine owns semantic layout: scene size, exits, waterways, buildings,
  walls, doors, wayfinding signs, prop anchors, z-order, and NPC slots.
- Art is made of smaller, curated atoms: cottages, roof pieces, chimneys,
  smoke, wall segments, bridge pieces, wells, carts, trees, hedges, puddles,
  stream edges, furniture, interior props, and NPC sprites.
- AI generation is still useful, but primarily for **bounded assets** and style
  references, not canonical whole-location geometry.
- Runtime composition produces the final scene from asset instances. Bad assets
  can be replaced locally without repainting a whole location.
- Pure procedural / code-drawn fallback assets remain valid for early
  milestones and CI tests.

Full AI plates may still be used as **mood boards, underpaint sketches, or
temporary placeholders**, but the source of truth for gameplay-visible space is
the layout data plus composited assets.

## What The Engine Already Provides

Everything below exists today and is reused unchanged:

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

The diorama is a **presentation layer over the existing simulation**. It does
not change world graph semantics, NPC schedules, dialogue, or save format.

## Decisions

1. **Scene-first layout.** The composed scene becomes the main viewport; the
   dialogue panel and text input remain alongside, preserving text parity.
   Gated by a `diorama` feature flag during build-out and flipped to a
   default-on kill-switch once curated content exists.
2. **Runtime composition over monolithic plates.** Location views are assembled
   from asset instances at runtime. Optional backplates are allowed only as
   temporary underlays or style references; hotspots and NPC slots must align
   with semantic layout data, not with an opaque bitmap.
3. **Developer-side art pipeline.** A Rust `parish-art-tool` helps build and
   curate style references, sprite sheets, prop cutouts, and preview renders.
   No image generation happens at gameplay runtime.
4. **Reuse the existing 22 nodes.** One authored layout per location. The
   `indoor` flag selects interior vs exterior composition. Interior/exterior
   node splits remain possible later, but are not part of the MVP.
5. **No player avatar in the MVP.** The player is the camera. The sample image
   includes a central figure, but Rundale's navigation remains first-person
   click-to-travel until a later phase introduces positional mechanics.

## Scene Data Model

A mod declares scene layouts in one index file, mirroring the `world.json` /
`npcs.json` one-file-per-domain pattern. `mod.toml` `[files]` gains an
optional `scenes = "scenes.json"`; mods without it load unchanged.

The schema sketch below shows the updated compositor direction. It is more
explicit than the first draft's `plate + slots` model:

```json
{
  "asset_packs": ["assets/scenes/common/pack.json"],
  "scenes": [
    {
      "location_id": 15,
      "slug": "kilteevan-main-lane",
      "native_size": [640, 480],
      "underlay": null,
      "layers": [
        {
          "id": "stream",
          "asset": "stream-bend-a",
          "x": 6.0,
          "y": 76.0,
          "z": 10,
          "scale": 1.0
        },
        {
          "id": "left-cottage",
          "asset": "cottage-whitewash-thatched-a",
          "x": 13.0,
          "y": 34.0,
          "z": 30,
          "scale": 1.0
        },
        {
          "id": "kilteevan-wayfinding",
          "asset": "wayfinding-sign-three-arm-white-blank-a",
          "x": 33.0,
          "y": 57.0,
          "z": 60,
          "scale": 1.0,
          "labels": [
            { "text": "KILTEEVAN", "anchor": [48.0, 30.0], "rotation": -2.0 },
            { "text": "CROSSROADS", "anchor": [51.0, 47.0], "rotation": 1.0 },
            { "text": "CHAPEL", "anchor": [50.0, 64.0], "rotation": -1.0 }
          ]
        }
      ],
      "hotspots": [
        {
          "id": "lane-to-crossroads",
          "shape": { "rect": [42.0, 39.0, 14.0, 18.0] },
          "label": "Toward the Crossroads",
          "action": { "travel_to": 1 }
        },
        {
          "id": "stream",
          "shape": { "rect": [0.0, 73.0, 42.0, 18.0] },
          "label": "The stream",
          "action": { "inspect": "Water hurries under the small footbridge." }
        }
      ],
      "slots": [
        { "id": "lane-center", "x": 52.0, "y": 55.0, "z": 80, "scale": 1.0 },
        { "id": "cottage-door", "x": 20.0, "y": 44.0, "z": 82, "scale": 0.95 }
      ]
    }
  ],
  "assets": [
    {
      "id": "wayfinding-sign-three-arm-white-blank-a",
      "kind": "wayfinding_sign",
      "image": "assets/scenes/props/wayfinding-sign-three-arm-white-blank-a.png",
      "anchor": [50.0, 92.0]
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

- Coordinates are percentages of the scene's native dimensions. `x,y` is the
  asset's anchor point; each asset declares its own anchor so cottages, props,
  and NPC sprites can all be foot- or base-aligned.
- `z` is an integer draw order. Lower layers draw first; NPC slots can sit
  between foreground and background props.
- `labels` are optional runtime text overlays for assets that need readable
  marks. The MVP uses them primarily for wayfinding signs, with the text
  validated from known location names instead of trusted to image generation.
- `underlay` is optional and never authoritative. It can help early prototypes
  or painterly mood, but every interactive route and NPC position comes from
  `layers`, `hotspots`, and `slots`.
- `HotspotAction = TravelTo(location_id) | TalkTo(npc_id) | Inspect(text)`.
  A `polygon` shape variant is reserved for later.
- Schema lives in `parish/crates/parish-mod/src/scenes.rs`
  (`SceneIndex::load` / `scene_for` / `sprite_for` / `asset_for`). Every asset
  reference passes `assets::canonical_mod_asset_path`; cross-validation
  confirms ids, coordinate ranges, draw-order sanity, missing assets, and
  connection coverage.

## Layered Scene Composition

Each location view is composed at runtime from these layers:

1. **Base terrain / underlay** — flat fill, procedural fallback, or optional
   reference underlay. It is not gameplay-authoritative.
2. **World asset layer** — cottages, roofs, chimneys, wall segments, water
   pieces, bridges, carts, wayfinding signs, wells, trees, hedges, puddles,
   interior furniture, and other authored prop instances from `scenes.json`.
3. **Hotspot layer** — invisible clickable regions, with a debug overlay for
   authoring and automated geometry tests.
4. **Character layer** — NPC sprites placed dynamically from
   `npcs_at(player_location)` and the slot list. Who is present comes from the
   live schedule simulation, never from baked art.
5. **State overlay layer** — palette tint from `parish-palette`, CSS weather,
   optional smoke/window-light overlays, and later festival/encounter overlays.
6. **UI layer** — the existing StatusBar, dialogue panel, input field, and any
   runtime title cards or labels. UI text is not baked into scene images.

### Wayfinding Signs

Wayfinding signs are an important placeable element, not background filler.
They should read as rural, handmade, and practical: white-painted timber boards
with black hand-painted lettering, slight irregularity in the boards and brush
work, and local destination names that match the world graph.

Implementation guidance:

- Treat a sign as a normal composited scene layer with `kind:
"wayfinding_sign"`.
- Prefer blank sign-board assets plus runtime black lettering from validated
  scene data. This preserves correct spelling and lets the same sign asset
  point toward different connected locations.
- If a generated/painted sign includes baked lettering, it must go through
  human review for spelling, destination validity, date/period fit, and
  legibility.
- The sign's clickable hotspot should usually be `inspect`, while the road or
  path region remains the `travel_to` hotspot. A sign helps the player
  understand routes; it is not the route itself.

## Backend Design

Per AGENTS rule 12, orchestration is written once in `parish-core`
(`src/ipc/scene.rs`) and adapted by thin wiring in each entry point:

```rust
pub fn build_scene_state(
    world: &World,
    npcs: &NpcManager,
    scenes: &SceneIndex,
    flags: &FeatureFlags,
    asset_url: &dyn Fn(&str) -> Option<String>,
) -> Option<SceneState>;

pub struct SceneState {
    pub location_id: u32,
    pub slug: String,
    pub native_width: u32,
    pub native_height: u32,
    pub variant: String,
    pub underlay_url: Option<String>,
    pub layers: Vec<SceneLayerView>,
    pub hotspots: Vec<SceneHotspotView>,
    pub npcs: Vec<SceneNpcView>,
    pub overflow_npcs: Vec<String>,
}

pub struct SceneLayerView {
    pub id: String,
    pub asset_url: String,
    pub kind: String,
    pub labels: Vec<SceneLayerLabelView>,
    pub x: f32,
    pub y: f32,
    pub z: i32,
    pub scale: f32,
    pub flip: bool,
    pub opacity: f32,
}

pub struct SceneLayerLabelView {
    pub text: String,
    pub anchor: (f32, f32),
    pub rotation: f32,
}
```

**Introduction semantics (gameplay correctness):** NPCs are anonymous until
introduced. `SceneNpcView` mirrors the dialogue system exactly:
`display_name` is the brief description until introduced, `real_name` is
populated only after introduction, and sprite tooltips and click-to-address
follow the same rule. The diorama must never name an NPC the dialogue system
would still keep anonymous.

- Returns `None` when the flag is off or the location has no scene.
- Deterministic slot assignment seats `prefer_npc` occupants first, then fills
  remaining slots with present NPCs sorted by id. Leftovers go to
  `overflow_npcs`.
- Variant selection can choose night/smoke/window-light overlays by clock hour.
  Weather overlays are suppressed indoors.
- Server and Tauri serve only validated assets under `assets/scenes/`.
- Headless CLI exposes `/scene` with scene id, variant, layers, hotspots, and
  slot assignments.
- MCP exposes `parish_scene_state` so QA agents can assert the composed scene
  structurally against `parish_engine_state`, instead of reading pixels.

## Frontend Design

New component tree under `parish/apps/ui/src/components/diorama/`:

```text
DioramaView.svelte        // aspect-ratio wrapper: composed scene + fallback
├── SceneUnderlay.svelte   // optional underlay, never authoritative
├── SceneLayerStack.svelte // prop/building/terrain asset instances by z-order
├── HotspotLayer.svelte    // SVG viewBox="0 0 100 100"
├── NpcSpriteLayer.svelte  // foot-anchored dynamic NPC sprites
└── SceneOverlay.svelte    // palette/weather/time/festival overlays
```

One CSS `aspect-ratio` wrapper sizes all layers, so percentage coordinates in
the SVG hotspot layer and absolutely-positioned sprites align at any viewport
size. The first implementation can be DOM/CSS layered PNGs; a backend-rendered
composite can be added later if profiling or snapshot tests demand it.

Wiring:

- `src/stores/scene.ts` (`sceneState` writable) and `src/lib/ipc/scene.ts`
  (`getSceneState()` through the existing `command()` seam).
- `page-controller.ts` fetches scene-state at mount and on every
  `world-update`, alongside `getMap()` / `getNpcsHere()`.
- `+page.svelte`: `$sceneState !== null` renders the scene-first grid;
  `null` renders the existing layout, which is also the per-location fallback.
- `SceneLayerStack` renders wayfinding labels as black, slightly irregular
  runtime text on top of white sign-board assets. Text comes from scene data
  validated against known locations.
- `src/lib/scene-actions.ts` maps clicks onto existing input paths:
  `travel_to` -> `submitInput("go to <name>")`; `talk_to` / sprite click ->
  focus the input with `addressed_to`; `inspect` -> local system entry.
- During travel, the departing composition dims on `travel-start`; the new
  composition cross-fades in on arrival `world-update`.
- When the debug panel is open, `HotspotLayer` renders rects, z-order labels,
  slot anchors, and asset outlines visibly for authoring.

## The Core Loop This Enables

1. Arrive at Kilteevan Main Lane; the runtime composition shows the village
   with its current hour, weather, props, and villagers.
2. Click a villager -> dialogue using the existing addressed NPC path.
3. Click a lane hotspot -> travel to the Crossroads.
4. Time advances; schedules move people; the grapevine carries what you said.
5. Return later — same semantic place, different light, different faces,
   different weather, and maybe a changed prop overlay.

The game rewards attention: noticing who is present, who is absent, where
people live, and how information moves — all of which the simulation already
models and the diorama now makes visible.

## `parish-art-tool` — Developer-Side Asset Pipeline

The art tool now focuses on curated **asset atoms** and preview composition,
not whole-scene backplates:

```sh
parish-art-tool init
parish-art-tool gen-reference kilteevan-main-lane --ref IMG --note "target style"
parish-art-tool gen-prop cottage-whitewash-thatched --kind building --n 4
parish-art-tool gen-prop stream-bend --kind terrain --n 4
parish-art-tool gen-sprite <npc-id> --n 4
parish-art-tool compose-preview <location-id>
parish-art-tool list [--pending|--accepted|--rejected]
parish-art-tool review <asset-id>
parish-art-tool accept <asset-id> [--note "..."]
parish-art-tool reject <asset-id> --reason "..."
```

- **Style bible:** committed `art/style-bible.md` records the ChatGPT sample
  as the target reference and spells out the desired material culture,
  perspective, palette, density, and negative rules.
- **Provider adapters:** OpenAI and Google image providers are initial
  candidates, but exact model names and request shapes must be rechecked at
  implementation time. The tool is dev-only and reads image-provider keys from
  environment variables.
- **Prompts:** built from real engine data (`LocationData`, NPC fields,
  `indoor`, mythological notes) plus the style bible. Prompts ask for isolated
  transparent-background props or sprites whenever possible.
- **Manifest:** `art/manifest.json` records id, kind, target id, provider,
  prompt, references, created time, status, anchor/style lineage, cleanup notes,
  and output path.
- **Post-processing:** trims transparent bounds, checks alpha/nonblank content,
  downscales with crisp pixel sampling, rejects obvious wrong dimensions, and
  exports accepted assets under `mods/rundale/assets/scenes/`.
- **Human gate:** every accepted asset must be reviewed for setting fit,
  spelling, impossible geometry, and whether it composes cleanly with existing
  anchors.

## MVP Content Scope

The first shippable slice should prove the compositor before scaling content:

- **2 exterior scenes:** Kilteevan Main Lane / village start and the Crossroads.
- **1 interior scene:** Darcy's Pub.
- **Common asset pack:** mud paths, grass/flower patches, puddles, stream
  pieces, bridge pieces, wall segments, fences, white-painted wayfinding
  signboards, cart, well, cottage variants, chimney/smoke overlays, pub
  furniture, hearth, table/bench props.
- **NPC sprites:** Padraig and Niamh Darcy, Fr. Declan Tierney, one farmer,
  one older woman, one child/young person, plus a generic-villager fallback.
- **Hotspots:** every world connection from the covered scenes, plus at least
  one inspect hotspot per scene.

After the vertical slice is visually convincing, expand to the original eight
locations: Kilteevan Village, The Crossroads, Darcy's Pub, St. Brigid's Church,
The Forge, The Holy Well, Murphy's Farm, and The Bog Road.

## Implementation Roadmap

> Detailed task breakdown and test plan: [Implementation Plan](../../plans/parish-diorama-implementation.md).

```text
M1 ──► M2 ──► M3 ──► M5 ──► M6
        └──► M4 ─────┘
```

| Milestone            | Delivers                                                                                          | Key tests / proof                                                                                                      |
| -------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **M1** Scene schema  | `scenes.json` compositor schema, asset catalog, validation, placeholder asset pack                | serde roundtrip, asset traversal rejection, z-order/coord validation, optional-file back-compat                        |
| **M2** Backend       | shared `SceneState` builder, scene asset serving, Tauri command, CLI `/scene`, MCP scene tool     | slot determinism, layer ordering, variant rules, asset-route traversal 4xx, structural `parish_scene_state` transcript |
| **M3** Frontend      | `components/diorama/` runtime compositor, hotspots, debug overlay, scene-first layout             | geometry tests, action mapping, keyboard hotspots, e2e click-to-travel, flag-off baseline stability                    |
| **M4** Art tool      | style bible, asset manifest, prop/sprite generation, postprocess, compose-preview, accept/reject  | golden prompts, manifest transitions, nonblank/alpha/dim guards, payload caps, preview render proof                    |
| **M5** Content slice | 3 composed scenes, common asset pack, initial NPC sprites, authored hotspots/slots                | scripted walk with screenshots, structural scene-vs-engine assertions, human visual review                             |
| **M6** Polish & flip | tint/weather tuning, transitions, expansion path, flag default-on, README/docs/screenshots update | before/after gif, e2e green, deliberate baseline regen                                                                 |

## Risks & Mitigations

- **Whole-scene AI artifacts** — do not use whole-scene AI plates as canonical
  geometry. Generate small assets, compose them from deterministic layout data,
  and use full-scene images only as style references or temporary underlays.
- **Style drift across assets** — style bible + accepted reference image +
  anchor assets + manifest lineage + human accept gate.
- **Composition looks tiled or repetitive** — support variants, flips, scale,
  small detail props, foreground occluders, and hand-authored layout jitter.
- **Layer coordinate skew** — one aspect-ratio wrapper, percentage coordinates,
  asset anchors, and geometry unit tests.
- **Baked text mistakes** — no baked UI text; wayfinding signs use runtime
  validated black lettering on white boards where possible; baked sign text
  requires manual review.
- **Baseline churn** — flag stays off until the deliberate M6 flip.
- **Save compatibility** — none at risk; scene state is derived from existing
  world/NPC data and mod assets.

## Future Work

- Visual hotspot/slot/layout editor in Parish Designer.
- Backend-rendered composite PNGs and snapshot tests, if DOM composition proves
  hard to verify.
- Interior/exterior node splits for buildings where outside/inside differences
  become mechanically meaningful.
- Festival, market-day, encounter, and rumor-state prop overlays.
- Weather/season asset variants beyond CSS overlays.
- Localization support for runtime wayfinding text and other plaque-like props.
- Player avatar and sprite animation, only if later mechanics need explicit
  player position.
