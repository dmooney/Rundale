# Graphics V2 Overhead Art

Dedicated experiments for direct overhead map art: source-map-to-watercolor map
tiles without the isometric/2.5D background-plate pipeline.

- `cycle-cb/` — first Beechwood high-resolution direct-transform prompt matrix,
  with and without the OS 6-inch legend as a symbol-interpretation aid.
- `cycle-cc-character-concepts/` — player/NPC token and scale concepts for using
  the overhead watercolor map as the main gameplay surface.
- `cycle-cd-d-pawns-3x/` — focused follow-up combining Cycle CC's D-style
  pawns with a 3x overhead gameplay map scale.
- `cycle-ce-county-tile-continuity/` — real NLS Roscommon multi-tile continuity
  proof comparing independent tile styling against mosaic-first/supertile-first
  styling, plus one imagegen-generated continuous supertile split into runtime
  tiles.
- `cycle-cf-production-county-pipeline/` — production-shaped proof run from the
  reusable county tile pipeline CLI: 10x10 real Roscommon z17 source tiles, 100
  runtime tiles, seam contracts, provenance manifest, masked seam-repair
  template, bounded repair proof on a known failed adjacent-imagegen stitch,
  contact sheets, and passing validation.
- `county-tile-continuity-plan.md` — proposed county-scale overhead tile
  pipeline with deterministic seam contracts, padded supertiles, semantic
  master layers, and validation gates.
- `full-county-map-generation-api-design.md` — storage/versioning plan,
  full-county data estimates, API generation tool design, and current cost
  estimates for generating the map through the OpenAI API.
