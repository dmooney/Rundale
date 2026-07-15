# NPC Portrait Prompt Template

Use this template for Graphics V2 portrait generation. Fill only the bracketed
fields. Keep prompts compact and avoid leaking plot secrets or relationship
knowledge into the visual brief.

## Illustrated Notebook Head V1

```text
Use case: game UI character portrait
Asset type: tiny square NPC head icon for the left-side nearby-people list
Character: [NPC_NAME], [AGE]-year-old [OCCUPATION] in rural County Roscommon, Ireland, 1820.

Identity brief: [ONE_SENTENCE_VISIBLE_IDENTITY].
Expression/posture: [VISIBLE_MOOD_OR_TEMPERAMENT], restrained and natural.
Clothing/status cues: [PERIOD_CLOTHING_AND_CLASS_CUES].

Style target: tiny rough head doodle from the Illustrated Parish Notebook UI, matching the left-side people list in `illustrated-parish-notebook.png`. It should look drawn quickly in a notebook margin, not like a finished portrait. Sepia ink contour lines on warm cream paper, a few scratchy pencil hatching marks, almost no paint, slight hand-drawn wobble. Drawn as if it was only ever meant to be a native `72 x 82` UI asset.

Line/detail budget: very sparse, about 25 to 40 visible drawing strokes total. Simple contour lines, a few loose hair lines, a few clothing lines. No dense crosshatching, no facial shading, no realistic rendered planes, no polished portrait anatomy. Face must remain readable at `72 x 82` and `64 x 64`.

Composition: one centered tiny head-and-shoulders doodle on plain warm paper, no scenery, no card frame, no dark portrait-card background, no text, no name label, no icons. Keep shoulders and clothing visible enough to hint at role and class, but abbreviate them with only a few economical strokes.

Historical constraints: 1820 rural Irish clothing and grooming only. Homespun wool, linen, frieze, shawls, kerchiefs, aprons, cloaks, waistcoats, broad-brimmed hats, caps, simple hair. No Victorian fashion, no modern jacket, no modern shirt collar, no makeup/fashion portrait, no photography lighting, no fantasy costume, no royal/landed-gentry styling unless explicitly supported.

Negative constraints: no text, no watermark, no signature, no ornate frame, no full body, no finished bust portrait, no refined illustration, no perfect fashion-model skin, no dense hair rendering, no large rendered shoulders, no modern hair, no modern accessories, no weapons focus, no fantasy magic, no cinematic poster lighting.
```

## Art Brief Field Rules

`ONE_SENTENCE_VISIBLE_IDENTITY` should describe only things that can be seen:

- Good: "A sharp-eyed shopkeeper with a composed face and practical confidence."
- Good: "A tired but capable widow farmer, weathered from outdoor work."
- Bad: "She knows the cart is delayed and is worried about Cormac's prices."

`VISIBLE_MOOD_OR_TEMPERAMENT` should be physical:

- Good: "wary, observant, lips set but not hostile"
- Good: "warm, tired, faint smile, shoulders relaxed"
- Bad: "secretly planning to emigrate"

`PERIOD_CLOTHING_AND_CLASS_CUES` should be specific:

- Publican: good but worn waistcoat, linen shirt, dark frieze coat, practical cap.
- Farmer: homespun wool, apron or shawl where appropriate, work-worn fabric.
- Priest: plain black clerical coat, sober linen collar, no ornate vestments for ordinary portrait.
- Teacher: simple but tidy dress or jacket, shawl, modest respectability.
- Child/apprentice: plain homespun, cap or tousled hair, youthful face, no adult glamour.
