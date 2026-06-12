#!/usr/bin/env python3
"""Structure-guided master plate generation for the Parish diorama (M5).

The image model breaks spatial continuity when it has to invent the layout
(rivers render as disconnected segments, bridges sit illogically). We hand it
a programmatic flat-colour block-in as a control image, narrate the same
geometry in words, and demand continuity of every linear feature — then gate
each render through a vision check before a human ever looks at it.

Subcommands:

  schematic <out.png>
      Render the Kilteevan Village layout block-in (1536x1024 PNG).
      Needs Pillow:  uv run --with pillow gen_master.py schematic /tmp/s.png

  narrate
      Print the layout narration generated from the same constants the
      schematic is drawn from (it is embedded in the render prompt).

  generate <schematic.png> <style_ref.png> <out.png>
      One render via the OpenAI Responses API (gpt-5.5 + image_generation,
      1536x1024 high): schematic as control image + old master as style
      reference. Stdlib only. OPENAI_API_KEY required. Write output to the
      iCloud art dir, never a git-tracked path.

  check <candidate.png>
      Vision gate: sends a downscaled full frame + a full-res bridge crop to
      gpt-5.5 and asks for a JSON verdict on river continuity, bank
      alignment at the bridge, and lanes reaching their frame edges.
      Needs Pillow (crops):  uv run --with pillow gen_master.py check c.png
      Exit code 0 = pass, 1 = fail (machine-gateable).

Exit-direction policy (Kilteevan): world.json lat/lon bearings CONTRADICT the
player-facing path_descriptions (crossroads "road north" sits at bearing 295,
holy well "path south" at 342, mill "track west" at 25 — geo pins follow the
historical OS map, the descriptions are hand-written narrative). The
NARRATIVE wins for art, because it is what players read; bearings only fill
gaps where the description names no direction (weaver, bearing 56 + indoor →
the NE cottage is the weaver's, its door is the in-scene exit). Connections
described only as "an old lane between settlements" (targets 3, 9, 14, 4) get
no painted lane of their own — their hotspots will share the nearest painted
exit, keeping painted-lane count at 5 while graph degree is 10.
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


# ---------------------------------------------------------------------------
# Kilteevan Village layout (screen-space %, derived from the locked master's
# composition + world.json connections). The narration below is generated
# from these same constants — edit them and both schematic and prompt follow.
# ---------------------------------------------------------------------------

# ONE river: enters the RIGHT edge about two-thirds down, flows west-southwest,
# runs straight through the bridge point, exits the LEFT edge three-quarters
# down. The long straight segment (72,72)->(44,79) passes through the bridge
# so the watercourse is locally straight at the crossing (a bend next to the
# bridge makes the model misalign the banks).
RIVER = [(100, 64), (88, 68), (72, 72), (44, 79), (28, 77), (14, 75), (0, 73)]
BRIDGE_AT = (57, 75.7)  # on the straight river segment, where the south road crosses

# Painted exits. The south road crosses the bridge PERPENDICULAR to the river
# (locally vertical road over a locally horizontal river), then forks beyond
# it: the road proper bends southeast to the bottom edge, the mossy holy-well
# path peels off southwest — two bottom-edge exits, ONE river crossing.
LANES = {
    "north-road": [(47, 32), (44, 14), (42, 0)],
    "forge-lane": [(62, 42), (82, 40), (100, 38)],
    "mill-track": [(33, 48), (20, 57), (8, 62), (0, 64)],
    "south-road": [(53, 58), (57, 68), (57, 84), (64, 92), (68, 100)],
    "holywell-path": [(57, 84), (50, 90), (44, 100)],  # forks off the south road
}
LANE_WIDTH = {"holywell-path": 24}  # mossy path is narrower; default 46
PLAZA = (48, 44, 245, 154)  # cx%, cy%, rx px, ry px — the open common
WELL = (47, 35)  # inside the common, off every lane band
COTTAGES = [  # (cx%, cy%, w%, h%, tag)
    (19, 23, 13, 11, "NW"),
    (68, 19, 14, 11, "NE (the weaver's — door faces the common)"),
    (12, 49, 12, 11, "SW"),
    (78, 57, 12, 10, "SE"),
]
PLOTS = [(31, 25.5, 8, 7), (24, 64, 9, 7)]  # empty tilled soil
# Walls exist out of necessity (field clearance + demarcation), so every
# polyline is ANCHORED at both ends — to a lane band, a cottage corner, the
# river bank, or out of the frame. No isolated fragments: an unanchored wall
# stub is exactly the kind of randomness the model then multiplies.
WALLS = [
    [(41, 10), (20, 12), (0, 14)],  # field wall: north road -> left frame edge
    [(25.5, 18.5), (37, 20), (37, 31), (25.5, 29)],  # NW yard bracket: cottage->plot->cottage
    [(75, 14), (86, 16), (86, 27), (75, 24.5)],  # NE yard bracket (the weaver's paddock)
    [(89, 38), (91, 18), (92, 0)],  # field wall: forge lane -> top frame edge
    [(100, 44), (90, 50), (87, 55), (87, 58.8)],  # field wall: right frame edge -> T-junction
    [(18, 52), (26, 50), (33, 48)],  # yard wall: SW cottage -> mill track head
    [(59, 64), (66, 63), (72, 61.5)],  # field wall: south road -> SE cottage
    [(84, 59.5), (92, 58.5), (100, 57.5)],  # field wall: SE cottage -> right frame edge
    # Corner fields below the river, both wall ends running OFF the frame —
    # walls that stop short of the scene edge read as random fragments;
    # off-frame continuation reads as a field system bigger than the view.
    [(0, 84), (10, 88), (18, 100)],  # left frame -> bottom frame, below the river
    [(100, 78), (90, 84), (84, 100)],  # right frame -> bottom frame, below the river
]


def draw_schematic(out_path):
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (W, H), "#dcecc4")  # light green fields
    d = ImageDraw.Draw(img)

    # lanes, then the plaza over them so lane bands stop at the common's edge
    for name, lane in LANES.items():
        d.line([pt(*p) for p in lane], fill="#a07444", width=LANE_WIDTH.get(name, 46))
    cx, cy = pt(PLAZA[0], PLAZA[1])
    d.ellipse([cx - PLAZA[2], cy - PLAZA[3], cx + PLAZA[2], cy + PLAZA[3]], fill="#a07444")

    # the ONE river, over the lanes so the single crossing is visible
    d.line([pt(*p) for p in RIVER], fill="#2e6fd0", width=30)

    # bridge marker at the single crossing
    bx, by = pt(*BRIDGE_AT)
    d.rectangle([bx - 30, by - 34, bx + 30, by + 34], fill="#7a7a7a", outline="#3c3c3c", width=4)

    for cxp, cyp, wp, hp, _tag in COTTAGES:
        x0, y0 = pt(cxp - wp / 2, cyp - hp / 2)
        x1, y1 = pt(cxp + wp / 2, cyp + hp / 2)
        d.rectangle([x0, y0, x1, y1], fill="#c0392b", outline="#7a1f14", width=4)

    wx, wy = pt(*WELL)
    d.ellipse([wx - 26, wy - 26, wx + 26, wy + 26], outline="#111111", width=10)

    for wall in WALLS:
        d.line([pt(*p) for p in wall], fill="#4a4a4a", width=8)

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


def narration():
    """Layout narrated in words, from the same constants the schematic uses.

    Text constraints bind the model harder than spatial structure in images;
    saying the geometry twice (image + words) is the point.
    """
    return (
        "THE LAYOUT, in words: a village common of open bare earth fills the "
        "center, with the stone well standing in it, north-west of the middle "
        "of the common. Four whitewashed thatched cottages surround the "
        "common, one per corner of the scene: north-west, north-east (this is "
        "the weaver's cottage, its door facing the common), south-west and "
        "south-east. The single river enters the frame at the RIGHT edge "
        "about two-thirds of the way down, flows west-southwest in a gentle "
        "line, passes UNDER the stone footbridge, and exits at the LEFT edge "
        "about three-quarters of the way down. It is the only water in the "
        "scene. Five lanes leave the common: a road leaves the TOP edge "
        "slightly left of center (the north road); a lane leaves the RIGHT "
        "edge above the river (toward the forge); a cart track leaves the "
        "LEFT edge, running beside and above the river on its north bank "
        "(toward the mill); and the south road runs from the common straight "
        "south, crosses the river at the stone footbridge at a right angle, "
        "and beyond the bridge FORKS: the road itself bends southeast to the "
        "BOTTOM edge, while a narrower mossy footpath peels off southwest to "
        "the BOTTOM edge. The bridge is the ONLY crossing, and the river "
        "flows THROUGH its arch — the arch opening faces the water, the road "
        "runs over the bridge deck. EXACTLY five lanes touch the frame "
        "edges: one at the top, one at the right, one at the left, two at "
        "the bottom — there are NO other lanes or paths anywhere in the "
        "scene. Low dry-stone field walls divide the land: every wall runs "
        "from anchor to anchor — a lane, a cottage, the river bank, or it "
        "runs OFF the edge of the frame. A wall near the scene edge always "
        "continues out of frame; no wall ever just stops in open grass. "
        "Walled yards sit behind the north-west and north-east cottages, "
        "and the two corner fields below the river are walled toward the "
        "bottom frame corners. Two plots of bare tilled soil sit beside the "
        "north-west and south-west cottages."
    )


STYLE_TEXT = (
    "STYLE: crisp hand-pixelled isometric pixel art, fine 32-bit-era detail "
    "(Stardew-Valley-class fidelity at 1536x1024), rich texture work in "
    "grass, stone and thatch; palette of lush summer greens, warm earth "
    "browns, whitewashed limewash cottage walls, golden weathered oat-straw "
    "thatch, grey limestone; bright clear sunny daylight, warm directional "
    "light, soft crisp shadows. No outlines, no UI, painterly pixel clusters."
)


def render_prompt(with_style_ref=True):
    return (
        "The FIRST image is the rough colour block-in of the painting you "
        "must produce — it is not a diagram, it is the actual composition. "
        "Refine it into a finished 1536x1024 isometric pixel-art plate "
        "WITHOUT MOVING ANYTHING: every shape stays exactly where it is. "
        "Block-in colour code: blue = the river, brown = dirt lanes and the "
        "open common, grey block = the stone footbridge, red rectangles = "
        "whitewashed thatched cottages, black circle = the stone well, dark "
        "thin lines = low dry-stone walls, dark hatched rectangles = empty "
        "tilled-soil plots.\n\n"
        + narration()
        + "\n\nCONTINUITY RULES (these matter most): every linear feature — "
        "river, lane, wall — is CONTINUOUS and traceable end to end. The "
        "river never begins or ends inside the frame, never splits, and is "
        "never interrupted; where it passes under the bridge, the watercourse "
        "and both banks line up perfectly on the two sides of the arch, same "
        "width, same direction. Every lane connects the common to its frame "
        "edge without gaps. Walls run unbroken between their endpoints and "
        "never block a lane.\n\n"
        + (
            (
                "The SECOND image is a STYLE REFERENCE (either a sheet of "
                "isolated texture samples on dark borders, or a finished "
                "plate). Take ONLY the art style from it — palette, pixel "
                "technique, grass/stone/thatch texture; take NO layout from "
                "it whatsoever. Fully paint EVERY surface of the output — "
                "grass, soil, water, stone — in that style: no flat block-in "
                "colour may remain anywhere; the flat green fields become "
                "rich textured grass, the flat blue line becomes lively "
                "water. The samples deliberately show NO thatch ridge: "
                "paint every ridge from the MATERIAL NOTES below (a plain "
                "rolled straw cap, nothing on it — no rod, no cable, no "
                "pipe). If the reference walls look like uniform shaped "
                "blocks, follow the MATERIAL NOTES instead. The composition "
                "comes exclusively from the first image.\n\n"
            )
            if with_style_ref
            else STYLE_TEXT + "\n\n"
        )
        + "MATERIAL NOTES (these override the swatches where they differ): "
        "the dry-stone walls are built from irregular limestone fieldstones "
        "of mixed sizes and rounded shapes, cleared from the very fields "
        "they enclose and stacked WITHOUT mortar — uneven courses, a few "
        "bigger boulders low in the wall, occasional daylight gaps between "
        "stones; NOT uniform blocks, NOT brick-like coursing. The thatched "
        "roofs are plain weathered oat-straw thatch; each ridge is finished "
        "as a simple rolled straw cap in the same material, slightly darker "
        "— NOTHING protrudes from or sits on any roof: no pipes, no vents, "
        "no poles, no chimney pots. Most cottages have NO chimney; at most "
        "one simple stone chimney in the whole scene, and NO smoke "
        "anywhere.\n\n"
        "Hard rules: this is an empty stage, 1820 rural Ireland. NO people, "
        "NO animals, NO crops, NO signs, NO carts, NO new objects. All "
        "windows dark and unlit. Lanes open and empty. No UI, no text, no "
        "labels, no block-in colours leaking through — fully painted "
        "scenery."
    )


def make_swatches(master_path, out_path):
    """Build a style swatch sheet from a master plate.

    Every render that took the full old master as style reference leaked its
    (broken-river) COMPOSITION into the output — the style image fights the
    control image. Swatch crops carry palette/texture/technique but
    physically cannot carry layout. Crops are taken from the v1 master's
    known regions (cottage, well, bridge+river bank, grass/wall/tree).
    """
    from PIL import Image

    im = Image.open(master_path)
    w, h = im.size
    boxes = [  # (x0,y0,x1,y1) in %
        # Cottage cropped BELOW the ridge line: walls, door, windows, eave
        # thatch — the ridge is excluded because every reference that showed
        # a ridge re-imported the v1 "rod below the ridge" defect (g-01/03
        # via this tile, g-04/05 via cand-16). What the model can't see, it
        # can't copy; the ridge itself is specified in the MATERIAL NOTES.
        (58, 17, 84, 35),
        (36, 22, 60, 46),  # well on the common
        (42, 62, 72, 92),  # bridge + river bank
        # Grass/wall/tree/plot sample from the SW field — the old top-left
        # corner crop caught the NW cottage's roof (ridge + smoke), which
        # re-opened the defect channel the cottage re-crop had just closed.
        (0, 58, 26, 96),
    ]
    tile_w, tile_h, gap = 760, 504, 8
    sheet = Image.new("RGB", (tile_w * 2 + gap * 3, tile_h * 2 + gap * 3), "#222222")
    for i, (x0, y0, x1, y1) in enumerate(boxes):
        crop = im.crop((int(x0 / 100 * w), int(y0 / 100 * h), int(x1 / 100 * w), int(y1 / 100 * h)))
        crop = crop.resize((tile_w, tile_h))
        px = gap + (i % 2) * (tile_w + gap)
        py = gap + (i // 2) * (tile_h + gap)
        sheet.paste(crop, (px, py))
    sheet.save(out_path)
    print(f"swatches -> {out_path}")


def b64url(path):
    return "data:image/png;base64," + base64.b64encode(open(path, "rb").read()).decode()


def responses_call(body_dict, timeout=600):
    key = os.environ["OPENAI_API_KEY"]
    body = json.dumps(body_dict).encode()
    for attempt in range(3):
        try:
            req = urllib.request.Request(
                "https://api.openai.com/v1/responses",
                data=body,
                headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
            )
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            msg = e.read().decode()[:300]
            if e.code in (429, 500, 503) and attempt < 2:
                time.sleep(10 * (attempt + 1))
                continue
            sys.exit(f"HTTP {e.code}: {msg}")


def google_call(model, parts, gen_config=None, timeout=600):
    """generateContent against the Gemini API (GEMINI_API_KEY / GOOGLE_API_KEY)."""
    key = os.environ.get("GEMINI_API_KEY") or os.environ["GOOGLE_API_KEY"]
    body_dict = {"contents": [{"parts": parts}]}
    if gen_config:
        body_dict["generationConfig"] = gen_config
    body = json.dumps(body_dict).encode()
    url = (
        f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={key}"
    )
    for attempt in range(3):
        try:
            req = urllib.request.Request(
                url, data=body, headers={"Content-Type": "application/json"}
            )
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            msg = e.read().decode()[:300]
            if e.code in (429, 500, 503) and attempt < 2:
                time.sleep(10 * (attempt + 1))
                continue
            sys.exit(f"HTTP {e.code}: {msg}")


def g_image_part(b64_data):
    return {"inline_data": {"mime_type": "image/png", "data": b64_data}}


def file_b64(path):
    return base64.b64encode(open(path, "rb").read()).decode()


def google_parts_text_and_images(d):
    """Flatten a generateContent response into (text, [b64 images])."""
    text, images = "", []
    for cand in d.get("candidates", []):
        for p in cand.get("content", {}).get("parts", []):
            if "text" in p:
                text += p["text"]
            blob = p.get("inlineData") or p.get("inline_data")
            if blob and blob.get("data"):
                images.append(blob["data"])
    return text, images


def generate(schematic, style_ref, out_path, provider="openai"):
    prompt = render_prompt(with_style_ref=style_ref is not None)
    if provider == "google":
        # Nano Banana Pro (gemini-3-pro-image): native image gen/edit with
        # input images. 3:2 @ 2K is the closest shape to 1536x1024.
        parts = [{"text": prompt}, g_image_part(file_b64(schematic))]
        if style_ref:
            parts.append(g_image_part(file_b64(style_ref)))
        d = google_call(
            "gemini-3-pro-image",
            parts,
            {
                "responseModalities": ["IMAGE"],
                "imageConfig": {"aspectRatio": "3:2", "imageSize": "2K"},
            },
        )
        _, images = google_parts_text_and_images(d)
        if images:
            open(out_path, "wb").write(base64.b64decode(images[0]))
            print(f"master -> {out_path} ({os.path.getsize(out_path)} bytes)")
            return
        sys.exit(f"no image in response: {json.dumps(d)[:400]}")
    content = [
        {"type": "input_text", "text": prompt},
        {"type": "input_image", "image_url": b64url(schematic)},
    ]
    if style_ref:
        content.append({"type": "input_image", "image_url": b64url(style_ref)})
    d = responses_call(
        {
            "model": "gpt-5.5",
            "input": [{"role": "user", "content": content}],
            "tools": [{"type": "image_generation", "size": "1536x1024", "quality": "high"}],
        }
    )
    for o in d.get("output", []):
        if o.get("type") == "image_generation_call" and o.get("result"):
            open(out_path, "wb").write(base64.b64decode(o["result"]))
            print(f"master -> {out_path} ({os.path.getsize(out_path)} bytes)")
            return
    sys.exit(f"no image in response: {json.dumps(d)[:400]}")


CHECK_PROMPT = (
    "You are checking a generated isometric pixel-art village plate for "
    "geometric coherence. Image 1 is the full frame (downscaled); image 2 is "
    "a full-resolution crop of the bridge area; images 3 and 4 are full-"
    "resolution strips of the LEFT and RIGHT frame edges — use these two to "
    "verify which lanes actually REACH those edges (a lane fading into grass "
    "before the edge does NOT count). Intended layout: ONE river "
    "entering at the right edge about two-thirds down, flowing continuously "
    "west-southwest, passing under a single stone footbridge that carries a "
    "north-south road at a right angle, exiting at the left edge; the road "
    "forks beyond the bridge into two bottom-edge exits; one lane each "
    "leaves the top, right and left edges — five frame-edge lanes total and "
    "no others. Answer STRICTLY as JSON, no other text: "
    '{"river_continuous": bool,  // one unbroken watercourse, no gaps, no '
    "extra segments\n"
    ' "banks_aligned_at_bridge": bool,  // same width and direction on both '
    "sides of the arch, banks line up\n"
    ' "single_bridge": bool,\n'
    ' "bridge_arch_over_water": bool,  // the river flows THROUGH the arch '
    "opening; the road runs over the deck — not rotated toward the road\n"
    ' "lanes_reach_edges": bool,\n'
    ' "no_extra_lanes": bool,  // exactly 5 lane terminations: 1 top, 1 '
    "right, 1 left, 2 bottom; no extra paths (e.g. toward a corner)\n"
    ' "no_roof_protrusions": bool,  // no pipe/vent/pole/rod/cable-like '
    "objects on or along any thatch ridge (one simple chimney is allowed)\n"
    ' "cottages_in_quadrants": bool,  // exactly four cottages, one per '
    "quadrant (NW, NE, SW, SE); none migrated toward the center or bottom\n"
    ' "walls_anchored": bool,  // every wall connects to a lane, building, '
    "river bank or frame edge; no short isolated fragments\n"
    ' "notes": "short"}'
)

CHECK_KEYS = (
    "river_continuous",
    "banks_aligned_at_bridge",
    "single_bridge",
    "bridge_arch_over_water",
    "lanes_reach_edges",
    "no_extra_lanes",
    "no_roof_protrusions",
    "cottages_in_quadrants",
    "walls_anchored",
)


def edit_image(in_path, out_path, instruction):
    """Targeted retouch via Nano Banana Pro (google only): keep everything,
    change only what the instruction names."""
    parts = [
        {
            "text": "Edit this image. "
            + instruction
            + " Change NOTHING else — every other pixel region keeps its exact "
            "content, style and position. Same resolution and framing."
        },
        g_image_part(file_b64(in_path)),
    ]
    d = google_call(
        "gemini-3-pro-image",
        parts,
        {"responseModalities": ["IMAGE"], "imageConfig": {"aspectRatio": "3:2", "imageSize": "2K"}},
    )
    _, images = google_parts_text_and_images(d)
    if images:
        open(out_path, "wb").write(base64.b64decode(images[0]))
        print(f"edited -> {out_path} ({os.path.getsize(out_path)} bytes)")
        return
    sys.exit(f"no image in response: {json.dumps(d)[:400]}")


def check(candidate, provider="openai"):
    import io

    from PIL import Image

    im = Image.open(candidate)
    w, h = im.size
    full = im.resize((768, 512))
    crop = im.crop((int(w * 0.36), int(h * 0.56), int(w * 0.80), int(h * 0.98)))
    # Full-res edge strips: lane-reach verdicts from the downscaled full
    # frame alone proved unreliable in BOTH directions (gpt-5.5 passed a
    # missing left exit on cand-15; a pixel probe missed real lanes at the
    # border) — the edge strips were what settled it.
    left_strip = im.crop((0, 0, 160, h))
    right_strip = im.crop((w - 160, 0, w, h))

    def to_b64(image):
        buf = io.BytesIO()
        image.save(buf, format="PNG")
        return base64.b64encode(buf.getvalue()).decode()

    shots = [full, crop, left_strip, right_strip]
    if provider == "google":
        d = google_call(
            "gemini-3.5-flash",
            [{"text": CHECK_PROMPT}] + [g_image_part(to_b64(s)) for s in shots],
            timeout=180,
        )
        text, _ = google_parts_text_and_images(d)
    else:
        d = responses_call(
            {
                "model": "gpt-5.5",
                "input": [
                    {
                        "role": "user",
                        "content": [{"type": "input_text", "text": CHECK_PROMPT}]
                        + [
                            {
                                "type": "input_image",
                                "image_url": "data:image/png;base64," + to_b64(s),
                            }
                            for s in shots
                        ],
                    }
                ],
            },
            timeout=180,
        )
        text = ""
        for o in d.get("output", []):
            if o.get("type") == "message":
                for c in o.get("content", []):
                    if c.get("type") == "output_text":
                        text += c["text"]
    try:
        verdict = json.loads(text[text.index("{") : text.rindex("}") + 1])
    except ValueError:
        sys.exit(f"unparseable check response: {text[:300]}")
    ok = all(verdict.get(k) is True for k in CHECK_KEYS)
    print(json.dumps({"candidate": os.path.basename(candidate), "pass": ok, **verdict}))
    sys.exit(0 if ok else 1)


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "schematic":
        draw_schematic(sys.argv[2])
    elif len(sys.argv) == 2 and sys.argv[1] == "narrate":
        print(narration())
    elif len(sys.argv) == 4 and sys.argv[1] == "swatches":
        make_swatches(sys.argv[2], sys.argv[3])
    elif len(sys.argv) in (5, 6) and sys.argv[1] == "generate":
        style = None if sys.argv[3] in ("none", "-") else sys.argv[3]
        provider = sys.argv[5] if len(sys.argv) == 6 else "openai"
        generate(sys.argv[2], style, sys.argv[4], provider)
    elif len(sys.argv) in (3, 4) and sys.argv[1] == "check":
        check(sys.argv[2], sys.argv[3] if len(sys.argv) == 4 else "openai")
    elif len(sys.argv) == 5 and sys.argv[1] == "edit":
        edit_image(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
