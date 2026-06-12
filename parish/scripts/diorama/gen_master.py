#!/usr/bin/env python3
"""Structure-guided master plate generation for the Parish diorama (M5).

Fixes the river/layout coherence blocker (see
docs/plans/parish-diorama-art-handover.md): the image model on its own renders
the river as disconnected segments. We hand it a programmatic layout schematic
as a control image so it paints within a coherent plan it didn't invent.

Two subcommands:

  schematic <out.png>
      Render the Kilteevan Village layout schematic (1536x1024 PNG): ONE
      continuous river polyline (right edge -> left edge, crossing only the
      south lane), bridge marker at that single crossing, 4 cottages, central
      well, N/S/E/W lanes, dry-stone walls, empty tilled plots.
      Needs Pillow:  uv run --with pillow gen_master.py schematic /tmp/s.png

  generate <schematic.png> <style_ref.png> <out.png>
      Call the OpenAI Responses API (gpt-5.5 + image_generation, 1536x1024
      high) with the schematic as control image and the previous master as
      style reference. Stdlib only. OPENAI_API_KEY required.
      Write output to the iCloud art dir (never a git-tracked path):
      ~/Library/Mobile Documents/com~apple~CloudDocs/Rundale/diorama-art/<slug>/
"""

import base64
import json
import os
import sys
import time
import urllib.error
import urllib.request

W, H = 1536, 1024


def pt(xpct, ypct):
    return (xpct / 100 * W, ypct / 100 * H)


# Kilteevan Village layout (screen-space %, matches the locked master
# composition): 4 cottages one per quadrant, well left-of-center, lane cross,
# river along the lower third crossed by the south lane at the bridge.
RIVER = [(100, 66), (88, 70), (74, 74), (63, 77), (52, 81), (38, 85), (22, 88), (8, 90), (0, 91)]
BRIDGE_AT = (57.5, 78.5)  # on the river, where the south lane crosses
LANES = [
    [(50, 47), (44, 25), (40, 0)],  # north lane (to the crossroads)
    [(50, 47), (25, 46), (0, 45)],  # west lane (track toward the mill)
    [(50, 47), (75, 44), (100, 42)],  # east lane
    [(50, 47), (54, 62), (57.5, 78.5), (60, 88), (62, 100)],  # south lane over the bridge
]
COTTAGES = [  # (cx%, cy%, w%, h%)
    (19, 23, 13, 11),  # NW
    (68, 19, 14, 11),  # NE
    (12, 58, 12, 11),  # SW
    (78, 59, 12, 10),  # SE
]
# Well sits inside the open common (as in the locked master), not on a lane:
# the plaza ellipse below is drawn over the lane lines, so the well ring lands
# on open brown ground.
WELL = (47, 35)
PLOTS = [(31, 28, 8, 7), (17, 71, 9, 7)]  # empty tilled soil
WALLS = [
    [(8, 33), (30, 31)],  # below NW cottage
    [(18, 11), (36, 13)],  # top field, west of the north lane
    [(60, 30), (78, 29)],  # below NE cottage
    [(82, 47), (98, 50)],  # E field, below the east lane
    [(4, 50), (22, 51)],  # W field, between west lane and SW cottage
    [(28, 62), (42, 66)],  # SW field
    [(62, 67), (78, 70)],  # SE field, clear of river and cottage
]


def draw_schematic(out_path):
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (W, H), "#dcecc4")  # light green fields
    d = ImageDraw.Draw(img)

    # lanes + central plaza (brown). The plaza is the broad open common of
    # the master plate — drawn AFTER the lanes so lane bands don't cut
    # through it; lanes visually stop at its edge.
    for lane in LANES:
        d.line([pt(*p) for p in lane], fill="#a07444", width=46)
    cx, cy = pt(48, 44)
    d.ellipse([cx - 245, cy - 154, cx + 245, cy + 154], fill="#a07444")

    # the ONE river (blue), drawn over the lanes so the crossing is visible
    d.line([pt(*p) for p in RIVER], fill="#2e6fd0", width=30)

    # bridge marker (grey) at the single crossing
    bx, by = pt(*BRIDGE_AT)
    d.rectangle([bx - 38, by - 26, bx + 38, by + 26], fill="#7a7a7a", outline="#3c3c3c", width=4)

    # cottages (red)
    for cxp, cyp, wp, hp in COTTAGES:
        x0, y0 = pt(cxp - wp / 2, cyp - hp / 2)
        x1, y1 = pt(cxp + wp / 2, cyp + hp / 2)
        d.rectangle([x0, y0, x1, y1], fill="#c0392b", outline="#7a1f14", width=4)

    # well (black ring)
    wx, wy = pt(*WELL)
    d.ellipse([wx - 26, wy - 26, wx + 26, wy + 26], outline="#111111", width=10)

    # dry-stone walls (dark grey thin lines)
    for wall in WALLS:
        d.line([pt(*p) for p in wall], fill="#4a4a4a", width=8)

    # empty tilled plots (dark brown, hatched)
    for cxp, cyp, wp, hp in PLOTS:
        x0, y0 = pt(cxp - wp / 2, cyp - hp / 2)
        x1, y1 = pt(cxp + wp / 2, cyp + hp / 2)
        d.rectangle([x0, y0, x1, y1], fill="#6b4a2b", outline="#43301c", width=3)
        x = x0 + 10
        while x < x1:
            d.line([(x, y0 + 4), (x, y1 - 4)], fill="#43301c", width=2)
            x += 14

    img.save(out_path)
    print(f"schematic -> {out_path}")


INSTRUCTION = (
    "Create a 1536x1024 isometric pixel-art village background plate of a small "
    "Irish village, 1820: whitewashed thatched cottages around a stone well, a "
    "shallow stream and an old stone footbridge.\n\n"
    "The FIRST image is a LAYOUT SCHEMATIC you must follow exactly. Legend: "
    "BLUE line = the river. It is ONE single continuous stream flowing from the "
    "right edge to the left edge of the frame, exactly along the blue line. It "
    "must never break, fork, or appear anywhere else in the image — no other "
    "water. GREY block = the ONLY stone footbridge, carrying the south lane "
    "over the river at exactly that point. BROWN = the dirt lanes and the "
    "central open space. RED rectangles = the four cottages. BLACK circle = "
    "the stone well. DARK THIN lines = low dry-stone field walls. DARK HATCHED "
    "rectangles = empty tilled-soil garden plots (bare soil, nothing planted).\n\n"
    "The SECOND image is a STYLE REFERENCE: reproduce its exact art style — "
    "crisp hand-pixelled isometric rendering, palette, grass/stone/thatch "
    "texture, whitewashed walls, summer foliage, bright sunny daylight — but "
    "with the corrected layout from the schematic.\n\n"
    "Hard rules: NO people, NO animals, NO crops, NO signs, NO carts, NO new "
    "objects. All windows dark and unlit. Poor 1820 Irish cottages: most have "
    "NO chimney (faint smoke seeps through the thatch ridge), at most one "
    "simple chimney in the whole scene. Keep the lanes open and empty. "
    "No UI, no text, no labels, no schematic colors leaking through — fully "
    "painted scenery."
)


def b64url(path):
    return "data:image/png;base64," + base64.b64encode(open(path, "rb").read()).decode()


def generate(schematic, style_ref, out_path):
    key = os.environ["OPENAI_API_KEY"]
    body = json.dumps(
        {
            "model": "gpt-5.5",
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": INSTRUCTION},
                        {"type": "input_image", "image_url": b64url(schematic)},
                        {"type": "input_image", "image_url": b64url(style_ref)},
                    ],
                }
            ],
            "tools": [{"type": "image_generation", "size": "1536x1024", "quality": "high"}],
        }
    ).encode()
    for attempt in range(3):
        try:
            req = urllib.request.Request(
                "https://api.openai.com/v1/responses",
                data=body,
                headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
            )
            with urllib.request.urlopen(req, timeout=600) as r:
                d = json.load(r)
            for o in d.get("output", []):
                if o.get("type") == "image_generation_call" and o.get("result"):
                    open(out_path, "wb").write(base64.b64decode(o["result"]))
                    print(f"master -> {out_path} ({os.path.getsize(out_path)} bytes)")
                    return
            sys.exit(f"no image in response: {json.dumps(d)[:400]}")
        except urllib.error.HTTPError as e:
            msg = e.read().decode()[:300]
            if e.code in (429, 500, 503) and attempt < 2:
                time.sleep(10 * (attempt + 1))
                continue
            sys.exit(f"HTTP {e.code}: {msg}")


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "schematic":
        draw_schematic(sys.argv[2])
    elif len(sys.argv) == 5 and sys.argv[1] == "generate":
        generate(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
