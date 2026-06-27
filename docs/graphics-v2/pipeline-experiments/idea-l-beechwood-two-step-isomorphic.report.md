# Beechwood Two-Step Isomorphic Report

Output: `docs/graphics-v2/pipeline-experiments/idea-l-beechwood-two-step-isomorphic.png`

Generation: built-in `image_gen` using the supplied Cycle L2 prompt and attached images. The generated native bitmap was 1457x1080, so the final saved PNG was converted to 1920x1080 by deterministic horizontal edge extension; the center map-derived image content was not stretched or cropped.

## PASS/FAIL

- North-up isomorphic camera: PASS. The result is a 3/4 orthographic/isomorphic plate, and the source-map top remains visually up.
- No UI/text: PASS. No labels, signs, UI widgets, map pins, survey numbers, or readable text are visible.
- Topology preserved from original map and top-down control: PASS with caveat. The central content preserves the main northwest-to-lower-center road, the southeast-running lane, enclosed garden/orchard, fields, walls, and woodland massing. The 16:9 edge extension is synthetic and should not be read as additional source-map topology.
- Building groups use notes/control: PASS. B1 is the dominant rectilinear/courtyard-adjacent roofed group near the road; B2/B3/B4 are rendered as subordinate ambiguous outbuildings.
- No copied style objects: PASS. The style reads as ink/watercolor material transfer rather than copying a specific swatch object.
- No unsupported church/graveyard/water/bridge: PASS. None visible.
- No random chimneys/chimneys in walls/smoke: PASS. No smoke; any roof detail reads as roof texture rather than free-standing chimney clutter.
- Source-map fidelity: PASS with caveat. Strong for central layout and note-derived content; weaker at the extreme left/right margins because the 16:9 format was achieved by edge extension after generation.
- Two-step better/same/worse than Cycle K one-step map-reader-guided render: FAIL / UNASSESSED. Cycle K was not included in the allowed input set for this clean-context task, so I did not inspect or compare against it.

## Notes

The two-step conversion appears useful for keeping the cleaned garden, walls, lanes, and tree masses coherent while lifting buildings into readable rural volumes. The only material risk in this saved artifact is the non-native 16:9 side fill; for a strict production plate, a native 16:9 regeneration or explicit outpaint pass would be preferable.
