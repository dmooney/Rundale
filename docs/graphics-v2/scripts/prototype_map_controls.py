#!/usr/bin/env python3
"""Prototype historic-map control images for background-plate experiments.

This is intentionally dependency-free so it can run in a stock agent session.
It is not a production feature extractor. The goal is to make rough artifacts
for prompt/pipeline research:

- an ink mask,
- a coarse semantic mask,
- a north-up oblique raw-map warp,
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
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(rows), 6)) + chunk(b"IEND", b"")
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


def blend_px(img: bytearray, width: int, height: int, x: int, y: int, color: RGBA, alpha: float) -> None:
    if 0 <= x < width and 0 <= y < height:
        i = (y * width + x) * 4
        inv = 1.0 - alpha
        img[i] = int(img[i] * inv + color[0] * alpha)
        img[i + 1] = int(img[i + 1] * inv + color[1] * alpha)
        img[i + 2] = int(img[i + 2] * inv + color[2] * alpha)
        img[i + 3] = 255


def draw_line(img: bytearray, width: int, height: int, x0: int, y0: int, x1: int, y1: int, color: RGBA, thick: int = 1) -> None:
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


def fill_rect(img: bytearray, width: int, height: int, x0: int, y0: int, x1: int, y1: int, color: RGBA) -> None:
    x0, x1 = sorted((max(0, x0), min(width - 1, x1)))
    y0, y1 = sorted((max(0, y0), min(height - 1, y1)))
    for y in range(y0, y1 + 1):
        off = (y * width + x0) * 4
        for _x in range(x0, x1 + 1):
            img[off : off + 4] = bytes(color)
            off += 4


def draw_poly_outline(img: bytearray, width: int, height: int, pts: list[tuple[int, int]], color: RGBA, thick: int = 1) -> None:
    for a, b in zip(pts, pts[1:] + pts[:1]):
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


def connected_components(mask: bytearray, width: int, height: int, max_components: int = 20000) -> list[Component]:
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


def classify_small_symbols(components: list[Component], buildings: list[Component]) -> list[Component]:
    building_ids = {id(c) for c in buildings}
    out: list[Component] = []
    for c in components:
        if id(c) in building_ids:
            continue
        aspect = c.width / max(1, c.height)
        if 3 <= c.width <= 34 and 3 <= c.height <= 34 and 5 <= c.area <= 450 and 0.12 <= aspect <= 4.5:
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


def make_ink_mask(width: int, height: int, rgba: bytearray) -> tuple[bytearray, bytearray, bytearray]:
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


def make_semantic(width: int, height: int, light: bytearray, buildings: list[Component], symbols: list[Component]) -> bytearray:
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


def make_blockout(src_w: int, src_h: int, light: bytearray, buildings: list[Component], symbols: list[Component], dst_w: int, dst_h: int) -> bytearray:
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
        fill_rect(out, dst_w, dst_h, x0 - 2, y0 - height_px - 7, x1 + 2, y0 - height_px + 4, (113, 95, 68, 255))
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


def make_linework_control(src_w: int, src_h: int, light: bytearray, symbols: list[Component], dst_w: int, dst_h: int) -> bytearray:
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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--prefix", default="map")
    parser.add_argument("--width", type=int, default=1600)
    parser.add_argument("--height", type=int, default=900)
    args = parser.parse_args()

    src_w, src_h, rgba = read_png(args.input)
    light, dark, ink = make_ink_mask(src_w, src_h, rgba)
    components = connected_components(dark, src_w, src_h)
    buildings = classify_buildings(components)
    symbols = classify_small_symbols(components, buildings)

    args.out_dir.mkdir(parents=True, exist_ok=True)
    write_png(args.out_dir / f"{args.prefix}-ink-mask.png", src_w, src_h, ink)
    write_png(args.out_dir / f"{args.prefix}-semantic-mask.png", src_w, src_h, make_semantic(src_w, src_h, light, buildings, symbols))
    write_png(args.out_dir / f"{args.prefix}-oblique-raw-warp.png", args.width, args.height, make_oblique_raw(src_w, src_h, rgba, args.width, args.height))
    write_png(args.out_dir / f"{args.prefix}-oblique-ink-warp.png", args.width, args.height, make_oblique_raw(src_w, src_h, ink, args.width, args.height))
    write_png(args.out_dir / f"{args.prefix}-linework-control.png", args.width, args.height, make_linework_control(src_w, src_h, light, symbols, args.width, args.height))
    write_png(args.out_dir / f"{args.prefix}-extruded-blockout.png", args.width, args.height, make_blockout(src_w, src_h, light, buildings, symbols, args.width, args.height))

    report = [
        "# Prototype Map Control Report",
        "",
        f"Input: `{args.input}`",
        f"Input size: {src_w}x{src_h}",
        f"Connected dark components: {len(components)}",
        f"Building-like components: {len(buildings)}",
        f"Small symbol-like components: {len(symbols)}",
        "",
        "These counts are heuristic, not authoritative. The point of this pass is",
        "to produce control images for clean-context image-generation experiments",
        "without hand-authored per-location interpretation.",
        "",
        "Outputs:",
        f"- `{args.prefix}-ink-mask.png`",
        f"- `{args.prefix}-semantic-mask.png`",
        f"- `{args.prefix}-oblique-raw-warp.png`",
        f"- `{args.prefix}-oblique-ink-warp.png`",
        f"- `{args.prefix}-linework-control.png`",
        f"- `{args.prefix}-extruded-blockout.png`",
    ]
    (args.out_dir / f"{args.prefix}-control-report.md").write_text("\n".join(report) + "\n")
    print(os.linesep.join(report))


if __name__ == "__main__":
    main()
