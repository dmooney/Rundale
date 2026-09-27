# Bundled MapLibre glyphs

This directory contains the 256 Basic Multilingual Plane glyph ranges used by
MapLibre for the `Open Sans Regular` map-label font. They are served by both the
web build and Tauri from `/map-glyphs/{fontstack}/{range}.pbf`.

## Reproducible source and generation

- Font: `OpenSans-Regular.ttf` from `googlefonts/opensans` commit
  `bd7e37632246368c60fdcbd374dbf9bad11969b6`.
- Font SHA-256: `c53aceea2dcf5b4098099c0c4d0a061d17e178a049317b42a422b1a9f7f8eb59`.
- Licence: SIL Open Font License 1.1; the pinned upstream `OFL.txt` is copied
  alongside the ranges (SHA-256
  `01cd5ffb3a528c219a86e49a814c6c53bd8a69b9a3c305dd237074ba1c811af7`).
- Generator: `generate.js` from `openmaptiles/fonts` commit
  `d48c5fce2fc58b55c98d353558d807cac45e7262` (script SHA-256
  `01c0455bf11fffb12a0790e5f0f7bb2196c9572e96723ebb22d041ac17f56d98`),
  with `fontnik@0.7.7` and `@mapbox/glyph-pbf-composite@0.0.3`.
  `fontnik@0.7.7` npm integrity is
  `sha512-ksIBy3itR4h8Gr6r7qYjqw+XZANdB0Xr/pCOC53Gx0qe+WTiTjSqVJ54sEzdyCwFPP8E6GuXPaxg7X+taBYpbw==`.
  `@mapbox/glyph-pbf-composite@0.0.3` npm integrity is
  `sha512-VcsYpDcFDuly8P4sbqBpFKpTrNsOqyvCkuAsoaQrQv9Y4cQnwrwdWgY3zBXJdS6OgukWfVaRwcUE6dwrDC0URA==`.

Fetch the pinned TTF, place it at `OpenSans/OpenSans-Regular.ttf` beside the
pinned generator, install those exact generator dependencies, and run
`node generate.js`. The output is `_output/Open Sans Regular/`. Verify it
against `SHA256SUMS` before replacing these files.

`SHA256SUMS` is sorted bytewise by relative pathname and covers exactly every
256-codepoint range from `0-255.pbf` through `65280-65535.pbf`.
