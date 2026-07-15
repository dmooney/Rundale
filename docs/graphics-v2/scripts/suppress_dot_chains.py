#!/usr/bin/env python3
"""Suppress likely non-physical dotted/pecked map-boundary chains.

This is a prototype control-prep tool for graphics-v2 experiments. It uses only
pixel evidence from the input map crop: compact dark components that align into
long dotted/pecked chains are masked and locally filled from nearby unmasked
pixels. It does not know any place names, coordinates, or hand-authored road
notes.

The goal is not perfect cartographic segmentation. The goal is to create a
secondary "physical linework" map/control input that makes administrative or
survey dot chains less tempting for image models to turn into walls or hedges.
Always keep the original map crop as source evidence beside this cleaned crop.
"""

from __future__ import annotations

import argparse
import math
import sys
from collections import defaultdict
from itertools import pairwise
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from prototype_map_controls import (  # noqa: E402
    Component,
    connected_components,
    gray_at,
    make_oblique_raw,
    read_png,
    write_png,
)


def make_dark_mask(width: int, height: int, rgba: bytearray, threshold: int) -> bytearray:
    mask = bytearray(width * height)
    for y in range(height):
        for x in range(width):
            pi = y * width + x
            if gray_at(rgba, pi * 4) < threshold:
                mask[pi] = 1
    return mask


def compact_dot_candidates(
    components: list[Component],
    *,
    min_area: int,
    max_area: int,
    max_size: int,
    min_density: float,
) -> list[Component]:
    out: list[Component] = []
    for c in components:
        aspect = c.width / max(1, c.height)
        if (
            min_area <= c.area <= max_area
            and 2 <= c.width <= max_size
            and 2 <= c.height <= max_size
            and min_density <= c.density <= 1.0
            and 0.25 <= aspect <= 4.0
        ):
            out.append(c)
    return out


def component_centers(components: list[Component]) -> list[tuple[float, float]]:
    return [((c.x0 + c.x1) / 2.0, (c.y0 + c.y1) / 2.0) for c in components]


def detect_chain_members(
    candidates: list[Component],
    width: int,
    height: int,
    *,
    angle_step: int = 3,
    rho_bin_size: float = 5.0,
    min_members: int = 7,
    min_span: float = 90.0,
    max_median_gap: float = 24.0,
) -> tuple[set[int], list[tuple[float, float, float, float]]]:
    centers = component_centers(candidates)
    votes: dict[tuple[int, int], list[int]] = defaultdict(list)
    diag = math.hypot(width, height)
    for idx, (x, y) in enumerate(centers):
        for angle in range(0, 180, angle_step):
            theta = math.radians(angle)
            rho = x * math.cos(theta) + y * math.sin(theta)
            bucket = int(round((rho + diag) / rho_bin_size))
            votes[(angle, bucket)].append(idx)

    marked: set[int] = set()
    segments: list[tuple[float, float, float, float]] = []
    for (angle, _bucket), ids in votes.items():
        if len(ids) < min_members:
            continue
        theta = math.radians(angle)
        rhos = [centers[i][0] * math.cos(theta) + centers[i][1] * math.sin(theta) for i in ids]
        ts = sorted(-centers[i][0] * math.sin(theta) + centers[i][1] * math.cos(theta) for i in ids)
        span = ts[-1] - ts[0]
        if span < min_span:
            continue
        gaps = [b - a for a, b in pairwise(ts) if b - a > 1.0]
        if not gaps:
            continue
        median_gap = sorted(gaps)[len(gaps) // 2]
        density = len(ids) / max(1.0, span)
        if median_gap <= max_median_gap and density >= 0.045:
            marked.update(ids)
            segments.append((theta, sum(rhos) / len(rhos), ts[0], ts[-1]))
    return marked, segments


def dilate_component_mask(
    width: int,
    height: int,
    components: list[Component],
    marked_ids: set[int],
    *,
    dilation: int,
) -> bytearray:
    mask = bytearray(width * height)
    for idx in marked_ids:
        c = components[idx]
        for y in range(max(0, c.y0 - dilation), min(height, c.y1 + dilation + 1)):
            for x in range(max(0, c.x0 - dilation), min(width, c.x1 + dilation + 1)):
                mask[y * width + x] = 1
    return mask


def add_chain_corridors(
    width: int,
    height: int,
    mask: bytearray,
    segments: list[tuple[float, float, float, float]],
    *,
    strip_width: float,
) -> bytearray:
    """Mask narrow strips through accepted long dot chains.

    Component dilation removes dots but can leave a dotted visual scar. This
    optional pass suppresses the corridor between the dots for accepted long
    chains, still using only the data-derived chain geometry.
    """

    if strip_width <= 0:
        return mask
    out = bytearray(mask)
    margin = int(math.ceil(strip_width)) + 2
    for theta, rho, t0, t1 in segments:
        cos_t = math.cos(theta)
        sin_t = math.sin(theta)
        # Convert line endpoints back to image coordinates for a tight scan box.
        x0 = rho * cos_t - t0 * sin_t
        y0 = rho * sin_t + t0 * cos_t
        x1 = rho * cos_t - t1 * sin_t
        y1 = rho * sin_t + t1 * cos_t
        xmin = max(0, int(math.floor(min(x0, x1) - strip_width - margin)))
        xmax = min(width - 1, int(math.ceil(max(x0, x1) + strip_width + margin)))
        ymin = max(0, int(math.floor(min(y0, y1) - strip_width - margin)))
        ymax = min(height - 1, int(math.ceil(max(y0, y1) + strip_width + margin)))
        for y in range(ymin, ymax + 1):
            for x in range(xmin, xmax + 1):
                px = x + 0.5
                py = y + 0.5
                point_rho = px * cos_t + py * sin_t
                point_t = -px * sin_t + py * cos_t
                if abs(point_rho - rho) <= strip_width and t0 - margin <= point_t <= t1 + margin:
                    out[y * width + x] = 1
    return out


def fill_mask_from_neighbors(
    width: int,
    height: int,
    rgba: bytearray,
    mask: bytearray,
    *,
    radius: int,
) -> bytearray:
    out = bytearray(rgba)
    fallback = (236, 228, 207)
    for y in range(height):
        for x in range(width):
            pi = y * width + x
            if not mask[pi]:
                continue
            samples: list[tuple[int, int, int]] = []
            for yy in range(max(0, y - radius), min(height, y + radius + 1)):
                for xx in range(max(0, x - radius), min(width, x + radius + 1)):
                    qi = yy * width + xx
                    if mask[qi]:
                        continue
                    dx = xx - x
                    dy = yy - y
                    if dx * dx + dy * dy <= radius * radius:
                        oi = qi * 4
                        samples.append((rgba[oi], rgba[oi + 1], rgba[oi + 2]))
            if samples:
                samples.sort(key=lambda rgb: rgb[0] + rgb[1] + rgb[2])
                sample = samples[len(samples) // 2]
            else:
                sample = fallback
            oi = pi * 4
            out[oi : oi + 4] = bytes((sample[0], sample[1], sample[2], 255))
    return out


def make_overlay(width: int, height: int, rgba: bytearray, mask: bytearray) -> bytearray:
    out = bytearray(rgba)
    for y in range(height):
        for x in range(width):
            pi = y * width + x
            if mask[pi]:
                oi = pi * 4
                out[oi] = int(out[oi] * 0.35 + 230 * 0.65)
                out[oi + 1] = int(out[oi + 1] * 0.35 + 30 * 0.65)
                out[oi + 2] = int(out[oi + 2] * 0.35 + 30 * 0.65)
                out[oi + 3] = 255
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--prefix", default="dot-suppressed")
    parser.add_argument("--dark-threshold", type=int, default=154)
    parser.add_argument("--min-candidate-area", type=int, default=3)
    parser.add_argument("--max-candidate-area", type=int, default=70)
    parser.add_argument("--max-candidate-size", type=int, default=14)
    parser.add_argument("--min-candidate-density", type=float, default=0.30)
    parser.add_argument("--min-chain-members", type=int, default=8)
    parser.add_argument("--min-chain-span", type=float, default=110.0)
    parser.add_argument("--max-median-gap", type=float, default=22.0)
    parser.add_argument("--dilation", type=int, default=3)
    parser.add_argument("--strip-width", type=float, default=0.0)
    parser.add_argument("--fill-radius", type=int, default=7)
    parser.add_argument("--width", type=int, default=1672)
    parser.add_argument("--height", type=int, default=941)
    args = parser.parse_args()

    width, height, rgba = read_png(args.input)
    dark = make_dark_mask(width, height, rgba, args.dark_threshold)
    components = connected_components(dark, width, height)
    candidates = compact_dot_candidates(
        components,
        min_area=args.min_candidate_area,
        max_area=args.max_candidate_area,
        max_size=args.max_candidate_size,
        min_density=args.min_candidate_density,
    )
    marked_ids, chain_segments = detect_chain_members(
        candidates,
        width,
        height,
        min_members=args.min_chain_members,
        min_span=args.min_chain_span,
        max_median_gap=args.max_median_gap,
    )
    suppress_mask = dilate_component_mask(
        width, height, candidates, marked_ids, dilation=args.dilation
    )
    suppress_mask = add_chain_corridors(
        width,
        height,
        suppress_mask,
        chain_segments,
        strip_width=args.strip_width,
    )
    cleaned = fill_mask_from_neighbors(width, height, rgba, suppress_mask, radius=args.fill_radius)
    overlay = make_overlay(width, height, rgba, suppress_mask)
    oblique = make_oblique_raw(width, height, cleaned, args.width, args.height)

    args.out_dir.mkdir(parents=True, exist_ok=True)
    cleaned_path = args.out_dir / f"{args.prefix}-no-admin-map-crop.png"
    overlay_path = args.out_dir / f"{args.prefix}-suppression-overlay.png"
    oblique_path = args.out_dir / f"{args.prefix}-no-admin-oblique-raw-warp.png"
    report_path = args.out_dir / f"{args.prefix}-suppression-report.md"

    write_png(cleaned_path, width, height, cleaned)
    write_png(overlay_path, width, height, overlay)
    write_png(oblique_path, args.width, args.height, oblique)

    report = [
        "# Dot-Chain Suppression Report",
        "",
        f"Input: `{args.input}`",
        f"Input size: {width}x{height}",
        f"Dark threshold: {args.dark_threshold}",
        f"Candidate area range: {args.min_candidate_area}..{args.max_candidate_area}",
        f"Candidate max width/height: {args.max_candidate_size}",
        f"Candidate min density: {args.min_candidate_density}",
        f"Minimum chain members/span: {args.min_chain_members}/{args.min_chain_span}",
        f"Maximum median chain gap: {args.max_median_gap}",
        f"Connected dark components: {len(components)}",
        f"Compact dot/dash candidates: {len(candidates)}",
        f"Candidates marked as long dot/peck chains: {len(marked_ids)}",
        f"Accepted chain segments: {len(chain_segments)}",
        f"Continuous chain strip width: {args.strip_width}",
        f"Suppressed pixels after dilation: {sum(1 for v in suppress_mask if v)}",
        "",
        "Method: threshold dark ink, find compact connected components, run a",
        "coarse Hough-style long-chain detector over candidate centers, dilate",
        "only the marked components, and locally fill masked pixels from nearby",
        "unmasked map texture. No location name, road graph, or hand-authored",
        "feature hints are used.",
        "",
        "Outputs:",
        f"- `{cleaned_path.name}`",
        f"- `{overlay_path.name}`",
        f"- `{oblique_path.name}`",
        "",
        "Caveats: this is a prototype. It may miss curved or irregular dotted",
        "administrative boundaries, and it may suppress some legitimate physical",
        "dot chains if they have similar geometry. Always pass the original crop",
        "alongside the cleaned crop, and audit visually before using the cleaned",
        "crop as physical-linework authority.",
    ]
    report_path.write_text("\n".join(report) + "\n")
    print("\n".join(report))


if __name__ == "__main__":
    main()
