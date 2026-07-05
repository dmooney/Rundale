# Fresh Boundary-Material Notebook Plate Report

Generated with the built-in `image_gen` tool from the attached references and the saved prompt. Final selected image:
`docs/graphics-v2/pipeline-experiments/idea-bc-kilteevan-boundary-material-fresh-notebook.png`

## Audit

- Topology: Broad road corridors, the house/outbuilding cluster, orchard/tree massing, and the garden region are recognizable from the tight crop/control family. The composition still regularizes the scene into a handsome centered road junction more than the source map warrants, so it is not a perfect topology lock.
- Notebook style: Stronger than the deterministic/control plates. It has sepia ink, mottled watercolor grass, muddy scumbled roads, roof hatching, and dense hand detail. It reads as a notebook-style game background, though the camera remains a bit high and survey-plate-like in the fields.
- Boundary/garden softening versus BA: Improved in open fields and tree masses, but not fully solved. Several garden beds and orchard/garden edges are rendered as continuous low stone-wall-like borders, especially around the central/right planted plots. The boundary-material cue softened some field seams but did not prevent hard garden enclosure language.
- Camera/facades: Low 3/4 is present enough to show roofs plus vertical walls and thresholds, but the pitch is still closer to high isometric than the original notebook sample. The main cottage facade is readable; smaller buildings show facades but are compressed.
- Person-sized openings/doors: The main cottage has a readable timber plank door and threshold. The lower outbuildings appear to have dark door/opening marks and yard connections. The small upper/northwest sheds are less certain: some facades show dark vertical marks, but at this output scale they are only marginally readable as human-usable plank doors.
- Chimney/roof nubs: Fails the absolute roof rule. The main cottage roof has a small chimney-like nub near the ridge. A few roof highlights/ends on smaller buildings could also be read as minor roof protrusions, though the main visible violation is the central cottage nub. No smoke is visible.
- Semantic leaks: No UI, people, animals, carts, labels, churches, graveyards, rivers, bridges, shops, or readable map text are apparent. The plate does not visibly copy the notebook sample's named content. The major semantic leaks are structural rather than object leaks: scenic crossroads regularization and hard wall materialization around soft planting zones.

## Verdict

Useful as a fresh notebook-style background candidate and a better no-prior-render visual target than a raw control plate, but it should not be treated as a clean pass for the roof rule or boundary-material objective. A bounded repair pass would need to remove the main roof nub and repaint the garden/planting borders into softer hedges, earth banks, and vegetation texture without moving roads or buildings.
