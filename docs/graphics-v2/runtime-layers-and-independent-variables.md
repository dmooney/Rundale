# Graphics V2 Runtime Layers And Independent Variables

## Purpose

This note captures the production-facing layer model implied by the Graphics V2
experiments. The main decision is to keep each location's generated background
plate as stable as possible, then express time, weather, season, life, props,
characters, and local state through runtime filters, masks, decals, particles,
and sprites.

The goal is to avoid generating many separate images per location when the
change should not alter geometry. Generated variants are expensive, and image
models tend to drift in doors, paths, walls, roads, roof shapes, and building
positions between versions. Runtime layers keep walkability, occlusion, and
sprite scale stable.

## Core Principle

Generate one canonical neutral plate per location:

- neutral diffuse daylight,
- no strong directional sunlight,
- no baked sunset/night color,
- no baked smoke, fog, rain, animals, people, or movable props,
- minimal baked shadows that would conflict with time-of-day filters,
- stable low 3/4 orthographic camera,
- readable doors, thresholds, roads, yards, gates, and paths.

Then derive or author supporting masks, sockets, and overlays for runtime.
Generate alternate full plates only for state changes that physically change
large parts of the world.

## Stable Per-Location Base

These are fixed for a location unless the location itself changes.

| Layer / Data               | Purpose                                                         |
| -------------------------- | --------------------------------------------------------------- |
| `base_neutral_day_plate`   | The canonical illustrated background plate.                     |
| `source_map_crop`          | Historic map source for audit/provenance.                       |
| `topology_control`         | Building/road/garden/boundary control or equivalent.            |
| `camera_metadata`          | Projection, north-up orientation, sprite scale, plate extent.   |
| `regional_boundary_prior`  | Hedge/bank/ditch/stone-wall material policy for the area.       |
| `building_facade_map`      | Building faces, door walls, windows, roofs.                     |
| `road_path_semantic_mask`  | Roads, lanes, paths, yard surfaces, thresholds.                 |
| `boundary_semantic_mask`   | Hedge, ditch, bank, stone-earthen bank, wall, fence, uncertain. |
| `vegetation_semantic_mask` | Trees, shrubs, hedges, garden rows, grass, scrub.               |
| `water_semantic_mask`      | Streams, ponds, wells, boggy/wet areas, if present.             |

The base plate should be treated as a location's visual geometry. It should not
contain momentary activity.

## Navigation And Rendering Masks

These layers make the plate playable.

| Layer / Data                | Purpose                                                               |
| --------------------------- | --------------------------------------------------------------------- |
| `walkable_mask`             | Surfaces a sprite may stand or move on.                               |
| `blocked_mask`              | Buildings, walls, dense hedges, water, tree trunks, impassable edges. |
| `soft_blocked_mask`         | Slow/avoid zones such as mud, high grass, shallow puddles, clutter.   |
| `occlusion_mask`            | Foreground pixels that can cover sprites.                             |
| `occlusion_height_map`      | Relative occlusion priority for roofs, trees, walls, hedges.          |
| `depth_sort_map`            | Stable y/depth sorting for sprites and props.                         |
| `interaction_zones`         | Doors, gates, wells, signs, carts, named object hotspots.             |
| `interior_transition_zones` | Door thresholds that transition to interiors.                         |
| `camera_safe_bounds`        | Crop-safe playable area and UI-safe margins.                          |
| `audit_markers`             | Optional debug layer for doors, gates, roads, walls, anchors.         |

These masks should align to the canonical plate. They should not change with
ordinary time/weather filters.

## Sockets And Emitters

Sockets are stable anchors where runtime systems can attach visual effects,
props, or state.

| Socket / Emitter      | Examples                                                               |
| --------------------- | ---------------------------------------------------------------------- |
| `door_sockets`        | Open/closed door overlays, threshold highlights, interior transitions. |
| `window_sockets`      | Night light, candle flicker, shutters open/closed.                     |
| `chimney_sockets`     | Smoke emitters, soot decals, seasonal hearth activity.                 |
| `hearth_sockets`      | Interior glow spilling through door/window openings.                   |
| `lamp_sockets`        | Lanterns, candles, chapel/pub doorway lights.                          |
| `gate_sockets`        | Open/closed gate variants, latch interactions.                         |
| `well_sockets`        | Bucket/rope/water interaction overlays.                                |
| `prop_anchor_points`  | Buckets, tools, barrels, baskets, turf stacks, hay.                    |
| `npc_spawn_points`    | Socially plausible standing/walking anchors.                           |
| `animal_spawn_points` | Livestock yard, roadside, pasture, hen-yard anchors.                   |
| `particle_emitters`   | Smoke, dust, rain splash, leaf flutter, mist pockets.                  |
| `audio_region_tags`   | Road, yard, trees, water, animals, pub, churchyard, interior edge.     |

Sockets should be named and typed. A chimney socket should know its pixel
position, depth sort value, smoke direction bias, and whether it is active for a
given household.

## Runtime Filters

These should usually be shader/color-grade style adjustments applied over the
same base plate.

### Time Of Day

| Variable  | Visual Treatment                                                     |
| --------- | -------------------------------------------------------------------- |
| Dawn      | Cool shadows, pale warm highlights, low saturation.                  |
| Morning   | Neutral warm daylight, clean readability.                            |
| Midday    | Neutral daylight, highest clarity.                                   |
| Afternoon | Slightly warmer, mild contrast.                                      |
| Dusk      | Amber/rose highlights, blue-green shadows, lowered brightness.       |
| Night     | Blue desaturation, lowered exposure, localized window/lantern light. |
| Midnight  | Deeper blue/gray, reduced color, stronger local light contrast.      |
| Moonlight | Optional cool rim/edge light, low saturation, high readability.      |

The base plate should be authored so all of these remain legible.

### Weather

| Variable       | Likely Runtime Layer                                               |
| -------------- | ------------------------------------------------------------------ |
| Clear          | No overlay, neutral color grade.                                   |
| Overcast       | Softer contrast, cooler tint, lower highlights.                    |
| Drizzle        | Subtle rain streaks, wet-road sheen, desaturation.                 |
| Heavy rain     | Rain particles, splash decals, darker roads, sound layer.          |
| Mist/fog       | Soft depth fog overlay, reduced contrast, localized low clouds.    |
| Storm          | Dark grade, wind motion, heavier rain, occasional lightning flash. |
| Frost          | Pale grass/roof edge decals, cooler grade, crisp contrast.         |
| Snowfall       | Falling snow particles, subtle accumulation if light.              |
| Wet after rain | Puddle decals, road darkening, reflective mud patches.             |
| Wind           | Smoke lean, leaf/grass animation, tree/hedge motion.               |

Rain, fog, snow, and wind should be dynamic overlays. Avoid baking them into
the plate except for rare full-state variants.

### Season

| Variable   | Likely Runtime Layer                                             |
| ---------- | ---------------------------------------------------------------- |
| Spring     | Fresh greens, blossom decals, wet ground, early garden growth.   |
| Summer     | Dense hedges, fuller trees, flowers, stronger garden growth.     |
| Autumn     | Muted grass, yellow/brown leaves, harvest clutter, muddy fields. |
| Winter     | Bare hedges/trees, dull grass, frost, lower saturation.          |
| Snow cover | Alternate accumulation layer or generated variant.               |
| Crop stage | Garden/crop decals: bare rows, sprouts, full growth, harvest.    |
| Leaf state | Leaf-on / leaf-off tree and hedge overlays.                      |

Season can be partly filter-based, but trees, hedges, crops, snow cover, and
harvest clutter likely need decal or alternate overlay layers.

## Stateful World Overlays

These are driven by world simulation or location state.

### Household And Building State

- Chimney smoke active/inactive.
- Smoke density, color, and direction.
- Window light on/off/intensity.
- Door open/closed/latched.
- Shutter open/closed.
- Gate open/closed/broken.
- Hearth glow.
- Occupied/unoccupied/abandoned state.
- Roof repair or leak marker.
- Soot, scorch, fire damage.
- Construction or repair scaffold.
- Ruin/collapse state.

### Daily Activity

- Washing line.
- Bucket at well.
- Tools outside a shed.
- Churns, barrels, baskets, crates.
- Turf stacks.
- Hay or straw piles.
- Cart present/absent.
- Market/fair setup.
- Chapel/pub/school crowding.
- Fresh footprints.
- Cart ruts.
- Mud churn near gates.
- Road debris or fallen branches.

### Characters And Creatures

These should be sprites/entities, not baked into background plates.

- Player.
- Named NPCs.
- Crowd/background villagers.
- Children.
- Cows.
- Sheep.
- Goats.
- Pigs.
- Hens.
- Horse/donkey.
- Dogs/cats.
- Birds.
- Carts/boats if mobile.

Actor sprites need stable sort anchors, collision radius, foot-point, scale,
and occlusion behavior.

## Props And Decals

Reusable placeable assets should be separate from the base plate when they can
appear/disappear or vary by season, event, or household.

| Prop / Decal Family | Notes                                                           |
| ------------------- | --------------------------------------------------------------- |
| Domestic            | Buckets, churns, baskets, benches, stools, laundry.             |
| Farm                | Tools, hay, straw, turf, sacks, barrels, carts, harness.        |
| Road                | Ruts, puddles, stones, mud patches, fallen branches.            |
| Garden              | Crop-stage rows, cabbages, herbs, weeds, flowering plants.      |
| Boundary            | Hedge gaps, broken gate pieces, fallen stones, ditch wetness.   |
| Ritual/social       | Well offerings, fair ribbons, chapel event markers, wake signs. |
| Damage              | Scorch, broken roof patch, collapsed wall, boarded window.      |

Props should have collision/occlusion metadata if they affect movement.

## When To Generate Alternate Plates

Generate a separate full image only when a runtime filter or decal would fight
the art too much or alter large areas of geometry.

Good candidates for alternate plates:

- Heavy snow cover on roofs/roads/fields.
- Flooding or seasonal inundation.
- Burned/damaged/ruined location state.
- Major construction/repair state.
- Interior/exterior alternates.
- A special night hub variant only if window/lantern light must be deeply
  painted into the architecture.
- Large fair/market/event set dressing if it substantially changes the place.

Bad candidates for alternate plates:

- Dawn/day/dusk/night color.
- Ordinary rain or overcast.
- Chimney smoke.
- Window light.
- NPCs or animals.
- Small tools, buckets, carts, barrels, or laundry.
- Gate open/closed.
- Minor mud/puddle changes.

## Suggested Render Stack

Render order should remain predictable:

1. `base_neutral_day_plate`
2. season ground/vegetation overlays
3. weather ground decals such as mud, puddles, frost, light snow
4. static state decals such as damage, repair, clutter, open gates
5. actors and movable props sorted through `depth_sort_map`
6. occlusion overlays from roofs, walls, hedges, trees, foreground structures
7. particles such as smoke, rain, snow, dust, leaves
8. emissive overlays such as windows, lanterns, hearth spill, firelight
9. global time-of-day/weather color grade
10. local post effects such as fog, lightning flash, vignette if used

The exact order can change by renderer, but color grading should not destroy
readability, and occlusion should remain stable across time and weather.

## Independent Variables Checklist

Use this as the high-level variable inventory.

- Location id.
- Plate id/version.
- Camera/projection version.
- Time bucket.
- Sun/moon/cloud state.
- Weather type.
- Weather intensity.
- Wind direction/speed.
- Season.
- Crop/garden growth stage.
- Leaf-on/leaf-off state.
- Ground wetness/mud level.
- Snow/frost accumulation.
- Window light state per building.
- Door state per building.
- Chimney smoke state per chimney.
- Gate state per gate.
- Household occupancy/activity.
- Props present/absent by anchor.
- NPC/entity positions.
- Animal positions.
- Event/festival/market state.
- Damage/repair/ruin state.
- Region boundary material prior.
- Audio ambience tags.

## Minimum Production Bundle Per Location

A first production-quality location should include:

- neutral base plate,
- source map/provenance,
- prompt/report sidecar,
- walkable mask,
- blocked mask,
- depth sort map,
- occlusion mask,
- road/path semantic mask,
- boundary semantic mask,
- door/window/chimney/gate sockets,
- NPC and animal spawn anchors,
- light and smoke emitter metadata,
- one validation screenshot/contact sheet with masks overlaid.

The first implementation can start smaller, but these are the layers we should
expect to need if the background plate becomes real gameplay terrain rather
than just concept art.

## Open Questions

- Should masks be hand-authored in an editor, semi-automatically derived from
  controls, or generated by a segmentation pass and corrected by hand?
- Do we store masks as raster PNGs, vector polygons, tiled data, or a hybrid?
- How many time buckets are visually distinct enough to justify separate LUTs?
- Should snow/flood/fire variants be generated per location or composed from
  reusable terrain/material overlays?
- Can we define a standard socket schema shared by doors, windows, chimneys,
  gates, props, and spawn points?
- How strict should the isomorphic/orthographic scale lock remain if the final
  art direction prefers the relaxed concept-art look?
