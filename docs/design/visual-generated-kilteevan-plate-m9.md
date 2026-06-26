# Visual Generated Kilteevan Plate M9

## Purpose

The previous compositor proofs improved topology and validation but did not produce a satisfying game image. M9 changes the center of gravity: Kilteevan Village should be presented first as a coherent generated pixel-art plate, with code supplying semantic constraints and interactive overlays instead of visibly assembling the whole environment from generic chunks.

## Player Experience

On launch, the player should see a full-screen high 3/4 isometric pixel-art Irish village scene that feels intentionally illustrated: cottages, road, bridge, stream, well, sign, foliage, smoke, and NPC scale all belong to the same world. The player can still click exits, inspect the well/sign/bridge, select NPCs, and use a compact text fallback, but the first read is the world, not the UI or a compositor proof.

## Affected Subsystems

- `mods/rundale/`: store the generated Kilteevan plate PNG, scene asset metadata, and any semantic plate spec used to generate/reproduce it.
- `parish/apps/visual`: render the generated plate as the primary Kilteevan visual surface while keeping hotspots, NPC sprites, hover/click cues, captions, and proof screenshots.
- `parish/apps/visual/scripts`: add or extend proof tooling that turns the semantic plate spec into a deterministic prompt/manifest and validates screenshot output.
- `parish/crates/parish-mod` / `parish-core`: no schema break intended; existing scene-state plate/layer/hotspot/slot contracts should remain additive.

## Data Model

Add a semantic plate spec rather than another sprite-placement recipe. The spec should describe:

- camera/aspect/native size;
- art style and negative prompt constraints;
- road graph and entry/exit directions;
- stream polyline and bridge crossing;
- cottage pads, door-facing, chimney sockets, and smoke origin requirements;
- well/sign/cart/prop sockets and forbidden water overlaps;
- NPC-safe standing sockets;
- hover/hotspot target regions.

The generated PNG plate is a concrete asset derived from that spec. Runtime scene state can continue to expose legacy `plate`/`plate_url`, ordered layers, hotspots, slots, and activation hints.

## Validation

Validation should check the semantic layout before generation and the rendered screenshot after integration. The pre-generation checks prove roads/water/bridge/props are physically coherent; screenshot proof verifies the generated plate did not ignore the spec in obvious ways.

This milestone does not attempt fully automated image understanding. The judge may be human-authored but must explicitly evaluate the failure modes that have hurt the previous proofs: low camera, disconnected river, misplaced bridge, props over water, chimney smoke offset, visible tiling, pasted-object scale drift, debug labels, and non-game-like composition.

## Rollback

This is a visual-data change rather than new gameplay behavior. Rollback is the scene metadata pointer in `mods/rundale/scenes.json`: Kilteevan can return to the previous underlay/atom stack without changing the scene-state contract.

## Non-Goals

- Do not build ten variants in this milestone.
- Do not make terrain chunks the visible final art.
- Do not require decomposing the generated plate back into atoms yet.
- Do not replace the scene-state contract or movement/gameplay behavior.
