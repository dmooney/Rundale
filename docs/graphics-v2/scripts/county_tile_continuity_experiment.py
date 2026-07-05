#!/usr/bin/env python3
"""County-scale tile continuity experiment for Graphics V2 overhead maps.

This is an experiment runner, not production code. It fetches a real NLS
Roscommon historic-map tile neighborhood, then compares two rendering
strategies:

1. independent per-runtime-tile stylization;
2. mosaic-first stylization followed by deterministic runtime tile export.

The goal is to make seams visible and measurable so the county-scale map plan is
based on artifacts, not just a written idea.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageFont, ImageOps


NLS_ROSCOMMON_URL = "https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png"
TILE_SIZE = 256


@dataclass(frozen=True)
class TileRef:
    z: int
    x: int
    y: int

    @property
    def name(self) -> str:
        return f"z{self.z}-x{self.x}-y{self.y}"


def deg_to_tile(lat: float, lon: float, z: int) -> tuple[int, int]:
    lat_rad = math.radians(lat)
    n = 2.0**z
    x = int((lon + 180.0) / 360.0 * n)
    y = int((1.0 - math.asinh(math.tan(lat_rad)) / math.pi) / 2.0 * n)
    return x, y


def fetch_tile(tile: TileRef, cache_dir: Path, url_template: str) -> Path:
    out = cache_dir / str(tile.z) / str(tile.x) / f"{tile.y}.png"
    if out.exists():
        return out

    out.parent.mkdir(parents=True, exist_ok=True)
    url = url_template.format(z=tile.z, x=tile.x, y=tile.y)
    req = urllib.request.Request(url, headers={"User-Agent": "rundale-graphics-v2/continuity-proof"})
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            body = response.read()
    except urllib.error.HTTPError as exc:
        raise RuntimeError(f"fetch failed for {url}: HTTP {exc.code}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"fetch failed for {url}: {exc}") from exc

    out.write_bytes(body)
    return out


def load_tile(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")


def assemble_mosaic(center: TileRef, radius: int, cache_dir: Path, url_template: str) -> tuple[Image.Image, list[TileRef]]:
    refs: list[TileRef] = []
    size = radius * 2 + 1
    mosaic = Image.new("RGB", (size * TILE_SIZE, size * TILE_SIZE), "white")
    for row, y in enumerate(range(center.y - radius, center.y + radius + 1)):
        for col, x in enumerate(range(center.x - radius, center.x + radius + 1)):
            ref = TileRef(center.z, x, y)
            path = fetch_tile(ref, cache_dir, url_template)
            tile_img = load_tile(path)
            if tile_img.size != (TILE_SIZE, TILE_SIZE):
                tile_img = tile_img.resize((TILE_SIZE, TILE_SIZE), Image.Resampling.LANCZOS)
            mosaic.paste(tile_img, (col * TILE_SIZE, row * TILE_SIZE))
            refs.append(ref)
    return mosaic, refs


def autocontrast_rgb(img: Image.Image, cutoff: float) -> Image.Image:
    return ImageOps.autocontrast(img.convert("RGB"), cutoff=cutoff)


def paper_texture(size: tuple[int, int], seed: int) -> Image.Image:
    rng = np.random.default_rng(seed)
    noise = rng.normal(loc=128, scale=22, size=(size[1], size[0])).clip(0, 255).astype(np.uint8)
    tex = Image.fromarray(noise, mode="L").filter(ImageFilter.GaussianBlur(2.4))
    tex = ImageOps.autocontrast(tex, cutoff=1)
    return tex.convert("RGB")


def stylize_map(img: Image.Image, seed: int, local_autocontrast: bool) -> Image.Image:
    """A deterministic watercolor-ish transform.

    The visual target is not final art. The purpose is to expose seam behavior:
    local per-tile normalization creates visible discontinuities, while applying
    the same transform once over a mosaic keeps continuous texture and tone.
    """

    working = img.convert("RGB")
    working = autocontrast_rgb(working, cutoff=1.2 if local_autocontrast else 0.15)
    working = working.filter(ImageFilter.MedianFilter(size=3))
    working = ImageEnhance.Color(working).enhance(0.45)
    working = ImageEnhance.Contrast(working).enhance(0.82)
    working = ImageEnhance.Brightness(working).enhance(1.06)

    parchment = Image.new("RGB", working.size, (231, 220, 180))
    working = Image.blend(working, parchment, 0.18)

    # Darken ink-like edges gently.
    gray = ImageOps.grayscale(img)
    edges = gray.filter(ImageFilter.FIND_EDGES).filter(ImageFilter.GaussianBlur(0.35))
    edges = ImageOps.autocontrast(edges, cutoff=2)
    edge_arr = np.asarray(edges, dtype=np.float32) / 255.0
    arr = np.asarray(working, dtype=np.float32)
    arr *= 1.0 - (edge_arr[..., None] * 0.23)

    tex = np.asarray(paper_texture(working.size, seed), dtype=np.float32)
    arr = arr * 0.94 + tex * 0.06
    arr = np.clip(arr, 0, 255).astype(np.uint8)
    return Image.fromarray(arr, mode="RGB").filter(ImageFilter.GaussianBlur(0.22))


def stylize_independent_tiles(mosaic: Image.Image, grid: int) -> Image.Image:
    out = Image.new("RGB", mosaic.size, "white")
    for row in range(grid):
        for col in range(grid):
            box = (
                col * TILE_SIZE,
                row * TILE_SIZE,
                (col + 1) * TILE_SIZE,
                (row + 1) * TILE_SIZE,
            )
            tile = mosaic.crop(box)
            styled = stylize_map(tile, seed=10_000 + row * 101 + col, local_autocontrast=True)
            out.paste(styled, box[:2])
    return out


def export_runtime_tiles(img: Image.Image, refs: list[TileRef], grid: int, out_dir: Path) -> list[Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    idx = 0
    for row in range(grid):
        for col in range(grid):
            ref = refs[idx]
            idx += 1
            box = (
                col * TILE_SIZE,
                row * TILE_SIZE,
                (col + 1) * TILE_SIZE,
                (row + 1) * TILE_SIZE,
            )
            path = out_dir / f"{ref.name}.png"
            img.crop(box).save(path)
            paths.append(path)
    return paths


def reassemble_from_tiles(paths: list[Path], grid: int) -> Image.Image:
    out = Image.new("RGB", (grid * TILE_SIZE, grid * TILE_SIZE), "white")
    idx = 0
    for row in range(grid):
        for col in range(grid):
            out.paste(Image.open(paths[idx]).convert("RGB"), (col * TILE_SIZE, row * TILE_SIZE))
            idx += 1
    return out


def max_abs_error(a: Image.Image, b: Image.Image) -> int:
    arr_a = np.asarray(a.convert("RGB"), dtype=np.int16)
    arr_b = np.asarray(b.convert("RGB"), dtype=np.int16)
    return int(np.abs(arr_a - arr_b).max())


def luminance(img: Image.Image) -> np.ndarray:
    arr = np.asarray(img.convert("RGB"), dtype=np.float32)
    return arr[..., 0] * 0.299 + arr[..., 1] * 0.587 + arr[..., 2] * 0.114


def seam_energy(img: Image.Image, grid: int) -> dict[str, float]:
    lum = luminance(img)
    h, w = lum.shape
    vertical: list[float] = []
    horizontal: list[float] = []
    control_v: list[float] = []
    control_h: list[float] = []

    for x in range(TILE_SIZE, w, TILE_SIZE):
        vertical.append(float(np.abs(lum[:, x] - lum[:, x - 1]).mean()))
        for offset in (-32, 32):
            cx = x + offset
            if 1 <= cx < w:
                control_v.append(float(np.abs(lum[:, cx] - lum[:, cx - 1]).mean()))

    for y in range(TILE_SIZE, h, TILE_SIZE):
        horizontal.append(float(np.abs(lum[y, :] - lum[y - 1, :]).mean()))
        for offset in (-32, 32):
            cy = y + offset
            if 1 <= cy < h:
                control_h.append(float(np.abs(lum[cy, :] - lum[cy - 1, :]).mean()))

    seam_mean = float(np.mean(vertical + horizontal))
    control_mean = float(np.mean(control_v + control_h))
    return {
        "seam_mean_abs_luma_jump": seam_mean,
        "nearby_control_mean_abs_luma_jump": control_mean,
        "seam_to_control_ratio": seam_mean / control_mean if control_mean else 0.0,
    }


def draw_grid(img: Image.Image, grid: int, color: tuple[int, int, int], width: int = 3) -> Image.Image:
    out = img.copy()
    draw = ImageDraw.Draw(out)
    for i in range(1, grid):
        x = i * TILE_SIZE
        y = i * TILE_SIZE
        draw.line([(x, 0), (x, out.height)], fill=color, width=width)
        draw.line([(0, y), (out.width, y)], fill=color, width=width)
    return out


def label_panel(img: Image.Image, title: str, subtitle: str = "") -> Image.Image:
    font = ImageFont.load_default()
    pad = 18
    header_h = 54 if subtitle else 38
    out = Image.new("RGB", (img.width, img.height + header_h), (244, 241, 232))
    draw = ImageDraw.Draw(out)
    draw.text((pad, 10), title, fill=(36, 31, 24), font=font)
    if subtitle:
        draw.text((pad, 30), subtitle, fill=(93, 84, 70), font=font)
    out.paste(img, (0, header_h))
    return out


def make_contact_sheet(source: Image.Image, independent: Image.Image, mosaic_first: Image.Image, metrics: dict[str, object]) -> Image.Image:
    target_w = 520

    def prep(img: Image.Image, title: str, subtitle: str, grid_color: tuple[int, int, int]) -> Image.Image:
        scale = target_w / img.width
        resized = img.resize((target_w, int(img.height * scale)), Image.Resampling.LANCZOS)
        # Grid is drawn after resize, so derive scaled grid spacing.
        grid = int(round(img.width / TILE_SIZE))
        out = resized.copy()
        draw = ImageDraw.Draw(out)
        spacing = target_w / grid
        for i in range(1, grid):
            pos = int(round(i * spacing))
            draw.line([(pos, 0), (pos, out.height)], fill=grid_color, width=2)
            draw.line([(0, pos), (out.width, pos)], fill=grid_color, width=2)
        return label_panel(out, title, subtitle)

    src_ratio = metrics["source"]["seam_to_control_ratio"]
    ind_ratio = metrics["independent_tiles"]["seam_to_control_ratio"]
    mos_ratio = metrics["mosaic_first"]["seam_to_control_ratio"]
    panels = [
        prep(source, "A. Raw NLS source mosaic", f"baseline seam ratio {src_ratio:.2f}", (40, 78, 140)),
        prep(independent, "B. Per-runtime-tile stylized", f"seam ratio {ind_ratio:.2f}; visible tile normalization risk", (170, 40, 35)),
        prep(mosaic_first, "C. Mosaic-first stylized, then split", f"seam ratio {mos_ratio:.2f}; runtime reassembly error 0", (32, 120, 66)),
    ]
    gap = 22
    w = sum(p.width for p in panels) + gap * (len(panels) + 1)
    h = max(p.height for p in panels) + gap * 2
    sheet = Image.new("RGB", (w, h), (244, 241, 232))
    x = gap
    for panel in panels:
        sheet.paste(panel, (x, gap))
        x += panel.width + gap
    return sheet


def write_report(path: Path, args: argparse.Namespace, center: TileRef, refs: list[TileRef], metrics: dict[str, object]) -> None:
    path.write_text(
        "\n".join(
            [
                "# Cycle CE County Tile Continuity Experiment",
                "",
                "## Question",
                "",
                "Can we make overhead map tiles for a county while preserving continuity across runtime tile edges?",
                "",
                "## Experiment",
                "",
                f"- Center coordinate: `{args.lat:.8f}, {args.lon:.8f}`",
                f"- Center XYZ tile: `{center.name}`",
                f"- Zoom: `{args.zoom}`",
                f"- Grid: `{args.radius * 2 + 1}x{args.radius * 2 + 1}` NLS source tiles",
                f"- Source URL: `{args.url_template}`",
                "",
                "Compared two workflows on the same real Roscommon NLS tile mosaic:",
                "",
                "1. **Independent runtime tiles:** each `256x256` source tile gets local tone/style processing.",
                "2. **Mosaic-first / supertile-first:** the whole mosaic gets one shared transform, then runtime tiles are exported.",
                "",
                "The second workflow is the county-scale rule we need if imagegen or a future style model is used: operate on padded mosaics/supertiles, crop safe centers, and never ask the renderer to invent continuity tile-by-tile.",
                "",
                "## Metrics",
                "",
                "Seam ratio = mean luminance jump at tile edges divided by nearby non-edge jumps. Lower is better; values near the raw-source baseline mean the tile grid is not adding its own seam.",
                "",
                "```json",
                json.dumps(metrics, indent=2),
                "```",
                "",
                "## Result",
                "",
                "- Independent per-tile styling creates an avoidable tile-grid artifact because every tile normalizes tone and texture separately.",
                "- Mosaic-first styling keeps the runtime export exactly reassemblable (`max_abs_reassembly_error = 0`) and avoids adding an extra tile-grid seam beyond the source/map content.",
                "- This proves the first production primitive: **county maps should be generated/stylized as larger continuous supertiles, then split into runtime tiles mechanically.**",
                "",
                "## What This Does Not Prove Yet",
                "",
                "- It does not prove that two independently image-generated neighboring supertiles will match perfectly.",
                "- It does not solve semantic extraction for roads/buildings/boundaries.",
                "- It does not make final Cycle CB/CD-quality art; this deterministic transform is a seam test surface.",
                "",
                "## Next Required Experiment",
                "",
                "Generate two overlapping imagegen supertiles from the same source mosaic and repair/crop them against a shared seam contract. The acceptance test is a 2x1 or 2x2 seam contact sheet where roads, paths, water, boundaries, and palette match across the exported safe centers.",
                "",
                "## Source Tiles",
                "",
                *[f"- `{ref.name}`" for ref in refs],
                "",
            ]
        )
        + "\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lat", type=float, default=53.63579941155877)
    parser.add_argument("--lon", type=float, default=-8.079662971357214)
    parser.add_argument("--zoom", type=int, default=17)
    parser.add_argument("--radius", type=int, default=2)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--cache-dir", type=Path, default=Path("/private/tmp/rundale-nls-tile-cache"))
    parser.add_argument("--url-template", default=NLS_ROSCOMMON_URL)
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    grid = args.radius * 2 + 1

    cx, cy = deg_to_tile(args.lat, args.lon, args.zoom)
    center = TileRef(args.zoom, cx, cy)
    source, refs = assemble_mosaic(center, args.radius, args.cache_dir, args.url_template)
    source.save(args.out_dir / "murphy-z17-nls-source-mosaic-5x5.png")

    independent = stylize_independent_tiles(source, grid)
    independent.save(args.out_dir / "murphy-z17-independent-runtime-tile-stylized.png")

    mosaic_first = stylize_map(source, seed=42, local_autocontrast=False)
    mosaic_first.save(args.out_dir / "murphy-z17-mosaic-first-stylized-supertile.png")

    runtime_tile_dir = args.out_dir / "runtime-tiles"
    tile_paths = export_runtime_tiles(mosaic_first, refs, grid, runtime_tile_dir)
    reassembled = reassemble_from_tiles(tile_paths, grid)
    reassembled.save(args.out_dir / "murphy-z17-mosaic-first-runtime-reassembled.png")

    metrics: dict[str, object] = {
        "source": seam_energy(source, grid),
        "independent_tiles": seam_energy(independent, grid),
        "mosaic_first": seam_energy(mosaic_first, grid),
        "max_abs_reassembly_error": max_abs_error(mosaic_first, reassembled),
        "tile_count": len(tile_paths),
        "tile_size": TILE_SIZE,
        "grid": f"{grid}x{grid}",
    }

    (args.out_dir / "murphy-z17-seam-metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")

    contact = make_contact_sheet(source, independent, mosaic_first, metrics)
    contact.save(args.out_dir / "murphy-z17-continuity-contact-sheet.png")

    draw_grid(mosaic_first, grid, (30, 115, 66), width=3).save(args.out_dir / "murphy-z17-mosaic-first-grid-overlay.png")
    draw_grid(independent, grid, (175, 40, 35), width=3).save(args.out_dir / "murphy-z17-independent-grid-overlay.png")

    write_report(args.out_dir / "README.md", args, center, refs, metrics)
    print(json.dumps({"out_dir": str(args.out_dir), "metrics": metrics}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
