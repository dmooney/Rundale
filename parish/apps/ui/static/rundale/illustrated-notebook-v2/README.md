# Illustrated Notebook v2 Runtime Assets

This directory is the clean visual boundary for the issue #1630 rebuild. The
canonical comparison target is
[`docs/graphics-v2/illustrated-parish-notebook.png`](../../../../../../docs/graphics-v2/illustrated-parish-notebook.png),
but no runtime file is cut from that concept image.

## Provenance

- `parish-crossroads-watercolor.png` and
  `parish-crossroads-watercolor-mobile.png` were generated as fresh desktop and
  vertical watercolor plates for this rebuild, using the canonical concept's
  1820 rural-Irish setting, low-oblique composition, fine ink, muted watercolor,
  and open center-scene direction.
- Portrait art, cast coverage, identity mapping, and fallback behavior belong
  to a separate issue. Issue #1630 uses simple runtime-drawn initial cards to
  reserve the Nearby and selected-person layout without adding portrait art.
- `sewn-notebook-page.png` is the one user-approved exception retained from the
  discarded attempt. It is a 440×620 hand-sewn page with no rings, ring holes,
  or paperclip.
- `visual-scenes.json` records the fresh plate paths, written-source provenance,
  camera direction, and plate-normalized scene anchors validated by
  `parish-world` tests.
- `parchment-*.png` provides the raster top ribbon, Nearby rail, five-cell
  action strip, intent slip, bottom cards, and scene labels. Each was
  generated independently with the built-in image generator, using the
  canonical concept as a style/composition reference and the approved sewn
  page as the material reference.
- `notebook-index-rail.png` is one transparent assembly of five distinct
  folded-vellum finding tabs. Its buried tails sit behind the sewn page while
  raster ink symbols, desktop labels, and five independent transparent hit
  targets preserve legibility and accessibility.
- `icon-*.png` provides transparent 128×128 action, tab, map, time, quill, and
  compass cutouts generated from the concept's loose charcoal/sepia symbol
  language. The five tab cutouts are a loose notes folio, anonymous People
  bust, rural chapel, rumours bubble, and plain open journal; none introduces
  portrait identity or modern binding hardware.
  The compass contains no generated lettering; the renderer supplies `N`.
- `portrait-slot-frame.png` is intentionally empty. It reserves layout space
  without introducing portrait art from the separate portrait issue.
- `ui-assets.json` records every runtime image's role, dimensions, alpha
  contract, provenance class, and SHA-256 hash.

The rejected `rundale/notebook-ui/` asset kit and dead visual renderer have
been removed. The active renderer preloads these v2 raster cutouts and overlays
only dynamic text, hit targets, focus treatments, trust dots, and fine selection
ink.

## Regenerating UI Cutouts

Use the built-in image-generation tool once per distinct asset. The shared
prompt direction is:

> Create one isolated blank rag-paper UI surface or one isolated charcoal/sepia
> ink symbol. Match the hand-inked watercolor language of
> `docs/graphics-v2/illustrated-parish-notebook.png`; for paper assets also
> harmonize with `sewn-notebook-page.png`. Center the complete cutout on a
> perfectly flat `#00ff00` chroma background. No text, portraits, scene content,
> metal, rings, ring holes, spiral binding, paperclips, shadows, or watermark.

Remove the chroma background with the installed imagegen helper using border
auto-keying, soft matte, despill, and thresholds 12/220. Trim, fit, and center
the result on the exact dimensions declared in `ui-assets.json`; keep all four
corners transparent and update the stored hash only after visual inspection.
