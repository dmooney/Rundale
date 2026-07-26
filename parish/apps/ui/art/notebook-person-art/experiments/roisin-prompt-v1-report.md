# Roisin Prompt V1 Experiment

Goal: perfect the first GitHub-friendly notebook person-art prompt contract for
issue #1628 deliverable 2.

Prompt file:

- `.github/prompts/rundale-notebook-person-art.prompt.md`

Accepted preview outputs after the definitive no-color UI-portrait direction:

- `roisin-prompt-v2-portrait-ink.png`
- `roisin-prompt-v1-marker-keyed.png`

## Iterations

1. Combined portrait/marker proof sheet. Rejected: too polished and too large;
   portrait read like finished character art rather than a tiny notebook asset.
2. Smaller proof sheet. Accepted as directionally useful: sparse portrait and
   full body visible, no labels.
3. Marker-only chroma-key source. Rejected: background was good, but figure
   filled the canvas and read as a full illustration.
4. Marker-only chroma-key source, 45% canvas-height figure. Accepted: flat key,
   full body, feet visible, ledger cue clear, enough padding for trim/downscale.
5. Portrait-only source, 45% canvas-height head-and-shoulders. Accepted:
   notebook-margin feel, full hair/shoulders visible, no card or label.
6. User direction clarified the final surface split: the actual world is the
   only painted surface; UI portraits must be uncolored pen-and-ink line art
   with minimal shading. The colored `roisin-prompt-v1-portrait.png` is now
   superseded. `roisin-prompt-v2-portrait-ink.png` is the accepted portrait
   direction.

## Accepted Prompt Adjustments

- Explicitly require the asset to occupy about 45% of image height.
- Use "tiny notebook-margin" and "small map/scene marker asset" language.
- Prioritize silhouette and one large readable prop for markers.
- For keyed markers, require a perfectly flat #ff00ff background with no
  shadows, texture, floor plane, or border.
- For portraits, require generous warm paper margin and incomplete shoulders
  fading into paper.
- For portraits, require uncolored sepia/graphite pen-and-ink only. No colored
  wash, no color fill, no painted clothing blocks, and no skin-tone wash.

## Current Limit

This proves the text prompt direction with the built-in image-generation tool.
It does not replace the later API pipeline, candidate receipt storage, review
gate, or batch generation across all NPCs.
