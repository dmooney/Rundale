# Graphical World View — Procedural Pixel Scenes

> Status: Superseded — see [Interactive Parish Diorama](parish-diorama.md) · Updated: 2026-06-17 · [Docs Index](../../index.md)
>
> The runtime-composed diorama RFC incorporates this document's deterministic
> sprite/scene-composition direction while keeping the newer diorama UX,
> scene-state, and hotspot contract.

> [Docs Index](../../index.md) · [Architecture Overview](../overview.md) · [GUI Design](../gui-design.md) · [Map Evolution](map-evolution.md) — RFC

## Context

Rundale is text-driven. The player reads narrative descriptions, talks to NPCs, and sees the world through chat plus a MapLibre minimap. The map answers "where am I in the parish, where can I go" and stays exactly as it is. What's missing is a **graphical view of the present moment**: a small pixel-art window showing the player's current location, the NPCs physically there right now, and the time-of-day/weather mood.

This document proposes adding that view as a new panel alongside chat — a complement to the text, not a replacement for any existing system. The map, the lat/lon world graph, the chat loop, the save format, and the mod content schema all remain unchanged.

**Key design decision: deterministic procedural generation, not LLM.** Sprites and scenes are derived from existing NPC and location attributes via a seeded recipe — reproducible, zero runtime cost, fully offline. An LLM "stylist" is parked as an optional later hook for high-attention NPCs, never on the critical path. Rendering lives in a new backend-agnostic Rust crate so CLI/web/Tauri produce identical pixels and the `/prove` harness can snapshot-test the output (mode parity, [AGENTS.md](../../../AGENTS.md) rule #2).

## Goals & non-goals

**Goals**

- A pixel-art scene panel that visualizes the player's current location and the NPCs present.
- Per-NPC pixel portraits usable inline in the chat (speaker headers) and the Designer editor.
- Time-of-day / season / weather mood applied via the existing `parish-palette` output.
- Deterministic, snapshot-testable, no required art assets for the MVP.

**Non-goals (v1)**

- No player avatar movement / walking controller — gameplay stays text/click-driven.
- No replacement of the MapLibre map or the lat/lon world graph.
- No tile-based "overworld" parish map. (Possible future; out of scope here.)
- No LLM on the generation critical path.
- No animation (idle bob, walk cycles) — deferred to a stretch phase.

## Architecture

A new leaf crate **`parish-sprite`** exposes two pure, deterministic functions:

```rust
pub fn render_npc(npc: &Npc, palette: &RawPalette) -> Vec<u8>;            // PNG bytes
pub fn render_scene(location: &Location, present: &[&Npc], palette: &RawPalette) -> Vec<u8>;
```

Same inputs always produce the same bytes. NPC sprites are composed from a parts library (silhouette, garments, hair, accessories), each part chosen by hashing `npc.id` salted with attributes (`occupation`, `age`, `mood`). Location scenes pick one of ~10 hand-authored templates keyed by `building_form` / location name, then stamp NPC sprites into slot positions. Output is tinted by the current `parish-palette` palette without re-rendering.

### Crate layout

```text
crates/parish-sprite/
├── Cargo.toml
└── src/
    ├── lib.rs           // public API: render_npc, render_scene, recipe types
    ├── recipe.rs        // NpcRecipe, SceneRecipe + deterministic derivation
    ├── parts/
    │   ├── mod.rs       // PartLibrary trait + ProceduralParts impl
    │   ├── silhouette.rs, head.rs, garments.rs, accessories.rs
    ├── scene/
    │   ├── mod.rs       // SceneTemplate registry + slot resolution
    │   └── templates.rs // CABIN, CHAPEL, PUB, MILL, BOG, CROSSROADS, ...
    ├── compose.rs       // RGBA buffer + blit + 1-bit ordered dither
    ├── tint.rs          // value-channel multiplicative tint from parish-palette
    └── png.rs           // encode RGBA → PNG (via `png` crate)
```

Constraints: backend-agnostic (no `tauri`/`axum`/`tower`/`wry`/`tao` deps), re-exported via `parish-core` per rule #1, registered in `crates/parish-core/tests/architecture_fitness.rs`.

### Files touched outside the new crate

| File                                                                      | Change                                                                                    |
| ------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `crates/parish-core/Cargo.toml`, `src/lib.rs`                             | Add + re-export `parish-sprite`                                                           |
| `crates/parish-core/tests/architecture_fitness.rs`                        | Register crate, assert no backend deps                                                    |
| `crates/parish-server/src/routes.rs` (or equiv)                           | `GET /api/sprite/npc/{id}.png`, `GET /api/scene/{location_id}.png?t=<minute>&w=<weather>` |
| `crates/parish-tauri/src/commands.rs`                                     | IPC commands `get_npc_sprite`, `get_location_scene`                                       |
| `crates/parish-cli/src/...`                                               | `parish sprites dump <out_dir>` for headless snapshot diffs                               |
| `apps/ui/src/components/LocationScenePanel.svelte`                        | **New** — the visible feature                                                             |
| `apps/ui/src/components/CharacterPortrait.svelte`                         | **New** — inline chat portraits                                                           |
| `apps/ui/src/routes/+page.svelte`                                         | Slot scene panel above chat; portrait per message author                                  |
| `apps/ui/src/components/editor/NpcDetail.svelte`, `LocationDetail.svelte` | Portrait/scene preview + "regenerate seed" debug button                                   |

The MapLibre map (`MapPanel.svelte`, `FullMapOverlay.svelte`) and `mods/rundale/world.json` are **not** modified.

## Recipe schemas

Closed enums and bounded ints throughout, so every value is renderable and derivation can never produce an invalid sprite.

```rust
// recipe.rs
pub struct NpcRecipe {
    pub seed: u64,                  // hash(npc.id)
    pub silhouette: Silhouette,     // Slim | Average | Stocky | Stooped   (biased by age, occupation)
    pub head: HeadShape,            // Round | Long | Square | Gaunt
    pub skin_tone: SkinTone,        // Pale | Fair | Ruddy | Tanned | Weathered (biased by outdoor work)
    pub hair: HairStyle,            // Bald | Short | Tied | Wild | Bonnet | Cap
    pub hair_color: HairColor,
    pub torso: GarmentTorso,        // Shirt | Waistcoat | Shawl | Coat | Apron | Ragged (biased by occupation)
    pub legs: GarmentLegs,
    pub footwear: Footwear,
    pub accessories: Vec<Accessory>,// ≤2, biased by occupation (priest→Rosary, farmer→WalkingStick…)
    pub posture: Posture,           // Upright | Hunched | Leaning | ArmsAkimbo (biased by mood)
}

pub struct SceneRecipe {
    pub seed: u64,                  // hash(location.id)
    pub template: SceneTemplate,    // chosen from building_form / location name
    pub variant: u8,                // 0..=N sub-layout within the template
    pub foliage_density: u8,        // 0..=3
    pub weather_overlay: WeatherOverlay, // None | Drizzle | Fog | Storm
    pub npc_slots: Vec<SceneSlot>,  // resolved at render time from `present`
}
```

`derive_npc_recipe(&Npc) -> NpcRecipe` and `derive_scene_recipe(&Location) -> SceneRecipe` are pure. No persistent cache needed — recomputation is sub-millisecond. (An in-process LRU keyed by `(recipe, palette_bucket)` can be added if profiling demands.)

## Parts library

MVP draws parts in code: each part writes a flat-colour block plus 1-bit ordered dithering into a sub-rect of an RGBA buffer. NPC canvas **24×32**, scene canvas **160×120**, both displayed at 2× with `image-rendering: pixelated`.

```rust
pub trait PartLibrary {
    fn draw_torso(&self, buf: &mut RgbaBuf, garment: GarmentTorso, palette: &PartPalette);
    // … one per part slot
}
```

`ProceduralParts` is bundled and requires **no art assets**. `LayeredParts(Vec<Box<dyn PartLibrary>>)` lets a mod override individual parts with hand-pixeled PNGs dropped under `mods/rundale/sprites/parts/` — opt-in, later.

Variety budget: ≥4 silhouettes × 5 garments × 6 hair × 3 accessories × 4 postures yields thousands of distinct combinations before any hand art.

## Scene templates

Each template draws a base composition (terrain + building) from shared sub-routines (`draw_thatched_roof`, `draw_drystone_wall`, `draw_bog_patch`, `draw_gorse_clump`) and declares NPC slot positions. Render steps:

1. Draw the template base (deterministic from `seed` + `variant`).
2. For each present NPC, pick a free slot (deterministic by `npc.id` ordering), render the NPC sprite, blit at slot position with fore/background scale.
3. Overlay weather if any (e.g. diagonal drizzle).
4. Apply palette tint.

Initial set (~10): `Cabin`, `Chapel`, `Pub`, `Mill`, `Schoolhouse`, `Crossroads`, `Bog`, `Field`, `Pasture`, `Coastline`. Template choice reuses the name-matching approach already in `apps/ui/src/lib/map-icons.ts`.

## Palette integration

`parish-palette::compute_palette(hour, minute, season, weather)` yields a 7-colour UI palette. The tint pipeline (`tint.rs`) does **not** remap the scene into that 7-colour set; it converts the rendered RGBA to HSV, multiplies the V channel by a coefficient derived from `palette.bg` luminance, and nudges hue toward `palette.accent` by a small clamped amount. Hue is preserved, so dawn/dusk/night feel right without re-rendering, and cached scene bytes can be re-tinted cheaply per game-minute. A `tint_strength` knob in palette config controls weather mood without letting `tint.rs` shift hue freely.

## Play-view wiring sequence

1. Game state advances → `current_location_id` and `present` NPCs change.
2. Frontend requests `/api/scene/{location_id}.png?t=<minute>&w=<weather>` (or Tauri IPC).
3. Server calls `parish_sprite::render_scene(location, present, palette)`, returns PNG.
4. `LocationScenePanel.svelte` swaps `<img>` src with a soft CSS cross-fade.
5. Each chat message renders `CharacterPortrait.svelte` from `/api/sprite/npc/{id}.png` (browser-cached by URL).

Rendering is on the request path (fast enough); no streaming/lazy generation.

## Phasing

- **Phase 0 — NPC portraits only (~1 week wedge).** `parish-sprite` crate + NPC parts library + `CharacterPortrait.svelte` in chat. Demo: chat with Maeve, see her portrait. Snapshot tests for ~6 NPCs.
- **Phase 1 — Static location scenes.** Scene templates + `LocationScenePanel.svelte`. Demo: travel to the pub, see a pub vignette, tinted by existing palette.
- **Phase 2 — Populate scenes with present NPCs.** Resolve `present` against scene slots and render NPCs in. Demo: enter the pub, see Darcy behind the bar.
- **Phase 3 — Polish & extensibility.** Weather overlays; PNG override path for hand-pixeled parts; optional LLM-stylist hook (Background lane, `crates/parish-inference/src/lib.rs`) emitting recipe overrides for high-attention NPCs.
- **Phase 4 (stretch).** Animation frames; per-season template variants; optional "click a map location → open its scene" wire-up in `MapPanel.svelte`.

## Verification

- **Unit tests (`parish-sprite`):** `derive_npc_recipe_is_deterministic`, `recipe_serde_roundtrip`, `tint_preserves_hue_modulates_value`.
- **Snapshot tests:** golden PNGs under `crates/parish-sprite/snapshots/` for 6 NPCs + 4 scenes; CI fails on diff with a byte-diff report.
- **Architecture-fitness:** extend `architecture_fitness.rs` to register `parish-sprite` and forbid backend deps.
- **`/prove sprites`** (`testing/proofs/prove_sprites.txt`):
  1. Capture Maeve portrait via `parish sprites dump`; assert valid PNG header.
  2. Save → restart → load → re-capture; assert byte-identical (determinism survives persistence).
  3. Fast-forward dawn→midday→dusk; assert scene hashes differ (tint applied) but underlying indexed mask is identical.
  4. Travel pub→chapel; assert template changes and matches snapshot.
- **Playwright:** new baselines for `LocationScenePanel` (two locations) and chat `CharacterPortrait`.
- **Manual smoke:** `just run` (Tauri) and `just run-headless` both show pixel art at expected slots.

## Risks & tradeoffs

- **Procedural sprites can look samey.** Mitigated by the variety budget; PNG override path differentiates hero NPCs later.
- **Scene templates are hand-authored code.** Real cost (~hundreds of lines each). Mitigated by shared draw sub-routines.
- **Hue-preserving tint is deliberate.** Designers wanting stronger weather mood use `tint_strength` rather than hue shifts.
- **No LLM in v1** means no narrative-driven per-NPC detail until the Phase-3 stylist hook.
- **Map stays separate.** "Click map → scene" is a small Phase-4 follow-up, not blocking.
