# Cycle CC: Overhead Player and NPC Concepts

Experiment: explore how the player and NPCs might appear if the Cycle CB
`B no legend` overhead watercolor map became the main gameplay surface.

## Source

- Map/style reference:
  `../cycle-cb/idea-cb-b-no-legend-clear-game-map.png`
- Premise:
  readable player/NPC tokens do not need to be microscopically true to the
  original full-map scale. The gameplay map can be zoomed or AI-upscaled by
  roughly `2x-3x` so map-native tokens remain legible.

## Concepts Generated

| File | Concept |
| --- | --- |
| `idea-cc-a-integrated-miniatures-on-map.png` | Tiny watercolor people painted into the map surface; best for atmosphere and judging whether people can feel native to the tile. |
| `idea-cc-b-symbolic-map-tokens.png` | More symbolic, readable map markers: cloak shapes, hat/shoulder marks, rings, accents. |
| `idea-cc-c-interaction-state-mockup.png` | Selection/hover/talkable/path states on the map; useful, but drifts toward modern UI and should be softened. |
| `idea-cc-d-sprite-pawn-sheet.png` | Animation/sprite-sheet direction; charming but too tall/body-like for strict overhead map scale. |
| `idea-cc-e-flat-human-glyphs.png` | Best token vocabulary so far: tiny flat overhead glyphs, readable against trees/roads/garden dots. |
| `idea-cc-f-2x-map-scale-token-mockup.png` | 2x map-scale gameplay mockup with larger readable tokens placed on roads/yards/garden edges. |
| `idea-cc-g-3x-map-scale-token-mockup.png` | 3x map-scale gameplay mockup; token scale works, but map features start becoming more architectural. |
| `idea-cc-h-2p5x-base-map-upscale-no-tokens.png` | No-character 2.5x base-map upscale; useful to test if the map surface itself can support larger tokens. |

## Quick Read

- Best character/token direction: `idea-cc-e-flat-human-glyphs.png`.
- Best gameplay-scale mockup: `idea-cc-f-2x-map-scale-token-mockup.png`.
- Best base-map scale test: `idea-cc-h-2p5x-base-map-upscale-no-tokens.png`, with the caveat that future prompts should keep roof footprints flatter and less architectural.

The promising direction is a `2x-2.5x` overhead gameplay surface with flat
human glyphs around the width of a lane detail, plus very restrained selection
marks: pale wash halo, thin dashed ink ring, or small muted color accent.

Full generation prompts are saved in `PROMPTS.md`.
