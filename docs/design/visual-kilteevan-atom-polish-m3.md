# Visual Kilteevan Atom Polish M3 Design Note

> Status: Approved by continued visual direction. Task:
> `visual-kilteevan-atom-polish-m3`.

## Player Experience

The Kilteevan scene should feel less like independently pasted cutouts and more
like a coherent place. Buildings, walls, the well, bridge, cart, and signpost
should cast or imply contact with the muddy ground, while smoke and lighting
remain controllable compositor effects.

## Affected Subsystems

- `mods/rundale`: update or add raster PNG atoms and tune Kilteevan layer order.
- `parish-mod` / `parish-server`: existing scene tests should continue proving
  PNG atom exposure through the scene route.
- `parish/apps/visual`: no renderer/schema change is expected unless screenshot
  proof exposes a framing issue.

## Data Model

No schema change is required. Integration assets use existing `SceneAsset` and
`SceneLayer` fields. New ground-contact atoms must avoid `kind: ground` unless
they are full-stage layers, because the visual client treats ground as a
stage-filling layer.

## Observable Signals

The harness proves the atom layer contract and deterministic activation hints.
The desktop/mobile screenshots prove the visual quality improvement: shadows,
footings, and cleaned atoms should make the scene read as one adventure-game
screen.
