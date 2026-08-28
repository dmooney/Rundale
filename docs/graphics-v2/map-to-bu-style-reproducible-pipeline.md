# Map To BU-Style Reproducible Pipeline

## Purpose

This pipeline turns historical map data for a Kilteevan parish exterior into a
Graphics V2 background plate that is geometrically accurate, low 3/4
orthographic, and in the accepted BU E2 / parish-notebook concept-realism style.

This is not yet a production batch renderer. Current image models are not
reliable enough for arbitrary real-world locations or fully automated runtime
graphics. For now, scope is deliberately limited to Kilteevan parish exterior
locations where we have historical map evidence, can afford clean-context
subagent review, and can manually accept or reject the final plate.

The target outcome is reproducible in the practical sense: a fresh run with the
same saved source crop, control artifacts, prompt, and subagent contracts should
recover the same map interpretation and a comparable plate. Pixel-level
identity is not expected.

## Non-Negotiable Principle

Every major reasoning step runs in a clean-context subagent. A long exploratory
thread may coordinate the work, but it must not be the sole context that reads
the map, builds the render prompt, calls image generation, or judges the result.
The point is to avoid contaminating a new location with earlier failed renders,
hidden chat assumptions, Beechwood/Grove/Murphy overfitting, or hand-authored
local hints.

Preferred render path: the render subagent calls image generation with only the
declared source/control/style inputs. If the current tool surface prevents a
subagent from calling image generation, the coordinator may run the imagegen
call only from a deliberately narrowed/fresh context and must record that as a
pipeline exception in the report. Such an exception can produce a useful visual
candidate, but it is weaker recipe evidence than a true render-subagent run.

The historical map crop is always the primary evidence and veto authority. All
notes, controls, style targets, and renders are secondary aids.

## Required Artifacts Per Location

Each attempted exterior needs a saved artifact bundle. PNG working files use
the ignored `pipeline-experiments/` directory and retained runs are ingested
into the content-addressed external archive; prompt, report, and audit sidecars
remain tracked:

- canonical source historic map crop from `map-sources/` or an archived source
  tile/mosaic bundle,
- crop rationale: why this ground area and playable scale were chosen,
- clean-context map-reader notes,
- deterministic or reproducibly generated topology/control artifact,
- deterministic oblique/perspective cue,
- prompt-builder handoff note or final prompt sidecar,
- imagegen output copied into the ignored working directory and then archived,
- comparison plate: source map -> control/topology -> perspective cue -> render,
- independent audit report for geometry, perspective, style, doors, and
  historical semantics,
- bounded-correction report when a one-edit repair is used,
- model/tool metadata when available.

Do not count an image as recipe evidence if any of these are missing. Edited
visual targets can still be valuable, but they must be labeled as edited
visual-target evidence rather than fresh pipeline proof.

## Subagent Roles

Use clean-context subagents with narrow inputs. Do not fork the long exploratory
thread into these workers unless the task explicitly requires context from that
thread. Prefer passing only the files and instructions named below.

### 1. Map-Reader Subagent

Inputs:

- target historical map crop,
- `map-reader-stage-template.md`.

Must not receive:

- previous generated plates,
- user/location lore beyond the crop identity,
- style targets,
- failed attempts,
- hand-authored roads/buildings/walls,
- broad project history.

Output:

- confidence-graded map-reader notes saved beside the experiment artifacts.

Contract:

- Identify buildings, roads/lanes, boundaries, planting, likely yards/gardens,
  and negative evidence using the generic rubric.
- Preserve uncertainty instead of converting ambiguous linework into hard
  instructions.
- Printed labels, large letters, survey numbers, scan artifacts, and paper
  texture are ignore marks.
- Dotted, pecked, dashed, or dot-chain linework is nonphysical unless
  corroborated by physical evidence.

### 2. Control-Builder Subagent

Inputs:

- source map crop,
- map-reader notes,
- existing deterministic control scripts or documented manual-control recipe.

Output:

- topology/control artifact,
- oblique camera cue,
- short control report describing exactly what each color/mark means.

Contract:

- Control generation must be explicit and repeatable. Do not reuse an earlier
  Grove/Beechwood/Murphy control as a template unless the method used to produce
  it is also rerun for the new location.
- Controls are layout authorities, not style references.
- Do not invent topology the map-reader notes left uncertain.
- Remove or de-emphasize nonphysical survey/admin linework before final render
  when possible.

### 3. Prompt-Builder Subagent

Inputs:

- source crop,
- map-reader notes,
- topology/control artifact,
- oblique camera cue,
- BU E2 style target,
- approved door-fixed style crops,
- regional boundary/wall rules.

Output:

- final render prompt sidecar.

Contract:

- Name every image role explicitly: source authority, topology authority,
  camera cue, style-only reference, door-only reference.
- Put geometry and perspective requirements before style.
- Use BU E2 for material language, not layout.
- Use door-fixed crops only for fitted plank doors, thresholds, wall/roof
  texture, and low-camera facade treatment.
- Guard strongly against full-scene semantic leakage: no copied church, shop,
  graveyard, bridge, river, people, animals, signs, UI, carts, smoke, or
  whole-scene road/courtyard layouts.

### 4. Render Subagent

Inputs:

- final prompt,
- declared image references only.

Output:

- one fresh render saved into `pipeline-experiments/`.

Contract:

- The render subagent calls image generation where the tool surface permits.
- The render stage should not see prior failures for the same location.
- Use image generation only for the final candidate render or bounded edit.
- Deterministic crop/control/perspective artifacts may be produced locally.
- Stop after the planned render plus at most one bounded correction unless the
  user explicitly starts a new experiment.

### 5. Independent Audit Subagent

Inputs:

- source map crop,
- map-reader notes,
- topology/control artifact,
- final render,
- style target,
- door audit crops when needed.

Output:

- audit report with pass/fail/caveat bullets,
- comparison plate references,
- explicit recommendation: accept, bounded correction, or reject and revise
  inputs.

Contract:

- The auditor should not be the same context that generated the prompt.
- Failures must be concrete and auditable, not vibes.
- If the render fails broad topology or doors, revise upstream inputs rather
  than polishing the image.
- If the render is close but has one bounded failure, allow one targeted edit
  and audit it separately.

## Pipeline Steps

1. Choose the exterior and crop.
   Work only on Kilteevan parish exterior locations for now. Pick the desired
   playable plate scale first, then crop/back-calculate the historical map
   evidence needed for that area and nearby exits. Do not let an arbitrary map
   crop force a survey-board composition.

2. Run the map-reader subagent.
   Save the confidence-graded notes. The notes may be location-specific because
   they come from the crop, but the rubric and context must be uniform.

3. Build the topology/control and oblique cue.
   Use deterministic local processing where possible. If a generated top-down
   control is used, label it as generated and keep the source map as veto
   authority. North stays up.

4. Lock perspective before style.
   The ground plan remains north-up. The camera is low 3/4 orthographic or
   near-orthographic: no horizon, no vanishing-point composition, no steep drone
   view, no scenic rotation. Use marker overlays or scale checks when the
   candidate may be used with sprites; distant objects should not shrink merely
   because they are north/up-frame.

5. Build the style-last prompt.
   Layout comes from source/control/camera artifacts. BU E2 and the door-fixed
   crops only provide material, brushwork, door, facade, roof, wear, and
   notebook realism.

6. Render one candidate through the render subagent.
   Save the output under the ignored `pipeline-experiments/` working path,
   retain its prompt/report sidecars in Git, and ingest any retained run into
   the external archive. Do not leave the asset only in the generated-image
   cache. If imagegen had to be called by the coordinator because of tool
   limitations, write that exception into the report.

7. Audit independently.
   Create a comparison plate and audit against the gates below.

8. Make at most one bounded correction.
   A bounded correction may repair a concrete issue such as a missing door,
   over-strong survey line, or tiny roof artifact. It must not redo the layout
   or add richness. Label direct recipe evidence and edited visual targets
   separately.

## Acceptance Gates

Geometry:

- building count, grouping, separation/connection, relative sizes, and
  positions remain auditable against the map/control,
- major roads/lanes/yards/gardens/exits remain in source-supported positions,
- unsupported roads, paths, yards, gardens, wall grids, and extra buildings are
  absent,
- uncertain linework stays ambiguous or disappears into texture rather than
  becoming physical content.

Perspective:

- low 3/4 orthographic game camera, not a drone/survey board,
- north-up ground plan unless a future location explicitly requires otherwise,
- no horizon, no fisheye, no vanishing-point road composition,
- stable object/sprite scale from near to far,
- readable facades and thresholds without zooming so close that map context is
  lost.

Style:

- BU E2 / parish-notebook family: warm worn paper, rough ink, watercolor grain,
  stained limewash, rough roof texture, moss, weeds, ochre mud, handmade
  irregularity,
- neutral daylight base plate, readable and not over-dark,
- sparse practical rural wear, not repeated prop patterns or decorative clutter,
- no UI or in-world labels unless a future runtime layer adds them separately.

Doors:

- every visible person-sized opening on a walkable facade contains a fitted
  wooden plank door plus threshold/step,
- dark voids fail,
- door audit should use focused crops when buildings are small.

Historical semantics:

- no unsupported church, graveyard, water, bridge, shop, signs, people, animals,
  smoke, carts, UI, labels, or extra landmark structures,
- dotted/pecked/admin/survey lines disappear unless corroborated by physical
  evidence,
- ordinary Roscommon/Kilteevan field divisions default toward hedges, banks,
  ditches, remnant hedges, and stone-earthen banks instead of continuous stone
  wall grids.

Wall material:

- any real stone wall must be irregular dry fieldstone or slabby local limestone
  with mixed sizes, gaps, uneven coping, moss/lichen/weeds, and a broken
  hand-built silhouette,
- reject uniform rectangular blockwork, ashlar, brick-like courses, tidy
  cobblestone chains, and identical gray bead walls.

Reproducibility:

- all subagent outputs and prompts are saved,
- a fresh clean-context rerun should produce the same map interpretation and a
  comparable layout/style outcome,
- failures trigger upstream input revision or a bounded correction, not an
  unbounded polish loop.

## Known Validation Evidence

Grove BV showed that BU E2 style can transfer to a second topology, but it used
earlier Grove controls and one bounded correction. Treat BV as promising
validation, not proof that the pipeline generalizes to every exterior.

Grove BZ is the first clean subagent-gated proof run of this pipeline. It used
separate map-reader, control-builder, prompt-builder, render, and audit
subagents, and the render subagent called imagegen exactly once. The independent
audit verdict was PASS WITH CAVEATS: major Grove geometry, doors, camera, and
BU-style concept realism passed, but ambiguous boundaries still over-promoted
into continuous blocky stone walls. Treat BZ as proof that the staged workflow
can execute and preserve one Grove topology, not as proof of production batch
readiness.

Murphy BY/E3 showed the importance of separating geometry from style. E2d/E2f
locked the three-building geometry; E3c is the preferred edited
accuracy+style candidate. Because the E3 render/audit were not originally
delegated to clean subagents, count it as a strong candidate and a lesson for
this pipeline, not a fully disciplined recipe proof.

Future confidence requires repeated clean-context runs on distinct Kilteevan
parish exterior topology types: small farmstead by bog, separate yard buildings,
connected compound, road junction, edge-of-village cluster, and open-field
boundary-heavy sites.

## Overfitting Risks

- Copying Beechwood/BU E2's connected compound, garden, wall network, road, or
  clutter into unrelated locations.
- Treating Grove/BV plus Murphy/E3 as enough evidence for broad batch use.
- Hiding recipe failure behind edited visual targets.
- Using stronger negative prompt wording that fixes doors/chimneys while
  spending topology budget on scenic roads or wall grids.
- Letting generated controls smuggle artifacts or override the source map.
- Declaring success on one Kilteevan crop and assuming all Kilteevan exteriors
  will behave the same.

## Current Status

Use this subagent-gated pipeline for the next Kilteevan parish exterior pass,
but add a hard boundary-material gate before accepting a render: ambiguous
Grove/Roscommon field and enclosure edges should stay hedges, banks, ditches,
intermittent trees, wood fencing, or short broken dry-stone remnants unless the
source strongly supports a wall. Do not promote the pipeline to production
batch status until multiple distinct Kilteevan exterior topologies pass the
same clean-context workflow with saved artifacts, independent audits, and no
continuous wall-grid failure.
