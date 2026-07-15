# BZ Grove Subagent Control Report

Clean-context control-builder pass for Grove. This pass used the Grove source
crop, `prototype_map_controls.py`, and the Graphics V2 pipeline note. I did not
call imagegen and did not use previous generated final renders as layout
evidence.

Command run:

```sh
python3 docs/graphics-v2/scripts/prototype_map_controls.py \
  --input docs/graphics-v2/grove-map-target-site-crop.png \
  --out-dir docs/graphics-v2/pipeline-experiments \
  --prefix idea-bz-grove-subagent-control
```

## Layout Authority

Primary layout/control authority:

- `idea-bz-grove-subagent-control-literal-paint-control.png`

Use this as the deterministic north-up topology/control handoff beside the raw
source crop. It preserves the source crop geometry and muted linework while
avoiding hand-authored Grove-specific interpretation. It should be treated as a
layout aid, not as a style reference; the source crop remains the final veto
authority for map evidence.

Secondary material controls:

- `idea-bz-grove-subagent-control-boundary-material-control.png`
- `idea-bz-grove-subagent-control-soft-planting-control.png`

Use these only to communicate soft planting, hedges, scrub, and de-emphasized
linework. They are not hard wall, road, or building masks.

Diagnostics / weak cues:

- `idea-bz-grove-subagent-control-ink-mask.png`
- `idea-bz-grove-subagent-control-semantic-mask.png`
- `idea-bz-grove-subagent-control-linework-control.png`
- `idea-bz-grove-subagent-control-road-topology-control.png`

These are useful for checking what the script saw, but the semantic and road
classes are heuristic. Do not let them override the raw source crop or the
literal paint control.

## Camera-Only

Primary oblique camera cue:

- `idea-bz-grove-subagent-control-oblique-raw-warp.png`

This is suitable for a render prompt as a low 3/4 orthographic pitch cue. It is
camera-only: it should not be read as cleaned topology or material authority.

Additional camera/material cue options:

- `idea-bz-grove-subagent-control-literal-paint-oblique.png`
- `idea-bz-grove-subagent-control-boundary-material-oblique.png`
- `idea-bz-grove-subagent-control-soft-planting-oblique.png`
- `idea-bz-grove-subagent-control-oblique-ink-warp.png`
- `idea-bz-grove-subagent-control-road-topology-oblique.png`

Use these only when the render prompt needs the same y-squashed pitch applied to
a particular control channel.

Blockout cue:

- `idea-bz-grove-subagent-control-extruded-blockout.png`

This is camera/scale/facade cue only. The component detector reported 58
building-like components, and visual inspection shows it over-promotes map
texture, tree symbols, and linework into building-like boxes. Do not use it as a
building-count or footprint authority.

## Script Report Counts

The deterministic script reported:

- input size: 1200x820
- connected dark components: 229
- building-like components: 58
- small symbol-like components: 123
- suppressed/no-data comparison pixels: 0
- soft planting/material pixels: 97202
- soft planting suppressed-control pixels: 99789
- soft planting suppressed-control core pixels: 97268
- soft planting suppressed-control edge pixels: 2521

The generated script report is saved as
`idea-bz-grove-subagent-control-control-report.md`.

## Caveats

- No map-reader notes were used in this pass because the allowed inputs for this
  control-builder run were limited to the crop, script, and pipeline note.
- No `--original` comparison crop was supplied, so the script could not mark
  cleaned/suppressed admin or no-data areas; its suppressed/no-data count is 0.
- Road extraction is intentionally weak and noisy. The road-topology artifacts
  overpaint some vegetation and symbol clusters, so they are prompt cues at
  most.
- Building detection is only connected-component heuristics. The extruded
  blockout is useful for camera feel, but it is not reliable layout evidence.
- These artifacts are fresh deterministic controls, not renders, style
  references, or visual acceptance evidence.
