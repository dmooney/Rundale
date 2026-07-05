# Cycle CG Single Tile Provider Imagegen Tests

Purpose: compare cheaper image-generation models on one real OS 6-inch z17 map
tile before spending money on parish-scale generation.

- Source tile: `murphy-z17-x62594-y42309-source-tile.png`
- Prompt: `prompt.md`
- Combined contact sheet: `single-tile-provider-comparison.png`
- GPT Image 1 contact sheet: `single-tile-gpt-image-1-comparison.png`

## Results

| Provider | Model | Status | Observed cost | Output | Verdict |
| --- | --- | --- | ---: | --- | --- |
| openrouter | `black-forest-labs/flux.2-klein-4b` | ok | $0.015 | `openrouter-black-forest-labs-flux.2-klein-4b.png` | Not usable; too soft/small for 3x runtime tiles. |
| openrouter | `google/gemini-3.1-flash-lite-image` | ok | $0.03393425 | `openrouter-google-gemini-3.1-flash-lite-image.png` | Best cheap geometry tradeoff, but still not production-usable. |
| openrouter | `google/gemini-3.1-flash-image` | ok | $0.0678685 | `openrouter-google-gemini-3.1-flash-image.png` | Not usable; invents/recenters the compound and roads. |
| openrouter | `openai/gpt-image-1-mini` | ok | $0.0094855 | `openrouter-openai-gpt-image-1-mini.png` | Not usable; invents a regular field grid and loses source topology. |
| openrouter | `openai/gpt-image-1` | ok | $0.04639 | `openrouter-openai-gpt-image-1.png` | Best-looking tile art, but still not usable; regularizes/reinvents roads, hedges, and building layout. |
| openrouter | `sourceful/riverflow-v2.5-fast` | ok | $0.084868 | `openrouter-sourceful-riverflow-v2.5-fast.png` | Not usable; attractive but invents too much local enclosure/detail. |

Direct Google API calls were attempted first. They returned HTTP 429 because
the Google AI Studio prepayment credits are depleted. Those error reports are
kept as `gemini-*.report.json`; no direct-Google image outputs were produced.

## Current Read

None of these single-tile image-to-image tests are good enough for a
source-authoritative map tile pipeline.

The consistent failure is not price, it is geometry. Even when the tile looks
nice, the model turns the OS map into a plausible invented farm map: roads are
straightened, boundary relationships change, the building compound is
regularized, and extra enclosure logic appears. That is fatal for a gameplay
surface where the historical map is supposed to be the authority.

Do not scale this prompt/model setup to parish generation. The safer path is
still:

- deterministic local base map for the parish/county;
- API only for small local override plates;
- stronger semantic controls before any model-generated tile is accepted.

## Notes

- These are single-tile tests only. They do not prove seam continuity.
- Observed costs are from OpenRouter `usage.cost` where available.
- The runner is `docs/graphics-v2/scripts/single_tile_provider_imagegen.py`.
