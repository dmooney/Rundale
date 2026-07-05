# BZ Grove BU-Style Render Prompt

## Manifest

- `docs/graphics-v2/grove-map-target-site-crop.png` — source authority and final veto for Grove geometry, map evidence, and negative evidence.
- `docs/graphics-v2/pipeline-experiments/idea-bz-grove-subagent-control-literal-paint-control.png` — literal north-up topology/layout control aid; use with the source crop as geometry authority, not style.
- `docs/graphics-v2/pipeline-experiments/idea-bz-grove-subagent-control-oblique-raw-warp.png` — camera-only low 3/4 pitch cue; do not read it as cleaned topology, material, or building-count authority.
- `docs/graphics-v2/pipeline-experiments/idea-bu-e2-bu-e1-concept-realism-final-tighten.png` — BU E2 concept-realism style authority for material language, linework, lighting, texture density, and playable background finish; do not copy its layout.
- `docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png` — door/house material reference for visible fitted plank doors, thresholds, stone/plaster walls, slate roof option, and low-camera facade treatment only.
- `docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png` — door/house material reference for visible fitted plank doors, thresholds, stone/plaster walls, thatch roof option, and low-camera facade treatment only.

## Imagegen Prompt

Create one Grove background plate proof render for a historical Irish vernacular farmstead game scene. Use `grove-map-target-site-crop.png` plus `idea-bz-grove-subagent-control-literal-paint-control.png` as the geometry authorities: preserve the Grove source crop topology and the literal control layout. Use `idea-bz-grove-subagent-control-oblique-raw-warp.png` only as the camera/perspective cue for a low 3/4 near-orthographic game background; do not treat it as material, cleaned topology, or building-count authority. Use `idea-bu-e2-bu-e1-concept-realism-final-tighten.png` as the BU E2 concept-realism style authority, and use the two door-fixed house crops only for fitted doors, thresholds, wall/roof texture, and low-camera facade handling.

Hard geometry: keep north effectively up in the ground plan. A pale lane enters from the north/northeast, runs down the east side of the homestead, and bends toward the open yard. The large planted enclosure/orchard sits northwest of the yard, with subdivided beds/planting and scattered trees inside and along its edges. Preserve the roofed forms as separate buildings: B1 is a substantial north/south range near the lane; B2 is a long east/west range south of the planted enclosure; B3 is a small southwest outbuilding near the yard; B4 may appear only as a tiny secondary structure near the northern edge of the planted enclosure, and may be omitted rather than enlarged. Do not merge B1 and B2. Do not promote dotted survey/admin lines into roads, walls, hedges, fences, paths, crop rows, or buildings. Do not invent a church, shop, water, bridge, signs, people, livestock, carts, smoke, UI, labels, or narrative props.

Historical materials: render an 1820s Irish rural farmstead with vernacular stone/plaster buildings, thatch and/or slate roofs as appropriate to the references, muddy pale lane surface, compacted earth/stone yard, orchard/garden planting, hedges, ditches, scrub, and simple wood fencing where boundaries are ambiguous. Use authentic irregular dry-fit stone boundary walls only where a wall is actually needed by the source/control geometry. Avoid uniform rectilinear block walls, modern masonry, decorative estate walls, and overbuilt fortification. Field and enclosure edges may be hedges, ditches, intermittent trees, low rough walls, or wood fencing depending on local ambiguity.

Style and perspective: match BU E2 concept-realism: hand-painted but grounded, readable game-background detail, textured vegetation, irregular stone, muted natural greens/browns/cream plaster, neutral daylight, no dramatic sunset or fantasy lighting. Use a closer playable zoom than a survey map, low 3/4 near-orthographic perspective with visible facades, no horizon-line scenic view, no steep drone look, and no rotated scenic composition that breaks the source geometry. Every accessible building must have a real visible fitted door on a visible facade, with threshold/step treatment; no missing doors, blank facades, black doorway voids, or inaccessible-looking buildings.

## Negative Prompt / Failure Checklist

- No map letters, printed labels, survey text, crop names, UI marks, pins, captions, or signage.
- No dotted, pecked, dashed, or dot-chain survey/admin linework rendered as paths, roads, walls, hedges, fences, rows, or property features.
- No extra path promotion: only the pale north/northeast lane down the east side bending into the yard is a strong route; avoid adding new roads, crossroads, village streets, bridges, rivers, or courtyards copied from style references.
- No orthographic survey-board look, flat plan diagram, miniaturized model-table view, steep top-down drone view, or missing facade depth.
- No missing doors, dark door voids, fake painted door marks, inaccessible buildings, merged B1/B2 mass, enlarged B4, or excessive invented buildings.
- No church, graveyard, shop, water, bridge, people, livestock, carts, smoke, modern objects, fantasy props, or whole-scene layout copied from BU E2 or the door crops.
