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

The renderer must not import the rejected `rundale/notebook-ui/` asset kit.
Parchment ribbons, tabs, cards, action cells, labels, and selection ink are drawn
at runtime in `src/lib/illustrated-parish/renderer.ts`.
