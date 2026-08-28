#!/usr/bin/env python3
"""Production-shaped county overhead tile pipeline for Graphics V2.

This script turns real NLS Roscommon historic-map XYZ tiles into continuous
overhead watercolor-style runtime tiles. It is intentionally explicit about the
production rules proven in Cycle CE:

- build a source mosaic first;
- stylize/render continuous panels or supertiles;
- split runtime tiles mechanically from the continuous parent;
- validate seams and reassembly;
- do not accept independent imagegen neighbors without a repair/contract pass.

The renderer here is deterministic and local. It is not the final art model,
but it proves the county-scale mechanics and produces a safe county base layer.
Imagegen remains an optional high-value/local-art branch documented by the
manifests and repair package outputs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import numpy as np
from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageFont, ImageOps

SCRIPT_VERSION = "county-tile-pipeline-v1"
NLS_ROSCOMMON_URL = "https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png"
NLS_ATTRIBUTION = 'Historic 6" OS Ireland (1829-1842), via National Library of Scotland'
NLS_LICENSE_NOTE = (
    "NLS historic tiles are documented in Rundale notices as CC-BY / permission of NLS."
)
TILE_SIZE = 256
DEFAULT_THRESHOLD = 1.15


@dataclass(frozen=True)
class TileRef:
    z: int
    x: int
    y: int

    @property
    def id(self) -> str:
        return f"z{self.z}-x{self.x}-y{self.y}"

    @property
    def path_name(self) -> str:
        return f"{self.id}.png"


@dataclass(frozen=True)
class SourceExtent:
    z: int
    x0: int
    y0: int
    cols: int
    rows: int

    @property
    def x1(self) -> int:
        return self.x0 + self.cols - 1

    @property
    def y1(self) -> int:
        return self.y0 + self.rows - 1

    @property
    def tile_count(self) -> int:
        return self.cols * self.rows

    @property
    def size_px(self) -> tuple[int, int]:
        return (self.cols * TILE_SIZE, self.rows * TILE_SIZE)

    def refs(self) -> list[TileRef]:
        return [
            TileRef(self.z, self.x0 + col, self.y0 + row)
            for row in range(self.rows)
            for col in range(self.cols)
        ]


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def deg_to_tile(lat: float, lon: float, z: int) -> tuple[int, int]:
    lat_rad = math.radians(lat)
    n = 2.0**z
    x = int((lon + 180.0) / 360.0 * n)
    y = int((1.0 - math.asinh(math.tan(lat_rad)) / math.pi) / 2.0 * n)
    return x, y


def centered_extent(lat: float, lon: float, z: int, cols: int, rows: int) -> SourceExtent:
    cx, cy = deg_to_tile(lat, lon, z)
    x0 = cx - cols // 2
    y0 = cy - rows // 2
    return SourceExtent(z=z, x0=x0, y0=y0, cols=cols, rows=rows)


def fetch_tile(ref: TileRef, cache_dir: Path, url_template: str) -> Path:
    out = cache_dir / str(ref.z) / str(ref.x) / f"{ref.y}.png"
    if out.exists():
        return out
    out.parent.mkdir(parents=True, exist_ok=True)
    url = url_template.format(z=ref.z, x=ref.x, y=ref.y)
    req = urllib.request.Request(
        url, headers={"User-Agent": "rundale-graphics-v2/county-tile-pipeline"}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            out.write_bytes(response.read())
    except urllib.error.HTTPError as exc:
        raise RuntimeError(f"fetch failed for {url}: HTTP {exc.code}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"fetch failed for {url}: {exc}") from exc
    return out


def assemble_source_mosaic(extent: SourceExtent, cache_dir: Path, url_template: str) -> Image.Image:
    mosaic = Image.new("RGB", extent.size_px, "white")
    for row in range(extent.rows):
        for col in range(extent.cols):
            ref = TileRef(extent.z, extent.x0 + col, extent.y0 + row)
            tile_path = fetch_tile(ref, cache_dir, url_template)
            tile = Image.open(tile_path).convert("RGB")
            if tile.size != (TILE_SIZE, TILE_SIZE):
                tile = tile.resize((TILE_SIZE, TILE_SIZE), Image.Resampling.LANCZOS)
            mosaic.paste(tile, (col * TILE_SIZE, row * TILE_SIZE))
    return mosaic


def world_stable_texture(
    size: tuple[int, int], world_x0: int, world_y0: int, seed: int
) -> Image.Image:
    """Create stable paper texture keyed to source tile coordinates.

    Texture must not reset at runtime tile edges. For proof runs we generate one
    full texture over the source mosaic; the coordinate-derived seed makes the
    output reproducible for a given source extent.
    """

    width, height = size
    coord_seed = (world_x0 * 73856093) ^ (world_y0 * 19349663) ^ seed
    rng = np.random.default_rng(coord_seed & 0xFFFFFFFF)
    base = rng.normal(loc=128, scale=21, size=(height, width)).clip(0, 255).astype(np.uint8)
    fine = rng.normal(loc=128, scale=8, size=(height, width)).clip(0, 255).astype(np.uint8)
    tex = Image.fromarray(base, mode="L").filter(ImageFilter.GaussianBlur(2.2))
    fine_img = Image.fromarray(fine, mode="L").filter(ImageFilter.GaussianBlur(0.45))
    tex = Image.blend(tex, fine_img, 0.18)
    tex = ImageOps.autocontrast(tex, cutoff=1)
    return tex.convert("RGB")


def render_county_base(source: Image.Image, extent: SourceExtent) -> Image.Image:
    """Deterministic continuous watercolor-ish county base render."""

    working = source.convert("RGB")
    working = ImageOps.autocontrast(working, cutoff=0.08)
    working = working.filter(ImageFilter.MedianFilter(size=3))
    working = ImageEnhance.Color(working).enhance(0.34)
    working = ImageEnhance.Contrast(working).enhance(0.80)
    working = ImageEnhance.Brightness(working).enhance(1.08)

    parchment = Image.new("RGB", working.size, (232, 222, 185))
    working = Image.blend(working, parchment, 0.22)

    gray = ImageOps.grayscale(source)
    edges = gray.filter(ImageFilter.FIND_EDGES).filter(ImageFilter.GaussianBlur(0.35))
    edges = ImageOps.autocontrast(edges, cutoff=2)

    arr = np.asarray(working, dtype=np.float32)
    edge_arr = np.asarray(edges, dtype=np.float32) / 255.0
    arr *= 1.0 - edge_arr[..., None] * 0.18

    # Add a restrained green/ochre wash based on source lightness. This keeps
    # empty paper fields from staying pure scan beige while avoiding invented
    # per-tile variation.
    lum = np.asarray(gray, dtype=np.float32) / 255.0
    green_wash = np.zeros_like(arr)
    green_wash[..., 0] = 178
    green_wash[..., 1] = 185
    green_wash[..., 2] = 132
    wash_alpha = np.clip((lum - 0.52) * 0.18, 0.02, 0.12)[..., None]
    arr = arr * (1.0 - wash_alpha) + green_wash * wash_alpha

    texture = np.asarray(
        world_stable_texture(working.size, extent.x0 * TILE_SIZE, extent.y0 * TILE_SIZE, seed=42),
        dtype=np.float32,
    )
    arr = arr * 0.945 + texture * 0.055
    arr = np.clip(arr, 0, 255).astype(np.uint8)
    return Image.fromarray(arr, mode="RGB").filter(ImageFilter.GaussianBlur(0.18))


def semantic_layers(source: Image.Image) -> tuple[np.ndarray, Image.Image, dict[str, Any]]:
    """Build coarse deterministic semantic classes from the source mosaic.

    This is not a substitute for future hand-corrected cartographic layers. It
    is a reproducible seam-contract layer: dark linework, vegetation/rough
    texture, pale open/road ground, and paper/background.
    """

    gray_img = ImageOps.grayscale(source)
    gray = np.asarray(gray_img, dtype=np.uint8)
    edges_img = gray_img.filter(ImageFilter.FIND_EDGES).filter(ImageFilter.GaussianBlur(0.45))
    edges = np.asarray(ImageOps.autocontrast(edges_img, cutoff=2), dtype=np.uint8)

    labels = np.zeros(gray.shape, dtype=np.uint8)
    dark = gray < 135
    strong_edge = edges > 70
    texture = (edges > 38) & (gray < 222)
    pale = gray > 214

    labels[pale] = 1  # open/road/paper ground, kept separate for seam stats
    labels[texture] = 2  # vegetation/rough-pasture/symbol texture
    labels[strong_edge | dark] = 3  # ink/boundary/building/lettering linework

    colors = np.zeros((gray.shape[0], gray.shape[1], 3), dtype=np.uint8)
    colors[labels == 0] = (230, 220, 184)
    colors[labels == 1] = (245, 235, 202)
    colors[labels == 2] = (111, 139, 90)
    colors[labels == 3] = (55, 45, 34)
    overlay = Image.fromarray(colors, mode="RGB")
    meta = {
        "classes": {
            "0": "paper_or_unclassified",
            "1": "pale_open_ground_or_road_candidate",
            "2": "vegetation_rough_pasture_or_symbol_texture",
            "3": "ink_boundary_building_or_label_linework",
        },
        "method": "deterministic grayscale/edge thresholds over raw source mosaic",
        "caveat": "coarse proof layer; production semantic layers should be corrected with the map annotator",
    }
    return labels, overlay, meta


def contiguous_segments(
    values: np.ndarray, class_id: int, min_len: int = 3
) -> list[dict[str, int]]:
    mask = values == class_id
    segments: list[dict[str, int]] = []
    start: int | None = None
    for i, value in enumerate(mask):
        if value and start is None:
            start = i
        elif not value and start is not None:
            if i - start >= min_len:
                segments.append({"start_px": start, "end_px": i - 1, "length_px": i - start})
            start = None
    if start is not None and len(mask) - start >= min_len:
        segments.append(
            {"start_px": start, "end_px": len(mask) - 1, "length_px": len(mask) - start}
        )
    return segments


def seam_contracts(labels: np.ndarray, extent: SourceExtent) -> dict[str, Any]:
    contracts: list[dict[str, Any]] = []
    class_names = {
        1: "pale_open_ground_or_road_candidate",
        2: "vegetation_rough_pasture_or_symbol_texture",
        3: "ink_boundary_building_or_label_linework",
    }

    for col in range(1, extent.cols):
        x = col * TILE_SIZE
        band = labels[:, max(0, x - 1) : min(labels.shape[1], x + 2)]
        # Majority class in the 3px seam band at each y.
        values = np.array([np.bincount(row, minlength=4).argmax() for row in band], dtype=np.uint8)
        features = []
        for class_id, name in class_names.items():
            for seg in contiguous_segments(values, class_id):
                features.append({"class": name, **seg})
        contracts.append(
            {
                "orientation": "vertical",
                "x_px": x,
                "between_tile_cols": [col - 1, col],
                "world_between_x": [extent.x0 + col - 1, extent.x0 + col],
                "features": features,
            }
        )

    for row in range(1, extent.rows):
        y = row * TILE_SIZE
        band = labels[max(0, y - 1) : min(labels.shape[0], y + 2), :]
        values = np.array(
            [np.bincount(band[:, x], minlength=4).argmax() for x in range(band.shape[1])],
            dtype=np.uint8,
        )
        features = []
        for class_id, name in class_names.items():
            for seg in contiguous_segments(values, class_id):
                features.append({"class": name, **seg})
        contracts.append(
            {
                "orientation": "horizontal",
                "y_px": y,
                "between_tile_rows": [row - 1, row],
                "world_between_y": [extent.y0 + row - 1, extent.y0 + row],
                "features": features,
            }
        )

    return {
        "schema_version": 1,
        "tile_size": TILE_SIZE,
        "source_extent": asdict(extent),
        "contracts": contracts,
    }


def luminance(img: Image.Image) -> np.ndarray:
    arr = np.asarray(img.convert("RGB"), dtype=np.float32)
    return arr[..., 0] * 0.299 + arr[..., 1] * 0.587 + arr[..., 2] * 0.114


def seam_metrics(
    img: Image.Image, cols: int, rows: int, tile_size: int = TILE_SIZE
) -> dict[str, Any]:
    lum = luminance(img)
    seam_records: list[dict[str, Any]] = []

    def controls_for_vertical(x: int) -> list[float]:
        values = []
        for offset in (-64, -32, 32, 64):
            cx = x + offset
            if 1 <= cx < img.width:
                values.append(float(np.abs(lum[:, cx] - lum[:, cx - 1]).mean()))
        return values

    def controls_for_horizontal(y: int) -> list[float]:
        values = []
        for offset in (-64, -32, 32, 64):
            cy = y + offset
            if 1 <= cy < img.height:
                values.append(float(np.abs(lum[cy, :] - lum[cy - 1, :]).mean()))
        return values

    for col in range(1, cols):
        x = col * tile_size
        seam = float(np.abs(lum[:, x] - lum[:, x - 1]).mean())
        control = float(np.mean(controls_for_vertical(x)))
        seam_records.append(
            {
                "orientation": "vertical",
                "x_px": x,
                "between_cols": [col - 1, col],
                "mean_abs_luma_jump": seam,
                "nearby_control_mean_abs_luma_jump": control,
                "seam_to_control_ratio": seam / control if control else 0.0,
            }
        )

    for row in range(1, rows):
        y = row * tile_size
        seam = float(np.abs(lum[y, :] - lum[y - 1, :]).mean())
        control = float(np.mean(controls_for_horizontal(y)))
        seam_records.append(
            {
                "orientation": "horizontal",
                "y_px": y,
                "between_rows": [row - 1, row],
                "mean_abs_luma_jump": seam,
                "nearby_control_mean_abs_luma_jump": control,
                "seam_to_control_ratio": seam / control if control else 0.0,
            }
        )

    ratios = [r["seam_to_control_ratio"] for r in seam_records]
    return {
        "seams": seam_records,
        "max_seam_to_control_ratio": max(ratios) if ratios else 0.0,
        "mean_seam_to_control_ratio": float(np.mean(ratios)) if ratios else 0.0,
        "seam_count": len(seam_records),
    }


def export_runtime_tiles(
    img: Image.Image, extent: SourceExtent, out_dir: Path, parent_artifact: str
) -> list[dict[str, Any]]:
    out_dir.mkdir(parents=True, exist_ok=True)
    records: list[dict[str, Any]] = []
    for row in range(extent.rows):
        for col in range(extent.cols):
            ref = TileRef(extent.z, extent.x0 + col, extent.y0 + row)
            box = (
                col * TILE_SIZE,
                row * TILE_SIZE,
                (col + 1) * TILE_SIZE,
                (row + 1) * TILE_SIZE,
            )
            rel_path = Path("runtime-tiles") / str(ref.z) / str(ref.x) / f"{ref.y}.png"
            tile_path = out_dir / str(ref.z) / str(ref.x) / f"{ref.y}.png"
            tile_path.parent.mkdir(parents=True, exist_ok=True)
            img.crop(box).save(tile_path)
            records.append(
                {
                    "tile_id": ref.id,
                    "z": ref.z,
                    "x": ref.x,
                    "y": ref.y,
                    "source_ref": ref.id,
                    "parent_artifact": parent_artifact,
                    "pixel_box_in_parent": list(box),
                    "path": str(rel_path),
                }
            )
    return records


def reassemble_tiles(
    tile_records: list[dict[str, Any]], run_dir: Path, extent: SourceExtent
) -> Image.Image:
    out = Image.new("RGB", extent.size_px, "white")
    by_id = {record["tile_id"]: record for record in tile_records}
    for row in range(extent.rows):
        for col in range(extent.cols):
            ref = TileRef(extent.z, extent.x0 + col, extent.y0 + row)
            record = by_id[ref.id]
            tile = Image.open(run_dir / record["path"]).convert("RGB")
            out.paste(tile, (col * TILE_SIZE, row * TILE_SIZE))
    return out


def max_abs_error(a: Image.Image, b: Image.Image) -> int:
    arr_a = np.asarray(a.convert("RGB"), dtype=np.int16)
    arr_b = np.asarray(b.convert("RGB"), dtype=np.int16)
    return int(np.abs(arr_a - arr_b).max())


def draw_grid(
    img: Image.Image,
    extent: SourceExtent,
    color: tuple[int, int, int] = (31, 121, 69),
    width: int = 3,
) -> Image.Image:
    out = img.copy()
    draw = ImageDraw.Draw(out)
    for col in range(1, extent.cols):
        x = col * TILE_SIZE
        draw.line([(x, 0), (x, out.height)], fill=color, width=width)
    for row in range(1, extent.rows):
        y = row * TILE_SIZE
        draw.line([(0, y), (out.width, y)], fill=color, width=width)
    return out


def seam_heatmap(img: Image.Image, metrics: dict[str, Any], threshold: float) -> Image.Image:
    out = img.copy()
    draw = ImageDraw.Draw(out)
    for record in metrics["seams"]:
        ratio = record["seam_to_control_ratio"]
        color = (31, 121, 69) if ratio <= threshold else (196, 44, 38)
        width = 3 if ratio <= threshold else 7
        if record["orientation"] == "vertical":
            x = int(record["x_px"])
            draw.line([(x, 0), (x, out.height)], fill=color, width=width)
        else:
            y = int(record["y_px"])
            draw.line([(0, y), (out.width, y)], fill=color, width=width)
    return out


def label_panel(img: Image.Image, title: str, subtitle: str = "") -> Image.Image:
    font = ImageFont.load_default()
    header_h = 58 if subtitle else 40
    out = Image.new("RGB", (img.width, img.height + header_h), (244, 241, 232))
    draw = ImageDraw.Draw(out)
    draw.text((14, 10), title, fill=(36, 31, 24), font=font)
    if subtitle:
        draw.text((14, 31), subtitle, fill=(93, 84, 70), font=font)
    out.paste(img, (0, header_h))
    return out


def scaled_panel(img: Image.Image, width: int, title: str, subtitle: str = "") -> Image.Image:
    scale = width / img.width
    scaled = img.resize((width, max(1, int(img.height * scale))), Image.Resampling.LANCZOS)
    return label_panel(scaled, title, subtitle)


def make_contact_sheet(
    run_dir: Path,
    source: Image.Image,
    styled: Image.Image,
    semantic: Image.Image,
    heatmap: Image.Image,
    metrics: dict[str, Any],
) -> Path:
    panel_w = 520
    panels = [
        scaled_panel(source, panel_w, "A. Raw NLS source mosaic", "10x10 real Roscommon z17 tiles"),
        scaled_panel(
            styled,
            panel_w,
            "B. Continuous county-base supertile",
            "rendered once, then split mechanically",
        ),
        scaled_panel(
            semantic,
            panel_w,
            "C. Deterministic semantic proof layer",
            "coarse classes for seam contracts",
        ),
        scaled_panel(
            heatmap,
            panel_w,
            "D. Runtime seam validation",
            f"max ratio {metrics['max_seam_to_control_ratio']:.2f}",
        ),
    ]
    gap = 24
    width = gap * 3 + panel_w * 2
    height = (
        gap * 3 + max(panels[0].height, panels[1].height) + max(panels[2].height, panels[3].height)
    )
    sheet = Image.new("RGB", (width, height), (244, 241, 232))
    positions = [
        (gap, gap),
        (gap * 2 + panel_w, gap),
        (gap, gap * 2 + max(panels[0].height, panels[1].height)),
        (gap * 2 + panel_w, gap * 2 + max(panels[0].height, panels[1].height)),
    ]
    for panel, pos in zip(panels, positions, strict=True):
        sheet.paste(panel, pos)
    out = run_dir / "county-pipeline-proof-contact-sheet.png"
    sheet.save(out)
    return out


def write_repair_package(
    run_dir: Path, extent: SourceExtent, styled: Image.Image
) -> dict[str, Any]:
    """Write a masked seam-repair template for future imagegen/local repair.

    The county base export does not require repair. This package documents the
    accepted repair shape for any high-value imagegen panel seam: a narrow band,
    explicit mask, seam contract overlay, and fail-until-validated status.
    """

    repair_dir = run_dir / "masked-seam-repair-template"
    repair_dir.mkdir(parents=True, exist_ok=True)
    join_x = (extent.cols // 2) * TILE_SIZE
    band_px = 96
    mask = Image.new("L", styled.size, 0)
    draw = ImageDraw.Draw(mask)
    draw.rectangle((join_x - band_px, 0, join_x + band_px, styled.height), fill=255)
    mask.save(repair_dir / "seam-band-mask.png")

    overlay = styled.copy()
    draw = ImageDraw.Draw(overlay)
    draw.rectangle(
        (join_x - band_px, 0, join_x + band_px, styled.height), outline=(196, 44, 38), width=6
    )
    draw.line([(join_x, 0), (join_x, styled.height)], fill=(196, 44, 38), width=3)
    overlay.save(repair_dir / "seam-contract-overlay.png")

    manifest = {
        "status": "template_pending_imagegen_or_local_repair",
        "purpose": "Bound seam repairs to this mask/contract shape; do not accept broad global repaint without validation.",
        "join_x_px": join_x,
        "band_half_width_px": band_px,
        "mask": "masked-seam-repair-template/seam-band-mask.png",
        "contract_overlay": "masked-seam-repair-template/seam-contract-overlay.png",
        "acceptance": {
            "max_repaired_seam_to_control_ratio": DEFAULT_THRESHOLD,
            "requires_before_after_contact_sheet": True,
            "requires_topology_drift_note": ["roads", "paths", "buildings", "water", "boundaries"],
        },
    }
    (repair_dir / "repair-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def vertical_join_metric(img: Image.Image, join_x: int) -> dict[str, float]:
    lum = luminance(img)
    join_x = max(1, min(img.width - 1, join_x))
    seam = float(np.abs(lum[:, join_x] - lum[:, join_x - 1]).mean())
    controls = []
    for offset in (-96, -64, -32, 32, 64, 96):
        cx = join_x + offset
        if 1 <= cx < img.width:
            controls.append(float(np.abs(lum[:, cx] - lum[:, cx - 1]).mean()))
    control = float(np.mean(controls)) if controls else seam
    return {
        "join_x_px": join_x,
        "join_mean_abs_luma_jump": seam,
        "nearby_control_mean_abs_luma_jump": control,
        "join_to_control_ratio": seam / control if control else 0.0,
    }


def repair_vertical_seam(
    img: Image.Image, join_x: int, band_half_width: int
) -> tuple[Image.Image, Image.Image]:
    """Bounded deterministic seam repair.

    This is deliberately conservative: it harmonizes color/texture inside a
    vertical mask band and adds a tiny blur at the exact join. It does not
    invent, reroute, or align topology. If road/boundary geometry is mismatched,
    validation should still fail in visual review.
    """

    join_x = max(1, min(img.width - 1, join_x))
    band_half_width = max(8, band_half_width)
    x0 = max(0, join_x - band_half_width)
    x1 = min(img.width, join_x + band_half_width)
    arr = np.asarray(img.convert("RGB"), dtype=np.float32).copy()
    left = arr[:, x0:join_x, :]
    right = arr[:, join_x:x1, :]
    if left.size == 0 or right.size == 0:
        raise ValueError("seam band must include pixels on both sides of join")

    left_mean = left.reshape(-1, 3).mean(axis=0)
    right_mean = right.reshape(-1, 3).mean(axis=0)
    target = (left_mean + right_mean) / 2.0
    left_offset = target - left_mean
    right_offset = target - right_mean

    for x in range(x0, x1):
        if x < join_x:
            weight = (x - x0) / max(1, join_x - x0)
            arr[:, x, :] += left_offset * weight
        else:
            weight = (x1 - x) / max(1, x1 - join_x)
            arr[:, x, :] += right_offset * weight

    repaired = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8), mode="RGB")

    # Feather the exact join very narrowly to prevent a one-pixel hard cut.
    narrow = repaired.crop(
        (max(0, join_x - 6), 0, min(repaired.width, join_x + 6), repaired.height)
    )
    narrow_blur = narrow.filter(ImageFilter.GaussianBlur(1.15))
    repaired.paste(narrow_blur, (max(0, join_x - 6), 0))

    mask = Image.new("L", img.size, 0)
    draw = ImageDraw.Draw(mask)
    draw.rectangle((x0, 0, x1, img.height), fill=255)
    return repaired, mask


def repair_seam(args: argparse.Namespace) -> int:
    out_dir = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    original = Image.open(args.input).convert("RGB")
    join_x = args.join_x if args.join_x is not None else original.width // 2
    before = vertical_join_metric(original, join_x)
    repaired, mask = repair_vertical_seam(original, join_x, args.band_half_width)
    after = vertical_join_metric(repaired, join_x)

    original.save(out_dir / "repair-before.png")
    repaired.save(out_dir / "repair-after.png")
    mask.save(out_dir / "repair-mask.png")

    overlay = repaired.copy()
    draw = ImageDraw.Draw(overlay)
    draw.rectangle(
        (
            max(0, join_x - args.band_half_width),
            0,
            min(overlay.width, join_x + args.band_half_width),
            overlay.height,
        ),
        outline=(196, 44, 38),
        width=5,
    )
    draw.line([(join_x, 0), (join_x, overlay.height)], fill=(196, 44, 38), width=3)
    overlay.save(out_dir / "repair-after-overlay.png")

    scaled_before = scaled_panel(
        original,
        520,
        "Before masked seam repair",
        f"join ratio {before['join_to_control_ratio']:.2f}",
    )
    scaled_after = scaled_panel(
        overlay, 520, "After masked seam repair", f"join ratio {after['join_to_control_ratio']:.2f}"
    )
    sheet = Image.new(
        "RGB", (520 * 2 + 72, max(scaled_before.height, scaled_after.height) + 48), (244, 241, 232)
    )
    sheet.paste(scaled_before, (24, 24))
    sheet.paste(scaled_after, (520 + 48, 24))
    sheet.save(out_dir / "repair-contact-sheet.png")

    report = {
        "status": "pass_metrics_requires_visual_topology_review"
        if after["join_to_control_ratio"] <= args.threshold
        else "fail_metrics",
        "input": str(args.input),
        "join_x_px": join_x,
        "band_half_width_px": args.band_half_width,
        "threshold": args.threshold,
        "before": before,
        "after": after,
        "artifacts": {
            "before": "repair-before.png",
            "after": "repair-after.png",
            "mask": "repair-mask.png",
            "overlay": "repair-after-overlay.png",
            "contact_sheet": "repair-contact-sheet.png",
        },
        "topology_drift_note": {
            "roads": "Local color/texture harmonization only; road geometry is not rerouted or guaranteed aligned.",
            "paths": "Local color/texture harmonization only; path geometry is not rerouted or guaranteed aligned.",
            "buildings": "No building-specific edits are attempted.",
            "water": "No water-specific edits are attempted.",
            "boundaries": "Boundary linework is preserved as pixels; mismatched independent generations still require visual rejection or imagegen/masked inpaint.",
        },
    }
    (out_dir / "repair-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"].startswith("pass") else 1


def write_run_readme(
    run_dir: Path, manifest: dict[str, Any], validation: dict[str, Any] | None = None
) -> None:
    source = manifest["source"]
    metrics = manifest.get("metrics", {})
    lines = [
        "# Cycle CF Production County Tile Pipeline Proof",
        "",
        "## Purpose",
        "",
        "Production-shaped proof for county-scale overhead map tiles. This run uses",
        "real NLS Roscommon historic tiles, renders one continuous deterministic",
        "county-base supertile, mechanically exports runtime tiles, and validates",
        "tile seam continuity plus lossless reassembly.",
        "",
        "## Source",
        "",
        f"- Center: `{source['center_lat']}, {source['center_lon']}`",
        f"- Zoom: `{source['extent']['z']}`",
        f"- XYZ range: `x={source['extent']['x0']}..{source['extent']['x1']}`, `y={source['extent']['y0']}..{source['extent']['y1']}`",
        f"- Source tile count: `{source['extent']['tile_count']}`",
        f"- URL template: `{source['url_template']}`",
        f"- Attribution: {source['attribution']}",
        "",
        "## Outputs",
        "",
        "- `source-mosaic.png`",
        "- `county-base-supertile.png` (retired from Git; regenerate with the pipeline)",
        "- `semantic-mask.png`",
        "- `seam-contracts.json`",
        "- `runtime-tiles/`",
        "- `runtime-reassembled.png` (retired from Git; regenerate with the pipeline)",
        "- `county-pipeline-proof-contact-sheet.png`",
        "- `masked-seam-repair-template/`",
        "- `masked-seam-repair-proof/` when `repair-seam` has been run on a failed adjacent-panel stitch",
        "",
        "The generated county supertile, reassembly, grid overlay, seam-validation",
        "overlay, and seam-contract overlay are retired from the clean checkout.",
        "They remain generation outputs: the tracked source mosaic, semantic",
        "layer, manifests, metrics, and reproducible pipeline regenerate the proof.",
        "",
        "## Metrics",
        "",
        "```json",
        json.dumps(metrics, indent=2),
        "```",
        "",
        "## Imagegen Policy",
        "",
        "Imagegen is not accepted as an independent county runtime-tile generator.",
        "Cycle CE proved overlapping independent imagegen supertiles can fail at",
        "their safe-center join. This pipeline therefore treats imagegen as optional",
        "for high-value local panels only, and requires a masked seam repair package",
        "plus validation before any repaired seam is accepted.",
        "",
        "`repair-seam` is a bounded local color/texture harmonization tool. It",
        "writes a mask, before/after metrics, a contact sheet, and topology",
        "review notes; it does not reroute or realign roads, paths, buildings,",
        "water, or boundaries.",
    ]
    if validation is not None:
        lines.extend(
            [
                "",
                "## Validation",
                "",
                "```json",
                json.dumps(validation, indent=2),
                "```",
            ]
        )
    (run_dir / "README.md").write_text("\n".join(lines) + "\n")


def run_proof(args: argparse.Namespace) -> int:
    run_dir = args.out_dir
    run_dir.mkdir(parents=True, exist_ok=True)
    extent = centered_extent(args.lat, args.lon, args.zoom, args.cols, args.rows)

    source = assemble_source_mosaic(extent, args.cache_dir, args.url_template)
    source_path = run_dir / "source-mosaic.png"
    source.save(source_path)

    styled = render_county_base(source, extent)
    styled_path = run_dir / "county-base-supertile.png"
    styled.save(styled_path)

    labels, semantic, semantic_meta = semantic_layers(source)
    semantic_path = run_dir / "semantic-mask.png"
    semantic.save(semantic_path)

    contracts = seam_contracts(labels, extent)
    contracts_path = run_dir / "seam-contracts.json"
    contracts_path.write_text(json.dumps(contracts, indent=2) + "\n")

    tile_records = export_runtime_tiles(
        styled, extent, run_dir / "runtime-tiles", "county-base-supertile.png"
    )
    reassembled = reassemble_tiles(tile_records, run_dir, extent)
    reassembled_path = run_dir / "runtime-reassembled.png"
    reassembled.save(reassembled_path)

    metrics = seam_metrics(styled, extent.cols, extent.rows)
    metrics["max_abs_reassembly_error"] = max_abs_error(styled, reassembled)
    metrics["tile_count"] = len(tile_records)
    metrics["tile_size"] = TILE_SIZE
    metrics["threshold"] = args.threshold
    metrics["status"] = (
        "pass"
        if metrics["max_abs_reassembly_error"] == 0
        and metrics["max_seam_to_control_ratio"] <= args.threshold
        and len(tile_records) >= args.min_tile_count
        else "fail"
    )
    (run_dir / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")

    grid_overlay = draw_grid(styled, extent)
    grid_overlay_path = run_dir / "county-base-grid-overlay.png"
    grid_overlay.save(grid_overlay_path)
    heat = seam_heatmap(styled, metrics, args.threshold)
    heat_path = run_dir / "seam-validation-overlay.png"
    heat.save(heat_path)
    contact_path = make_contact_sheet(run_dir, source, grid_overlay, semantic, heat, metrics)

    repair_template = write_repair_package(run_dir, extent, styled)

    manifest = {
        "schema_version": 1,
        "script_version": SCRIPT_VERSION,
        "created_at": utc_now(),
        "preset": args.preset,
        "source": {
            "center_lat": args.lat,
            "center_lon": args.lon,
            "extent": {
                **asdict(extent),
                "x1": extent.x1,
                "y1": extent.y1,
                "tile_count": extent.tile_count,
            },
            "url_template": args.url_template,
            "attribution": NLS_ATTRIBUTION,
            "license_note": NLS_LICENSE_NOTE,
            "cache_dir": str(args.cache_dir),
        },
        "render": {
            "mode": "deterministic_continuous_county_base",
            "parent_artifact": "county-base-supertile.png",
            "style_version": SCRIPT_VERSION,
            "source_sha256": sha256_file(source_path),
            "parent_sha256": sha256_file(styled_path),
        },
        "semantic_layers": {
            "artifact": "semantic-mask.png",
            "metadata": semantic_meta,
            "contracts": "seam-contracts.json",
        },
        "runtime_tiles": {
            "directory": "runtime-tiles",
            "tile_count": len(tile_records),
            "tiles": tile_records,
        },
        "metrics": metrics,
        "human_review": {
            "contact_sheet": contact_path.name,
            "grid_overlay": grid_overlay_path.name,
            "seam_validation_overlay": heat_path.name,
        },
        "imagegen_policy": {
            "county_base_status": "do_not_use_independent_imagegen_tiles",
            "cycle_ce_independent_join_status": "fail_requires_repair",
            "cycle_ce_independent_join_ratio": 2.747887018399838,
            "allowed_use": "high_value_local_panels_or_single_large_supertile_only",
            "repair_template": repair_template,
        },
    }
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

    validation = validate_run_dir(run_dir, write_files=True)
    write_run_readme(run_dir, manifest, validation)
    print(json.dumps(validation, indent=2))
    return 0 if validation["status"] == "pass" else 1


def validate_run_dir(run_dir: Path, write_files: bool = True) -> dict[str, Any]:
    manifest_path = run_dir / "manifest.json"
    metrics_path = run_dir / "metrics.json"
    contracts_path = run_dir / "seam-contracts.json"
    if not manifest_path.exists():
        return {"status": "fail", "errors": [f"missing {manifest_path}"]}
    if not metrics_path.exists():
        return {"status": "fail", "errors": [f"missing {metrics_path}"]}
    if not contracts_path.exists():
        return {"status": "fail", "errors": [f"missing {contracts_path}"]}

    manifest = json.loads(manifest_path.read_text())
    metrics = json.loads(metrics_path.read_text())
    contracts = json.loads(contracts_path.read_text())

    threshold = float(metrics.get("threshold", DEFAULT_THRESHOLD))
    errors: list[str] = []
    warnings: list[str] = []

    tile_count = int(metrics.get("tile_count", 0))
    if tile_count < 100:
        errors.append(f"tile_count {tile_count} < 100")
    if int(metrics.get("max_abs_reassembly_error", -1)) != 0:
        errors.append(
            f"max_abs_reassembly_error is {metrics.get('max_abs_reassembly_error')}, expected 0"
        )
    if float(metrics.get("max_seam_to_control_ratio", 999.0)) > threshold:
        errors.append(
            f"max_seam_to_control_ratio {metrics.get('max_seam_to_control_ratio')} exceeds threshold {threshold}"
        )
    if manifest.get("render", {}).get("mode") != "deterministic_continuous_county_base":
        errors.append("render mode is not deterministic_continuous_county_base")
    if manifest.get("runtime_tiles", {}).get("tile_count") != tile_count:
        errors.append("runtime tile count mismatch between manifest and metrics")
    if not manifest.get("source", {}).get("url_template"):
        errors.append("manifest missing source url_template")
    if not manifest.get("source", {}).get("attribution"):
        errors.append("manifest missing source attribution")
    if not contracts.get("contracts"):
        errors.append("seam contracts are empty")
    if (
        manifest.get("imagegen_policy", {}).get("cycle_ce_independent_join_status")
        != "fail_requires_repair"
    ):
        errors.append("imagegen independent join policy is not marked fail_requires_repair")

    repair_proof_path = run_dir / "masked-seam-repair-proof" / "repair-report.json"
    repair_proof: dict[str, Any] | None = None
    if repair_proof_path.exists():
        repair_report = json.loads(repair_proof_path.read_text())
        repair_status = str(repair_report.get("status", ""))
        if not repair_status.startswith("pass"):
            errors.append(f"masked seam repair proof status is {repair_status}")
        for rel in repair_report.get("artifacts", {}).values():
            artifact_path = repair_proof_path.parent / rel
            if not artifact_path.exists():
                errors.append(f"missing masked seam repair proof artifact {artifact_path}")
        repair_proof = {
            "status": repair_status,
            "report": str(repair_proof_path.relative_to(run_dir)),
            "contact_sheet": "masked-seam-repair-proof/repair-contact-sheet.png",
            "before_join_to_control_ratio": repair_report.get("before", {}).get(
                "join_to_control_ratio"
            ),
            "after_join_to_control_ratio": repair_report.get("after", {}).get(
                "join_to_control_ratio"
            ),
            "topology_review_required": repair_status.startswith("pass_metrics"),
        }

    required_files = [
        "source-mosaic.png",
        "county-base-supertile.png",
        "semantic-mask.png",
        "runtime-reassembled.png",
        "county-pipeline-proof-contact-sheet.png",
        "county-base-grid-overlay.png",
        "seam-validation-overlay.png",
        "masked-seam-repair-template/seam-band-mask.png",
        "masked-seam-repair-template/repair-manifest.json",
    ]
    for rel in required_files:
        if not (run_dir / rel).exists():
            errors.append(f"missing output artifact {rel}")

    status = "pass" if not errors else "fail"
    report = {
        "status": status,
        "run_dir": str(run_dir),
        "tile_count": tile_count,
        "max_abs_reassembly_error": metrics.get("max_abs_reassembly_error"),
        "max_seam_to_control_ratio": metrics.get("max_seam_to_control_ratio"),
        "threshold": threshold,
        "seam_contract_count": len(contracts.get("contracts", [])),
        "imagegen_independent_join_status": manifest.get("imagegen_policy", {}).get(
            "cycle_ce_independent_join_status"
        ),
        "masked_seam_repair_proof": repair_proof,
        "errors": errors,
        "warnings": warnings,
    }
    if write_files:
        (run_dir / "validation-report.json").write_text(json.dumps(report, indent=2) + "\n")
        write_validation_md(run_dir, report)
    return report


def write_validation_md(run_dir: Path, report: dict[str, Any]) -> None:
    lines = [
        "# County Tile Pipeline Validation Report",
        "",
        f"- Status: `{report['status']}`",
        f"- Tile count: `{report['tile_count']}`",
        f"- Max seam ratio: `{report['max_seam_to_control_ratio']}`",
        f"- Threshold: `{report['threshold']}`",
        f"- Reassembly error: `{report['max_abs_reassembly_error']}`",
        f"- Seam contracts: `{report['seam_contract_count']}`",
        f"- Imagegen independent join status: `{report['imagegen_independent_join_status']}`",
        "",
    ]
    if report.get("masked_seam_repair_proof"):
        repair = report["masked_seam_repair_proof"]
        lines.extend(
            [
                "## Masked Seam Repair Proof",
                "",
                f"- Status: `{repair['status']}`",
                f"- Before ratio: `{repair['before_join_to_control_ratio']}`",
                f"- After ratio: `{repair['after_join_to_control_ratio']}`",
                f"- Contact sheet: `{repair['contact_sheet']}`",
                f"- Topology review required: `{repair['topology_review_required']}`",
                "",
            ]
        )
    if report["errors"]:
        lines.extend(["## Errors", "", *[f"- {error}" for error in report["errors"]], ""])
    if report["warnings"]:
        lines.extend(["## Warnings", "", *[f"- {warning}" for warning in report["warnings"]], ""])
    (run_dir / "validation-report.md").write_text("\n".join(lines))


def validate_command(args: argparse.Namespace) -> int:
    report = validate_run_dir(args.run_dir, write_files=True)
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "pass" else 1


def prepare_repair(args: argparse.Namespace) -> int:
    img = Image.open(args.input).convert("RGB")
    run_dir = args.out_dir
    run_dir.mkdir(parents=True, exist_ok=True)
    extent = SourceExtent(
        z=args.zoom,
        x0=0,
        y0=0,
        cols=max(1, img.width // TILE_SIZE),
        rows=max(1, img.height // TILE_SIZE),
    )
    manifest = write_repair_package(run_dir, extent, img)
    print(json.dumps(manifest, indent=2))
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Build and validate production-shaped county-scale overhead map tiles: "
            "fetch/assemble source mosaics, render continuous supertiles, export runtime tiles, "
            "write semantic seam contracts, validate seams, and prepare masked repair packages."
        )
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    proof = sub.add_parser("run-proof", help="run the Murphy/Roscommon production proof pipeline")
    proof.add_argument(
        "--preset", default="murphy-production-proof", choices=["murphy-production-proof"]
    )
    proof.add_argument("--out-dir", type=Path, required=True)
    proof.add_argument(
        "--cache-dir", type=Path, default=Path("/private/tmp/rundale-nls-tile-cache")
    )
    proof.add_argument("--url-template", default=NLS_ROSCOMMON_URL)
    proof.add_argument("--lat", type=float, default=53.63579941155877)
    proof.add_argument("--lon", type=float, default=-8.079662971357214)
    proof.add_argument("--zoom", type=int, default=17)
    proof.add_argument("--cols", type=int, default=10)
    proof.add_argument("--rows", type=int, default=10)
    proof.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD)
    proof.add_argument("--min-tile-count", type=int, default=100)
    proof.set_defaults(func=run_proof)

    validate = sub.add_parser(
        "validate", help="validate an existing county tile pipeline run directory"
    )
    validate.add_argument("--run-dir", type=Path, required=True)
    validate.set_defaults(func=validate_command)

    repair = sub.add_parser(
        "prepare-repair", help="write a masked seam repair template for a stitched panel"
    )
    repair.add_argument("--input", type=Path, required=True)
    repair.add_argument("--out-dir", type=Path, required=True)
    repair.add_argument("--zoom", type=int, default=17)
    repair.set_defaults(func=prepare_repair)

    repair_run = sub.add_parser(
        "repair-seam", help="run a bounded deterministic vertical seam repair"
    )
    repair_run.add_argument("--input", type=Path, required=True)
    repair_run.add_argument("--out-dir", type=Path, required=True)
    repair_run.add_argument("--join-x", type=int, default=None)
    repair_run.add_argument("--band-half-width", type=int, default=96)
    repair_run.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD)
    repair_run.set_defaults(func=repair_seam)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    result = args.func(args)
    return int(result) if isinstance(result, int) else 0


if __name__ == "__main__":
    raise SystemExit(main())
