---
description: Generate production Rundale illustrated-notebook NPC portrait and marker prompts from an NPC art-input record.
agent: 'agent'
tools: ['search/codebase']
argument-hint: 'npc_record=<one npcs[] JSON object from npc-art-inputs-v1.json>'
---

# Rundale Notebook Person Art Prompt

Use this prompt file to turn one NPC record from
`parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json` into image-model
prompts for one tiny notebook portrait and one tiny in-scene marker.

The visual authority is `docs/graphics-v2/illustrated-parish-notebook.png`.
Do not use prior experimental portrait sheets, marker concept sheets,
procedural busts, or unrelated graphics cycles as source art.

Definitive surface rule: the actual world/scene layer is the painted watercolor
surface. The rest of the UI is uncolored pen-and-ink line art on paper with
minimal value shading. NPC portraits are UI art, so portraits must not be
colored in and must not bake in parchment or paper texture. In-world NPC markers
may use restrained watercolor because they sit on the painted world surface.

## Input

NPC record:

`${input:npc_record:Paste one npcs[] JSON object from npc-art-inputs-v1.json}`

Required fields:

- `name`
- `age`
- `occupation`
- `brief_description`
- `art_direction.portrait_identity`
- `art_direction.marker_identity`
- `art_direction.avoid`
- `global_style` or equivalent copied style constraints

## Output

Return exactly three sections:

1. `PORTRAIT_PROMPT`
2. `MARKER_PROMPT`
3. `REVIEW_CHECKLIST`

Do not invent new biography facts. Use canonical NPC/world facts and the
reviewed art-direction fields. Add only composition and production constraints
needed by the image model.

## Shared Style Contract

Use the Illustrated Parish Notebook concept-art style:

- painted watercolor only for actual world/scene assets and in-world markers
- uncolored pen-and-ink for UI portraits, tabs, icons, notes, and UI furniture
- transparent-background portrait line art composited over UI-controlled paper
- sepia or graphite ink outlines
- restrained muted wash only where explicitly allowed for world/marker art
- clean transparent portrait source or clean chroma-key marker source as specified
- compact game-readable people
- sparse handmade line economy
- rural County Roscommon, Ireland, 1820
- period-appropriate ordinary parish clothing
- dignity and specificity, no caricature

## Source Asset Contract

- Portrait source: 1024x1024 transparent-background PNG. The drawing occupies
  roughly 45 percent of source height, centered with generous transparent
  padding, with the top of hair/head covering and shoulders fully visible.
- Portrait background: true alpha/transparent. Do not generate parchment,
  paper texture, colored wash, portrait card, frame, border, label, shadowed
  backdrop, or other UI furniture in the portrait source.
- Marker source: 1024x1024 PNG on perfectly flat #ff00ff chroma-key background.
  The full-body figure occupies roughly 45 percent of source height, centered
  with feet visible and generous flat margins.
- Runtime derivatives: approved portrait sources are converted to transparent
  PNGs and composited over the UI paper; approved marker sources are chroma-keyed
  to transparent PNGs and depth-scaled by the notebook renderer.
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
- avoid saturated primaries; #ff00ff is allowed only as marker chroma key

Hard avoid-list:

- modern clothing
- Victorian bustle, top hat, bowler, modern uniform, photography-era styling
- fantasy costume or magic glow
- colored portrait wash, color fill, or painted clothing blocks in UI portraits
- polished portrait-card treatment
- procedural placeholder bust
- abstract icon variant
- text, labels, watermark, UI frame, border
- extra characters unless explicitly requested

## Portrait Prompt Template

```text
Use case: historical-scene
Asset type: production source for one tiny Rundale notebook NPC portrait
Primary request: Create one tiny notebook-margin head-and-shoulders portrait for <NAME>, <AGE>-year-old <OCCUPATION> in rural County Roscommon, Ireland, 1820.
Canvas/output: 1024x1024 transparent-background PNG source. The portrait drawing should occupy only about 45 percent of the image height, centered with generous transparent padding, like a small Nearby rail portrait asset rather than a finished portrait study.
Subject identity: <PORTRAIT_IDENTITY.face_and_hair>; <PORTRAIT_IDENTITY.clothing>; <PORTRAIT_IDENTITY.pose_expression>; include only these subtle props if they fit a head-and-shoulders sketch: <PORTRAIT_IDENTITY.props>.
Style/medium: uncolored pen-and-ink notebook doodle in the Illustrated Parish Notebook UI style; sparse sepia or graphite line from the concept palette, minimal monochrome value shading only, transparent background, incomplete shoulders fading into alpha, tiny UI readability.
Composition/framing: one head-and-shoulders portrait only, no frame, no card, no border, no text, no label, no decorative background. Keep the top of hair/head covering and shoulders fully visible, with padding on all sides.
Color/value: transparent alpha plus sepia or graphite ink only. Treat <PORTRAIT_IDENTITY.palette_notes> as value/texture cues, not hue instructions. No parchment background, no paper texture, no colored wash, no color fill, no painted clothing blocks.
Constraints: tiny notebook portrait readability, period-appropriate 1820 rural Irish clothing, no modern clothing, no Victorian fashion, no glamour, no formal bust portrait, no dark card background, no watercolor fill, no color, no photorealism, no fantasy, no text, no watermark. Also avoid: <ART_DIRECTION.avoid>.
```

## Marker Prompt Template

```text
Use case: historical-scene
Asset type: production source for one tiny Rundale notebook NPC marker sprite, chroma-key source
Primary request: Create one tiny full-body in-scene game marker for <NAME>, <AGE>-year-old <OCCUPATION> in rural County Roscommon, Ireland, 1820.
Canvas/output: 1024x1024 PNG source. The figure must occupy only 45 percent of the image height, centered with very large flat background margins. It must look like a small map/scene marker asset, not a full character illustration.
Subject identity: <MARKER_IDENTITY.silhouette>; <MARKER_IDENTITY.pose>; readable props: <MARKER_IDENTITY.readable_props>; tie the figure to this canonical cue: <BRIEF_DESCRIPTION>.
Style/medium: loose hand-inked watercolor miniature in the Illustrated Parish Notebook concept-art style; sparse sepia line, restrained muted wash, simple readable silhouette, no detailed portrait face.
Composition/framing: one single full-body figure only, front three-quarter view, feet fully visible, generous padding on all sides, no cropping, no ground shadow, no floor plane, no extra props beyond the listed readable props.
Background: perfectly flat solid #ff00ff chroma-key background for removal. The background must be one uniform color with no texture, no shadows, no gradients, no border, no lighting variation. Do not use #ff00ff anywhere in the subject.
Color palette: concept palette only: sepia/graphite ink, olive-grey, weathered tan, umber, muted wool grey, bog green, dull brick red, peat brown, faded indigo as subordinate accents, <PORTRAIT_IDENTITY.palette_notes>.
Surface rule: this marker sits on the actual world/scene layer, so restrained watercolor is allowed. Keep the wash muted and subordinate to the scene.
Constraints: tiny game-marker readability, silhouette first, one or two large props, period-appropriate 1820 rural Irish clothing, no modern clothing, no Victorian bustle, no top hat, no cash register, no fantasy, no polished portrait-card treatment, no text, no label, no watermark, no extra figures. Also avoid: <ART_DIRECTION.avoid>.
```

## Review Checklist

- Portrait reads as a tiny notebook-margin asset, not a finished character card.
- Portrait is uncolored pen-and-ink only, with minimal monochrome shading.
- Portrait is a transparent-background PNG source with no baked paper/parchment.
- Portrait has padding above hair/head covering and does not crop shoulders.
- Portrait source is 1024x1024 and the drawing uses roughly 45 percent of height.
- Marker is full-body, feet visible, and centered on a flat #ff00ff background.
- Marker source is 1024x1024.
- Marker occupies roughly 45 percent of image height and can be trimmed later.
- Marker uses the concept palette, with #ff00ff only as background key color.
- Identity comes from silhouette, posture, and one large prop, not facial detail.
- Clothing is plausible for rural County Roscommon in 1820.
- No text, label, border, watermark, UI chrome, modern object, fantasy cue, or
  extra character appears.
