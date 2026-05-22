# Stylized NLS Tile Pipeline

> Parent: [Map Evolution](map-evolution.md) | [Docs Index](../index.md)
>
> Status: **Implemented** — `parish-tile-art` crate + `seed-tiles` seeder

## Overview

The `rundale-map` tile source provides an illustrated parchment art style
inspired by RDR2's map aesthetic: warm cream-sepia paper, dark sepia ink
strokes, muted sage woodland, cool slate water, and a subtle Perlin paper
grain. It is pre-generated offline by `parish-geo-tool seed-tiles` and
served from the existing `TileCache` bundled-dir path — no runtime
stylization, no changes to the live serving path.

## Pipeline

Source tiles: NLS OS Ireland 1st Edition 6-inch (1829–1842), via S3
(`mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`).

Two rendering backends in `parish/crates/parish-tile-art/`:

### Parchment pipeline (default, always offline)

1. **Box blur** (3×3) — smooths scan/JPEG artefacts so they don't become
   false ink strokes.
2. **Sobel edge extraction** — luma-channel gradient magnitude → edge-strength
   map `[0, 255]`.
3. **Colour remap** — per-pixel HSV-keyed transform toward the Rundale
   parchment palette (see table below).
4. **Ink overlay** — gamma-darkened Sobel mask multiply-blended as sepia
   ink strokes.
5. **Paper grain** — deterministic Perlin noise (fastnoise-lite) seeded by
   tile z/x/y; blended at 7% as warm cream overlay. Same tile coords always
   produce identical grain.
6. **PNG encode** — lossless, same 256×256 dimensions.

### Diffusion pipeline (optional, requires local Stable Diffusion WebUI)

1. **Canny pre-processing** — imageproc Canny edges sent as ControlNet hint
   to preserve geographic structure.
2. **img2img POST** to `{endpoint}/sdapi/v1/img2img` (A1111-compatible) with
   `"DPM++ 2M Karras"` sampler, `denoising_strength=0.65`,
   `controlnet_strength=0.85`.
3. **Fallback** — automatically uses parchment pipeline if the endpoint is
   unreachable or returns an error; tile is never missing.

## Colour palette

| Region | NLS signal | Output |
|--------|-----------|--------|
| Parchment bg | light cream, low sat | `#f4e6c0` |
| Ink / roads / text | near-black luma | `#2d1a0e` |
| Medium sepia | mid-dark luma | `#6b4a2a` |
| Water | hue 170–235°, sat > 0.12 | `#b0bec5` |
| Fields | mid-tone | `#e8d5a0` |
| Woodland | hue 65–158°, sat > 0.10 | `#8a9a6a` |
| Settlements | warm hue 15–40°, mid-luma | `#b07850` |

## Seeding

```sh
# Parchment — seed Kiltoom/Kilteevan area, zoom 10–17
cargo run -p parish-geo-tool -- seed-tiles \
  --style parchment \
  --bbox 53.45,-8.15,53.65,-7.85 \
  --zoom 10-17 \
  --source-id rundale-map \
  --concurrency 8 \
  --output mods/rundale/tiles/

# AI-enhanced — requires local SD WebUI at localhost:7860
cargo run -p parish-geo-tool -- seed-tiles \
  --style diffusion \
  --diffusion-endpoint http://localhost:7860 \
  --bbox 53.45,-8.15,53.65,-7.85 \
  --zoom 14-17 \
  --source-id rundale-map \
  --output mods/rundale/tiles/
```

Then set in `parish.toml`:
```toml
[engine.map]
default_tile_source = "rundale-map"
bundled_tiles_dir = "mods/rundale/tiles"
```

Or via env var: `PARISH_BUNDLED_TILES_DIR=mods/rundale/tiles`.

## Tile count estimates (Kiltoom/Kilteevan bbox)

| Zoom | Approx. tiles | Parchment time | Diffusion time (SD ~20s/tile) |
|------|--------------|----------------|-------------------------------|
| 10–13 | ~60 | < 1 min | ~20 min |
| 14 | ~90 | < 2 min | ~30 min |
| 15 | ~350 | ~5 min | ~2 hrs |
| 16 | ~1 400 | ~20 min | ~8 hrs |
| 17 | ~5 600 | ~1.5 hrs | ~31 hrs |

Diffusion at z=14–15 (the most visible zoom range) is practical overnight.
Parchment pipeline for the full z=10–17 range runs in a few hours on one core.

## Licensing

See `docs/licenses/NLS_CC-BY_derivative.txt` for the full notice. The
stylized tiles are derivative works of NLS CC-BY material. Attribution
is preserved in `TileSourceConfig.attribution` (shown in MapLibre's
attribution control on every map view). Contact `geo@nls.uk` before any
public release that bundles pre-generated tiles.

## Feature flag

`stylized-nls-tiles` (default-on). When disabled, the `rundale-map` source
falls through to raw NLS tiles from the upstream URL.

## Related

- [Map Evolution](map-evolution.md) — Phase D.2 offline bundling context
- `parish/crates/parish-tile-art/` — pipeline implementation
- `parish/crates/parish-geo-tool/src/tile_seeder.rs` — seeder
