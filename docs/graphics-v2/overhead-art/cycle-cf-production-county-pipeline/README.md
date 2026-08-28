# Cycle CF Production County Tile Pipeline Proof

## Purpose

Production-shaped proof for county-scale overhead map tiles. This run uses
real NLS Roscommon historic tiles, renders one continuous deterministic
county-base supertile, mechanically exports runtime tiles, and validates
tile seam continuity plus lossless reassembly.

## Source

- Center: `53.63579941155877, -8.079662971357214`
- Zoom: `17`
- XYZ range: `x=62589..62598`, `y=42304..42313`
- Source tile count: `100`
- URL template: `https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`
- Attribution: Historic 6" OS Ireland (1829-1842), via National Library of Scotland

## Outputs

- `source-mosaic.png`
- `county-base-supertile.png` (retired from Git; regenerate with the pipeline)
- `semantic-mask.png`
- `seam-contracts.json`
- `runtime-tiles/`
- `runtime-reassembled.png` (retired from Git; regenerate with the pipeline)
- `county-pipeline-proof-contact-sheet.png`
- `masked-seam-repair-template/`
- `masked-seam-repair-proof/`

The generated county supertile, reassembly, grid overlay, seam-validation
overlay, and seam-contract overlay are retired from the clean checkout. They
are generation outputs, not runtime assets; the tracked source mosaic,
semantic layer, manifests, metrics, and reproducible pipeline remain the source
of truth for regenerating the proof. Original blobs are recoverable from Git
history and the Wave 2 retirement ledger in `docs/agent/repository-artifacts.md`.

## Metrics

```json
{
  "seams": [
    {
      "orientation": "vertical",
      "x_px": 256,
      "between_cols": [0, 1],
      "mean_abs_luma_jump": 4.62288236618042,
      "nearby_control_mean_abs_luma_jump": 4.378909349441528,
      "seam_to_control_ratio": 1.0557154755373064
    },
    {
      "orientation": "vertical",
      "x_px": 512,
      "between_cols": [1, 2],
      "mean_abs_luma_jump": 4.3000807762146,
      "nearby_control_mean_abs_luma_jump": 4.5838096141815186,
      "seam_to_control_ratio": 0.9381019584475955
    },
    {
      "orientation": "vertical",
      "x_px": 768,
      "between_cols": [2, 3],
      "mean_abs_luma_jump": 4.524956703186035,
      "nearby_control_mean_abs_luma_jump": 4.16544234752655,
      "seam_to_control_ratio": 1.0863088060438444
    },
    {
      "orientation": "vertical",
      "x_px": 1024,
      "between_cols": [3, 4],
      "mean_abs_luma_jump": 4.68048095703125,
      "nearby_control_mean_abs_luma_jump": 4.640967607498169,
      "seam_to_control_ratio": 1.008514032605882
    },
    {
      "orientation": "vertical",
      "x_px": 1280,
      "between_cols": [4, 5],
      "mean_abs_luma_jump": 4.855849266052246,
      "nearby_control_mean_abs_luma_jump": 4.588292956352234,
      "seam_to_control_ratio": 1.0583128218370614
    },
    {
      "orientation": "vertical",
      "x_px": 1536,
      "between_cols": [5, 6],
      "mean_abs_luma_jump": 4.54111385345459,
      "nearby_control_mean_abs_luma_jump": 4.516627788543701,
      "seam_to_control_ratio": 1.0054213156490328
    },
    {
      "orientation": "vertical",
      "x_px": 1792,
      "between_cols": [6, 7],
      "mean_abs_luma_jump": 4.535578727722168,
      "nearby_control_mean_abs_luma_jump": 4.453360915184021,
      "seam_to_control_ratio": 1.0184619693090269
    },
    {
      "orientation": "vertical",
      "x_px": 2048,
      "between_cols": [7, 8],
      "mean_abs_luma_jump": 4.6762213706970215,
      "nearby_control_mean_abs_luma_jump": 4.643545627593994,
      "seam_to_control_ratio": 1.0070368088791577
    },
    {
      "orientation": "vertical",
      "x_px": 2304,
      "between_cols": [8, 9],
      "mean_abs_luma_jump": 4.282288551330566,
      "nearby_control_mean_abs_luma_jump": 4.40055513381958,
      "seam_to_control_ratio": 0.9731246220323205
    },
    {
      "orientation": "horizontal",
      "y_px": 256,
      "between_rows": [0, 1],
      "mean_abs_luma_jump": 5.102211952209473,
      "nearby_control_mean_abs_luma_jump": 5.555959582328796,
      "seam_to_control_ratio": 0.9183313659151685
    },
    {
      "orientation": "horizontal",
      "y_px": 512,
      "between_rows": [1, 2],
      "mean_abs_luma_jump": 4.74373197555542,
      "nearby_control_mean_abs_luma_jump": 4.749638199806213,
      "seam_to_control_ratio": 0.9987564896519835
    },
    {
      "orientation": "horizontal",
      "y_px": 768,
      "between_rows": [2, 3],
      "mean_abs_luma_jump": 4.976553440093994,
      "nearby_control_mean_abs_luma_jump": 5.27791154384613,
      "seam_to_control_ratio": 0.9429020169723175
    },
    {
      "orientation": "horizontal",
      "y_px": 1024,
      "between_rows": [3, 4],
      "mean_abs_luma_jump": 4.76964807510376,
      "nearby_control_mean_abs_luma_jump": 4.901432275772095,
      "seam_to_control_ratio": 0.9731131242351858
    },
    {
      "orientation": "horizontal",
      "y_px": 1280,
      "between_rows": [4, 5],
      "mean_abs_luma_jump": 4.613995552062988,
      "nearby_control_mean_abs_luma_jump": 4.475309133529663,
      "seam_to_control_ratio": 1.0309892377029928
    },
    {
      "orientation": "horizontal",
      "y_px": 1536,
      "between_rows": [5, 6],
      "mean_abs_luma_jump": 4.50407600402832,
      "nearby_control_mean_abs_luma_jump": 4.35522186756134,
      "seam_to_control_ratio": 1.034178313067281
    },
    {
      "orientation": "horizontal",
      "y_px": 1792,
      "between_rows": [6, 7],
      "mean_abs_luma_jump": 4.18587589263916,
      "nearby_control_mean_abs_luma_jump": 4.488756418228149,
      "seam_to_control_ratio": 0.9325246243349187
    },
    {
      "orientation": "horizontal",
      "y_px": 2048,
      "between_rows": [7, 8],
      "mean_abs_luma_jump": 4.775005340576172,
      "nearby_control_mean_abs_luma_jump": 4.573271036148071,
      "seam_to_control_ratio": 1.0441116003913942
    },
    {
      "orientation": "horizontal",
      "y_px": 2304,
      "between_rows": [8, 9],
      "mean_abs_luma_jump": 4.713523864746094,
      "nearby_control_mean_abs_luma_jump": 4.4905173778533936,
      "seam_to_control_ratio": 1.0496616465604915
    }
  ],
  "max_seam_to_control_ratio": 1.0863088060438444,
  "mean_seam_to_control_ratio": 1.0041981238429423,
  "seam_count": 18,
  "max_abs_reassembly_error": 0,
  "tile_count": 100,
  "tile_size": 256,
  "threshold": 1.15,
  "status": "pass"
}
```

## Imagegen Policy

Imagegen is not accepted as an independent county runtime-tile generator.
Cycle CE proved overlapping independent imagegen supertiles can fail at
their safe-center join. This pipeline therefore treats imagegen as optional
for high-value local panels only, and requires a masked seam repair package
plus validation before any repaired seam is accepted.

## Masked Repair Proof

`repair-seam` was run on the known-failed Cycle CE adjacent-imagegen stitch.
The repair was bounded to a `192px` vertical seam band and wrote
`masked-seam-repair-proof/repair-contact-sheet.png`.

- Before repair: `join_to_control_ratio = 2.933157159189815`
- After repair: `join_to_control_ratio = 0.7641509792052803`
- Status: `pass_metrics_requires_visual_topology_review`

This proves the local masked repair primitive can remove a color/texture seam.
It does not prove mismatched independent linework has been topologically
aligned; roads, paths, buildings, water, and boundaries still require visual
review against the seam contract.

## Validation

```json
{
  "status": "pass",
  "run_dir": "docs/graphics-v2/overhead-art/cycle-cf-production-county-pipeline",
  "tile_count": 100,
  "max_abs_reassembly_error": 0,
  "max_seam_to_control_ratio": 1.0863088060438444,
  "threshold": 1.15,
  "seam_contract_count": 18,
  "imagegen_independent_join_status": "fail_requires_repair",
  "masked_seam_repair_proof": {
    "status": "pass_metrics_requires_visual_topology_review",
    "report": "masked-seam-repair-proof/repair-report.json",
    "contact_sheet": "masked-seam-repair-proof/repair-contact-sheet.png",
    "before_join_to_control_ratio": 2.933157159189815,
    "after_join_to_control_ratio": 0.7641509792052803,
    "topology_review_required": true
  },
  "errors": [],
  "warnings": []
}
```
