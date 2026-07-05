# Idea L - Grove Topdown Cleaned Report

Output: `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-l-grove-topdown-cleaned.png`

Built-in generated candidate selected from:
`/Users/dmooney/.codex/generated_images/019f0a1b-79b5-7c80-889f-6d8ccf7ec83f/ig_041a994871cdb9b0016a400720b6c88197ac35e64737e216a1.png`

The selected candidate was padded, not cropped, to a 1845 x 1038 PNG so the project output is effectively 16:9 while preserving the generated map content.

## Checks

| Criterion                                    | Result | Notes                                                                                                                                                                                  |
| -------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Strict top-down plan view                    | PASS   | Buildings, roads, boundaries, planting, and fields read as an orthographic plan.                                                                                                       |
| North-up orientation                         | PASS   | Source top remains final-image top; the northwest clipped corridor, northeast road, center-left enclosure, and lower boundaries keep their source relationships.                       |
| No UI/text                                   | PASS   | No labels, signs, pins, UI, survey numbers, or readable text are present in the generated artwork.                                                                                     |
| Topology preserved                           | PASS   | Main road/lane structure, building cluster, enclosed planting, lower-left hedgerow, right-center boundary, and angled parcel lines are preserved. Minor smoothing/cleaning is present. |
| Building footprints use map-reader notes     | PASS   | B1, B2, B3/B4, B5, and B6 are represented in plausible plan form; B4 appears merged with B3 and low-confidence B7 is omitted/ambiguous.                                                |
| No copied style objects                      | PASS   | Style swatches influenced texture/color only; no identifiable swatch objects or perspective building details were copied.                                                              |
| No unsupported church/graveyard/water/bridge | PASS   | No church, graveyard, watercourse, pond, or bridge appears.                                                                                                                            |
| No perspective/facades/chimneys/smoke        | PASS   | Roof hatching stays plan-view; no facades, chimneys, smoke, horizon, or cast 3D shadows are visible.                                                                                   |
| Usefulness as second-stage control           | PASS   | The plate is clean, readable, and layout-preserving enough to guide a later isomorphic/isometric conversion step.                                                                      |

## Overall

PASS. The image is suitable as a Cycle L1 top-down cleaned control plate, with the main caveat that the generated source aspect was narrower than 16:9 and was padded to desktop ratio after generation.
