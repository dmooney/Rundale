# Map Crop Selection Protocol

Use this when preparing historic map inputs for one-shot Rundale background
plates.

## Core Rule

The historic map crop is source geography, not the output composition. Pick the
game plate scale first, then derive the map crop needed to support it.

## Inputs To Fix First

- Target location center or crop anchor.
- Desired output aspect ratio, usually 16:9.
- Target game camera: fixed orthographic 3/4 isometric.
- Target orientation: north-up; source-map top should become final-image top.
- Target sprite scale: a person should read as doorway-sized, not map-pin-sized.
- Target playable extent: named site, yard/core enclosure, local gates, immediate
  roads/lanes, and two to four exits.

## Crop Selection

1. Center the crop on the named site, not on the center of the source map image.
2. Keep the crop north-up. Do not pre-rotate the map tile for composition.
3. Include the named site's full building group, yard/core enclosure, garden or
   orchard if present, and immediate approach lanes.
4. Include enough road/field context for clear exits, but crop before distant
   landmarks become tempting focal points.
5. Exclude or edge-crop churches, bridges, wells, crossroads, unrelated houses,
   and large field systems unless the named site itself is that feature.
6. Keep road exits that cross the crop edge visible and unambiguous.
7. Prefer a crop that is slightly too tight over one that encourages a regional
   survey-map render.
8. Do not write hand-authored per-location interpretation notes for the
   generator. If road, wall, river, or building classes need to be
   disambiguated, produce a reproducible map-reader note or control artifact
   with the same process for every location.

## Generator Prompt Implication

The prompt should say the map extent is not the output frame. If the supplied
crop still contains extra context, the model may omit it. The pass/fail test is
whether the generated plate supports movement at the intended camera and sprite
scale, not whether every source-map mark appears in the image. The model should
not rotate the map into a diagonal game-board composition; the output remains
north-up.

## Grove Case Study

- Wider source crop: useful for understanding church context and road layout,
  but it repeatedly produced high survey-like renders.
- Target-site source crop: `grove-map-target-site-crop.png`; keeps Grove,
  orchard/garden, local roads, field boundaries, and exits while dropping the
  church and most district context.
- Earlier experiments included hand-read Grove road notes. Treat those as
  invalid for production prompt design. Future tests should rely on the map
  crop, generic legend, and/or deterministic preprocessing outputs only.
