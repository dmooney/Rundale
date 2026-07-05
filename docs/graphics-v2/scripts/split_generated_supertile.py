#!/usr/bin/env python3
"""Split a generated overhead-art supertile into runtime tile diagnostics."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont


def luminance(img: Image.Image) -> np.ndarray:
    arr = np.asarray(img.convert("RGB"), dtype=np.float32)
    return arr[..., 0] * 0.299 + arr[..., 1] * 0.587 + arr[..., 2] * 0.114


def seam_energy(img: Image.Image, cols: int, rows: int, tile_size: int) -> dict[str, float]:
    lum = luminance(img)
    h, w = lum.shape
    vertical: list[float] = []
    horizontal: list[float] = []
    control_v: list[float] = []
    control_h: list[float] = []

    for col in range(1, cols):
        x = col * tile_size
        vertical.append(float(np.abs(lum[:, x] - lum[:, x - 1]).mean()))
        for offset in (-32, 32):
            cx = x + offset
            if 1 <= cx < w:
                control_v.append(float(np.abs(lum[:, cx] - lum[:, cx - 1]).mean()))

    for row in range(1, rows):
        y = row * tile_size
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


def max_abs_error(a: Image.Image, b: Image.Image) -> int:
    arr_a = np.asarray(a.convert("RGB"), dtype=np.int16)
    arr_b = np.asarray(b.convert("RGB"), dtype=np.int16)
    return int(np.abs(arr_a - arr_b).max())


def draw_grid(img: Image.Image, cols: int, rows: int, tile_size: int, color: tuple[int, int, int]) -> Image.Image:
    out = img.copy()
    draw = ImageDraw.Draw(out)
    for col in range(1, cols):
        x = col * tile_size
        draw.line([(x, 0), (x, out.height)], fill=color, width=3)
    for row in range(1, rows):
        y = row * tile_size
        draw.line([(0, y), (out.width, y)], fill=color, width=3)
    return out


def label_panel(img: Image.Image, title: str, subtitle: str) -> Image.Image:
    font = ImageFont.load_default()
    header_h = 54
    out = Image.new("RGB", (img.width, img.height + header_h), (244, 241, 232))
    draw = ImageDraw.Draw(out)
    draw.text((14, 10), title, fill=(36, 31, 24), font=font)
    draw.text((14, 30), subtitle, fill=(93, 84, 70), font=font)
    out.paste(img, (0, header_h))
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--cols", type=int, default=5)
    parser.add_argument("--rows", type=int, default=4)
    parser.add_argument("--tile-size", type=int, default=256)
    parser.add_argument("--prefix", default="imagegen-supertile")
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    img = Image.open(args.input).convert("RGB")
    target_size = (args.cols * args.tile_size, args.rows * args.tile_size)
    normalized = img.resize(target_size, Image.Resampling.LANCZOS)
    normalized_path = args.out_dir / f"{args.prefix}-normalized-{target_size[0]}x{target_size[1]}.png"
    normalized.save(normalized_path)

    tiles_dir = args.out_dir / f"{args.prefix}-runtime-tiles"
    tiles_dir.mkdir(parents=True, exist_ok=True)
    tile_paths: list[Path] = []
    for row in range(args.rows):
        for col in range(args.cols):
            box = (
                col * args.tile_size,
                row * args.tile_size,
                (col + 1) * args.tile_size,
                (row + 1) * args.tile_size,
            )
            path = tiles_dir / f"{args.prefix}-r{row:02d}-c{col:02d}.png"
            normalized.crop(box).save(path)
            tile_paths.append(path)

    reassembled = Image.new("RGB", target_size, "white")
    i = 0
    for row in range(args.rows):
        for col in range(args.cols):
            reassembled.paste(Image.open(tile_paths[i]).convert("RGB"), (col * args.tile_size, row * args.tile_size))
            i += 1
    reassembled_path = args.out_dir / f"{args.prefix}-runtime-reassembled.png"
    reassembled.save(reassembled_path)

    metrics = {
        "input": str(args.input),
        "original_size": list(img.size),
        "normalized_size": list(target_size),
        "grid": f"{args.cols}x{args.rows}",
        "tile_size": args.tile_size,
        "tile_count": len(tile_paths),
        "seam": seam_energy(normalized, args.cols, args.rows, args.tile_size),
        "max_abs_reassembly_error": max_abs_error(normalized, reassembled),
    }
    (args.out_dir / f"{args.prefix}-split-metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")

    grid_overlay = draw_grid(normalized, args.cols, args.rows, args.tile_size, (35, 120, 68))
    grid_overlay_path = args.out_dir / f"{args.prefix}-grid-overlay.png"
    grid_overlay.save(grid_overlay_path)

    scaled = grid_overlay.resize((720, int(720 * grid_overlay.height / grid_overlay.width)), Image.Resampling.LANCZOS)
    contact = label_panel(
        scaled,
        "Imagegen continuous supertile split test",
        f"{args.cols}x{args.rows} runtime tiles; seam ratio {metrics['seam']['seam_to_control_ratio']:.2f}; reassembly error {metrics['max_abs_reassembly_error']}",
    )
    contact.save(args.out_dir / f"{args.prefix}-split-contact-sheet.png")

    print(json.dumps(metrics, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
