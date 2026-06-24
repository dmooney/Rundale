# Visual Sprite Compositor M1 Design Note

> Status: Approved by follow-up direction. Task: `visual-sprite-compositor-m1`.

## Player Experience

Kilteevan should stop feeling like one full-screen illustration with invisible
rectangles over it and start behaving like a sprite-composed adventure scene.
The player still sees the same damp 1820s Irish pixel-art village, but under the
hood the scene is assembled from reusable transparent PNG atoms: ground,
roads, water, buildings, walls, signs, well, carts, smoke, NPCs, and overlays.

## Affected Subsystems

- `mods/rundale`: adds transparent raster compositor atoms and changes
  Kilteevan's scene definition to layer them.
- `parish-mod`: validates the larger layer/asset graph already supported by the
  scene contract.
- `parish-core`: emits the ordered layer stack through `SceneState.layers`.
- `parish/apps/visual`: Pixi renders the ordered raster stack without drawing
  the legacy plate underneath when layers are present.

## Data Model

No breaking schema change is required. `SceneAsset` and `SceneLayer` already
support reusable atoms with image, anchor, `x/y/z`, scale, opacity, flip, and
labels. This milestone uses those fields with transparent PNG assets. The
legacy `plate` and `underlay` fields remain compatible fallback/reference
fields, but the live Kilteevan first read comes from the layer stack.

## Observable Signals

The script fixture prints `/debug scenes` and `/scene` for Kilteevan. The proof
must show more than a single Kilteevan layer, atom URLs under
`assets/scenes/kilteevan-village/atoms/`, no `.svg` layer URLs, and unchanged
deterministic hotspot commands. Browser screenshots prove that the composed
scene still reads as a coherent graphical adventure scene.
