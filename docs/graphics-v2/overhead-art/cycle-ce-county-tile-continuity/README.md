# Cycle CE County Tile Continuity Experiment

## Question

Can we make overhead map tiles for a county while preserving continuity across runtime tile edges?

## Experiment

- Center coordinate: `53.63579941, -8.07966297`
- Center XYZ tile: `z17-x62594-y42309`
- Zoom: `17`
- Grid: `5x5` NLS source tiles
- Source URL: `https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`

Compared two workflows on the same real Roscommon NLS tile mosaic:

1. **Independent runtime tiles:** each `256x256` source tile gets local tone/style processing.
2. **Mosaic-first / supertile-first:** the whole mosaic gets one shared transform, then runtime tiles are exported.

The second workflow is the county-scale rule we need if imagegen or a future style model is used: operate on padded mosaics/supertiles, crop safe centers, and never ask the renderer to invent continuity tile-by-tile.

## Metrics

Seam ratio = mean luminance jump at tile edges divided by nearby non-edge jumps. Lower is better; values near the raw-source baseline mean the tile grid is not adding its own seam.

```json
{
  "source": {
    "seam_mean_abs_luma_jump": 2.097756862640381,
    "nearby_control_mean_abs_luma_jump": 1.9055110067129135,
    "seam_to_control_ratio": 1.1008893967288593
  },
  "independent_tiles": {
    "seam_mean_abs_luma_jump": 10.691583275794983,
    "nearby_control_mean_abs_luma_jump": 5.508934825658798,
    "seam_to_control_ratio": 1.9407714220900056
  },
  "mosaic_first": {
    "seam_mean_abs_luma_jump": 5.512311935424805,
    "nearby_control_mean_abs_luma_jump": 5.2063866555690765,
    "seam_to_control_ratio": 1.0587596158515218
  },
  "max_abs_reassembly_error": 0,
  "tile_count": 25,
  "tile_size": 256,
  "grid": "5x5"
}
```

## Result

- Independent per-tile styling creates an avoidable tile-grid artifact because every tile normalizes tone and texture separately.
- Mosaic-first styling keeps the runtime export exactly reassemblable (`max_abs_reassembly_error = 0`) and avoids adding an extra tile-grid seam beyond the source/map content.
- This proves the first production primitive: **county maps should be generated/stylized as larger continuous supertiles, then split into runtime tiles mechanically.**

## Imagegen Supertile Result

A follow-up built-in imagegen pass used the same 5x5 NLS source mosaic as
geography authority and the Cycle CB overhead map sample as the style reference.
The model generated one continuous art supertile:

- `murphy-z17-imagegen-continuous-supertile.png`

That generated supertile was normalized from `1402x1122` to `1280x1024`, split
into `5x4` runtime tiles, and reassembled mechanically:

- `murphy-z17-imagegen-continuous-split-contact-sheet.png`
- `murphy-z17-imagegen-continuous-grid-overlay.png`
- `murphy-z17-imagegen-continuous-runtime-tiles/`
- `murphy-z17-imagegen-continuous-runtime-reassembled.png`
- `murphy-z17-imagegen-continuous-split-metrics.json`

Imagegen split metrics:

```json
{
  "original_size": [1402, 1122],
  "normalized_size": [1280, 1024],
  "grid": "5x4",
  "tile_count": 20,
  "seam_to_control_ratio": 1.008927045504885,
  "max_abs_reassembly_error": 0
}
```

Visual read: the imagegen supertile is not perfect map transcription, but it is
a usable proof that one continuous generated supertile can be split into runtime
tiles without visible grid seams. Roads and field boundaries cross the green
diagnostic grid naturally. This supports the supertile-first approach for
county-scale art.

## Adjacent Supertile Result

The next test prepared a `6x5` NLS source mosaic and two overlapping source
windows:

- `murphy-overlap-west-source-input.png`
- `murphy-overlap-east-source-input.png`
- `murphy-overlap-manifest.json`

Each window was rendered independently with the same prompt shape. The safe
centers were cropped and stitched:

- `murphy-overlap-west-imagegen-supertile.png`
- `murphy-overlap-east-imagegen-supertile.png`
- `murphy-overlap-independent-imagegen-safe-centers-stitched.png`
- `murphy-overlap-independent-imagegen-stitch-contact-sheet.png`
- `murphy-overlap-independent-imagegen-stitch-metrics.json`

Result: **safe-center overlap alone failed**. The stitched join was visibly
obvious and measured badly:

```json
{
  "join_to_control_ratio": 2.747887018399838
}
```

This is the decisive county-scale finding. We cannot rely on independently
generated neighboring imagegen supertiles to match, even if their source windows
overlap and the prompt asks for continuity.

## Seam Repair Result

A follow-up imagegen seam repair edited the stitched map and asked only for the
vertical center seam to be blended:

- `murphy-overlap-seam-repair-imagegen.png`
- `murphy-overlap-seam-repair-imagegen-split-contact-sheet.png`
- `murphy-overlap-seam-repair-imagegen-split-metrics.json`
- `murphy-overlap-seam-repair-imagegen-runtime-tiles/`

The repair brought the seam metric back to the acceptable range:

```json
{
  "seam_to_control_ratio": 1.007601247025902,
  "max_abs_reassembly_error": 0
}
```

Visual read: the seam is removed, but the repair behaves like a broad repaint
rather than a guaranteed local inpaint. It is useful as a repair primitive only
if bounded by masks/semantic overlays and audited against source geometry.

## Working Pipeline From CE

The experiment supports this pipeline:

1. Build a real source mosaic and semantic/control layers in county coordinates.
2. Generate or stylize the largest continuous supertile/panel the tool allows.
3. Mechanically split that continuous output into runtime tiles.
4. When two generated panels must meet, stitch their safe centers into a working
   canvas and run a bounded seam repair before final export.
5. Reject any panel where roads, paths, buildings, water, or material classes
   drift from the source/control layers.

The experiment rejects this pipeline:

1. Generate each runtime tile independently.
2. Generate adjacent supertiles independently and trust overlap/safe-center
   cropping to hide the join.

## What This Does Not Prove Yet

- It does not solve semantic extraction for roads/buildings/boundaries.
- The deterministic transform is only a seam test surface; the imagegen pass is
  closer to the Cycle CB/CD art target but still needs semantic controls to
  keep map interpretation from drifting.
- It does not prove the seam repair can be constrained tightly enough for
  production without masks. The first repair succeeded visually but should be
  treated as a broad repaint until a masked/semantic repair pass proves tighter.

## Next Required Experiment

Run the same adjacent-supertile test with explicit semantic seam overlays and a
masked repair band. The acceptance test is a 2x1 or 2x2 contact sheet where the
repair removes the visual seam while preserving every road, path, water,
building, and boundary crossing from the source/control layers.

## Source Tiles

- `z17-x62592-y42307`
- `z17-x62593-y42307`
- `z17-x62594-y42307`
- `z17-x62595-y42307`
- `z17-x62596-y42307`
- `z17-x62592-y42308`
- `z17-x62593-y42308`
- `z17-x62594-y42308`
- `z17-x62595-y42308`
- `z17-x62596-y42308`
- `z17-x62592-y42309`
- `z17-x62593-y42309`
- `z17-x62594-y42309`
- `z17-x62595-y42309`
- `z17-x62596-y42309`
- `z17-x62592-y42310`
- `z17-x62593-y42310`
- `z17-x62594-y42310`
- `z17-x62595-y42310`
- `z17-x62596-y42310`
- `z17-x62592-y42311`
- `z17-x62593-y42311`
- `z17-x62594-y42311`
- `z17-x62595-y42311`
- `z17-x62596-y42311`
