# Graphics V2 Map Annotator

Small static GUI for human-verified annotation of historic OS map crops.

Open it from a local server rooted at `docs/graphics-v2/`:

```sh
python3 -m http.server 8765 --directory docs/graphics-v2
```

Then visit:

```text
http://127.0.0.1:8765/map-annotator/
```

The tool can also be opened directly from `index.html`, but the built-in Grove,
Murphy Farm, and Kilteevan buttons are most reliable through the local server.

## Workflow

1. Load a crop using one of the preset buttons, or use **Open image**.
2. Pick a category and a shape tool.
3. Draw features:
   - **Point**: click once.
   - **Line**: click points, then double-click or press Enter.
   - **Polygon**: click vertices, then double-click or press Enter.
   - **Box**: drag a rectangle.
4. Select an item to edit its label, confidence, notes, or vertices.
5. Export JSON when the annotation pass is ready.
6. Export PNG when you want a quick review plate.

Coordinates are stored normalized to the original image dimensions, so exported
annotations remain usable after display scaling.

## Intended Categories

- Structure
- Road
- Unfenced path / track
- Administrative boundary
- Physical boundary
- Hedge / bank / ditch
- Dry stone wall
- Deciduous tree
- Coniferous tree
- Orchard / crops
- Rough vegetation / bog
- Water / wet ground
- Printed label
- Ignore / not physical
- Uncertain
