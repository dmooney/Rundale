#!/usr/bin/env python3
"""Generate 18 seasonal x lighting variants of a location master plate via
the OpenAI Responses API (gpt-5.5 + image_generation tool), each an edit off
the locked master so composition stays identical. Parallelised, with retries.

Salvaged from the ad-hoc /tmp/gen_variants.py used for the first Kilteevan
batch (see docs/plans/parish-diorama-art-handover.md). The variant
instruction text is LOCKED — proven across 18 frames; do not reword casually.

Usage:
    OPENAI_API_KEY=... python3 gen_variants.py <master.png> <out-dir>

Art output belongs in the iCloud art dir (or a gitignored scratch), never a
git-tracked path:
    ~/Library/Mobile Documents/com~apple~CloudDocs/Rundale/diorama-art/<slug>/
"""

import base64
import concurrent.futures
import json
import os
import sys
import time
import urllib.error
import urllib.request

if len(sys.argv) != 3:
    sys.exit("usage: gen_variants.py <master.png> <out-dir>")

KEY = os.environ["OPENAI_API_KEY"]
MASTER = sys.argv[1]
OUT = sys.argv[2]
os.makedirs(OUT, exist_ok=True)

master_b64 = base64.b64encode(open(MASTER, "rb").read()).decode()
master_url = "data:image/png;base64," + master_b64

SEASONS = {
    "1-winter": "deep winter: snow blanketing the ground, rooftops and walls, bare leafless trees, icy stream edges, cold pale palette",
    "2-early-spring": "early spring: patchy melting snow and thawing mud, bare trees with the first green buds, sparse new shoots",
    "3-late-spring": "late spring: fresh lush green grass, blossoming trees, scattered wildflowers, vibrant and verdant",
    "4-summer": "high summer: full deep green foliage, lush hedgerows, warm dry ground",
    "5-early-fall": "early autumn: foliage turning gold and amber, a few fallen leaves, mellow warm tones",
    "6-late-fall": "late autumn: brown and russet foliage, many bare branches, fallen leaves, damp cold ground, muted",
}
LIGHTS = {
    "a-day-sunny": "a bright clear SUNNY day, warm directional sunlight, soft crisp shadows",
    "b-day-overcast": "an OVERCAST grey day, flat soft diffuse light, muted and desaturated",
    "c-night-moonlit": "NIGHT under moonlight, dark cool blue tones, soft moonlight and deep shadows; all windows DARK and unlit",
}


def gen(skey, lkey):
    name = f"{skey}__{lkey}"
    out = f"{OUT}/{name}.png"
    if os.path.exists(out) and os.path.getsize(out) > 100000:
        return name, "skip(exists)"
    instr = (
        "This is a fixed isometric pixel-art village background plate. Keep the EXACT same "
        "composition, camera angle, buildings, thatch roofs, dry-stone walls, well, stone "
        "footbridge, river course and overall layout UNCHANGED. Change ONLY the season and "
        f"lighting to: SEASON = {SEASONS[skey]}; LIGHTING = {LIGHTS[lkey]}. "
        "Keep all windows dark and unlit. Do NOT add people, animals, crops, signs, carts or "
        "any new objects. Keep the central lane open and empty. Same crisp hand-pixelled "
        "isometric style. No UI, no text."
    )
    body = json.dumps(
        {
            "model": "gpt-5.5",
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": instr},
                        {"type": "input_image", "image_url": master_url},
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
                headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
            )
            with urllib.request.urlopen(req, timeout=300) as r:
                d = json.load(r)
            for o in d.get("output", []):
                if o.get("type") == "image_generation_call" and o.get("result"):
                    open(out, "wb").write(base64.b64decode(o["result"]))
                    return name, f"ok({os.path.getsize(out)})"
            return name, "no-image"
        except urllib.error.HTTPError as e:
            msg = e.read().decode()[:160]
            if e.code in (429, 500, 503) and attempt < 2:
                time.sleep(8 * (attempt + 1))
                continue
            return name, f"HTTP{e.code}:{msg}"
        except Exception as e:
            if attempt < 2:
                time.sleep(8)
                continue
            return name, f"ERR:{e}"
    return name, "fail"


combos = [(s, l) for s in SEASONS for l in LIGHTS]
print(f"generating {len(combos)} variants, 4 at a time...", flush=True)
done = 0
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as ex:
    futs = {ex.submit(gen, s, l): (s, l) for s, l in combos}
    for f in concurrent.futures.as_completed(futs):
        name, status = f.result()
        done += 1
        print(f"[{done}/{len(combos)}] {name}: {status}", flush=True)
print("DONE", flush=True)
