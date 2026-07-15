# Cycle CE Prompt Set

## Imagegen Continuous Supertile

Inputs shown to built-in image generation:

- Image 1: `../cycle-cb/idea-cb-b-no-legend-clear-game-map.png`
- Image 2: `murphy-z17-nls-source-mosaic-5x5.png`

```text
Use case: stylized-concept
Asset type: Graphics V2 county overhead watercolor supertile continuity experiment

Input images in this conversation:
- Image 1: approved Cycle CB overhead watercolor game-map style sample. Use it for palette, ink line quality, paper texture, and overhead map feel only.
- Image 2: real 5x5 NLS Roscommon historic source-map mosaic around Murphy's Farm. Use it as the geography/layout authority.

Primary request:
Create one continuous overhead ink-and-watercolor gameplay map supertile from the full source mosaic. This is not a finished single scene; it is a larger continuous map panel that will be mechanically split into runtime tiles later.

Geometry/layout:
Preserve the broad source layout: the west-east lane across the upper-middle, the diagonal lane/boundary running through the right half, the central farmstead/field enclosure, the lower-right small building group, the wooded/vegetation strip near the lower center, the northern boundary curve, and all exits continuing off-frame. Keep north-up overhead orientation. Do not recenter, rotate, crop into a scenic composition, or invent a new crossroads.

Style/medium:
Flat overhead rural Irish watercolor game-map surface, like the approved sample: parchment ground, muted moss and straw greens, pale dirt lanes/yards, raw umber and warm grey, fine black-brown ink, irregular hand-painted field/boundary marks, soft paper grain. Keep buildings as flat roof-footprint map shapes, not perspective architecture.

County tile constraints:
The entire image must feel continuous edge to edge, with no visible tile grid, no panel borders, and no sudden changes in palette or detail density. Roads, boundaries, tree belts, bog/rough pasture marks, and field lines should continue smoothly across where future tile edges might be.

Material rules:
Roscommon boundaries should mostly read as hedges, banks, ditches, earthen/stone banks, or overgrown field edges. Use dry fieldstone only as low irregular local fragments where appropriate. Avoid uniform rectangular block walls, bead-chain stones, estate walls, or hard stone grids.

Runtime constraints:
Neutral daylight. No people, animals, carts, smoke, weather effects, labels, nameplates, speech bubbles, compass, UI, legend, border, or readable text. Suppress source map letters/numbers as physical scenery; do not turn printed letters into objects. Keep it strictly overhead and tileable as a continuous gameplay map surface.

Avoid:
Isometric/perspective camera, side-view buildings, doors/windows/facades, modern map pins, decorative fantasy-map icons, labels, readable text, strong cast shadows, visible tile seams, independent tile panels, UI overlays.
```

## Local Commands

```sh
/private/tmp/rundale-graphics-venv/bin/python docs/graphics-v2/scripts/county_tile_continuity_experiment.py \
  --lat 53.63579941155877 \
  --lon -8.079662971357214 \
  --zoom 17 \
  --radius 2 \
  --out-dir docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity

/private/tmp/rundale-graphics-venv/bin/python docs/graphics-v2/scripts/split_generated_supertile.py \
  --input docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity/murphy-z17-imagegen-continuous-supertile.png \
  --out-dir docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity \
  --cols 5 \
  --rows 4 \
  --tile-size 256 \
  --prefix murphy-z17-imagegen-continuous
```

## Adjacent Supertile Prompt Shape

Inputs:

- Style reference: `../cycle-cb/idea-cb-b-no-legend-clear-game-map.png`
- West source: `murphy-overlap-west-source-input.png`
- East source: `murphy-overlap-east-source-input.png`

The west/east prompts used the same structure, changing only `WEST`/`EAST` and
the local feature list:

```text
Use case: stylized-concept
Asset type: Graphics V2 adjacent county overhead watercolor supertile experiment, WEST/EAST window

Input images in this conversation:
- Approved Cycle CB overhead watercolor game-map style sample: style only.
- The most recent source image is the WEST/EAST overlapping NLS Roscommon historic map window. Use it as geography/layout authority for this west/east supertile.

Primary request:
Create the WEST/EAST overlapping overhead watercolor supertile from this source window. It will later be cropped to its safe center and stitched to an independently generated neighbor supertile. Preserve enough edge continuity that the safe center can join a neighbor.

Geometry/layout:
Preserve the source layout, including the lanes, boundaries, farmstead/enclosure marks, rough pasture/bog marks, vegetation strips, building groups where present, and all exits continuing off-frame. Keep north-up overhead orientation. Do not recenter, rotate, invent a new crossroads, or turn the crop into a scenic composition.

Style/medium:
Flat overhead rural Irish watercolor game-map surface: parchment ground, muted moss/straw greens, pale dirt lanes/yards, raw umber/warm grey, fine black-brown ink, irregular hand-painted boundaries and vegetation, soft paper grain. Buildings must remain flat roof-footprint map shapes, not perspective architecture.

Overlap/continuity constraints:
This is one of two adjacent supertiles. Keep palette, paper texture, road width, field-boundary weight, vegetation density, and detail scale steady across the full image. Do not add decorative elements near the left/right edges just to fill space. Make roads and boundaries continue off-frame cleanly.

Material rules:
Roscommon boundaries should mostly read as hedges, banks, ditches, earthen/stone banks, or overgrown field edges. Dry fieldstone only as low irregular local fragments where appropriate. Avoid uniform rectangular block walls, bead-chain stones, estate walls, and hard wall grids.

Runtime constraints:
Neutral daylight. No people, animals, carts, smoke, weather effects, labels, nameplates, speech bubbles, compass, UI, legend, border, or readable text. Suppress source letters/numbers as physical scenery. Strictly overhead.

Avoid:
Visible tile seams, panel borders, isometric/perspective camera, side-view buildings, modern pins, labels, readable text, strong shadows, independent tile panels, UI overlays.
```

## Seam Repair Prompt

Input:

- Edit target: `murphy-overlap-independent-imagegen-safe-centers-stitched.png`

```text
Use case: precise-object-edit
Asset type: Graphics V2 county overhead map seam repair experiment

Edit target: the most recent image is a stitched overhead watercolor map made from two independently generated adjacent supertiles. It has a visible vertical seam exactly at the center of the image.

Primary request:
Repair the vertical center seam so the image reads as one continuous overhead watercolor map surface. Blend only the central seam band, roughly 10-15% of the image width around the center line. Preserve the rest of the map as much as possible.

What to fix:
- Make the horizontal road/lane cross the center seam continuously with consistent width, pale dirt color, ink edge weight, and tree/hedge detail.
- Blend field color, paper texture, rough-pasture hatch marks, hedges/banks, and watercolor grain across the center seam.
- Make the upper curving boundary/stream-like line and diagonal field boundary transition naturally across the join if they touch the seam band.
- Remove the abrupt left-green/right-yellow tone shift at the center.

Hard invariants:
Keep the image strictly overhead and north-up. Do not change the broad map layout. Do not add people, animals, carts, smoke, UI, labels, text, compass, borders, or tile grid. Do not turn buildings into perspective architecture. Do not create a new crossroads or reroute roads.

Style:
Same muted rural Irish ink-and-watercolor map style: parchment ground, moss/straw greens, raw umber/warm grey, fine black-brown ink, soft paper grain, irregular hedges/banks/ditches.

Avoid:
Global repaint, recropping, rotating, new landmarks, readable text, visible center seam, hard wall grids, uniform stone blockwork, modern map symbols.
```

## Adjacent Supertile Commands

```sh
/private/tmp/rundale-graphics-venv/bin/python docs/graphics-v2/scripts/overlap_supertile_experiment.py prepare \
  --out-dir docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity

/private/tmp/rundale-graphics-venv/bin/python docs/graphics-v2/scripts/overlap_supertile_experiment.py stitch \
  --out-dir docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity \
  --west-art docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity/murphy-overlap-west-imagegen-supertile.png \
  --east-art docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity/murphy-overlap-east-imagegen-supertile.png \
  --rows 5

/private/tmp/rundale-graphics-venv/bin/python docs/graphics-v2/scripts/split_generated_supertile.py \
  --input docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity/murphy-overlap-seam-repair-imagegen.png \
  --out-dir docs/graphics-v2/overhead-art/cycle-ce-county-tile-continuity \
  --cols 4 \
  --rows 5 \
  --tile-size 256 \
  --prefix murphy-overlap-seam-repair-imagegen
```
