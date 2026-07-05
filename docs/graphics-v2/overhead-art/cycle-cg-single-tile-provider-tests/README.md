# Cycle CG Single Tile Provider Imagegen Tests

Purpose: compare cheaper image-generation models on one real OS 6-inch
z17 map tile before spending money on parish-scale generation.

- Source tile: `murphy-z17-x62594-y42309-source-tile.png`
- Prompt: `prompt.md`
- Contact sheet: `single-tile-provider-comparison.png`

## Results

| Provider | Model | Status | Listed 1K output cost | Output |
| --- | --- | --- | ---: | --- |
| google | `gemini-3.1-flash-lite-image` | error | $0.0336 |  |
| google | `gemini-3.1-flash-image` | error | $0.0670 |  |
| google | `gemini-2.5-flash-image` | error | $0.0390 |  |

## Notes

- Costs are current listed output prices for 1K images; input image/text
  tokens are additional and should be measured in a larger pilot.
- These are single-tile tests only. They do not prove seam continuity.
- Use the comparison sheet to judge map fidelity, label leakage, and
  whether the model invents/recenters features.
