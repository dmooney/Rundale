# Fresh Map-Control Notebook Cycle BA/BB

## Purpose

Cycle BA tests whether the AZ visual direction can be approached from the
map/control evidence stack without using a previous isomorphic render as an edit
target. This is the important recipe question: can we get the original parish
notebook look while preserving Cycle M/Q-style topology from reusable inputs?

Cycle BB is a bounded repair on BA. It is not recipe evidence, but it tests
whether the most obvious BA failure, overbuilt garden/field boundaries, can be
fixed without re-layout.

## Inputs

BA used only generic, repeatable inputs:

- Top-down cleaned control:
  `pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.png`
- Tight original map crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
- Tight cleaned no-admin crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
- Deterministic oblique camera cue:
  `pipeline-experiments/idea-ar-kilteevan-playable-control-oblique-raw-warp.png`
- Full notebook UI sample, style only:
  `illustrated-parish-notebook.png`
- Clean single-building slate and thatch references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
- Tree/field watercolor reference:
  `style-crops/illustrated-style-trees-fields.png`

BB used BA as the edit target plus the raw/cleaned map crop, top-down control,
and notebook style reference as constrained repair references.

No hand-authored location-specific road, building, boundary, or landmark notes
were used.

## Outputs

| Cycle | Image                                                                   | Prompt                                                                        | Report                                                                        | Result                                                                             |
| ----- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| BA    | `pipeline-experiments/idea-ba-kilteevan-fresh-map-control-notebook.png` | `pipeline-experiments/idea-ba-kilteevan-fresh-map-control-notebook.prompt.md` | `pipeline-experiments/idea-ba-kilteevan-fresh-map-control-notebook.report.md` | Best fresh no-prior-render attempt at the AZ/notebook direction; garden too walled |
| BB    | `pipeline-experiments/idea-bb-kilteevan-ba-boundary-soften.png`         | `pipeline-experiments/idea-bb-kilteevan-ba-boundary-soften.prompt.md`         | `pipeline-experiments/idea-bb-kilteevan-ba-boundary-soften.report.md`         | Useful boundary-softened repair; loses some crisp ink/facade density               |

## BA Result

BA is a meaningful recipe improvement over AW/AX because it does not use a prior
isomorphic plate and does not drift as far into a generic walled scenic
crossroads. It preserves the broad source/control family:

- roads remain wide and walkable,
- building clusters stay west/left of the planted area,
- the garden/orchard block stays east/right,
- open fields remain at the frame edges,
- no obvious chimneys, smoke, UI, people, animals, church, river, bridge, or
  shop leakage appear.

BA also has a strong notebook-style surface: sepia ink, watercolor mottling,
muddy roads, rough vegetation, and readable low-ish facades.

The failure is boundary authority. The garden/orchard becomes too physically
enclosed and internally subdivided, and the central road junction is still more
composed than the strict crop asks for. The model continues to interpret
top-down-control/garden lines as wallable or terrace-like features even when the
text says not to.

## BB Result

BB softens BA's most obvious garden-boundary failure. Many hard wall-like
outlines become earth banks, planting edges, scrub, and broken hedge texture.
Roads, buildings, doors, and broad topology are preserved.

The trade-off is that BB washes out some of BA's crisp ink/facade density. It is
less fortress-like, but not fully solved: some garden edges still read as fairly
continuous pale bands, especially around the lower/right plots.

## Current Ranking For This Tight Kilteevan Crop

- `AZ`: best visual target, but edit-target-only and therefore not recipe proof.
- `BA`: best fresh no-prior-render recipe attempt at the notebook target.
- `BB`: best boundary-softened BA repair, useful visual candidate with softer
  boundaries but less crispness.
- `AU`: strongest topology-preserving edit target before notebook refinement.
- `AT`: best two-step recipe signal feeding AU/BA.
- `AW/AX`: useful style/door evidence only; topology is worse.

## Prompt Lessons

Keep these in future prompts:

- "Every person-sized dark vertical opening must contain a visible wooden plank
  door" works better than "readable doorway."
- Top-down controls must be described as fallible organization aids, not truth.
- Raw and cleaned map crops must explicitly veto generated seams, erased admin
  marks, and unsupported continuous boundaries.
- The full notebook sample can help the look when loudly constrained to
  style-only, but it should not be the only style input.

This is still not enough:

- Prose-only "do not make walls" cannot fully prevent the model from rendering
  garden/internal lines as physical enclosure.
- A bounded de-wall edit can help, but it trades off against crisp notebook
  density.

## Next Direction

The next real pipeline improvement should happen before final imagegen. The
control stage needs a boundary-material channel that distinguishes:

- roads/yards as walkable corridors,
- buildings as footprint/roof candidates,
- gardens/orchards as soft planted texture zones,
- domestic/garden boundaries as optional broken low features,
- ordinary parcel/admin/no-data lines as non-physical or no-trace zones.

In other words, do not ask the final model to infer "this garden line is
planting texture, not wall" from prose alone. Give it a control artifact where
the garden region is explicitly soft planting and where wallable edges are rare.
