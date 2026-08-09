# Plan: Interactive Parish Diorama — Runtime Compositor Implementation

> Parent design: [Interactive Parish Diorama](../design/ideas/parish-diorama.md) | [Docs Index](../index.md)
>
> **Status: Proposed**
>
> **Depends on:** nothing in flight — all engine seams it builds on are shipped
> **Depended on by:** future interior-node content work; design-doc graduation to `docs/design/`

## Goal

Ship the MVP of the revised diorama design: a scene-first graphical viewport
composed at runtime from curated sprite/prop assets, clickable hotspots,
simulation-driven NPC placement, a `diorama` feature flag, and a dev-side
`parish-art-tool` for style references, prop/sprite generation, curation, and
preview composition.

This replaces the earlier full-backplate plan. The desired fidelity is still
the user-approved ChatGPT sample style, but full generated plates are no longer
the source of truth for gameplay-visible geometry. The engine owns layout;
assets are small, replaceable visual atoms.

## Orchestration Rules

1. **One PR per milestone**, branched from the previous milestone's merge.
2. **Acceptance criteria:** each milestone should define
   `.proofs/diorama-m<N>/acceptance-criteria.md` and
   `parish/testing/proofs/play_diorama-m<N>.txt` before implementation.
3. **Proof (AGENTS rule 10):** milestones touching runtime paths need a
   live-proof bundle in the PR body. M4 is tool-only but still needs a transcript
   or fixture proving the tool behavior claimed.
4. Subagents run focused tests before reporting done; the milestone orchestrator
   runs the full gate (`just check`, `just ui-test`/`ui-e2e` where relevant,
   `just agent-check`) before the PR.
5. Tasks marked ∥ may run in parallel on non-overlapping files.

```text
M1 ──► M2 ──► M3 ──► M5 ──► M6
        └──► M4 ─────┘
```

---

## M1 — Scene Schema, Asset Catalog, Validation (PR 1)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                                  | Model    | Effort | Depends   |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | --------- |
| T1.0 | AC + fixture. Criteria: `scenes.json` parses; invalid asset path rejected; unknown location/NPC/asset ids warn without failing when optional; mods without `scenes` load unchanged; headless load log reports scene/layer/asset counts.                                                                                                                                               | `opus`   | low    | —         |
| T1.1 | **Compositor schema.** Create `parish/crates/parish-mod/src/scenes.rs`: `SceneIndex`, `SceneDef`, `SceneLayer`, `SceneLayerLabel`, `SceneAsset`, `Hotspot`, `HotspotAction`, `NpcSlot`, `SpriteDef`, `FallbackSprites`; percentage coords; integer z-order; optional `underlay`; reserved polygon shape; `kind = "wayfinding_sign"` support.                                          | `sonnet` | high   | T1.0      |
| T1.2 | **Asset validation.** `SceneIndex::load(mod_dir, rel)` validates every referenced asset through `assets::canonical_mod_asset_path`; export `asset_for`/`scene_for`/`sprite_for`; validate asset anchors, opacity, scale, duplicate ids, label anchors, label text budget, and draw-order sanity.                                                                                      | `sonnet` | medium | T1.1      |
| T1.3 | **Mod wiring.** Add `FileRefs.scenes: Option<String>` and `GameMod.scenes: Option<SceneIndex>`; cross-validation `validate_scenes(&SceneIndex, &WorldGraph, &NpcManager) -> Vec<String>` checks location ids, travel targets, prefer_npc, coord ranges, missing asset ids, wayfinding labels against known location names, and covered-location connection coverage where configured. | `sonnet` | medium | T1.1      |
| T1.4 | **Placeholder asset pack.** Add `mods/rundale/scenes.json` with two skeletal scenes (Kilteevan Main Lane/start and Darcy's Pub or Crossroads), flat-colour/procedural placeholder PNGs under `mods/rundale/assets/scenes/`, a generic NPC sprite, and `scenes = "scenes.json"` in `mods/rundale/mod.toml`.                                                                            | `haiku`  | low    | T1.2      |
| T1.5 | **Proof.** Run the fixture headless, capture the load log and `/scene`-ready structural data if available, write evidence/judge, and run `just agent-check`.                                                                                                                                                                                                                          | `sonnet` | medium | T1.3–T1.4 |

**Task-level tests:** serde roundtrip for every variant; `[x,y,w,h]` rect
parsing; traversal and absolute-path rejection; missing required asset ->
`ParishError::Config`; optional unknown ids produce warning strings;
wayfinding label text must be bounded and validated against known destinations;
`scene_for`/`asset_for`/`sprite_for` hit and miss; old mods without `scenes`
load unchanged.

---

## M2 — Shared Scene-State + Asset Serving, Three Runtimes (PR 2)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Model    | Effort | Depends   |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | --------- |
| T2.0 | AC + fixture. Criteria: flag off -> empty scene-state; flag on -> native size + layer list + hotspots + seated NPCs; traversal on asset route rejected; `/scene` parity output in headless; `parish_scene_state` returns the same view model over MCP.                                                                                                                                                                                                  | `opus`   | low    | M1 merged |
| T2.1 | **Shared handler (rule 12 seam).** Create `parish-core/src/ipc/scene.rs`: `SceneState`, `SceneLayerView`, `SceneLayerLabelView`, `SceneNpcView`, `SceneHotspotView`; `build_scene_state(world, npcs, scenes, flags, asset_url)` handles flag gate, optional underlay, asset URL resolution, z-order sorting, wayfinding labels, deterministic slot assignment, introduction semantics, overflow NPCs, variant overlays, and indoor weather suppression. | `opus`   | high   | T2.0      |
| T2.2 | ∥ **Server routes.** `parish-server/src/routes/scene.rs`: `GET /api/scene-state`, `GET /api/scene-asset/{*rel}`. Promote `canonical_mod_asset_path` to `pub`, restrict to `assets/scenes/`, serve immutable PNG/WebP assets with `?v=<mtime>` cache busting.                                                                                                                                                                                            | `sonnet` | medium | T2.1      |
| T2.3 | ∥ **Tauri command.** `parish-tauri/src/commands/scene.rs`: `get_scene_state` maps assets to data URLs through the existing mod-asset data URL helper; register in the command registry.                                                                                                                                                                                                                                                                 | `sonnet` | medium | T2.1      |
| T2.4 | ∥ **CLI parity.** `/scene` debug command prints scene id, variant, layers in z-order, hotspots, slot assignments, and overflow NPCs; extend the M2 fixture to assert it.                                                                                                                                                                                                                                                                                | `sonnet` | low    | T2.1      |
| T2.5 | ∥ **MCP exposure.** Add `parish_scene_state` to `parish-mcp` as a thin bridge over `/api/scene-state`; document in `AGENTS.md` and `parish/crates/parish-mcp/README.md`.                                                                                                                                                                                                                                                                                | `sonnet` | low    | T2.2      |
| T2.6 | **Gate + proof.** Live proof with `parish-mcp-backend.sh start`, `/flag enable diorama`, `curl /api/scene-state`, `curl /api/scene-asset/...`, and an MCP `parish_scene_state` transcript; evidence/judge; `just agent-check`.                                                                                                                                                                                                                          | `sonnet` | medium | T2.2–T2.5 |

**Task-level tests:** slot determinism, prefer_npc, overflow order, z-order
sort, asset-url failures, flag-off -> `None`, unplated location -> `None`,
unintroduced NPC display name, indoor overlay suppression, empty-slots scene
still returns hotspots. Axum route tests cover URL-encoded traversal, mime
type, immutable cache header, and flag-off null body.

---

## M3 — Frontend Runtime Compositor Behind The Flag (PR 3)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                                            | Model    | Effort | Depends   |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | --------- |
| T3.0 | AC + fixture. Criteria: flag on -> composed scene visible; layers draw in z-order; hotspot click travels; sprite click addresses NPC; flag off -> existing layout pixel-stable; unplated location falls back; unintroduced NPC tooltip shows brief description only; `travel-start` dims the scene until arrival.                                                                               | `opus`   | low    | M2 merged |
| T3.1 | **State + IPC wiring.** `src/stores/scene.ts`, `src/lib/ipc/scene.ts`, `SceneState` types in `src/lib/types.ts`; fetch at mount + every `world-update`; cache Tauri data URLs by asset path + mtime/hash where available.                                                                                                                                                                       | `sonnet` | medium | T3.0      |
| T3.2 | ∥ **Component tree.** `components/diorama/`: `DioramaView`, `SceneUnderlay`, `SceneLayerStack`, `HotspotLayer`, `NpcSpriteLayer`, `SceneOverlay`; one aspect-ratio wrapper; absolutely-positioned asset instances; `z-index` derived from scene z-order; runtime black hand-painted wayfinding labels on white sign assets; debug mode shows layer boxes, z labels, slot anchors, and hotspots. | `sonnet` | high   | T3.1      |
| T3.3 | ∥ **Action mapping.** `src/lib/scene-actions.ts`: `travel_to` -> existing map-click `submitInput("go to <name>")` path; `talk_to` / sprite click -> focus input with `addressed_to`; `inspect` -> local system entry; unknown ids are no-ops with debug warning.                                                                                                                                | `sonnet` | medium | T3.1      |
| T3.4 | **Layout switch.** `+page.svelte`: `$sceneState !== null` -> scene-first grid; `null` -> existing layout. Mobile breakpoint follows current 768 px behavior.                                                                                                                                                                                                                                    | `sonnet` | medium | T3.2–T3.3 |
| T3.5 | **Playwright e2e.** New `e2e/diorama.spec.ts`: enable flag, assert layered scene, click hotspot, click sprite, verify fallback, verify flag-off screenshot stability.                                                                                                                                                                                                                           | `sonnet` | high   | T3.4      |
| T3.6 | **Proof.** Screenshot pair (flag off/on with placeholder composition), live hotspot-travel transcript, structural scene-state dump, evidence/judge, `just agent-check`.                                                                                                                                                                                                                         | `sonnet` | medium | T3.5      |

**Task-level tests:** scene-store refresh, asset-cache behavior, z-order
rendering from mocked `SceneState`, percentage geometry, runtime wayfinding
label placement, missing underlay fallback, action mapping, keyboard hotspot
activation, unintroduced tooltip privacy, travel dim/clear behavior.

---

## M4 — `parish-art-tool` Asset-Atom Pipeline (PR 4)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                                                      | Model    | Effort | Depends   |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | --------- |
| T4.0 | AC. Criteria: every subcommand has observable behavior; dry-run works without image-provider keys; manifest invariants hold; compose-preview can render a nonblank mock scene.                                                                                                                                                                                                                            | `opus`   | low    | M2 merged |
| T4.1 | **Crate skeleton + manifest.** `parish/crates/parish-art-tool/` workspace member; clap commands `init`, `gen-reference`, `gen-prop`, `gen-sprite`, `compose-preview`, `list`, `review`, `accept`, `reject`; `ArtManifest` records assets, references, status transitions, and atomic save.                                                                                                                | `sonnet` | medium | T4.0      |
| T4.2 | **Style bible.** `art/style-bible.md` captures the approved ChatGPT sample as reference-only, records target fidelity/material culture/perspective/palette, and states negative rules: no baked UI labels, no invented readable signs, no fantasy drift, no impossible water/buildings, no baked NPCs in scene props. Wayfinding signs are white-painted timber boards with black hand-painted lettering. | `opus`   | medium | T4.1      |
| T4.3 | ∥ **Prompt builder.** `prompt.rs` builds prop/sprite prompts from `LocationData`, NPC fields, asset kind, and the style bible. Golden prompts cover cottage, stream segment, blank white wayfinding signboard, pub hearth, and Padraig Darcy sprite.                                                                                                                                                      | `opus`   | medium | T4.1      |
| T4.4 | ∥ **Provider adapters.** Image-provider trait plus OpenAI/Google adapters. Exact model names and request schemas are verified during implementation. Enforce client-side request-size caps per AGENTS rule 16.                                                                                                                                                                                            | `sonnet` | high   | T4.1      |
| T4.5 | ∥ **Postprocess + export.** Trim transparent bounds, preserve alpha, downscale crisply, reject blank/degenerate assets, enforce target dimensions by kind, export accepted assets under `mods/rundale/assets/scenes/`, and optionally upsert asset ids into `scenes.json`.                                                                                                                                | `sonnet` | medium | T4.1      |
| T4.6 | **Preview renderer.** `compose-preview <location-id>` reads `scenes.json`, layers accepted/placeholder assets, draws NPC placeholders, and writes a PNG with a nonblank-content guard.                                                                                                                                                                                                                    | `sonnet` | medium | T4.5      |
| T4.7 | **Docs + gate.** Crate README, root `.gitignore` for generated candidate assets, `just notices` if dependencies change, full `just check`, transcript of init -> dry-run generate -> accept -> compose-preview.                                                                                                                                                                                           | `haiku`  | low    | T4.2–T4.6 |

**Task-level tests:** manifest roundtrip and illegal transition rejection;
golden prompts; provider body construction against recorded JSON; payload
budget assertions; postprocess exact dims and alpha preservation; blank/solid
asset rejection; export path guard; preview render nonblank check.

---

## M5 — Curated Vertical Slice Content (PR 5)

Human-in-the-loop milestone: agents can generate and organize candidates, but
a human accepts every visible asset.

| #    | Task                                                                                                                                                                                                                                                                                               | Model    | Effort | Depends        |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | -------------- |
| T5.0 | AC. Criteria: 3 scenes render in-game; all covered-location connections are clickable; initial NPC sprites render or fall back; style resembles the accepted sample; no full-scene AI plate is required for correctness.                                                                           | `opus`   | low    | M3 + M4 merged |
| T5.1 | **Anchor asset round.** Generate/curate a small reference set: cottage, wall segment, stream bend, muddy path tile/patch, white wayfinding signboard, cart, pub hearth/interior prop, generic villager, Padraig Darcy. Mark accepted anchors in manifest.                                          | `opus`   | high   | T5.0           |
| T5.2 | ∥ **Exterior scenes.** Author Kilteevan Main Lane and The Crossroads layouts from accepted atoms; compose stream/bridge/path/walls/cottages/wayfinding signs/props with deterministic z-order; use runtime black sign lettering from validated destination names rather than baked generated text. | `sonnet` | high   | T5.1           |
| T5.3 | ∥ **Interior scene.** Author Darcy's Pub layout with bar/hearth/tables/door hotspots and slots for Padraig/Niamh/visitors.                                                                                                                                                                         | `sonnet` | medium | T5.1           |
| T5.4 | ∥ **Sprites.** Padraig and Niamh Darcy, Fr. Declan Tierney, one farmer, one older woman, one younger villager, plus generic fallback.                                                                                                                                                              | `opus`   | high   | T5.1           |
| T5.5 | **Hotspot/slot pass.** Ensure every world connection from the 3 covered scenes has a `travel_to` hotspot; 2–5 NPC slots per scene; `prefer_npc` for pub bar; inspect hotspots for stream, wayfinding sign, hearth, cart/well.                                                                      | `sonnet` | medium | T5.2–T5.3      |
| T5.6 | **Content gate + proof.** Loader validation warning-free; content sanity test; scripted walk through all covered scenes with screenshots; `parish_scene_state` asserted against `parish_engine_state` for location, variant, present NPCs, layers, and hotspots; evidence/judge.                   | `sonnet` | medium | T5.4–T5.5      |

After this PR, decide whether to expand directly to the original 8-location
scope or improve tooling/authoring first.

---

## M6 — Polish, Expansion Path, Flag Flip, Docs (PR 6)

| #    | Task                                                                                                                                                                                                                               | Model    | Effort | Depends   |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | --------- |
| T6.0 | AC. Criteria: default-on behavior, kill-switch works, transition smoothness, visual quality checklist for the 3-scene slice, and deliberate baseline regeneration.                                                                 | `opus`   | low    | M5 merged |
| T6.1 | ∥ **Visual polish.** Palette-tint strength, CSS rain/fog/smoke overlays gated by indoor/outdoor, travel fade, focus styles for hotspots, mobile layout polish.                                                                     | `sonnet` | medium | T6.0      |
| T6.2 | ∥ **Expansion plan.** Document asset/layout strategy for the next 5 locations: St. Brigid's Church, The Forge, The Holy Well, Murphy's Farm, The Bog Road. Add TODO-backed content checklist rather than silently expanding scope. | `opus`   | low    | T6.0      |
| T6.3 | **Flag flip.** `build_scene_state` gate -> default-on kill-switch (`!flags.is_disabled("diorama")`); regenerate Playwright baselines intentionally and document the churn.                                                         | `sonnet` | medium | T6.1      |
| T6.4 | ∥ **Docs.** README feature list + structure; `just screenshots`; graduate the design doc from `docs/design/ideas/` to `docs/design/diorama.md` only if the runtime slice is accepted.                                              | `haiku`  | low    | T6.3      |
| T6.5 | **Final proof.** Before/after gif or screenshot strip, full gate (`just check`, `just ui-test`, `just ui-e2e`, `just agent-check`), evidence/judge.                                                                                | `sonnet` | medium | T6.3      |

---

## Automated Test Plan

Every layer gets automated coverage; task lists above are the source of truth
for exact cases.

### 1. Rust Unit + Integration (`just check` / `cargo test`, every PR)

| Suite                             | Lives in                                    | Covers                                                                                                                                                  |
| --------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scene schema (`scenes::tests`)    | `parish-mod/src/scenes.rs`                  | serde roundtrips, asset/layer/label/action parsing, traversal rejection, missing-file error, lookup hit/miss, optional-file back-compat                 |
| Cross-validation                  | parish-core mod-load tests                  | unknown ids warn, coord/range/z-order checks, connection coverage, asset anchors, wayfinding labels                                                     |
| Scene-state (`ipc::scene::tests`) | `parish-core/src/ipc/scene.rs`              | slot determinism, prefer_npc, overflow, layer sorting, layer labels, asset URL mapping, variant selection, flag-off/unplated -> `None`                  |
| Server routes                     | `parish-server/src/routes/tests.rs`         | scene-asset traversal -> 4xx, mime + immutable cache headers, scene-state flag-off `null`                                                               |
| Art tool                          | `parish-art-tool` unit tests                | manifest transitions, golden prompts, provider bodies, payload caps, postprocess alpha/dims, blank rejection, export path guard, preview nonblank guard |
| Architecture fitness              | `parish-core/tests/architecture_fitness.rs` | new modules declared, no backend deps leak into leaf crates, `parish-art-tool` registered as a tool crate                                               |

### 2. Frontend Unit (`just ui-test`, M3+)

`scene-actions.test.ts` (action mapping), `scene.store.test.ts`
(world-update refresh + asset cache), `DioramaView.test.ts` (layer geometry,
z-order, runtime wayfinding labels, fallback render), `HotspotLayer.test.ts`
(keyboard activation and aria labels), and tooltip privacy tests for
unintroduced NPCs.

### 3. End-To-End (`just ui-e2e`, M3+)

`e2e/diorama.spec.ts` against the real auto-started server:

1. Enable the flag through real input (`/flag enable diorama`), reload.
2. Assert the composed scene is visible and layer count matches scene-state.
3. Click a travel hotspot -> StatusBar location changes; scene swaps.
4. Click an NPC sprite -> input gains the `addressed_to` chip.
5. Open debug overlay -> hotspots/slots/layer boxes are visible.
6. Flag off -> existing baseline screenshots remain stable until M6.

### 4. Script-Harness Fixtures

`parish/testing/proofs/play_diorama-m<N>.txt` is written during milestone
setup.
Runs assert the load line (M1), `/flag enable diorama` + `/scene` output
(M2+), clickable route parity where practical, and the 3-scene walk (M5).
MCP-driven QA asserts `parish_scene_state` against `parish_engine_state` at
each stop.

### 5. Visual And Artifact Guards

Runtime screenshots and generated preview PNGs must be nonblank and
nondegenerate. Asset postprocessing checks alpha coverage, dimensions, and
trim bounds. Human review remains required for semantic content: disconnected
water, impossible buildings, wrong signs, baked text, and setting mismatch.
Wayfinding signs get an extra check: white board, black hand-painted lettering,
valid destination names, and no AI-invented route text.

### 6. CI Gates

Rust quality gate, UI quality, Playwright e2e, fixture sweep, docs consistency,
and the agent proof gate must stay green. M4 adds `just notices` output to the
diff when provider/image dependencies land.

## Risks Carried From The Design Doc

Whole-scene AI artifacts, style drift, repetitive tiling, layer coordinate
skew, baked text mistakes, historically wrong signs, baseline churn, and proof burden. The central
mitigation is the compositor pivot: semantic layout is owned by data and code;
AI art supplies bounded assets that can be rejected or replaced locally.
