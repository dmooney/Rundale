#!/usr/bin/env python3
"""Prototype historic-map control images for background-plate experiments.

This is intentionally dependency-free so it can run in a stock agent session.
It is not a production feature extractor. The goal is to make rough artifacts
for prompt/pipeline research:

- an ink mask,
- a coarse semantic mask,
- a north-up oblique raw-map warp,
- a literal paint-by-numbers control that preserves crop geometry,
- a boundary-material control that demotes linework and highlights soft planting,
- a rough pale-corridor road/topology cue,
- a rough extruded blockout scaffold.

No per-location hints are baked into the script. It only reads pixels from the
input map crop.
"""

from __future__ import annotations

import argparse
import os
import struct
import zlib
from collections import deque
from dataclasses import dataclass
from pathlib import Path

RGBA = tuple[int, int, int, int]


def paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa = abs(p - a)
    pb = abs(p - b)
    pc = abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def read_png(path: Path) -> tuple[int, int, bytearray]:
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")

    pos = 8
    width = height = color_type = bit_depth = None
    payload = bytearray()
    while pos < len(data):
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        chunk_type = data[pos + 4 : pos + 8]
        chunk_data = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if chunk_type == b"IHDR":
            width, height, bit_depth, color_type, _comp, _filter, _interlace = struct.unpack(
                ">IIBBBBB", chunk_data
            )
        elif chunk_type == b"IDAT":
            payload.extend(chunk_data)
        elif chunk_type == b"IEND":
            break

    if width is None or height is None or bit_depth != 8 or color_type not in (2, 6):
        raise ValueError("Only 8-bit RGB/RGBA PNGs are supported")

    channels = 4 if color_type == 6 else 3
    stride = width * channels
    raw = zlib.decompress(bytes(payload))
    out = bytearray(width * height * 4)
    prev = bytearray(stride)
    src_pos = 0
    dst_pos = 0
    for _y in range(height):
        filter_type = raw[src_pos]
        src_pos += 1
        row = bytearray(raw[src_pos : src_pos + stride])
        src_pos += stride
        for i in range(stride):
            left = row[i - channels] if i >= channels else 0
            up = prev[i]
            up_left = prev[i - channels] if i >= channels else 0
            if filter_type == 1:
                row[i] = (row[i] + left) & 255
            elif filter_type == 2:
                row[i] = (row[i] + up) & 255
            elif filter_type == 3:
                row[i] = (row[i] + ((left + up) >> 1)) & 255
            elif filter_type == 4:
                row[i] = (row[i] + paeth(left, up, up_left)) & 255
            elif filter_type != 0:
                raise ValueError(f"Unsupported PNG filter type {filter_type}")
        if channels == 4:
            out[dst_pos : dst_pos + width * 4] = row
            dst_pos += width * 4
        else:
            for x in range(width):
                r, g, b = row[x * 3 : x * 3 + 3]
                out[dst_pos : dst_pos + 4] = bytes((r, g, b, 255))
                dst_pos += 4
        prev = row
    return width, height, out


def write_png(path: Path, width: int, height: int, rgba: bytearray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    rows = bytearray()
    stride = width * 4
    for y in range(height):
        rows.append(0)
        rows.extend(rgba[y * stride : (y + 1) * stride])

    def chunk(kind: bytes, body: bytes) -> bytes:
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(rows), 6))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


def gray_at(rgba: bytearray, idx: int) -> int:
    r, g, b = rgba[idx], rgba[idx + 1], rgba[idx + 2]
    return (299 * r + 587 * g + 114 * b) // 1000


def blank(width: int, height: int, color: RGBA) -> bytearray:
    return bytearray(color * width * height)


def put_px(img: bytearray, width: int, height: int, x: int, y: int, color: RGBA) -> None:
    if 0 <= x < width and 0 <= y < height:
        i = (y * width + x) * 4
        img[i : i + 4] = bytes(color)


def blend_px(
    img: bytearray, width: int, height: int, x: int, y: int, color: RGBA, alpha: float
) -> None:
    if 0 <= x < width and 0 <= y < height:
        i = (y * width + x) * 4
        inv = 1.0 - alpha
        img[i] = int(img[i] * inv + color[0] * alpha)
        img[i + 1] = int(img[i + 1] * inv + color[1] * alpha)
        img[i + 2] = int(img[i + 2] * inv + color[2] * alpha)
        img[i + 3] = 255


def draw_line(
    img: bytearray,
    width: int,
    height: int,
    x0: int,
    y0: int,
    x1: int,
    y1: int,
    color: RGBA,
    thick: int = 1,
) -> None:
    dx = abs(x1 - x0)
    dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx + dy
    while True:
        for yy in range(y0 - thick + 1, y0 + thick):
            for xx in range(x0 - thick + 1, x0 + thick):
                put_px(img, width, height, xx, yy, color)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy
            x0 += sx
        if e2 <= dx:
            err += dx
            y0 += sy


def fill_rect(
    img: bytearray, width: int, height: int, x0: int, y0: int, x1: int, y1: int, color: RGBA
) -> None:
    x0, x1 = sorted((max(0, x0), min(width - 1, x1)))
    y0, y1 = sorted((max(0, y0), min(height - 1, y1)))
    for y in range(y0, y1 + 1):
        off = (y * width + x0) * 4
        for _x in range(x0, x1 + 1):
            img[off : off + 4] = bytes(color)
            off += 4


def draw_poly_outline(
    img: bytearray, width: int, height: int, pts: list[tuple[int, int]], color: RGBA, thick: int = 1
) -> None:
    for a, b in zip(pts, pts[1:] + pts[:1], strict=True):
        draw_line(img, width, height, a[0], a[1], b[0], b[1], color, thick)


@dataclass
class Component:
    x0: int
    y0: int
    x1: int
    y1: int
    area: int

    @property
    def width(self) -> int:
        return self.x1 - self.x0 + 1

    @property
    def height(self) -> int:
        return self.y1 - self.y0 + 1

    @property
    def density(self) -> float:
        return self.area / max(1, self.width * self.height)


def connected_components(
    mask: bytearray, width: int, height: int, max_components: int = 20000
) -> list[Component]:
    seen = bytearray(width * height)
    comps: list[Component] = []
    neighbors = ((1, 0), (-1, 0), (0, 1), (0, -1))
    for start in range(width * height):
        if not mask[start] or seen[start]:
            continue
        sx = start % width
        sy = start // width
        q: deque[tuple[int, int]] = deque([(sx, sy)])
        seen[start] = 1
        x0 = x1 = sx
        y0 = y1 = sy
        area = 0
        while q:
            x, y = q.popleft()
            area += 1
            x0 = min(x0, x)
            y0 = min(y0, y)
            x1 = max(x1, x)
            y1 = max(y1, y)
            for dx, dy in neighbors:
                nx, ny = x + dx, y + dy
                if 0 <= nx < width and 0 <= ny < height:
                    ni = ny * width + nx
                    if mask[ni] and not seen[ni]:
                        seen[ni] = 1
                        q.append((nx, ny))
        if area >= 3:
            comps.append(Component(x0, y0, x1, y1, area))
            if len(comps) >= max_components:
                break
    return comps


def distance_to_mask(mask: bytearray, width: int, height: int, max_distance: int) -> bytearray:
    """Approximate city-block distance from each pixel to the nearest mask pixel."""
    dist = bytearray([255]) * (width * height)
    q: deque[tuple[int, int]] = deque()
    for i, value in enumerate(mask):
        if value:
            dist[i] = 0
            q.append((i % width, i // width))

    neighbors = ((1, 0), (-1, 0), (0, 1), (0, -1))
    while q:
        x, y = q.popleft()
        base = dist[y * width + x]
        if base >= max_distance:
            continue
        for dx, dy in neighbors:
            nx, ny = x + dx, y + dy
            if 0 <= nx < width and 0 <= ny < height:
                ni = ny * width + nx
                nd = base + 1
                if nd < dist[ni]:
                    dist[ni] = nd
                    q.append((nx, ny))
    return dist


def local_count(mask: bytearray, width: int, height: int, cx: int, cy: int, radius: int) -> int:
    total = 0
    y0 = max(0, cy - radius)
    y1 = min(height - 1, cy + radius)
    x0 = max(0, cx - radius)
    x1 = min(width - 1, cx + radius)
    for y in range(y0, y1 + 1):
        row = y * width
        for x in range(x0, x1 + 1):
            if mask[row + x]:
                total += 1
    return total


def dilate_mask(mask: bytearray, width: int, height: int, radius: int) -> bytearray:
    out = bytearray(width * height)
    for y in range(height):
        for x in range(width):
            if local_count(mask, width, height, x, y, radius):
                out[y * width + x] = 1
    return out


def erode_mask(
    mask: bytearray, width: int, height: int, radius: int, min_fraction: float = 0.72
) -> bytearray:
    out = bytearray(width * height)
    full = (radius * 2 + 1) * (radius * 2 + 1)
    threshold = max(1, int(full * min_fraction))
    for y in range(height):
        for x in range(width):
            if mask[y * width + x] and local_count(mask, width, height, x, y, radius) >= threshold:
                out[y * width + x] = 1
    return out


def classify_buildings(components: list[Component]) -> list[Component]:
    buildings: list[Component] = []
    for c in components:
        aspect = c.width / max(1, c.height)
        # Filled/hatched map building marks tend to be compact and fairly dense.
        if (
            11 <= c.width <= 110
            and 6 <= c.height <= 85
            and 90 <= c.area <= 2800
            and 0.42 <= c.density <= 0.95
            and 0.20 <= aspect <= 6.0
        ):
            buildings.append(c)
    return buildings


def classify_small_symbols(
    components: list[Component], buildings: list[Component]
) -> list[Component]:
    building_ids = {id(c) for c in buildings}
    out: list[Component] = []
    for c in components:
        if id(c) in building_ids:
            continue
        aspect = c.width / max(1, c.height)
        if (
            3 <= c.width <= 34
            and 3 <= c.height <= 34
            and 5 <= c.area <= 450
            and 0.12 <= aspect <= 4.5
        ):
            out.append(c)
    return out


def ground_transform(src_w: int, src_h: int, dst_w: int, dst_h: int) -> tuple[float, float, float]:
    y_squash = 0.58
    margin = 72
    scale = min((dst_w - margin * 2) / src_w, (dst_h - margin * 2) / (src_h * y_squash))
    off_x = (dst_w - src_w * scale) / 2
    off_y = (dst_h - src_h * scale * y_squash) / 2
    return scale, off_x, off_y


def project(x: float, y: float, scale: float, off_x: float, off_y: float) -> tuple[int, int]:
    return int(round(off_x + x * scale)), int(round(off_y + y * scale * 0.58))


def make_ink_mask(
    width: int, height: int, rgba: bytearray
) -> tuple[bytearray, bytearray, bytearray]:
    light_mask = bytearray(width * height)
    dark_mask = bytearray(width * height)
    ink = blank(width, height, (246, 238, 218, 255))
    for y in range(height):
        for x in range(width):
            pi = y * width + x
            gi = pi * 4
            g = gray_at(rgba, gi)
            if g < 192:
                light_mask[pi] = 1
                v = max(25, min(205, int((g - 45) * 1.35)))
                put_px(ink, width, height, x, y, (v, v, v, 255))
            if g < 132:
                dark_mask[pi] = 1
    return light_mask, dark_mask, ink


def make_semantic(
    width: int, height: int, light: bytearray, buildings: list[Component], symbols: list[Component]
) -> bytearray:
    img = blank(width, height, (245, 237, 219, 255))
    for y in range(height):
        for x in range(width):
            if light[y * width + x]:
                put_px(img, width, height, x, y, (132, 126, 113, 255))

    for c in symbols:
        cx = (c.x0 + c.x1) // 2
        cy = (c.y0 + c.y1) // 2
        for yy in range(cy - 4, cy + 5):
            for xx in range(cx - 4, cx + 5):
                if (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy) <= 18:
                    blend_px(img, width, height, xx, yy, (62, 124, 73, 255), 0.7)

    for c in buildings:
        fill_rect(img, width, height, c.x0, c.y0, c.x1, c.y1, (181, 81, 61, 255))
        draw_poly_outline(
            img,
            width,
            height,
            [(c.x0, c.y0), (c.x1, c.y0), (c.x1, c.y1), (c.x0, c.y1)],
            (80, 38, 32, 255),
            1,
        )
    return img


def make_oblique_raw(src_w: int, src_h: int, rgba: bytearray, dst_w: int, dst_h: int) -> bytearray:
    out = blank(dst_w, dst_h, (238, 229, 207, 255))
    scale, off_x, off_y = ground_transform(src_w, src_h, dst_w, dst_h)
    for y in range(dst_h):
        src_y = int((y - off_y) / (scale * 0.58))
        if not (0 <= src_y < src_h):
            continue
        for x in range(dst_w):
            src_x = int((x - off_x) / scale)
            if 0 <= src_x < src_w:
                si = (src_y * src_w + src_x) * 4
                oi = (y * dst_w + x) * 4
                out[oi : oi + 4] = rgba[si : si + 4]
    return out


def make_blockout(
    src_w: int,
    src_h: int,
    light: bytearray,
    buildings: list[Component],
    symbols: list[Component],
    dst_w: int,
    dst_h: int,
) -> bytearray:
    out = blank(dst_w, dst_h, (225, 218, 194, 255))
    scale, off_x, off_y = ground_transform(src_w, src_h, dst_w, dst_h)

    # Boundary/linework dots, sampled to keep the control image legible.
    for y in range(0, src_h, 2):
        for x in range(0, src_w, 2):
            if light[y * src_w + x]:
                px, py = project(x, y, scale, off_x, off_y)
                blend_px(out, dst_w, dst_h, px, py, (94, 94, 86, 255), 0.75)
                blend_px(out, dst_w, dst_h, px + 1, py, (94, 94, 86, 255), 0.45)

    # Tree-ish small symbols as simple canopies.
    for c in symbols[:600]:
        cx = (c.x0 + c.x1) / 2
        cy = (c.y0 + c.y1) / 2
        px, py = project(cx, cy, scale, off_x, off_y)
        radius = max(3, min(10, int(max(c.width, c.height) * scale * 0.22)))
        for yy in range(py - radius, py + radius + 1):
            for xx in range(px - radius, px + radius + 1):
                if (xx - px) * (xx - px) + (yy - py) * (yy - py) <= radius * radius:
                    blend_px(out, dst_w, dst_h, xx, yy, (71, 119, 65, 255), 0.8)
        draw_line(out, dst_w, dst_h, px, py, px, py + radius + 5, (87, 64, 46, 255), 1)

    # Buildings as extruded boxes from detected map components.
    for c in buildings:
        x0, y0 = project(c.x0, c.y0, scale, off_x, off_y)
        x1, y1 = project(c.x1, c.y1, scale, off_x, off_y)
        if x1 < x0:
            x0, x1 = x1, x0
        if y1 < y0:
            y0, y1 = y1, y0
        height_px = max(12, min(46, int(max(c.width, c.height) * scale * 0.42)))
        # Wall body.
        fill_rect(out, dst_w, dst_h, x0, y0 - height_px, x1, y1, (211, 199, 171, 255))
        # Roof slab.
        fill_rect(
            out,
            dst_w,
            dst_h,
            x0 - 2,
            y0 - height_px - 7,
            x1 + 2,
            y0 - height_px + 4,
            (113, 95, 68, 255),
        )
        # Facade linework.
        draw_poly_outline(
            out,
            dst_w,
            dst_h,
            [(x0, y0 - height_px), (x1, y0 - height_px), (x1, y1), (x0, y1)],
            (69, 58, 47, 255),
            1,
        )
        if x1 - x0 > 16 and y1 - y0 > 5:
            door_x = (x0 + x1) // 2
            fill_rect(out, dst_w, dst_h, door_x - 3, y1 - 12, door_x + 3, y1, (74, 54, 39, 255))

    return out


def make_linework_control(
    src_w: int, src_h: int, light: bytearray, symbols: list[Component], dst_w: int, dst_h: int
) -> bytearray:
    out = blank(dst_w, dst_h, (225, 218, 194, 255))
    scale, off_x, off_y = ground_transform(src_w, src_h, dst_w, dst_h)

    for y in range(0, src_h, 2):
        for x in range(0, src_w, 2):
            if light[y * src_w + x]:
                px, py = project(x, y, scale, off_x, off_y)
                blend_px(out, dst_w, dst_h, px, py, (84, 84, 78, 255), 0.8)

    for c in symbols[:800]:
        cx = (c.x0 + c.x1) / 2
        cy = (c.y0 + c.y1) / 2
        px, py = project(cx, cy, scale, off_x, off_y)
        radius = max(2, min(7, int(max(c.width, c.height) * scale * 0.18)))
        for yy in range(py - radius, py + radius + 1):
            for xx in range(px - radius, px + radius + 1):
                if (xx - px) * (xx - px) + (yy - py) * (yy - py) <= radius * radius:
                    blend_px(out, dst_w, dst_h, xx, yy, (67, 118, 65, 255), 0.8)

    return out


def make_road_topology_control(
    src_w: int,
    src_h: int,
    rgba: bytearray,
    light: bytearray,
    dark: bytearray,
    symbols: list[Component],
) -> bytearray:
    """Build a generic soft road cue from broad pale corridors near source ink."""
    out = blank(src_w, src_h, (225, 218, 194, 255))
    dist = distance_to_mask(dark, src_w, src_h, 18)
    candidate = bytearray(src_w * src_h)

    for y in range(src_h):
        for x in range(src_w):
            pi = y * src_w + x
            gi = pi * 4
            g = gray_at(rgba, gi)
            # Pale road interiors on these OS crops usually sit well inside a
            # light corridor, with little ink in the immediate center but some
            # ink in the surrounding annulus from road edges, yard lines, or
            # nearby mapped features. Open fields are often farther from ink;
            # label/letter counters have too much ink immediately nearby.
            near_dark = local_count(dark, src_w, src_h, x, y, 4)
            surrounding_dark = local_count(dark, src_w, src_h, x, y, 18)
            if (
                g >= 190
                and 5 <= dist[pi] <= 28
                and near_dark <= 4
                and 10 <= surrounding_dark <= 120
            ):
                candidate[pi] = 1

    comps = connected_components(candidate, src_w, src_h)
    road_like = [
        c for c in comps if c.area >= 90 and (c.width >= 28 or c.height >= 28) and c.density >= 0.08
    ]

    # Faint source linework remains visible for orientation, but the tan bands
    # are the only positive road/topology cue.
    for y in range(src_h):
        for x in range(src_w):
            if light[y * src_w + x]:
                blend_px(out, src_w, src_h, x, y, (98, 92, 82, 255), 0.38)

    for c in road_like:
        for y in range(c.y0, c.y1 + 1):
            for x in range(c.x0, c.x1 + 1):
                if candidate[y * src_w + x]:
                    for yy in range(y - 2, y + 3):
                        for xx in range(x - 2, x + 3):
                            blend_px(out, src_w, src_h, xx, yy, (198, 158, 88, 255), 0.62)

    for c in symbols[:800]:
        cx = (c.x0 + c.x1) // 2
        cy = (c.y0 + c.y1) // 2
        for yy in range(cy - 3, cy + 4):
            for xx in range(cx - 3, cx + 4):
                if (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy) <= 10:
                    blend_px(out, src_w, src_h, xx, yy, (71, 119, 65, 255), 0.72)

    return out


def make_diff_mask(
    width: int, height: int, current: bytearray, original: bytearray | None
) -> bytearray:
    diff = bytearray(width * height)
    if original is None:
        return diff
    if len(original) != len(current):
        raise ValueError("Original and cleaned images must have the same dimensions")
    for i in range(width * height):
        pi = i * 4
        delta = (
            abs(current[pi] - original[pi])
            + abs(current[pi + 1] - original[pi + 1])
            + abs(current[pi + 2] - original[pi + 2])
        )
        if delta >= 58:
            diff[i] = 1
    return diff


def make_literal_paint_control(
    width: int,
    height: int,
    rgba: bytearray,
    light: bytearray,
    dark: bytearray,
    symbols: list[Component],
    diff: bytearray,
) -> bytearray:
    """Create a flat, literal control without freehand beautification.

    This control deliberately avoids inventing confident wall/road/building
    classes. It keeps the cleaned crop's linework as muted evidence, paints
    only conservative vegetation/road hints, and marks cleaned/suppressed areas
    as neutral no-data zones.
    """
    out = blank(width, height, (181, 184, 119, 255))
    dist = distance_to_mask(dark, width, height, 18)
    road_candidate = bytearray(width * height)

    for y in range(height):
        for x in range(width):
            i = y * width + x
            pi = i * 4
            g = gray_at(rgba, pi)

            # Muted open-field base with source paper/scan variation retained.
            field = (
                max(145, min(212, 170 + (g - 180) // 6)),
                max(148, min(214, 174 + (g - 180) // 7)),
                max(92, min(152, 111 + (g - 180) // 10)),
                255,
            )
            put_px(out, width, height, x, y, field)

            # Very conservative pale-corridor cue. It is intentionally faint;
            # the raw map remains the authority for roads in the prompt.
            near_dark = local_count(dark, width, height, x, y, 4)
            surrounding_dark = local_count(dark, width, height, x, y, 16)
            if g >= 200 and 5 <= dist[i] <= 22 and near_dark <= 3 and 12 <= surrounding_dark <= 95:
                road_candidate[i] = 1

    # Roads/yards as very soft tan hints, not hard route extraction. Keep this
    # deliberately weaker than source linework so failed road inference cannot
    # dominate the prompt.
    for y in range(height):
        for x in range(width):
            if not road_candidate[y * width + x]:
                continue
            blend_px(out, width, height, x, y, (203, 157, 91, 255), 0.16)

    # Preserve all source linework as muted evidence so exact crop geometry
    # survives without promoting every line to a physical wall.
    for y in range(height):
        for x in range(width):
            i = y * width + x
            pi = i * 4
            g = gray_at(rgba, pi)
            if light[i]:
                ink_alpha = 0.28 if g >= 150 else 0.46
                blend_px(out, width, height, x, y, (67, 61, 50, 255), ink_alpha)

    # Tree/scrub symbols as muted green points. This does not erase the source
    # symbol beneath; it only gives imagegen a class cue for vegetation.
    for c in symbols[:900]:
        cx = (c.x0 + c.x1) // 2
        cy = (c.y0 + c.y1) // 2
        radius = max(2, min(5, max(c.width, c.height) // 3))
        for yy in range(cy - radius, cy + radius + 1):
            for xx in range(cx - radius, cx + radius + 1):
                if (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy) <= radius * radius:
                    blend_px(out, width, height, xx, yy, (45, 112, 61, 255), 0.64)

    # Mark cleaned/suppressed areas as muted no-data, not terrain. Keep it low
    # contrast so the mark reads as a veto/absence cue, not as a pale road.
    for y in range(height):
        for x in range(width):
            if not diff[y * width + x]:
                continue
            for yy in range(y - 1, y + 2):
                for xx in range(x - 1, x + 2):
                    blend_px(out, width, height, xx, yy, (161, 163, 139, 255), 0.42)

    return out


def make_boundary_material_control(
    width: int,
    height: int,
    rgba: bytearray,
    light: bytearray,
    dark: bytearray,
    symbols: list[Component],
    diff: bytearray,
) -> tuple[bytearray, int]:
    """Create a material-first control that makes wall authority rare.

    This artifact is aimed at the BA/BB failure mode: dense garden/internal
    linework became physical walls. It deliberately demotes ordinary source
    linework, paints dense planting texture as a soft zone, and keeps roads as
    weak walkable hints. It does not try to infer confident wall geometry.
    """
    out = blank(width, height, (173, 184, 119, 255))
    dist = distance_to_mask(dark, width, height, 20)
    road_candidate = bytearray(width * height)
    planting_seed = bytearray(width * height)

    for y in range(height):
        for x in range(width):
            i = y * width + x
            pi = i * 4
            g = gray_at(rgba, pi)
            field = (
                max(138, min(208, 166 + (g - 180) // 7)),
                max(145, min(214, 176 + (g - 180) // 8)),
                max(87, min(152, 111 + (g - 180) // 11)),
                255,
            )
            put_px(out, width, height, x, y, field)

            near_dark = local_count(dark, width, height, x, y, 4)
            surrounding_dark = local_count(dark, width, height, x, y, 16)
            if g >= 200 and 5 <= dist[i] <= 22 and near_dark <= 3 and 12 <= surrounding_dark <= 95:
                road_candidate[i] = 1

            # Dense small-scale ink in these crops often means garden/orchard,
            # hatching, scrub, or vegetation symbols. Treat it as soft planting
            # material, not as wall geometry. Very dense compact marks are left
            # to the raw map rather than promoted here as buildings.
            dark_r6 = local_count(dark, width, height, x, y, 6)
            dark_r12 = local_count(dark, width, height, x, y, 12)
            light_r10 = local_count(light, width, height, x, y, 10)
            if 5 <= dark_r6 <= 58 and 12 <= dark_r12 <= 210 and 35 <= light_r10 <= 260:
                planting_seed[i] = 1

    for c in symbols[:1000]:
        cx = (c.x0 + c.x1) // 2
        cy = (c.y0 + c.y1) // 2
        radius = max(2, min(7, max(c.width, c.height) // 2))
        for yy in range(cy - radius, cy + radius + 1):
            for xx in range(cx - radius, cx + radius + 1):
                if (
                    0 <= xx < width
                    and 0 <= yy < height
                    and (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy) <= radius * radius
                ):
                    planting_seed[yy * width + xx] = 1

    planting = dilate_mask(planting_seed, width, height, 3)

    # Walkable road/yard hints stay pale and soft.
    for y in range(height):
        for x in range(width):
            if road_candidate[y * width + x]:
                for yy in range(y - 1, y + 2):
                    for xx in range(x - 1, x + 2):
                        blend_px(out, width, height, xx, yy, (203, 158, 93, 255), 0.24)

    # Soft planting zones. This is the key channel: the final prompt should read
    # these as planted texture/hedge/scrub, never as hard wall outlines.
    for y in range(height):
        for x in range(width):
            if planting[y * width + x]:
                blend_px(out, width, height, x, y, (111, 144, 72, 255), 0.42)
                if dark[y * width + x]:
                    blend_px(out, width, height, x, y, (91, 112, 54, 255), 0.32)

    # Original linework is faint evidence only. Use a low alpha by design so
    # imagegen does not copy every boundary as a physical edge.
    for y in range(height):
        for x in range(width):
            i = y * width + x
            if light[i]:
                blend_px(out, width, height, x, y, (74, 67, 55, 255), 0.18 if planting[i] else 0.25)

    # Suppressed/admin/no-data marks: visible enough to veto, too dull to become
    # roads or walls.
    for y in range(height):
        for x in range(width):
            if diff[y * width + x]:
                for yy in range(y - 1, y + 2):
                    for xx in range(x - 1, x + 2):
                        blend_px(out, width, height, xx, yy, (144, 151, 132, 255), 0.50)

    return out, sum(1 for v in planting if v)


def make_soft_planting_control(
    width: int,
    height: int,
    rgba: bytearray,
    light: bytearray,
    dark: bytearray,
    symbols: list[Component],
    diff: bytearray,
) -> tuple[bytearray, int, int, int]:
    """Create a control that removes crisp wall-like planting perimeters.

    BC showed that coloring garden texture green was not enough if the same
    artifact still exposed dark enclosing outlines. This pass is intentionally
    more destructive inside likely planting/garden/scrub zones: it paints a soft
    material wash, feathers the edge, and suppresses source linework along the
    perimeter so the final model is not handed a ready-made wall outline.
    """
    out = blank(width, height, (176, 186, 124, 255))
    dist = distance_to_mask(dark, width, height, 20)
    road_candidate = bytearray(width * height)
    planting_seed = bytearray(width * height)

    for y in range(height):
        for x in range(width):
            i = y * width + x
            pi = i * 4
            g = gray_at(rgba, pi)
            field = (
                max(140, min(210, 169 + (g - 180) // 8)),
                max(148, min(216, 179 + (g - 180) // 9)),
                max(92, min(154, 116 + (g - 180) // 12)),
                255,
            )
            put_px(out, width, height, x, y, field)

            near_dark = local_count(dark, width, height, x, y, 4)
            surrounding_dark = local_count(dark, width, height, x, y, 16)
            if g >= 202 and 5 <= dist[i] <= 22 and near_dark <= 3 and 12 <= surrounding_dark <= 92:
                road_candidate[i] = 1

            dark_r6 = local_count(dark, width, height, x, y, 6)
            dark_r12 = local_count(dark, width, height, x, y, 12)
            light_r10 = local_count(light, width, height, x, y, 10)
            if 6 <= dark_r6 <= 54 and 16 <= dark_r12 <= 190 and 42 <= light_r10 <= 240:
                planting_seed[i] = 1

    # Small symbols are only weak planting seeds here. Avoid promoting every
    # mark to a strong tree, because that made BC too regular.
    for c in symbols[:700]:
        cx = (c.x0 + c.x1) // 2
        cy = (c.y0 + c.y1) // 2
        radius = max(1, min(5, max(c.width, c.height) // 3))
        for yy in range(cy - radius, cy + radius + 1):
            for xx in range(cx - radius, cx + radius + 1):
                if (
                    0 <= xx < width
                    and 0 <= yy < height
                    and (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy) <= radius * radius
                ):
                    planting_seed[yy * width + xx] = 1

    planting = dilate_mask(planting_seed, width, height, 4)
    planting_core = erode_mask(planting, width, height, 2, 0.56)
    planting_edge = bytearray(width * height)
    for i in range(width * height):
        if planting[i] and not planting_core[i]:
            planting_edge[i] = 1

    # Feathered planting: stronger in the core, lighter on the edge. The edge is
    # deliberately low-contrast so it cannot be copied as a wall outline.
    for y in range(height):
        for x in range(width):
            i = y * width + x
            if planting_core[i]:
                blend_px(out, width, height, x, y, (106, 142, 74, 255), 0.45)
                if dark[i]:
                    blend_px(out, width, height, x, y, (92, 115, 58, 255), 0.12)
            elif planting_edge[i]:
                blend_px(out, width, height, x, y, (125, 151, 86, 255), 0.22)

    # Road/yard hints remain extremely weak and are suppressed inside planting.
    # This control is about material, not route extraction.
    for y in range(height):
        for x in range(width):
            i = y * width + x
            if road_candidate[i] and not planting[i]:
                for yy in range(y - 1, y + 2):
                    for xx in range(x - 1, x + 2):
                        blend_px(out, width, height, xx, yy, (188, 169, 121, 255), 0.08)

    # Preserve source evidence outside planting. Inside soft planting, suppress
    # crisp outlines; only a whisper of texture remains in the core.
    for y in range(height):
        for x in range(width):
            i = y * width + x
            if not light[i]:
                continue
            if planting_edge[i]:
                alpha = 0.035
            elif planting_core[i]:
                alpha = 0.07
            else:
                alpha = 0.24
            blend_px(out, width, height, x, y, (74, 67, 55, 255), alpha)

    # Suppressed/admin/no-data marks stay a veto cue, not a physical feature.
    for y in range(height):
        for x in range(width):
            if diff[y * width + x]:
                for yy in range(y - 1, y + 2):
                    for xx in range(x - 1, x + 2):
                        blend_px(out, width, height, xx, yy, (146, 153, 134, 255), 0.44)

    return (
        out,
        sum(1 for v in planting if v),
        sum(1 for v in planting_core if v),
        sum(1 for v in planting_edge if v),
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--original", type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--prefix", default="map")
    parser.add_argument("--width", type=int, default=1600)
    parser.add_argument("--height", type=int, default=900)
    args = parser.parse_args()

    src_w, src_h, rgba = read_png(args.input)
    original_rgba = None
    if args.original:
        original_w, original_h, original_rgba = read_png(args.original)
        if (original_w, original_h) != (src_w, src_h):
            raise ValueError("--original must have the same dimensions as --input")
    light, dark, ink = make_ink_mask(src_w, src_h, rgba)
    components = connected_components(dark, src_w, src_h)
    buildings = classify_buildings(components)
    symbols = classify_small_symbols(components, buildings)

    args.out_dir.mkdir(parents=True, exist_ok=True)
    diff = make_diff_mask(src_w, src_h, rgba, original_rgba)
    literal_paint = make_literal_paint_control(src_w, src_h, rgba, light, dark, symbols, diff)
    boundary_material, planting_pixels = make_boundary_material_control(
        src_w, src_h, rgba, light, dark, symbols, diff
    )
    soft_planting, soft_planting_pixels, soft_planting_core_pixels, soft_planting_edge_pixels = (
        make_soft_planting_control(src_w, src_h, rgba, light, dark, symbols, diff)
    )
    road_topology = make_road_topology_control(src_w, src_h, rgba, light, dark, symbols)
    write_png(args.out_dir / f"{args.prefix}-ink-mask.png", src_w, src_h, ink)
    write_png(
        args.out_dir / f"{args.prefix}-semantic-mask.png",
        src_w,
        src_h,
        make_semantic(src_w, src_h, light, buildings, symbols),
    )
    write_png(
        args.out_dir / f"{args.prefix}-literal-paint-control.png", src_w, src_h, literal_paint
    )
    write_png(
        args.out_dir / f"{args.prefix}-literal-paint-oblique.png",
        args.width,
        args.height,
        make_oblique_raw(src_w, src_h, literal_paint, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-boundary-material-control.png",
        src_w,
        src_h,
        boundary_material,
    )
    write_png(
        args.out_dir / f"{args.prefix}-boundary-material-oblique.png",
        args.width,
        args.height,
        make_oblique_raw(src_w, src_h, boundary_material, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-soft-planting-control.png", src_w, src_h, soft_planting
    )
    write_png(
        args.out_dir / f"{args.prefix}-soft-planting-oblique.png",
        args.width,
        args.height,
        make_oblique_raw(src_w, src_h, soft_planting, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-oblique-raw-warp.png",
        args.width,
        args.height,
        make_oblique_raw(src_w, src_h, rgba, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-oblique-ink-warp.png",
        args.width,
        args.height,
        make_oblique_raw(src_w, src_h, ink, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-linework-control.png",
        args.width,
        args.height,
        make_linework_control(src_w, src_h, light, symbols, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-road-topology-control.png", src_w, src_h, road_topology
    )
    write_png(
        args.out_dir / f"{args.prefix}-road-topology-oblique.png",
        args.width,
        args.height,
        make_oblique_raw(src_w, src_h, road_topology, args.width, args.height),
    )
    write_png(
        args.out_dir / f"{args.prefix}-extruded-blockout.png",
        args.width,
        args.height,
        make_blockout(src_w, src_h, light, buildings, symbols, args.width, args.height),
    )

    report = [
        "# Prototype Map Control Report",
        "",
        f"Input: `{args.input}`",
        f"Original comparison: `{args.original}`" if args.original else "Original comparison: none",
        f"Input size: {src_w}x{src_h}",
        f"Connected dark components: {len(components)}",
        f"Building-like components: {len(buildings)}",
        f"Small symbol-like components: {len(symbols)}",
        f"Suppressed/no-data comparison pixels: {sum(1 for v in diff if v)}",
        f"Soft planting/material pixels: {planting_pixels}",
        f"Soft planting suppressed-control pixels: {soft_planting_pixels}",
        f"Soft planting suppressed-control core pixels: {soft_planting_core_pixels}",
        f"Soft planting suppressed-control edge pixels: {soft_planting_edge_pixels}",
        "",
        "These counts are heuristic, not authoritative. The point of this pass is",
        "to produce control images for clean-context image-generation experiments",
        "without hand-authored per-location interpretation.",
        "",
        "Outputs:",
        f"- `{args.prefix}-ink-mask.png`",
        f"- `{args.prefix}-semantic-mask.png`",
        f"- `{args.prefix}-literal-paint-control.png`",
        f"- `{args.prefix}-literal-paint-oblique.png`",
        f"- `{args.prefix}-boundary-material-control.png`",
        f"- `{args.prefix}-boundary-material-oblique.png`",
        f"- `{args.prefix}-soft-planting-control.png`",
        f"- `{args.prefix}-soft-planting-oblique.png`",
        f"- `{args.prefix}-oblique-raw-warp.png`",
        f"- `{args.prefix}-oblique-ink-warp.png`",
        f"- `{args.prefix}-linework-control.png`",
        f"- `{args.prefix}-road-topology-control.png`",
        f"- `{args.prefix}-road-topology-oblique.png`",
        f"- `{args.prefix}-extruded-blockout.png`",
    ]
    (args.out_dir / f"{args.prefix}-control-report.md").write_text("\n".join(report) + "\n")
    print(os.linesep.join(report))


if __name__ == "__main__":
    main()
