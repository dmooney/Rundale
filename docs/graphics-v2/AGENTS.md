# Graphics V2 Folder Notes

This folder contains exploratory concept art and prompt artifacts for a possible
graphical version of Rundale. These files are design references, not production
runtime assets.

## What Is Here

- `concept-7a-conversation-lens.png` — original reference for selected-NPC
  conversation gameplay. Useful for understanding the interaction model:
  select a person, read mood/knowledge, choose a compact action or type intent.
- `concept-7c-roads-and-schedules.png` — original reference for the preferred
  wider isometric zoom level. Useful for camera distance, exits, road context,
  and schedule/navigation readability.
- `illustrated-parish-notebook.png` — selected visual direction combining the
  7A conversation gameplay with the wider 7C camera.
- `illustrated-parish-notebook-prompt.md` — the prompt that produced the
  selected notebook UI render.
- `illustrated-parish-notebook-standalone-prompt.md` — a self-contained prompt
  for one-shot testing in other image models, replacing references to 7A/7C
  with explicit text.
- `illustrated-parish-scene-no-ui.png` — environment-only plate in the same
  illustrated style, with no UI.
- `illustrated-parish-scene-no-ui-prompt.md` — layout-first prompt for the
  no-UI environment plate, with explicit river, bridge, road, building, field,
  and footpath continuity constraints.
- `portable-background-plate-one-shot-template.md` — generic cleanroom prompt
  template for historic-map-tile-to-background-plate experiments.
- `one-shot-background-plate-test-protocol.md` — how to test one-shot prompts
  without contaminating the model context with previous failed/generated plates.
- `map-crop-selection-protocol.md` — how to choose/back-calculate historic map
  crops from desired plate scale instead of letting arbitrary source-map zoom
  drive the output composition.
- `map-to-background-plate-pipeline.md` — proposed reproducible map-reader and
  control-plate pipeline for consistent scale/perspective without hand-authored
  per-location hints.
- `map-reader-stage-template.md` — current generic rubric for producing
  reproducible, confidence-graded map-reader notes from a map crop before image
  generation.
- `map-to-background-plate-research-plan.md` — broader brainstorm and experiment
  plan for raw-map, warped-map, semantic-mask, blockout, procedural-render, and
  segmentation-model pipeline variants.
- `scripts/prototype_map_controls.py` — dependency-free prototype that emits
  rough control images from a historic map crop for clean-context experiments.
- `pipeline-experiments/` — generated control images and pipeline experiment
  renders. Treat these as research artifacts, not runtime assets.
- `one-shot-background-plate-candidate-cycle-j.md` — current best generic
  prompt/reference set for cleanroom historic-map-to-background-plate tests on
  Grove and Beechwood.
- `one-shot-background-plate-candidate-cycle-k.md` — current best candidate when
  building interpretation matters; uses the reproducible map-reader stage plus
  the Cycle J cleaned style swatches.
- `two-step-topdown-isomorphic-cycle-l.md` — experiment testing a top-down
  cleaned control plate followed by an isomorphic conversion; promising for
  stronger source topology, with native 16:9 framing still needing work.
- `lower-angle-notebook-style-cycle-p.md` — current best attempt to recover the
  original illustrated parish notebook look and lower playable 3/4 camera while
  preserving Cycle M's map accuracy.
- `camera-refinement-cycle-q.md` — current best overall candidate: refines the
  Cycle P notebook-style plates with the previous plate, Cycle M control, and a
  deterministic oblique camera-pitch cue.
- `close-crop-notebook-style-cycle-r.md` — close playable crop experiment that
  keeps the Cycle Q evidence stack but spends the same pixels on a smaller
  world area for more notebook-like scale/detail.
- `lower-rough-close-crop-cycle-s.md` — direct Cycle R refinement with stronger
  roughness/lower-camera language; useful mostly as a caution that over-anchoring
  on R preserves its polished isometric regularity.
- `tight-low-camera-thatched-cycle-t-u.md` — current best art/camera/material
  branch: tighter crops with cleaned door/threshold and thatch/no-chimney style
  references, plus the Beechwood topology caveat.
- `tight-control-cycle-v-w-x.md` — Beechwood follow-up using a tighter topology
  crop and then a smaller compound-focused crop; Cycle X is the current best
  Beechwood notebook-scale topology-preserving pass.
- `grove-beechwood-compound-crop-cycle-x-y.md` — paired Beechwood/Grove test of
  the current crop-scale method. Read this before deciding whether the pipeline
  generalizes beyond Beechwood.
- `paired-repair-cycle-z.md` — conservative repair pass on the X/Y pair.
  Beechwood Z and Grove Z are the current best candidates for the two tested
  topology types.
- `third-topology-kilteevan-cycle-ah.md` — third topology test from a
  data-derived NLS tile crop. Strong topology signal, but not a clean pass:
  it over-materializes at least one likely admin/survey boundary and adds a
  chimney-like stack.
- `boundary-roof-retry-cycle-ai.md` — prompt-only retries on the third crop.
  Roof/door/style improved, but admin-boundary no-trace still failed; points
  toward pre-cleaned physical-linework controls.
- `pipeline-experiments/idea-ak-beechwood-door-audit-repair.*` — bounded
  repair that fixes Beechwood's missed lower-right foreground cottage door.
- `pipeline-experiments/idea-al-beechwood-thatched-no-chimney.*` — bounded
  roof-material variant using the repaired Beechwood plate; useful for visual
  thatch/no-chimney evaluation, not recipe evidence.
- `cleaned-boundary-control-cycle-aj-am.md` — dot-chain suppression control
  experiment. Cycle AM improves the specific deleted-diagonal boundary failure
  but still over-materializes many thin plot lines as stone walls.
- `boundary-hierarchy-cycle-an.md` — direct-control retry with explicit
  boundary tiers and the full notebook sample as style-only. It improves style
  and facades, but still leaves too much continuous walling.
- `open-field-boundary-cycle-ao.md` — stricter open-field direct-control pass.
  Best current evidence for avoiding a stone-wall network, but it fails clean
  visual target status because of chimney/stub artifacts.
- `direct-open-fields-no-chimneys-cycle-aq.md` — fresh direct-control retry
  with stronger no-chimney/conflict wording. Better notebook style and roof
  discipline, but worse map/control fidelity than AO/AP3.
- `tight-control-no-scenic-crossroads-cycle-ar.md` — fresh direct-control retry
  using a tighter playable map/control crop. Better than AQ, but still
  regularizes roads into a scenic centered Y/crossroads.
- `road-topology-cue-cycle-as.md` — failed deterministic pale-corridor road cue.
  Do not use the AS/AS2 road-topology masks as render authorities.
- `tight-two-step-wall-door-cycle-at-au.md` — tight-crop two-step retry plus a
  bounded wall/door repair. AT is useful recipe evidence with boundary caveats;
  AU is a visual target only.
- `symbolic-topdown-control-cycle-av.md` — symbolic/minimal-boundary top-down
  retry. Useful negative result: less hard walling but more scenic drift and
  weaker shed-door readability than AU.
- `literal-paint-control-cycle-aw.md` — deterministic literal paint control
  retry. Strong style/camera/door result, but bad topology: the final image
  still regularizes into a scenic walled crossroads.
- `au-notebook-refinement-cycle-ay-az.md` — bounded visual refinements from AU.
  AZ is the best current visual target for the tight Kilteevan branch, but it is
  not one-shot recipe evidence because it edits prior rendered plates.
- `fresh-map-control-notebook-cycle-ba-bb.md` — fresh no-prior-render attempt
  at the AZ/notebook target plus a bounded boundary-softening repair. BA is the
  best fresh recipe attempt so far, but garden boundaries are still too wallable.
- `boundary-material-control-cycle-bc.md` — first deterministic
  soft-planting/boundary-material control attempt. Useful negative result:
  green planting tint was not enough because outlines still became walls and a
  roof nub returned.
- `soft-planting-control-cycle-bd.md` — stronger deterministic soft-planting
  control that suppresses wall-like garden edges. It improves door/roof
  discipline, especially with "doors on openings" wording, but still regularizes
  roads and garden boundaries too much for recipe status.
- `topology-target-notebook-refine-cycle-be-bh.md` — follow-up showing that
  raw/cleaned-map-only fresh rendering still composes scenic roads, while a
  Cycle-A-like topology target plus bounded notebook repaint plus local repair
  gives the best current visual target. Edit-target evidence only, not one-shot
  recipe proof.
- `beechwood-qm-notebook-refine-cycle-bj-bk.md` — Beechwood-specific bounded
  repaint from Cycle Q/M. BJ is the current Beechwood Q/M visual target:
  stronger notebook texture and door discipline with topology intact, but
  garden rows remain too survey-like. BL is the preferred queued next imagegen
  pass once credits are available.
- `beechwood-bj-visual-audit-crops.md` — focused crop audit explaining why BJ's
  remaining gap is garden regularity rather than compound topology, doors, or
  roof discipline.
- `handover-2026-06-28-notebook-qm.md` — current handover for the BJ/BL
  notebook Q/M branch. Read this first when resuming the active background-plate
  work.
- `isomorphic-transform-calibration-cycle-bm.md` — bounded six-render matrix
  that makes the top-down/control -> lower isomorphic transform explicit. The
  useful baseline is 55% playable crop plus `y_squash ~= 0.40`; camera/zoom
  improves strongly, while path/wall/garden semantics still need upstream
  material controls.
- `incremental-low-camera-cycle-bn.md` — north-extended low-camera follow-up
  to BM. BN E2 proves the camera can drop roughly 50% lower when the overhead
  source extends far north, but the lower angle makes garden/wall/path
  semantics more expensive.
- `orthographic-rectification-cycle-bo.md` — final-step test that turns BN E2's
  content-rich low-camera draft into a straighter low oblique orthographic
  plate. BO E2 is the preferred candidate; BO E1 proves the geometry but
  overbuilds fences/walls.
- `art-last-grid-check-cycle-bp.md` — order-of-operations test after BO:
  perspective/grid first, art last. BP E2 is the preferred visual target, with
  BP E1 retained as the stricter geometry baseline.
- `scale-lock-orthographic-cycle-bq.md` — correction to the BP grid definition:
  an isomorphic plate must preserve both parallel projection and constant
  object/sprite scale from foreground to top/north edge.
- `close-concept-relaxed-scale-cycle-br.md` — alternative concept-art branch:
  tiny Beechwood crop, slightly raised camera, relaxed isomorphic constraint,
  door-fixed references, and audit symbols added after generation.
- `door-height-calibration-cycle-bs.md` — BR follow-up using the original
  notebook's door height as the zoom/camera standard. BS E1 zooms out from BR
  while keeping fitted plank doors on visible openings.
- `concept-realism-weathering-cycle-bt.md` — BS E2 follow-up testing surface
  weathering, lived-in clutter, and irregular garden/wall prompts. Best prompt
  direction is sparse practical clutter plus capped irregular geometry.
- `concept-realism-convergence-cycle-bu.md` — BT follow-up and current relaxed
  concept-art visual target. BU E2 is the stop point for the BS/BT branch:
  warm, worn, handmade, fitted-door, no-UI Beechwood compound art.
- `map-to-bu-style-reproducible-pipeline.md` — reusable source/control/camera
  recipe distilled from BU, with Grove BV as the first validation.
- `kilteevan-exterior-pipeline-run-template.md` — per-location checklist for
  running the subagent-gated Kilteevan exterior pipeline and saving all
  reproducibility artifacts.
- `runtime-layers-and-independent-variables.md` — production-facing layer model
  for turning neutral generated plates into playable scenes with time,
  weather, season, actors, props, masks, sockets, and runtime overlays.
- `reproducible-pipeline-grove-cycle-bv.md` — Grove validation of the BU-style
  pipeline. BV E1 is direct recipe evidence; BV E2 is the preferred one-edit
  visual output.
- `reproducible-pipeline-grove-cycle-bz.md` — first fully subagent-gated Grove
  proof run. BZ passes with caveats: the staged chain preserves major geometry,
  doors, camera, and BU-style realism, but ambiguous boundaries still become too
  much continuous stone walling for batch readiness.
- `irish-dry-stone-wall-reference.md` — source-backed visual/audit rules for
  authentic Irish dry-stone walls. Use this before any future wall-material
  prompt, because uniform rectangular blockwork is wrong for historic field
  boundaries.
- `web-references/irish-dry-stone-walls/` — local copies of real-world
  Wikimedia Commons dry-stone wall references plus a two-image prompt sheet.
- `dry-stone-wall-authenticity-cycle-bw.md` — Grove material pass applying the
  dry-stone wall rules to BV. BW E3 is the first visibly different boundary
  pass, while BW E4 proves that a real wall reference alone is insufficient
  unless the prompt also stops over-promoting ordinary Roscommon boundaries into
  full stone walls.
- `murphy-farm-background-plate-cycle-bx.md` — Murphy's Farm application of the
  BU-style reproducible pipeline. E1 is direct recipe evidence; E2 is the
  preferred one-edit base plate. The west-side source texture is treated as peat
  bog / bog-edge terrain per user review.
- `grove-cleanroom-test-notes.md` — Grove experiment log and map-reading case
  study. Do not treat it as a reusable prompt; keep the reusable prompt generic.
- `style-crops/` — manually cropped or cleaned style references from approved
  illustrated concepts. Read `style-crops/README.md` before using them; it
  identifies recommended crops and known leaky intermediates.

## Working Guidelines

- Preserve the original images unless the user explicitly asks to replace them.
  Add new variants with descriptive filenames.
- Save the prompt beside any generated image so the idea can be reproduced or
  tested in another model.
- For environment plates, write layout constraints before style notes. Be
  explicit about rivers, bridges, roads, paths, and building placement; generated
  scenes often look beautiful while containing impossible geography.
- Keep prompts self-contained when they are intended for cross-model testing.
  Do not rely on hidden context or previous chat images unless the file clearly
  says it is reference-dependent.
- For reproducible pipeline evidence, use clean-context subagents or fresh model
  sessions for map-reading, control/topology interpretation, prompt assembly,
  rendering, and independent audit. Prefer a render subagent that calls image
  generation with only the declared inputs. If the coordinator must call
  imagegen because of tool limitations, record that exception and treat the
  result as weaker recipe evidence. Each subagent should receive only its
  declared inputs. Do not count a prompt evaluated in a context that has already
  seen failed renders as recipe proof.
- Do not create location-specific reusable prompts or hand-authored per-location
  hint notes. It is acceptable to create location-specific map-reader notes only
  when they are generated from the map crop by the same reproducible
  clean-context rubric for every location, saved as auditable artifacts, and
  kept confidence-graded.
- Treat historic map crop size as arbitrary source context, not as the output
  composition. Pick the desired plate scale, sprite scale, and orthographic
  camera first; then use or back-calculate the map crop needed to cover the
  named site's local area and exits.
- Keep reusable background-plate prompts north-up unless the user explicitly
  asks otherwise: source-map top should remain final-image top, with the 3/4
  game feel coming from oblique pitch and building extrusion rather than
  rotating the ground plan.
- Treat ambiguous historic-map linework conservatively in the generic prompt.
  Single thin lines are usually plot boundaries, hedges, walls, ditches, or
  overgrown walls, not extra footpaths. If more precision is needed, run the
  reproducible map-reader stage or another repeatable control process rather
  than writing hand-authored per-location notes.
- If a boundary becomes a stone wall, it must pass the Irish dry-stone wall
  rule: mortarless, irregular fieldstone or slabby local limestone, mixed
  shapes/sizes, visible gaps/chinks, uneven coping, moss/lichen/weeds, and a
  broken hand-built silhouette. Reject uniform rectangular block courses,
  castle/estate ashlar, tidy cobblestone chains, and identical gray beads. See
  `irish-dry-stone-wall-reference.md`.
- Treat base plates as static art layers. Avoid visible smoke/fog/weather in
  the plate; those belong in later runtime/composited layers.
- Avoid committing generated-image cache paths. Copy selected renders into this
  folder and reference them relatively from Markdown.
- For production-style one-shot tests, prefer small style/material swatches over
  full illustrated scenes. Full scenes are visually attractive but can leak
  landmarks, bridges, UI, props, and whole-scene layouts into unrelated map
  crops.
- For low-camera building style references, prefer
  `style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png`.
  Avoid `style-crops/illustrated-style-low-camera-building-door-clean.png` and
  `style-crops/illustrated-style-low-camera-thatched-door-clean.png` as general
  references; both were superseded by door-fixed variants after the old crops
  were found to teach dark doorway voids instead of fitted plank doors. See
  `style-crops/door-fix-cycle-2026-06-28.md`.
- Prefer Cycle K for building-heavy plates: run the reproducible map-reader
  stage first, then pass the map crop, map-reader note, and cleaned style
  swatches into the render prompt. The map remains the source of truth; the note
  is soft disambiguation with confidence.
- For active Kilteevan parish exterior work, follow
  `map-to-bu-style-reproducible-pipeline.md`. It is the current stricter
  subagent-gated pipeline: source crop, clean map-reader, reproducible
  topology/control, prompt-builder, fresh render, independent audit, and at most
  one bounded correction.
- Use Cycle L-style top-down cleaned plates when topology is hard to read. They
  are promising as an accuracy path, but do not treat them as source truth; the
  original map crop and map-reader note still outrank the cleaned plate. Before
  making Cycle L the default final-render path, solve native 16:9 framing
  without edge-extension artifacts.
- Use Cycle Q as the current final-render style/camera reference after the
  Cycle M topology path. Cycle P is still the best direct render prompt; Cycle Q
  improves camera consistency by using the successful prior plate plus the
  Cycle M control and an oblique warp as a pitch cue. The rough automatic
  extruded blockouts from `prototype_map_controls.py` are not ready to guide
  content because they over-detect texture as buildings.
- Use Cycle R when judging notebook-style richness and sprite-scale staging.
  Cycle R's main lesson is that the final plate should cover a smaller playable
  area than the arbitrary source-map crop. Keep roads/walls exiting off-frame
  rather than shrinking the whole map crop into one survey view.
- Treat Cycle S as a marginal branch, not the recommended endpoint. It shows
  that stronger wording alone does not overcome the clean generated plate used
  as the primary reference.
- Use Cycle U's style references for the next art/camera pass, but do not rely
  on prompt-only tight cropping for complex footprints. Beechwood U demonstrates
  that a connected courtyard can dissolve into separate buildings unless a
  tighter topology control crop is provided.
- Use Cycle X's crop policy for the next Beechwood-style batch experiment:
  generate a tight local topology crop before the final render, allow distant
  context to fall off-frame, and audit every visible playable facade for a real
  doorway/threshold. Cycle X improved the notebook feel because it reduced the
  depicted ground area, not because the prompt used stronger style adjectives.
- Treat the Beechwood X / Grove Y pair as the current leading evidence for the
  map-to-plate direction: X preserves a connected compound, Y preserves separate
  yard buildings. The pair is promising but not final; both still regularize
  garden/wall patterns and should be repeated on more crops before batch use.
- Prefer the repaired Cycle Z pair for visual references: Beechwood Z clarifies
  the side doorway without breaking the compound, and Grove Z lowers/roughens
  the separate-building yard plate without copying Beechwood. Do not keep
  polishing the same pair unless fixing a concrete audit failure; the next real
  confidence gain comes from another unrelated topology crop.
- Treat Cycle AA as a topology-control signal, not a visual target: direct
  local controls preserved Beechwood/Grove topology without prior rendered
  plates, but the results became too clean, high, and survey-board-like. Cycle
  AC is the follow-up using cleaned single-building style references.
- Treat Cycle AC as the best scalable direct-control branch so far. It preserves
  both tested topology types without prior rendered plates and avoids the leaky
  style-crop door issue, but it still needs a bounded style/camera refinement
  to reduce garden/roof/wall regularity and recover more of the original
  notebook's loose watercolor density.
- Treat Cycle AD as the current best visual repair pair, not a production
  one-shot recipe. It starts from AC and improves rough ink, mud, wall/garden
  irregularity, and facade readability while preserving topology. The remaining
  unsolved gap is a stronger reusable low-camera/facade cue for the direct
  control path.
- Cycle AE tests crop scale as that low-camera/facade cue: it uses smaller core
  topology controls with the same direct-control prompt family, no prior
  rendered plate. Audit whether it improves facade scale or merely crops away
  useful topology context.
- Cycle AE confirmed crop scale as the strongest direct-control lever for
  camera/facade readability so far. Its caveat is tiny chimney-like roof nubs;
  repair those concretely before treating the AE pair as the best visual target.
- Cycle AF repaired AE roof-nub defects surgically, but Beechwood AF still
  failed the all-visible-buildings doorway audit on the lower-right foreground
  thatched cottage. Cycle AG repairs that concrete failure. Treat Beechwood AG
  plus Grove AF as the current cleaned visual reference pair, and AE as the
  direct-control recipe evidence. The next confidence gain should be another
  unrelated topology crop, not more open-ended polish on Beechwood/Grove.
- Cycle AH is that unrelated topology crop. It preserves the central cluster,
  upper compound, planted enclosure, and broad lanes from a data-derived NLS
  z17 crop, but fails clean production criteria by turning likely survey/admin
  linework into walls and adding a chimney-like stack. Use it as evidence that
  the direct stack can generalize topology, and as evidence that boundary and
  roof protrusion suppression need another prompt/control cycle.
- Cycle AI shows prompt text can fix roof protrusions but not the admin-boundary
  problem. Even with a strict "no continuous trace" rule, the model keeps
  materializing likely dotted/pecked survey lines as walls or roads. The next
  credible move is an upstream cleaned map/control image that removes or
  de-emphasizes non-physical boundary linework before generation.
- Cycle AK fixes a visual-audit miss in Beechwood AG/AF: the lower-right
  foreground cottage needed its own clear doorway and threshold. Treat AK as a
  bounded repair target only.
- Cycle AL is a bounded thatched/no-chimney material variant from AK. It is
  useful as a roof-style sample, but it inherits the repaired render and should
  not be counted as direct one-shot recipe evidence.
- Cycle AM shows that the AJ2 cleaned no-admin control can reduce the specific
  bold-diagonal dot-chain failure. It is not final: the model still needs a
  stronger boundary-material hierarchy so thin/uncertain plot lines do not all
  become continuous stone walls.
- Cycle AN confirms the full illustrated notebook sample can be useful as
  style-only when the prompt loudly forbids semantic copying, but wall
  restraint still needs a stricter open-field-boundary rule.
- Cycle AO is the current best direct-control boundary-hierarchy signal:
  open fields remain open and topology survives. Keep its chimney/stub cleanup
  as a separate bounded edit so recipe evidence stays honest.
- Cycle AP/AP2/AP3 are bounded cleanup attempts from AO. AP still failed
  roof-stub removal; AP2 removed the lower-left chimney but kept a questionable
  upper-compound roof mark; AP3 removes that obvious mark and passes the
  inspected door crops. AP3 is the best cleaned AO visual target so far, but it
  still has whole-plate repaint softness. Do not treat it as direct recipe
  evidence or proof that the cleanup prompt is reliably pixel-local.
- Cycle AQ is a fresh direct-control retry using stronger roof/conflict
  language. It is prettier and mostly solves chimneys, but drifts into a
  generic composed crossroads with more walling and weaker map fidelity. Use
  its roof language, not its composition freedom.
- Cycle AR keeps AQ's roof language but uses a tighter playable map/control
  crop. It improves one-shot discipline and scale, but still regularizes road
  geometry into a centered scenic Y. The next attempt needs deterministic
  road/topology control, not just tighter cropping or more negative wording.
- Cycle AS/AS2 tried a deterministic pale-corridor road cue. It is too noisy:
  tree/symbol clusters and generic pale gaps are highlighted as roads. Do not
  feed these masks into imagegen as authorities.
- Cycle AT returns to the two-step path on the tighter playable crop. It
  improves camera, doors, road continuity, and no-chimney discipline over the
  direct AR render, but the generated top-down control still smuggles and
  strengthens boundary artifacts. Cycle AU repairs AT2's over-walled fields and
  marginal shed doors as a bounded edit. Treat AU as the current best visual
  target for this crop, not as one-shot recipe evidence. The next clean recipe
  should make top-down boundaries more symbolic/minimal and let the raw/cleaned
  map veto generated wall lines.
- Cycle AV tries that symbolic/minimal top-down control. It does not beat
  AT/AU: AV1 still redraws the scene into a prettier, wider plan, and AV2
  inherits scenic-crossroads regularization plus weak small-shed doors. The next
  step should reduce creative freedom before imagegen with a deterministic or
  literal paint-by-numbers control, not another freehand generated top-down
  control.
- Cycle AW shows that a deterministic literal paint control alone is still not
  enough. The result is visually excellent and door/roof disciplined, but the
  model overrides the control into a picturesque walled crossroads. Use AW/AX
  only as style and door-repair evidence, not topology evidence.
- Cycle AY/AZ shows bounded edits from AU can recover more of the original
  notebook texture while preserving the broad topology. AZ is the best visual
  target from this tight Kilteevan branch, but garden fencing became darker and
  more emphatic, so the reusable recipe still needs stronger geometry/boundary
  authority before final imagegen.
- Cycle BA is the best fresh no-prior-render attempt at the AZ direction. It
  preserves the broad map/control topology and achieves strong notebook style,
  but it still overbuilds the garden as physical enclosure. Cycle BB can soften
  that as a bounded edit, but loses some crisp ink/facade density. The next real
  gain needs a control artifact with explicit soft garden/planting zones and
  rare wallable edges, not just stronger prose.
- Cycle BC adds that first boundary-material control. It is not enough: the
  final render still turns garden outlines into hard edges, regularizes some
  vegetation, and reintroduces a main-roof chimney/nub. The next control should
  suppress or blur wallable garden perimeters, not merely tint dense planting
  zones green.
- Cycle BJ shows the bounded Q/M repaint path can improve Beechwood's notebook
  style while preserving the connected compound topology better than fresh
  renders. Treat BJ as a visual target only: its garden still reads too clean
  and plan-like. Cycle BL is the saved crop-aware next prompt to soften garden
  rows and lower facades without adding walling or changing topology.
- For the current BJ/BL branch, read
  `handover-2026-06-28-notebook-qm.md` before generating or editing another
  plate. BL is the preferred next clean-context imagegen run; BK remains saved
  only as an older queued prompt.
- Cycle BM confirms the top-down/control -> isomorphic transform should be an
  explicit deterministic step. A 55% crop around the playable core plus a
  `y_squash ~= 0.40` oblique pitch cue gives the best camera/zoom signal so far
  and generalizes to Beechwood/Grove, but walls/paths/garden rows still need
  upstream semantic/material controls rather than more camera wording.
- Cycle BN confirms the user's lower-camera diagnosis: the overhead source must
  extend far to the north before a 10-12 degree / roughly 50%-lower camera can
  keep the background source-backed. BN E2 is the new camera target proof, not a
  final recipe, because the lower angle hardens garden edges, fences, and
  boundary marks.
- Cycle BO confirms that camera lowering and orthographic rectification can be
  split into two imagegen steps. Use a low-camera, source-backed BN-style draft
  first, then a conservative "barrel correction only" pass. BO E2 is the best
  current final-step candidate; avoid BO E1's broad rectification wording
  because it adds too much fence/wall hardness.
- Cycle BP confirms BO's style breakdown was partly an ordering problem. Use a
  hard low-oblique isomorphic grid/check to hold projection first, then spend
  the final imagegen pass on notebook watercolor/ink style. BP E2 is the
  current visual target; BP E1 is the stricter geometry reference. This still
  does not solve garden/wall/planting semantics.
- Cycle BQ corrects the grid audit. Parallel lines are not sufficient: top/far
  trees must not shrink relative to near trees, or runtime sprites need
  y-dependent scaling. Use constant-size marker overlays as a required audit.
  BQ E1 partially fixes scale drift but hardens vegetation/garden material, so
  keep the audit and do not treat the render as final.
- Cycle BR is the current relaxed-scale concept-art branch. Shrinking the
  playable area and raising the camera slightly lets the model recover more of
  the original notebook warmth/detail. Door-fixed crops are mandatory here:
  older close Beechwood targets still teach black doorway voids.
- Cycle BS gates that branch by the original notebook's door height. BS E2 is
  the better scale target because it zooms out 20% from BS E1 while preserving
  readable fitted plank doors.
- Cycle BT shows concept realism needs use, not just dirt. Sparse practical
  clutter from BT E2 plus capped irregularity from BT E3 is the useful prompt
  direction; uncapped irregularity darkens and busies the plate too much.
- Cycle BU is the current relaxed concept-art visual target. BU E2 is close
  enough to stop this polish loop: warm paper, rough ink, muddy worn surfaces,
  fewer repeated props, readable fitted doors, and the BS E2 zoom/topology. It
  is still edit-target evidence, not a clean one-shot recipe.
- Cycle BV is the first reproducible-pipeline validation of BU on another
  location. The prompt shape works on Grove: E1 transfers style while preserving
  separate-building topology and doors, and E2 is the preferred one bounded
  correction. Before batch use, make control generation more explicit and test
  one unrelated third location.
