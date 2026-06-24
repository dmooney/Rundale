# Visual Kilteevan Art M2 Design Note

> Status: Approved by continued visual direction. Task:
> `visual-kilteevan-art-m2`.

## Player Experience

Kilteevan should look less like a proof of layering and more like the first
real screen of a pixel-art adventure game. The player should still be clicking
through a composed world made from PNG atoms, but the first read should feel
art-directed: consistent damp rural palette, fewer obvious repeated tiles,
clear cottages/road/well/bridge landmarks, and compact game UI.

## Affected Subsystems

- `mods/rundale`: updates Kilteevan raster atoms and scene layer placement.
- `parish/apps/visual`: may receive small composition or scaling adjustments if
  needed to improve the first viewport without changing the scene contract.
- `parish-mod` / `parish-server`: existing scene tests should continue to prove
  PNG atom exposure through `/scene`.

## Data Model

No schema change is required. This pass should use the existing `SceneAsset`
and `SceneLayer` fields: image, anchor, `x/y/z`, scale, opacity, and labels. The
important distinction is asset quality and composition, not new backend shape.

## Observable Signals

The harness proves the scene still has PNG atom layers and deterministic
activation hints. The live browser screenshots prove whether the visual first
read is closer to a finished pixel-art game screen at desktop and mobile sizes.
