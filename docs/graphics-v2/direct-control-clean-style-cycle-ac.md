# Direct Control With Clean Style Crops - Cycle AC

Cycle AC tests whether AA's direct-from-local-control route can recover more
of the original illustrated parish notebook look after replacing the leaky
low-camera slate reference with a cleaned single-building slate crop.

AC still does not pass previous rendered plates as image inputs. The render
stack is:

1. tight local top-down topology control crop,
2. deterministic oblique pitch cue,
3. original historic map crop,
4. original illustrated parish notebook sample,
5. cleaned single-building slate/limewash crop,
6. cleaned single-building thatched/no-chimney crop, and
7. cleaned material swatches.

The test is whether a scalable prompt/reference stack can move closer to the Z
pair's notebook scale and texture without relying on Z, X, Y, or any other
prior rendered plate.

## Outputs

| Site         | Output                                                                  | Prompt                                                                        | Report                                                                        | Result                               |
| ------------ | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------ |
| Beechwood AC | `pipeline-experiments/idea-ac-beechwood-direct-control-clean-style.png` | `pipeline-experiments/idea-ac-beechwood-direct-control-clean-style.prompt.md` | `pipeline-experiments/idea-ac-beechwood-direct-control-clean-style.report.md` | Best direct-control Beechwood so far |
| Grove AC     | `pipeline-experiments/idea-ac-grove-direct-control-clean-style.png`     | `pipeline-experiments/idea-ac-grove-direct-control-clean-style.prompt.md`     | `pipeline-experiments/idea-ac-grove-direct-control-clean-style.report.md`     | Best direct-control Grove so far     |

## Audit Questions

- Does AC keep Beechwood's connected-compound topology and Grove's
  separate-building topology without previous rendered plates?
- Does the new single-building slate crop reduce doorless-fragment leakage?
- Does AC become less clean/survey-like than AA?
- Does AC approach the Z pair's lower camera, facade readability, and rough
  notebook watercolor texture?
- Do garden rows, stone walls, roofs, and roads remain source-faithful without
  becoming perfect strategy-game geometry?

## Result

AC is the strongest direct-control branch so far.

Beechwood AC preserves the connected compound, nearby detached structures,
working yard, roads, walls, gates, and garden context while recovering more of
the rough notebook style than AA. It benefits from the cleaned single-building
style crops: visible buildings have readable doors/thresholds and there is no
obvious foreground-fragment leakage from the old slate reference.

Grove AC preserves the separate-building yard topology, road curve, garden
enclosures, and detached eastern building without copying Beechwood's compound
arrangement. It also avoids the old semantic leaks: no church, graveyard,
bridge, river, people, animals, labels, smoke, or obvious chimneys.

The caveat is shared: both AC plates still regularize gardens, walls, and roof
planes more than the original illustrated parish notebook sample. They are less
survey-board-like than AA, but not yet as loose, dense, low, and watercolor-rich
as the original sample or the repaired Z pair. Treat AC as the best scalable
direct-control candidate, not as the final visual endpoint.

## Recommendation

Use AC as the next production-shaped baseline: local topology control plus
source map plus original notebook sample plus cleaned single-building style
crops. For the next improvement cycle, keep topology locked to AC/control and
apply only a bounded style/camera refinement aimed at reducing garden-row
regularity, raising facade/threshold prominence, and adding more uneven
watercolor/ink texture. Avoid broad re-layout or open-ended beautification.
