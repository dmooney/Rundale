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
tiny-readability props, palette notes, or avoid-lists.

## Definitive Surface Direction

The actual world/scene layer is the painted watercolor surface. The rest of the
UI is pen-and-ink line art on paper with minimal value shading.

NPC portraits are UI art. They must be uncolored pen-and-ink: parchment/paper
tone is supplied by the UI, while the portrait source itself is transparent
alpha plus sepia or graphite line and sparse monochrome shading only. Do not add
colored wash, color fill, painted clothing blocks, baked parchment, paper
texture, background wash, or portrait-card treatment.

In-world NPC markers may use restrained watercolor because they sit on the
painted world surface. Keep marker color muted and subordinate to the scene.

## Source Asset Contract

Generate high-resolution canonical sources, then derive runtime assets from
approved sources. Do not ask the model for final tiny runtime pixels.

- Portrait source: `1024x1024` transparent-background PNG. The drawing occupies
  about 45% of source height, centered with generous transparent padding. Hair,
  head covering, and shoulders must be fully visible.
- Marker source: `1024x1024` PNG on a perfectly flat `#ff00ff` chroma-key
  background. The full-body figure occupies about 45% of source height, centered
  with feet visible and generous flat margins.
- Runtime portrait targets currently include approximately `99x112` px selected
  desktop, `58x66` px selected mobile, `51x57` px desktop nearby rail, and
  `36x41` px mobile nearby rail.
- Runtime markers are chroma-keyed to transparent PNG and depth-scaled by the
  notebook renderer.
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

Avoid saturated primaries. `#ff00ff` is allowed only as marker chroma key and
must not appear in the subject.

## Minimum Per-NPC Contract

Every named NPC needs:

- `portrait_identity.apparent_age`
- `portrait_identity.face_and_hair`
- `portrait_identity.clothing`
- `portrait_identity.pose_expression`
- `portrait_identity.props`
- `portrait_identity.palette_notes`
- `marker_identity.silhouette`
- `marker_identity.pose`
- `marker_identity.readable_props`
- `marker_identity.tiny_readability_notes`
- `avoid`
- `authoring_notes`

The marker fields are not optional. A good portrait description can still fail
as a game marker if the silhouette and large readable prop are undefined.

## Authoring Rules

- Start from canonical NPC/world facts, then add missing visual facts in the art
  supplement. Do not hide visual identity in one-off provider prompts.
- Use the illustrated notebook concept art as the only style authority for this
  slice. Do not use prior portrait experiments, marker sheets, procedural busts,
  or unrelated graphics cycles as substitutes.
- Keep source metadata provider-neutral. Provider/model syntax belongs in the
  later generation job, not in the NPC art-direction file.
- Treat source dimensions, transparency/chroma-key policy, concept palette, and
  sheet policy as global production constraints. Do not bury them in one-off
  per-NPC prompts.
- Avoid stereotype shortcuts. Occupation can suggest clothing and props, but it
  must not flatten the character into a caricature.
- Avoid anachronisms: no modern garments, modern tools, photography-era styling,
  Victorian fashion cues, fantasy costume, or uniform details not warranted by
  the NPC data.
- Keep tiny-readability explicit. At marker size, identity should come from
  silhouette, posture, and one or two large props, not facial detail alone.
- Preserve dignity and specificity for every NPC, including fallback art.

## Scaling Rule

At 23 NPCs, the supplement can be reviewed by hand. At 1,000 NPCs, the same
schema needs linting, batch reports, role defaults, and spot review. At millions
of NPCs, the authoring model has to become data-driven: controlled vocabularies
for clothing/body/prop classes, deterministic occupation defaults, confidence
scores, missing-field gates, sampled review queues, and automatic rejection of
records that would generate generic or stereotyped art.

The rule does not change with scale: the provider receives generated prompts
from structured source data, and any missing required visual contract is a data
error, not something to patch by hand in a prompt.
