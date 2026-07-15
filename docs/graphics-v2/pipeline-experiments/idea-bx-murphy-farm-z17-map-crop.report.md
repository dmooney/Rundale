# Idea BX Murphy's Farm Z17 Map Crop

Purpose: source/control crop for the Murphy's Farm background-plate pipeline
test.

Source: NLS Historic 6-inch OS Ireland, Roscommon 1st edition XYZ tiles:
`https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`

Coordinate used for crop center: lat `53.63579941155877`, lon
`-8.079662971357214` from `mods/rundale/world.json` Murphy's Farm.

Zoom: `17`.

Tile neighborhood: x `62593..62595`, y `42308..42310`.

Center tile float: x `62594.284486`, y `42309.427929`.

Mosaic: `pipeline-experiments/idea-bx-murphy-farm-z17-mosaic.png` (768x768).

Crop: `pipeline-experiments/idea-bx-murphy-farm-z17-map-crop.png` (768x432),
north-up, 16:9.

Crop origin inside mosaic: x `0`, y `150`; center pixel before clamp x `329`,
y `366`.

The crop contains a small farmstead/yard-like mark near center-left, textured
ground to the west that the user identified as likely peat bog / bog-edge
terrain, and a larger diagonal road/field boundary system. The prototype
detector found zero building-like components, so generated blockouts should not
be used as building truth. Murphy's Farm is a fictional world location pinned
to a real map coordinate; the world definition supplies the farm identity while
the map crop supplies field, bog-edge, road, boundary, and local placement
context.

No hand-authored road/path interpretation was added. The crop is data-derived
from the configured tile source and a stored world coordinate.
