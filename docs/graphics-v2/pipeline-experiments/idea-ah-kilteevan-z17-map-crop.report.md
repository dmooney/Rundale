# Idea AH Kilteevan Z17 Map Crop

Purpose: third topology/control crop for background-plate pipeline research.

Source: NLS Historic 6-inch OS Ireland, Roscommon 1st edition XYZ tiles: `https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`
Coordinate used for crop center: lat `53.632544989054054`, lon `-8.102168938364912` from `mods/rundale/world.json` Kilteevan Village.
Zoom: `17`.
Tile neighborhood: x `62585..62587`, y `42310..42312`.
Center tile float: x `62586.090314`, y `42311.426279`.
Mosaic: `pipeline-experiments/idea-ah-kilteevan-z17-mosaic.png` (768x768).
Crop: `pipeline-experiments/idea-ah-kilteevan-z17-map-crop.png` (768x432), north-up, 16:9.
Crop origin inside mosaic: x `0`, y `149`; center pixel before clamp x `279`, y `365`.

No hand-authored feature interpretation or location-specific road/building hints were added. The crop is data-derived from the configured tile source and a stored world coordinate.
