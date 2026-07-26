# NPC Art Metadata Guidelines

These guidelines define the source data required before the automated notebook
person-art pipeline calls an image provider.

## Source Layers

1. `mods/rundale/npcs.json` remains the canonical NPC identity file: name,
   age, pronouns, occupation, personality, mood, home/workplace, relationships,
   knowledge, and schedule.
2. `mods/rundale/world.json` supplies setting and location context.
3. `parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json` supplies
   reviewed authoring-only visual art direction that is not present in the
   runtime NPC schema.
4. `parish-npc-tool art-inputs` merges those sources into
   `parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json`.

`npcs.json` by itself is not enough for production art. It has useful identity
and activity data, and some entries have strong brief visual cues, but it does
not consistently define face/hair, clothing, portrait pose, marker silhouette,
intrinsic tiny-readability cues, palette notes, or avoid-lists.

## Definitive Surface Direction

The actual world/scene layer is the painted watercolor surface. The rest of the
UI is pen-and-ink line art on paper with minimal value shading.

NPC portraits are UI art. They must be uncolored pen-and-ink: parchment/paper
tone is supplied by the UI, while the portrait source itself is transparent
alpha plus sepia or graphite line and sparse monochrome shading only. Do not add
colored wash, color fill, painted clothing blocks, baked parchment, paper
texture, background wash, or portrait-card treatment.

The portrait is diegetic: it is a quick observational sketch the player
character made in their working parish notebook after meeting the NPC. Author
and generate it as sparse, irregular, economical linework with open shapes and
only a few short hatch marks. Most of the face, hair, clothing, and canvas must
remain unfilled so the UI paper shows through. A polished editorial portrait,
formal study, smoothly modeled face, filled garment, or dense cross-hatching is
a style failure even when the identity and period clothing are correct.

In-world NPC markers may use restrained watercolor because they sit on the
painted world surface. Keep marker color muted and subordinate to the scene.
Each marker is a transparent character-only cutout with empty hands, not a
miniature vignette. Worn clothing and headwear are valid identity cues; held or
carried objects, extra people, furniture, architecture, vegetation, scenery
fragments, ground planes, and shadows are not.

## Source Asset Contract

Generate high-resolution canonical sources, then derive runtime assets from
approved sources. Do not ask the model for final tiny runtime pixels.

- Production generation request: one identity-locked `2048x1024` provider
  response per NPC. The left `1024x1024` cell is the portrait and the right
  `1024x1024` cell is the marker. Both derive from the same metadata record and
  must preserve the same apparent age, face structure, hairline, hairstyle,
  and expression cues. Split only at the deterministic 1024-pixel boundary.
- Portrait delivery source: `1024x1024` transparent-background PNG. The drawing
  occupies about 45% of source height, centered with generous transparent
  padding. Hair, head covering, and shoulders must be fully visible. A provider
  that cannot emit alpha may return a flat `#ff00ff` raw candidate only when the
  automated pipeline removes that key and validates the transparent derivative.
- Marker source: `1024x1024` PNG on a perfectly flat `#ff00ff` chroma-key
  background. The full-body figure occupies about 45% of source height, centered
  with feet visible and generous flat margins.
- Runtime portrait targets currently include approximately `99x112` px selected
  desktop, `58x66` px selected mobile, `51x57` px desktop nearby rail, and
  `36x41` px mobile nearby rail.
- Runtime markers are chroma-keyed to transparent PNG and depth-scaled by the
  notebook renderer.
- Approve or reject both children and their cross-asset identity together. If
  either fails, rerender the pair in one new provider call. Never assemble an
  approved identity pair from unrelated stochastic calls.
- Do not require a full per-character animation sprite sheet for this current
  slice. Generate one static marker per NPC. Pack reviewed runtime portraits and
  markers into a shared atlas only if renderer performance requires it. Add
  per-NPC facing/pose sheets later only when gameplay explicitly needs animated
  movement, directional facing, or state-specific work poses.

## Concept Palette

Use the palette from `docs/graphics-v2/illustrated-parish-notebook.png` as a
hard anchor, not generic "earth tones":

- parchment anchors: `#deccae`, `#d7c6a7`, `#c7b393`
- sepia/graphite ink anchors: `#36362e`, `#454339`, `#5c5747`
- olive-grey and weathered tan anchors: `#6e634e`, `#807661`, `#9e8e75`,
  `#b5a285`
- umber/shadow anchors: `#4c4c40`, `#766c56`
- marker watercolor accents: muted wool grey, bog green, dull brick red, peat
  brown, and faded indigo as subordinate accents only

Avoid saturated primaries. `#ff00ff` is allowed only as a removable provider
background key and must never appear in a portrait or marker subject.

## Minimum Per-NPC Contract

Every named NPC needs:

- `portrait_identity.visual_identity_seed`
- `portrait_identity.identity_cohort`
- `portrait_identity.apparent_age`
- `portrait_identity.facial_geometry.face_shape`
- `portrait_identity.facial_geometry.proportions`
- `portrait_identity.facial_geometry.brow_and_eyes`
- `portrait_identity.facial_geometry.nose`
- `portrait_identity.facial_geometry.mouth`
- `portrait_identity.facial_geometry.jaw_and_chin`
- `portrait_identity.facial_geometry.cheekbones`
- `portrait_identity.facial_geometry.hairline`
- `portrait_identity.facial_geometry.age_detail`
- `portrait_identity.distinguishing_features` with at least two unique entries
- `portrait_identity.hair`
- `portrait_identity.hair_topology.color_and_texture`
- `portrait_identity.hair_topology.front.family` and `.description`
- `portrait_identity.hair_topology.rear.family` and `.description`
- `portrait_identity.hair_topology.covering.family` and `.description`
- `portrait_identity.hair_topology.silhouette.family` and `.description`
- `portrait_identity.hair_topology.loose_details`
- `portrait_identity.clothing`
- `portrait_identity.pose_expression`
- `portrait_identity.props`
- `portrait_identity.palette_notes`
- `marker_identity.composition` set to `character-only`
- `marker_identity.silhouette`
- `marker_identity.stance`
- `marker_identity.empty_hand_pose`
- `marker_identity.readability_cues` with at least two entries using distinct
  `kind` values from `face`, `hair-or-headwear`, `clothing`, `body-shape`, and
  `stance`, plus a nonempty `description` for each
- `marker_identity.tiny_readability_notes`
- `avoid`
- `authoring_notes`

The marker fields are not optional. A good portrait description can still fail
as a game marker if the person-specific silhouette, stance, and intrinsic
readability cues are undefined.

## Authoring Rules

- Start from canonical NPC/world facts, then add missing visual facts in the art
  supplement. Do not hide visual identity in one-off provider prompts.
- Treat personality adjectives as expression direction, not facial identity.
  Words such as thoughtful, lively, practical, warm, guarded, or anxious do not
  satisfy any facial-geometry field.
- Keep identity dimensions independently observable in sparse line art. Within
  each cohort, at least four of the nine facial-geometry dimensions must differ
  between every pair; exact seeds and facial fingerprints must be unique. Hair
  is not allowed to make a near-duplicate face pass.
- Treat hair/headwear topology as structured identity rather than one prose
  sentence. `front`, `rear`, `covering`, and `silhouette` use lowercase
  kebab-case family keys plus literal visual descriptions. Within each cohort,
  every pair must differ in at least two of those four families; fallback must
  differ from every named person by the same threshold.
- Keep `portrait_identity.hair` as the provider-facing rendering sentence. It
  must faithfully compose the structured topology, including rear anchor
  height and geometry, even when the portrait needs a three-quarter view to
  reveal it. The topology itself is internal authoring data and is deliberately
  omitted from the v2 provider-input export so adding a lint-only category does
  not invalidate unchanged paid jobs. The review packet instead binds the exact
  per-subject topology vector and canonical digest; approval and promotion
  re-read that one source record, so topology cannot drift while unrelated NPC
  edits remain incremental.
- Encode family resemblance deliberately by sharing a limited number of cues,
  never by cloning the full facial vector. Spouses must not acquire resemblance
  merely because they share a household record.
- Generate the portrait and marker together from one shared `pair_prompt` so
  the model can carry one face and hair identity across both rendering modes.
- Use the illustrated notebook concept art as the only style authority for this
  slice. Do not use prior portrait experiments, marker sheets, procedural busts,
  or unrelated graphics cycles as substitutes.
- Do not upload one named NPC's full face or full-body marker as the style prior
  for an unrelated cast. A full-face image-edit reference can overpower textual
  geometry even when the prompt calls it style-only; use the authoritative full
  concept or a genuinely identity-neutral derivative.
- Keep source metadata provider-neutral. Provider/model syntax belongs in the
  later generation job, not in the NPC art-direction file.
- Treat source dimensions, transparency/chroma-key policy, concept palette, and
  sheet policy as global production constraints. Do not bury them in one-off
  per-NPC prompts.
- Avoid stereotype shortcuts. Occupation can suggest clothing, but it must not
  flatten the character into a caricature or make a prop carry the identity.
- Avoid anachronisms: no modern garments, modern tools, photography-era styling,
  Victorian fashion cues, fantasy costume, or uniform details not warranted by
  the NPC data.
- Keep tiny-readability explicit. At marker size, identity should come from
  face, hair/headwear, clothing, body shape, and stance, not facial detail or
  occupational props alone.
- Preserve dignity and specificity for every NPC, including fallback art.
- Review identity across the complete cast. Portrait-marker consistency within
  one pair is necessary but insufficient when another NPC has the same face,
  age treatment, hair construction, or marker silhouette.

## Historical Hair Evidence Boundary

Pinned or covered working hair, kerchiefs, linen caps, bonnets, centre parts,
controlled front curls, plaits, and rear arrangements are plausible around 1820. There was not one frozen rural-Irish hairstyle: surviving Irish clothing
evidence shows local materials coexisting with wider British and European
fashion. Sources also overrepresent garments affluent enough to survive. Use
the [National Museum of Ireland overview](https://www.museum.ie/en-IE/Museums/Decorative-Arts-History/Exhibitions/The-Way-We-Wore),
the near-date [1808 County Clare costume survey](https://clarelibraries.ie/localstudies/history/economy-and-industry-in-clare/costume-in-county-clare/),
and [National Library of Ireland Brocas material](https://catalogue.nli.ie/Collection/vtls000747996/CollectionList)
as boundaries, not as proof of a role-by-role Roscommon taxonomy. Exact
arrangements are reviewed reconstructions chosen for plausibility, work safety,
and cast readability. Never invent rules such as "all married women wore X."

## Scaling Rule

At 23 NPCs, the supplement can be reviewed by hand and every same-cohort vector
can be compared. At 1,000 NPCs, derive geometry from the stable seed using
versioned distributions and family constraints, then add linting, batch reports,
nearest-neighbour identity checks, role defaults, and sampled review. At millions
of NPCs, store seeds plus authored overrides, derive geometry on demand, use
controlled vocabularies and distribution-entropy gates, and reject collisions
through approximate-nearest-neighbour checks on both metadata vectors and
generated portrait embeddings.

The rule does not change with scale: the provider receives generated prompts
from structured source data, and any missing required visual contract is a data
error, not something to patch by hand in a prompt.
