# Cycle K Map-Reader-Guided Render Report

- PNG: `docs/graphics-v2/pipeline-experiments/idea-k-beechwood-map-reader-guided.png`
- Prompt sidecar: `docs/graphics-v2/pipeline-experiments/idea-k-beechwood-map-reader-guided.prompt.md`
- Built-in generated source: `/Users/dmooney/.codex/generated_images/019f0994-4606-7a42-9791-1656b217ae5d/ig_00b67f93a0e6e465016a3fe4a04e488193a5585c975e375312.png`
- Final normalization: center-cropped from `1457 x 1079` to exact 16:9 `1456 x 819`.

## QA

- PASS: north-up isometric camera. The road/enclosure topology remains oriented with source-map top as final-image top, and the view is a readable orthographic/isometric game-board angle.
- PASS: no UI/text. No labels, signs, survey numbers, UI marks, or visible text are present.
- PASS: building shapes use map-reader notes. B1 is a dominant rectilinear/courtyard-adjacent roofed group near the road; B2/B3/B4 read as smaller subordinate service structures, with B3 simplified and B4 kept ambiguous.
- PASS: no copied style objects. The style swatches appear used for watercolor/ink/roof/wall material treatment only, not copied scene objects or compositions.
- PASS: no church/graveyard/water unless evidenced. None are rendered.
- PASS: no random chimneys/chimneys in walls. No freestanding chimneys, wall chimneys, smoke, or incoherent roof stacks are visible.
- PASS: source-map fidelity. The main northwest road, southeast lane, road junction, large planted enclosure, west/northwest dense planting, small southern planted enclosure, field boundaries, and eastern dotted/hedged boundary are preserved in approximate relative placement.
- BETTER: compared with a raw-map-only style-swatch prompt, this is better for building interpretation. The note-guided prompt kept the dominant building and subordinate outbuildings separated, avoided over-reading the unhatched rectangles as roofed buildings, and respected the negative evidence against churches, water, shops, bridges, smoke, people, and vehicles.

Overall: PASS for the requested Cycle K map-reader-guided background plate.
