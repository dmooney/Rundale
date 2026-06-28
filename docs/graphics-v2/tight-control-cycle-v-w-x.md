# Tight Control And Compound Crop - Cycles V/W/X

Cycles V, W, and X test whether a tighter topology-control crop can preserve
Beechwood's connected compound while moving the art back toward the original
illustrated parish notebook look.

The motivation came from Cycle U: the thatched low-camera art direction worked
well, but Beechwood's connected courtyard footprint dissolved into detached
farm buildings. These cycles keep the same style direction while strengthening
the structure target.

## Inputs

The useful reference stack is:

1. A tighter top-down control crop around the target compound, derived from the
   existing Cycle M/V control plate.
2. A deterministic oblique raw warp of that crop, used only as camera-pitch
   cue.
3. The previous successful render for structure/style continuity.
4. `illustrated-parish-notebook.png` for broad ink/watercolor/camera mood.
5. `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`.
6. `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`.
7. Cleaned material swatches for walls, roofs, fields, and ground.

Do not use `style-crops/illustrated-style-low-camera-building-door-clean.png` or
`style-crops/illustrated-style-low-camera-thatched-door-clean.png`; both include
partial foreground/background buildings that can read as acceptable doorless
fragments.

## Outputs

| Cycle | Output | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| V | `pipeline-experiments/idea-v-beechwood-tight-control-thatched-render.png` | `pipeline-experiments/idea-v-beechwood-tight-control-thatched-render.prompt.md` | `pipeline-experiments/idea-v-beechwood-tight-control-thatched-render.report.md` | Structure pass, style/camera partial |
| W | `pipeline-experiments/idea-w-beechwood-v-structure-lower-notebook-refine.png` | `pipeline-experiments/idea-w-beechwood-v-structure-lower-notebook-refine.prompt.md` | `pipeline-experiments/idea-w-beechwood-v-structure-lower-notebook-refine.report.md` | Topology + doorway pass, still elevated |
| X | `pipeline-experiments/idea-x-beechwood-compound-focused-low-camera.png` | `pipeline-experiments/idea-x-beechwood-compound-focused-low-camera.prompt.md` | `pipeline-experiments/idea-x-beechwood-compound-focused-low-camera.report.md` | Best Beechwood notebook-scale pass so far |

Control artifacts added for Cycle X:

- `pipeline-experiments/idea-x-beechwood-compound-focused-control.png`
- `pipeline-experiments/idea-x-beechwood-w-compound-focused-reference.png`
- `pipeline-experiments/idea-x-beechwood-compound-focused-control-oblique-raw-warp.png`

## Lessons So Far

- A tight topology crop helps: Cycle V preserved the attached courtyard compound
  that Cycle U loosened.
- Subagent reports are not enough. Cycle V's auto-report said all doors passed,
  but direct inspection found the lower-right outbuilding doorway weak; the
  written report was corrected.
- Cycle W fixed the weak edge-building doorway and softened the watercolor
  texture without breaking the connected compound.
- The remaining gap is camera/composition, not just surface style. V/W still
  show a lot of garden grid, so they read more like controlled map plates than
  the original notebook sample.
- Cycle X tests a smaller compound-focused frame so the same 16:9 image spends
  more pixels on facades, thresholds, muddy yard, walls, and roof texture rather
  than the wider garden.
- Cycle X is the best Beechwood match so far for the original notebook feel:
  closer crop, larger facades, stronger thatch/limewash texture, readable
  courtyard thresholds, and a still-connected compound. Its remaining caution is
  a small right-side exterior opening that can read more like a window than a
  full playable doorway if that side face is intended to be navigable.

## Current Recommendation

Use X's compound-focused crop policy as the leading Beechwood style/camera path:

```text
topology path -> tight local control crop -> oblique pitch cue
  -> structure-preserving render
  -> strict visual audit for connected footprint + every visible door
  -> if still too high, crop smaller before the render rather than just
     strengthening "lower camera" wording
```

For broader pipeline confidence, repeat the X-style crop policy on another site
instead of continuing to refine Beechwood alone. If the same procedure produces
a Grove plate that stays topology-clean and close to the notebook sample, then
the current prompt/control stack is a stronger candidate for batch testing.
