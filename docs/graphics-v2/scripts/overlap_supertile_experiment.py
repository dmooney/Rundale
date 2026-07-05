#!/usr/bin/env python3
"""Prepare and stitch overlapping imagegen supertile experiments."""

from __future__ import annotations

import argparse
import json
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

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


def fetch_tile(ref: TileRef, cache_dir: Path, url_template: str) -> Path:
    out = cache_dir / str(ref.z) / str(ref.x) / f"{ref.y}.png"
    if out.exists():
        return out
    out.parent.mkdir(parents=True, exist_ok=True)
    url = url_template.format(z=ref.z, x=ref.x, y=ref.y)
    req = urllib.request.Request(url, headers={"User-Agent": "rundale-graphics-v2/overlap-proof"})
    with urllib.request.urlopen(req, timeout=30) as response:
        out.write_bytes(response.read())
    return out


def assemble_rect(
    z: int, x0: int, y0: int, cols: int, rows: int, cache_dir: Path, url_template: str
) -> tuple[Image.Image, list[TileRef]]:
    img = Image.new("RGB", (cols * TILE_SIZE, rows * TILE_SIZE), "white")
    refs: list[TileRef] = []
    for row in range(rows):
        for col in range(cols):
            ref = TileRef(z, x0 + col, y0 + row)
            path = fetch_tile(ref, cache_dir, url_template)
            tile = Image.open(path).convert("RGB")
            img.paste(tile, (col * TILE_SIZE, row * TILE_SIZE))
            refs.append(ref)
    return img, refs


def luminance(img: Image.Image) -> np.ndarray:
    arr = np.asarray(img.convert("RGB"), dtype=np.float32)
    return arr[..., 0] * 0.299 + arr[..., 1] * 0.587 + arr[..., 2] * 0.114


def boundary_jump(img: Image.Image, x: int) -> dict[str, float]:
    lum = luminance(img)
    seam = float(np.abs(lum[:, x] - lum[:, x - 1]).mean())
    controls = []
    for offset in (-64, -32, 32, 64):
        cx = x + offset
        if 1 <= cx < img.width:
            controls.append(float(np.abs(lum[:, cx] - lum[:, cx - 1]).mean()))
    control = float(np.mean(controls))
    return {
        "join_mean_abs_luma_jump": seam,
        "nearby_control_mean_abs_luma_jump": control,
        "join_to_control_ratio": seam / control if control else 0.0,
    }


def draw_grid(
    img: Image.Image, cols: int, rows: int, tile_size: int, join_x: int | None = None
) -> Image.Image:
    out = img.copy()
    draw = ImageDraw.Draw(out)
    for col in range(1, cols):
        x = col * tile_size
        color = (35, 120, 68)
        width = 2
        if join_x is not None and x == join_x:
            color = (190, 38, 34)
            width = 5
        draw.line([(x, 0), (x, out.height)], fill=color, width=width)
    for row in range(1, rows):
        y = row * tile_size
        draw.line([(0, y), (out.width, y)], fill=(35, 120, 68), width=2)
    return out


def label_panel(img: Image.Image, title: str, subtitle: str = "") -> Image.Image:
    font = ImageFont.load_default()
    header_h = 54 if subtitle else 38
    out = Image.new("RGB", (img.width, img.height + header_h), (244, 241, 232))
    draw = ImageDraw.Draw(out)
    draw.text((14, 10), title, fill=(36, 31, 24), font=font)
    if subtitle:
        draw.text((14, 30), subtitle, fill=(93, 84, 70), font=font)
    out.paste(img, (0, header_h))
    return out


def prepare(args: argparse.Namespace) -> int:
    args.out_dir.mkdir(parents=True, exist_ok=True)
    source, refs = assemble_rect(
        args.z,
        args.x0,
        args.y0,
        args.cols,
        args.rows,
        args.cache_dir,
        args.url_template,
    )
    source.save(args.out_dir / "murphy-overlap-source-6x5.png")

    west = source.crop((0, 0, 4 * TILE_SIZE, args.rows * TILE_SIZE))
    east = source.crop((2 * TILE_SIZE, 0, 6 * TILE_SIZE, args.rows * TILE_SIZE))
    west.save(args.out_dir / "murphy-overlap-west-source-input.png")
    east.save(args.out_dir / "murphy-overlap-east-source-input.png")

    manifest = {
        "source_grid": f"{args.cols}x{args.rows}",
        "source_origin": {"z": args.z, "x0": args.x0, "y0": args.y0},
        "west_input_source_cols": [0, 1, 2, 3],
        "east_input_source_cols": [2, 3, 4, 5],
        "west_safe_center_source_cols": [1, 2],
        "east_safe_center_source_cols": [3, 4],
        "join_between_source_cols": [2, 3],
        "tile_refs": [ref.name for ref in refs],
    }
    (args.out_dir / "murphy-overlap-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n"
    )
    print(json.dumps(manifest, indent=2))
    return 0


def stitch(args: argparse.Namespace) -> int:
    args.out_dir.mkdir(parents=True, exist_ok=True)
    west = (
        Image.open(args.west_art)
        .convert("RGB")
        .resize((4 * TILE_SIZE, args.rows * TILE_SIZE), Image.Resampling.LANCZOS)
    )
    east = (
        Image.open(args.east_art)
        .convert("RGB")
        .resize((4 * TILE_SIZE, args.rows * TILE_SIZE), Image.Resampling.LANCZOS)
    )
    west.save(args.out_dir / "murphy-overlap-west-art-normalized.png")
    east.save(args.out_dir / "murphy-overlap-east-art-normalized.png")

    west_safe = west.crop((1 * TILE_SIZE, 0, 3 * TILE_SIZE, args.rows * TILE_SIZE))
    east_safe = east.crop((1 * TILE_SIZE, 0, 3 * TILE_SIZE, args.rows * TILE_SIZE))
    stitched = Image.new("RGB", (4 * TILE_SIZE, args.rows * TILE_SIZE), "white")
    stitched.paste(west_safe, (0, 0))
    stitched.paste(east_safe, (2 * TILE_SIZE, 0))
    stitched.save(args.out_dir / "murphy-overlap-independent-imagegen-safe-centers-stitched.png")

    tiles_dir = args.out_dir / "murphy-overlap-independent-imagegen-runtime-tiles"
    tiles_dir.mkdir(parents=True, exist_ok=True)
    for row in range(args.rows):
        for col in range(4):
            box = (col * TILE_SIZE, row * TILE_SIZE, (col + 1) * TILE_SIZE, (row + 1) * TILE_SIZE)
            stitched.crop(box).save(tiles_dir / f"murphy-overlap-r{row:02d}-c{col:02d}.png")

    metrics = {
        "grid": f"4x{args.rows}",
        "tile_count": 4 * args.rows,
        "tile_size": TILE_SIZE,
        "join_x": 2 * TILE_SIZE,
        "join": boundary_jump(stitched, 2 * TILE_SIZE),
    }
    (args.out_dir / "murphy-overlap-independent-imagegen-stitch-metrics.json").write_text(
        json.dumps(metrics, indent=2) + "\n"
    )

    overlay = draw_grid(stitched, 4, args.rows, TILE_SIZE, join_x=2 * TILE_SIZE)
    overlay.save(args.out_dir / "murphy-overlap-independent-imagegen-stitch-grid-overlay.png")

    scaled = overlay.resize(
        (720, int(720 * overlay.height / overlay.width)), Image.Resampling.LANCZOS
    )
    contact = label_panel(
        scaled,
        "Overlapping independent imagegen supertiles: safe-center stitch",
        f"red line is join; join ratio {metrics['join']['join_to_control_ratio']:.2f}",
    )
    contact.save(args.out_dir / "murphy-overlap-independent-imagegen-stitch-contact-sheet.png")
    print(json.dumps(metrics, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)

    prep = sub.add_parser("prepare")
    prep.add_argument("--out-dir", type=Path, required=True)
    prep.add_argument("--cache-dir", type=Path, default=Path("/private/tmp/rundale-nls-tile-cache"))
    prep.add_argument("--url-template", default=NLS_ROSCOMMON_URL)
    prep.add_argument("--z", type=int, default=17)
    prep.add_argument("--x0", type=int, default=62591)
    prep.add_argument("--y0", type=int, default=42307)
    prep.add_argument("--cols", type=int, default=6)
    prep.add_argument("--rows", type=int, default=5)
    prep.set_defaults(func=prepare)

    st = sub.add_parser("stitch")
    st.add_argument("--out-dir", type=Path, required=True)
    st.add_argument("--west-art", type=Path, required=True)
    st.add_argument("--east-art", type=Path, required=True)
    st.add_argument("--rows", type=int, default=5)
    st.set_defaults(func=stitch)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
