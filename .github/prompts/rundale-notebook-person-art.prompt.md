---
description: Generate one production Rundale illustrated-notebook NPC portrait-and-marker pair prompt from an NPC art-input record.
agent: 'agent'
tools: ['search/codebase']
argument-hint: 'npc_record=<one npcs[] JSON object from npc-art-inputs-v1.json>'
---

# Rundale Notebook Person Art Prompt

Use this prompt file to turn one NPC record from
`parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json` into one
identity-locked image-model request for a tiny notebook portrait and tiny
in-scene marker generated together.

The visual authority is `docs/graphics-v2/illustrated-parish-notebook.png`.
Do not use prior experimental portrait sheets, marker concept sheets,
procedural busts, or unrelated graphics cycles as source art.

Definitive surface rule: the actual world/scene layer is the painted watercolor
surface. The rest of the UI is uncolored pen-and-ink line art on paper with
minimal value shading. NPC portraits are UI art, so portraits must not be
colored in and must not bake in parchment or paper texture. In-world NPC markers
may use restrained watercolor because they sit on the painted world surface.

Definitive marker composition: every marker is a transparent character-only
cutout, never a miniature narrative vignette. The marker must show one complete
person with empty hands and no held or carried object, extra person, furniture,
architecture, vegetation, scenery fragment, ground plane, or shadow. Worn
clothing and headwear are allowed. Identity must read from face, hair/headwear,
clothing, body shape, and stance alone.

Definitive portrait lore: each NPC portrait is a quick observational sketch the
player character made by hand in the margin of their working parish notebook
after meeting that person. It must read as lived-in notebook evidence, not as a
commissioned illustration, formal portrait study, character card, or polished
concept painting.

## Input

NPC record:

`${input:npc_record:Paste one npcs[] JSON object from npc-art-inputs-v1.json}`

Required fields:

- `name`
- `age`
- `occupation`
- `brief_description`
- `art_direction.portrait_identity.visual_identity_seed`
- `art_direction.portrait_identity.identity_cohort`
- `art_direction.portrait_identity.facial_geometry`
- `art_direction.portrait_identity.distinguishing_features`
- `art_direction.portrait_identity.hair`
- `art_direction.portrait_identity.clothing`
- `art_direction.portrait_identity.pose_expression`
- `art_direction.marker_identity.composition` (`character-only`)
- `art_direction.marker_identity.silhouette`
- `art_direction.marker_identity.stance`
- `art_direction.marker_identity.empty_hand_pose`
- `art_direction.marker_identity.readability_cues`
- `art_direction.marker_identity.tiny_readability_notes`
- `art_direction.avoid`
- `global_style` or equivalent copied style constraints

The provider-facing `hair` prose is compiled from the reviewed schema-v4
`hair_topology` in `npc-art-direction-v1.json`. That source record separately
classifies the front arrangement, rear anchor, head covering, and overall
silhouette and is rejected before export when it collides with another member
of the same cohort. Preserve every construction named in `hair`; do not simplify
plaits, rolls, crowns, loops, caps, or kerchiefs into a generic low bun.

## Output

Return exactly two sections:

1. `PAIR_PROMPT`
2. `ATOMIC_REVIEW_CHECKLIST`

Do not invent new biography facts. Use canonical NPC/world facts and the
reviewed art-direction fields. Add only composition and production constraints
needed by the image model.

## Shared Style Contract

Use the Illustrated Parish Notebook concept-art style:

- painted watercolor only for actual world/scene assets and in-world markers
- uncolored pen-and-ink for UI portraits, tabs, icons, notes, and UI furniture
- transparent delivery portrait line art composited over UI-controlled paper
- sepia or graphite ink outlines
- sparse irregular contours and open, unfilled interior shapes for portraits
- only a few short loose hatch marks where structurally necessary
- restrained muted wash only where explicitly allowed for world/marker art
- clean transparent delivery source, using a removable provider key when alpha
  output is unavailable
- compact game-readable people
- sparse handmade line economy
- rural County Roscommon, Ireland, 1820
- period-appropriate ordinary parish clothing
- dignity and specificity, no caricature

## Source Asset Contract

- Production provider response: one `2048x1024` PNG generated in one request.
  The left `1024x1024` cell is the portrait and the right `1024x1024` cell is
  the marker. Both cells must depict the same person with matching apparent
  age, facial proportions, eyes, nose, jaw, hairline, hairstyle, and expression
  cues. The pipeline splits the response at the fixed 1024-pixel boundary.
- Portrait delivery source: 1024x1024 transparent-background PNG. The drawing
  occupies roughly 45 percent of source height, centered with generous
  transparent padding, with the top of hair/head covering and shoulders fully
  visible. If the configured provider cannot emit alpha, its raw response may
  use perfectly flat #ff00ff only when the automated pipeline removes the key.
- Portrait background: true alpha/transparent. Do not generate parchment,
  paper texture, colored wash, portrait card, frame, border, label, shadowed
  backdrop, or other UI furniture in the portrait source.
- Portrait ink contract: leave most of the face, hair, clothing, and canvas
  unfilled so the notebook paper can show through. Do not underpaint skin,
  clothing, hair, or props with white, cream, parchment, gray, or skin tone.
- Marker source: 1024x1024 PNG on perfectly flat #ff00ff chroma-key background.
  The full-body figure occupies roughly 45 percent of source height, centered
  with feet visible and generous flat margins. It contains the character only,
  with empty hands and no props, other people, furniture, scenery fragments,
  ground plane, or shadow.
- Runtime derivatives: approved portrait sources are converted to transparent
  PNGs and composited over the UI paper; approved marker sources are chroma-keyed
  to transparent PNGs and depth-scaled by the notebook renderer.
- Pair review policy: portrait, marker, and cross-asset identity are approved or
  rejected together. If either child or their shared identity fails, regenerate
  both in one new request rather than mixing children from different calls.
- Cast review policy: compare the face, age, hair, and silhouette against the
  complete review cast. A candidate that matches its own pair but reads as a
  near-duplicate of another NPC is rejected and regenerated atomically.
- Hair review policy: compare front arrangement, rear anchor, head covering,
  and overall silhouette as four separate visual dimensions. Wording or color
  differences do not rescue two candidates that share the same visible hair
  topology.
- Sheet policy: do not require per-character animation sprite sheets for this
  slice. Generate one static marker per NPC; pack reviewed runtime assets into a
  shared atlas later only if renderer performance requires it.

## Concept Palette

Use the palette from `docs/graphics-v2/illustrated-parish-notebook.png`.

- parchment anchors: #deccae, #d7c6a7, #c7b393
- sepia/graphite ink anchors: #36362e, #454339, #5c5747
- olive-grey and weathered tan anchors: #6e634e, #807661, #9e8e75, #b5a285
- umber/shadow anchors: #4c4c40, #766c56
- marker watercolor accents: muted wool grey, bog green, dull brick red, peat
  brown, faded indigo only as subordinate accents
- avoid saturated primaries; #ff00ff is allowed only as a removable provider
  background key and never in the subject

Hard avoid-list:

- modern clothing
- Victorian bustle, top hat, bowler, modern uniform, photography-era styling
- fantasy costume or magic glow
- colored portrait wash, color fill, or painted clothing blocks in UI portraits
- polished portrait-card treatment
- procedural placeholder bust
- abstract icon variant
- text, labels, watermark, UI frame, border
- held or carried marker props, tools, books, vessels, bundles, or baskets
- marker furniture, architecture, vegetation, scenery fragments, ground plane,
  or shadow
- extra characters in either cell

## Pair Prompt Template

```text
Production task: Generate one identity-locked portrait-and-marker pair for <NAME>, age <AGE>, <OCCUPATION>, in rural County Roscommon, Ireland, 1820. Return exactly one 2048x1024 PNG on a perfectly flat #ff00ff background. The left 1024x1024 cell contains only the notebook portrait; the right 1024x1024 cell contains only the painted-world marker.
Shared identity invariant: Both renderings unmistakably depict the same person. Reproduce every supplied identity dimension across cells: <PORTRAIT_IDENTITY.facial_geometry.face_shape>; <PORTRAIT_IDENTITY.facial_geometry.proportions>; <PORTRAIT_IDENTITY.facial_geometry.brow_and_eyes>; <PORTRAIT_IDENTITY.facial_geometry.nose>; <PORTRAIT_IDENTITY.facial_geometry.mouth>; <PORTRAIT_IDENTITY.facial_geometry.jaw_and_chin>; <PORTRAIT_IDENTITY.facial_geometry.cheekbones>; <PORTRAIT_IDENTITY.facial_geometry.hairline>; <PORTRAIT_IDENTITY.facial_geometry.age_detail>; distinguishing features <PORTRAIT_IDENTITY.distinguishing_features>; hair <PORTRAIT_IDENTITY.hair>. Render the hair construction literally: preserve its stated front arrangement, rear anchor height and geometry, head covering, exposed hair, and complete silhouette; never replace it with a generic centre-parted low bun or low coil. Clothing: <PORTRAIT_IDENTITY.clothing>; expression: <PORTRAIT_IDENTITY.pose_expression>. Canonical biography cue for clothing and expression only, never for setting, activity, or objects: <BRIEF_DESCRIPTION>.
Left portrait artifact/lore: A quick observational head-and-shoulders sketch the player character drew by hand in the margin of their working notebook after meeting <NAME>. It is diegetic notebook evidence, not a commissioned illustration, formal portrait study, character card, or polished concept painting.
Left portrait style: Sparse uncolored sepia/graphite contours, economical irregular lines, open shapes, and only a few isolated short structural hatch marks. Do not cross-hatch or shade any broad region of the face, hair, neck, scarf, waistcoat, coat, dress, or apron, and do not render a dark garment as a filled or densely hatched mass. Keep the complete ink drawing between 40 and 60 percent of the left-cell height with generous key-visible padding. Every pixel that is not a dark ink stroke must remain flat #ff00ff, including uninked regions inside the face, hair, neck, clothing, and optional simply outlined props <PORTRAIT_IDENTITY.props>. Any portrait prop stays entirely in the left cell. No white, cream, parchment, skin-tone, gray, watercolor, wash, or other fill. Keep hair/head covering and shoulders fully visible.
Right marker role: One tiny static full-body transparent character-only cutout designed for compositing into the painted parish world. Use <MARKER_IDENTITY.silhouette>; stance <MARKER_IDENTITY.stance>; empty-hand pose <MARKER_IDENTITY.empty_hand_pose>; intrinsic readability cues <MARKER_IDENTITY.readability_cues>. Keep the complete figure roughly 45 percent of the right-cell height, acceptable range 40 to 60 percent, centered with complete feet and generous key-visible margins. The marker contains the person only. Both hands are empty. Do not add or copy any held or carried object, tool, book, container, bundle, baby or other person, furniture, counter, architecture, vegetation, scenery fragment, ground plane, or shadow. Worn clothing and headwear are allowed. Identity must read from the person alone. Do not illustrate the biography cue's occupation, workplace, activity, or narrative context around the marker.
Right marker style: Loose sepia/graphite contours with restrained translucent watercolor. Use only olive-grey, weathered tan, umber, muted wool grey, bog green, dull brick red, peat brown, and faded indigo as subordinate accents. Keep the face simple but preserve the shared identity cues.
Reference role: Use only the attached full Illustrated Parish Notebook concept. Read its notebook portrait line language for the left cell and its painted-world marker language for the right cell. Do not attach or copy a named character's full-face portrait or full-body marker as a shared cast reference; subject identity comes only from the structured facts above.
Sheet constraints: Exactly two depictions of one character, one per assigned cell. Keep the center boundary flat key. No labels, dividers, panels, frames, cards, duplicate poses, shared props, extra people, sprite-sheet poses, modern or fantasy elements, text, watermark, or <ART_DIRECTION.avoid>. Never copy a left-cell portrait prop into the marker.
Final invariant: Portrait, marker, and cross-asset identity form one atomic candidate. If either cell fails, regenerate the pair together.
```

## Review Checklist

- Portrait reads as a tiny notebook-margin asset, not a finished character card.
- Portrait reads as a quick sketch made by the player character in their working
  notebook, not professional character illustration.
- Portrait is uncolored pen-and-ink only, with minimal monochrome shading.
- Most of the face, hair, clothing, and canvas remain open and unfilled; there
  is no underpainting, tonal modeling, or dense cross-hatching.
- Portrait delivery candidate has transparent alpha with no baked paper/parchment;
  any keyed provider raw file is retained separately as provenance.
- Portrait has padding above hair/head covering and does not crop shoulders.
- Portrait source is 1024x1024 and the drawing uses roughly 45 percent of height.
- Marker is full-body, feet visible, and centered on a flat #ff00ff background.
- Marker source is 1024x1024.
- Marker occupies roughly 45 percent of image height and can be trimmed later.
- Marker uses the concept palette, with #ff00ff only as background key color.
- Portrait and marker unmistakably depict the same person, including apparent
  age, face proportions, hairline, hairstyle, and expression cues.
- Compared with the complete review cast, the candidate has a distinct face,
  age treatment, hair construction, and marker silhouette rather than reusing
  another NPC's identity template.
- Hair/headwear visibly preserves the record-specific front arrangement, rear
  anchor, covering, and silhouette; two candidates with the same visible
  topology fail even when their hair color or wording differs.
- Marker contains one character only, with both hands visibly empty.
- Marker has no held or carried object, extra person, furniture, architecture,
  vegetation, scenery fragment, ground plane, or shadow.
- Marker identity remains readable from face, hair/headwear, clothing, body
  shape, and stance at runtime size rather than relying on facial detail alone.
- Clothing is plausible for rural County Roscommon in 1820.
- No text, label, border, watermark, UI chrome, modern object, fantasy cue, or
  extra character appears.
- The decision applies atomically to both assets. A failed child or identity
  match requires a new joint request; children from separate calls are not mixed.
