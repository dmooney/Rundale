# Diorama Art Pipeline — Handover (M5 art generation)

> Fresh-context handover for the Interactive Parish Diorama art work.
> Parent: [parish-diorama.md](../design/ideas/parish-diorama.md) (RFC #1428),
> [implementation plan](parish-diorama-implementation.md).
> Branch: `claude/recursing-pike-5073a3` · PR
> [#1429](https://github.com/dmooney/Rundale/pull/1429) (M1-M4, not merged).

## TL;DR

- Diorama **M1-M4 (engine code)** landed: scene schema, backend scene-state
  handler, Svelte frontend, `parish-art-tool` crate. Behind a default-off
  `diorama` flag. PR #1429.
- **M5 = art generation, in progress.** The **art style is LOCKED** and the
  generation approach is proven.
- **18 placeholder plates** for Kilteevan Village (location id 15) generated
  (6 seasons × 3 lighting), full-res 1536×1024. **Stored in iCloud Drive, NOT
  in git** (the owner decided art stays out of git history / LFS — too heavy,
  iterated too often). Path:
  `~/Library/Mobile Documents/com~apple~CloudDocs/Rundale/diorama-art/kilteevan-village/`
  (`0-master_summer-day-sunny.png` is the master).
- **OPEN BLOCKER:** the image model cannot keep **river/layout geometry
  coherent** — it renders the single stream as 2-3 disconnected segments and
  places the bridge illogically. Fix it with structural guidance (below) before
  generating the other locations.

## Locked decisions (do not re-litigate)

- **Provider/model: OpenAI Responses API** (`POST /v1/responses`), model
  **`gpt-5.5`**, with the **`image_generation` tool** (size `1536x1024` = "1024
  lines", quality `high`). The underlying renderer is gpt-image-2-class.
- **Why Responses API, not the Images API:** GPT-5.5 _art-directs_ — it writes
  a far better prompt and can reason about the scene. That orchestration is the
  quality gap. Raw `/v1/images/generations` hand-prompting (any model) was
  visibly worse.
- **Rejected models (don't revisit):** `gpt-image-1` (too many AI hallmarks),
  NVIDIA NIM FLUX.1-dev and fal FLUX.1-dev (painterly, not clean pixel art;
  NVIDIA also black-frames on mood words like "gritty"). All removed from the
  art-tool. Google/Imagen is **free-tier-blocked** (`limit: 0` on image models,
  Imagen needs a paid plan).
- **Variant consistency via edit-off-master:** generate ONE master base, then
  every variant is an **edit** of it — pass the master as `input_image` +
  "keep the exact same composition, change ONLY season+lighting". Composition
  stays locked across all variants. Proven across 18 frames.
- **Key:** `OPENAI_API_KEY` in `.env` (the art-tool loads `.env` via dotenvy
  now). User has ~$10 credits; ~$0.20-0.40 per high render; 18 plates ≈ $6.

## Plate spec (per the user)

- **Native res:** 1536×1024 (detailed — NOT the old design's chunky 480×270).
- **Empty stage only** — the plate is a bare background. NO people, animals,
  signposts, carts. NO planted crops (the garden plots are empty tilled soil so
  **farming can be a future gameplay layer**). NO lit windows (interior
  firelight is a separate **toggleable overlay layer**, off when NPCs sleep).
- **Historical accuracy:** 1820 poor Irish cottages mostly had **no chimney**
  (smoke vented through the thatch ridge); show most cottages chimney-less with
  faint roof-smoke, at most one simple chimney. (Also dodges the model's
  chimney-mangling.)
- **Variant matrix = 18 per location:** 6 seasons × 3 lighting.
  - Seasons (weighted to the dynamic transitions): `winter` ×1, `early-spring`,
    `late-spring`, `summer` ×1, `early-fall`, `late-fall` (spring + fall get 2
    each for finer seasonal change).
  - Lighting: `day-sunny`, `day-overcast`, `night-moonlit`.
- **Generation script (ad-hoc, in `/tmp` — port into the repo next session):**
  `/tmp/gen_variants.py` — base64s the master, fans out 18 edits 4-at-a-time
  with retries via the Responses API. The locked instruction text is in there.

## THE OPEN PROBLEM — river/layout coherence

The model can't reason about spatial continuity. In the Kilteevan plates the
single river is split into disconnected sections and the bridge doesn't sit
logically over it. The style is otherwise perfect.

**Recommended fix — structure-guided generation (try first):**

1. Programmatically generate a **logically-consistent layout schematic** of the
   scene — an SVG / block diagram with: ONE continuous river polyline, a bridge
   crossing it at exactly one point, cottage rectangles, dry-stone-wall lines,
   the path/lane network, the well. Rasterize it to PNG.
2. Feed that schematic to the generator as a **control / reference image**
   (`input_image` in the Responses `image_generation` call — the same mechanism
   the variant edits already use) alongside the art prompt: "paint this layout
   as a pixel-art plate, river follows the blue line, bridge where marked".
3. The generator then paints within a coherent plan it didn't have to invent.

**Secondary levers:**

- **Higher reasoning:** `gpt-5.5-pro` (reasoning model, available on the key)
  with higher reasoning effort to _plan_ the layout; or have `gpt-5.5` VISION-
  analyze the current master, identify the river breaks, and emit a corrected
  layout before rendering.
- **Combine:** gpt-5.5-pro reasons → emits the SVG layout → rasterize → control
  image → render. This is likely the strongest pipeline.

**Feasibility:** confirmed — the `image_generation` tool accepts input images
(we use it for variants), so a layout control-image is viable today.

The scene already has a real spatial source of truth: `mods/rundale/world.json`
(location connections) and the diorama `hotspots`/`slots` percentage coords —
the layout schematic can be derived from / aligned to those so the art matches
the clickable hotspots.

## Remaining M5 work (after the river fix)

1. **River/layout fix** (above) — regenerate the Kilteevan master with a
   coherent river, re-run the 18-variant batch.
2. **Firelight overlay** — per-cottage warm window glow as a separate layer,
   toggled by NPC sleep state.
3. **Wire the pipeline into `parish-art-tool`** — today the providers are
   offline request-builders with a **stubbed live `generate()`**; the real
   Responses/gpt-5.5 + edit-off-master flow lives only in `/tmp/gen_variants.py`.
   Add an `openai-responses` path (gpt-5.5 + image_generation, input-image
   variants + control image).
4. **Extend the engine for the variant matrix.** Today the schema
   (`parish/crates/parish-mod/src/scenes.rs`, `SceneDef.variants:
HashMap<String,String>`) + the selector
   (`parish/crates/parish-core/src/ipc/scene.rs::select_variant`) only do
   night-by-hour + reserved weather. Extend to **season × weather × time-of-day**
   so the 18 variants are actually served. Keep the rule-12 shared-handler shape.
5. **Other 7 MVP locations + ~12 sprites.** Plates: Crossroads (1), Darcy's Pub
   (2 — **indoor**), St. Brigid's (3), Murphy's Farm (9), Bog Road (12), Forge
   (16), Holy Well (17). Sprites are generated **separately** with transparent
   backgrounds (characters are NOT in plates).
   - Sprite scale: derive from the plate (a ~1.7 m person ≈ a fixed % of plate
     height). Reference: Stardew ≈ 16 px tile (~16 px/m native, 4× on screen).

## Repo state / where things are

- Branch `claude/recursing-pike-5073a3`, PR #1429 (diorama M1-M4).
- Commits this line of work: M1 `214cb217`, M2 `3a9326ef`, M3 `aef52a3d`, M4
  `00252437`, FLUX removal `39fceef9`, `.env` load `5de6a267`, GOOGLE_API_KEY
  fallback `073777cf`. (Code is pushed to PR #1429; **art is not in git**.)
- art-tool providers now: `openai` (default), `google`, `stability`
  (nvidia/fal removed). Live `generate()` still stubbed.
- **Art assets live in iCloud Drive, NOT git** (owner's decision —
  out of git history and LFS). Generated plates/masters:
  `~/Library/Mobile Documents/com~apple~CloudDocs/Rundale/diorama-art/<slug>/`.
  `.gitignore` ignores `art/**/*.png` so any local scratch under `art/` is never
  committed. The pipeline should write generated art to the iCloud folder (or a
  gitignored scratch), never into a tracked path.
- The flag is `diorama` (default-off). Enable in-game with `/flag enable diorama`.

## Resume checklist (fresh session)

1. Read this doc + `docs/design/ideas/parish-diorama.md`.
2. `source .env`; confirm `curl https://api.openai.com/v1/models` lists
   `gpt-image-2` and `gpt-5.5`.
3. Prototype the river fix: build a layout schematic for Kilteevan (derive from
   `world.json` + intended hotspots), rasterize, use as a control image with the
   Responses API, regenerate the master, re-run the 18-variant batch.
4. Then: firelight overlay → port pipeline into `parish-art-tool` → extend the
   scene schema + `select_variant` to season×weather×time → remaining locations
   - sprites → post-process/accept → wire into `scenes.json` → flip the flag (M6).
