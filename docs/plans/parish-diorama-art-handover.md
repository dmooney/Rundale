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
- **River/layout blocker: FIXED (2026-06-12)** via structure-guided
  generation — layout schematic as control image + layout narration +
  style SWATCHES (not the full old master, whose composition leaked) + an
  automated vision gate. Scripts live in `parish/scripts/diorama/`.
  Survivors awaiting owner sign-off:
  `…/diorama-art/kilteevan-village/master-candidates/cand-11.png`
  (stone arch, recommended) and `cand-09.png` (wooden bridge).
- **Current scope (owner decision 2026-06-12): good MASTERS only.** No
  variant batches, nothing merged to main (work rides PR #1429's branch).
  Everything else is deferred — see "Deferred work" below.

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
- **Generation scripts (now in the repo, `parish/scripts/diorama/`):**
  - `gen_variants.py` — salvaged from `/tmp`; base64s the master, fans out 18
    edits 4-at-a-time with retries via the Responses API. The locked variant
    instruction text is in there, byte-identical to the proven batch. Usage:
    `gen_variants.py <master.png> <out-dir>`.
  - `gen_master.py` — structure-guided master generation (the river fix).
    `schematic` subcommand renders the layout control image (needs Pillow:
    `uv run --with pillow …`); `generate` subcommand calls gpt-5.5 +
    image_generation with schematic as control + old master as style ref
    (stdlib only).

## River/layout coherence — RESOLVED (2026-06-12)

The model can't reason about spatial continuity on its own: in the first
Kilteevan batch the single river split into disconnected sections and the
bridge sat illogically. **Fix that worked (first try): structure-guided
generation** via `parish/scripts/diorama/gen_master.py`:

1. `gen_master.py schematic` renders a programmatic layout schematic
   (1536×1024 PNG, flat colors): ONE continuous blue river polyline entering
   the right edge and exiting the left, crossing exactly one lane; a grey
   bridge marker at that single crossing; red cottage rectangles; black well
   ring inside a brown plaza/common; lane network; dark wall lines; hatched
   empty tilled plots. Layout constants live at the top of the script —
   geometry was derived from the locked master's composition +
   `mods/rundale/world.json` connections.
2. `gen_master.py generate` feeds that schematic as a **control image** plus
   the old master as a **style reference** (two `input_image` entries, one
   Responses call, gpt-5.5 + image_generation, 1536×1024 high) with a legend
   prompt ("river follows the blue line, never breaks, bridge only at the
   grey marker…").
3. **v3 pipeline (current, after 12-render iteration):** block-in reframe
   ("this is the rough colour block-in, refine without moving anything") +
   a layout NARRATION generated from the same constants (`narrate`
   subcommand) + explicit continuity rules + a **style swatch sheet**
   instead of the full old master (`swatches` subcommand — the full-scene
   style ref leaked its broken-river composition into every render; swatch
   crops carry texture but cannot carry layout; 0/4 → 4/4 river continuity)
   - an automated **vision gate** (`check` subcommand: downscaled full
     frame + full-res bridge crop → gpt-5.5 strict-JSON verdict, exit code
     gateable). Render N=4, gate, human-pick survivors.
4. Exit model: connections classify as painted edge exits (5 for
   Kilteevan: north road, forge lane, mill track ∥ river on the left edge,
   south road over the bridge, holy-well mossy path forking off it past
   the bridge), in-scene doors (NE cottage = the weaver's), and generic
   links sharing a painted exit's hotspot. Narrative path_descriptions
   override world.json bearings when they conflict (they do — see the
   policy note atop `gen_master.py`).
5. Surviving candidates: `master-candidates/cand-09.png` (wooden plank
   bridge) and **`cand-11.png` (stone arch — matches spec, recommended)**.
   Awaiting owner sign-off. cand-01..08 kept as the failure record.

Schematic-drawing gotchas: keep the well ring inside the plaza blob (not on
a lane band), walls clear of lanes/river/cottages, the river locally
straight and PERPENDICULAR through the bridge crossing, and the mill track
clearly separated from the river at the frame edge — the model paints every
schematic mistake faithfully.

Escalation levers if a future location resists (NOT needed for Kilteevan):
gpt-5.5-pro plans the layout → emits SVG → rasterize → control image →
render.

## Deferred work (owner decision 2026-06-12 — do NOT lose this)

Scope was deliberately cut to "good masters only, on the PR branch". All of
the below is parked, none of it is done:

1. **18-variant batch regen per location** — the edit-off-master variant
   pipeline (`gen_variants.py`) is proven and untouched; re-run it off each
   accepted master (~$6/location). Kilteevan's existing 18 plates were made
   from the broken-river master and need regenerating off the accepted
   candidate.
2. **Firelight overlay** — per-cottage warm window glow as a separate
   transparent layer, toggled by NPC sleep state. Art (edit-off-master) +
   engine (layer toggle in scene state + Svelte).
3. **Port the pipeline into `parish-art-tool`** — providers are still offline
   request-builders with a stubbed live `generate()`; the real flow lives in
   `parish/scripts/diorama/`. Add an `openai-responses` path (gpt-5.5 +
   image_generation, input-image variants + control image).
4. **Extend the engine for the variant matrix.** Today the schema
   (`parish/crates/parish-mod/src/scenes.rs`, `SceneDef.variants:
HashMap<String,String>`) + the selector
   (`parish/crates/parish-core/src/ipc/scene.rs::select_variant`) only do
   night-by-hour + reserved weather. Extend to **season × weather × time-of-day**
   so the 18 variants are actually served. Keep the rule-12 shared-handler
   shape. Pure Rust — can proceed independently of art.
5. **Other 7 MVP location masters + ~12 sprites.** Plates: Crossroads (1),
   Darcy's Pub (2 — **indoor**, no river schematic), St. Brigid's (3),
   Murphy's Farm (9), Bog Road (12), Forge (16), Holy Well (17). Each needs
   its own layout schematic (new constants or a per-location table in
   `gen_master.py`). Sprites are generated **separately** with transparent
   backgrounds (characters are NOT in plates).
   - Sprite scale: derive from the plate (a ~1.7 m person ≈ a fixed % of plate
     height). Reference: Stardew ≈ 16 px tile (~16 px/m native, 4× on screen).
6. **Budget gap:** ~$0.2-0.4/render; 7 locations × 18 variants ≈ $40-50 +
   sprites. Key had ~$10 credits at handover time — owner must top up before
   the variant batches / remaining locations.
7. **Art distribution decision (pre-M6):** art lives in iCloud, not git —
   packaged builds need a delivery path (deploy artifact, download-on-first-
   run, or mod data dir). Owner call required before flipping the `diorama`
   flag on.
8. **Scene DSL pipeline (idea, owner-endorsed):** generalize the schematic
   layer to an LLM-interpreted text description → SVG → lint → control-image
   render pipeline — see
   [scene-dsl-pipeline.md](../design/ideas/scene-dsl-pipeline.md). Subsumes
   item 3's art-tool port if pursued.

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
3. Confirm the owner accepted a survivor (recommended:
   `…/diorama-art/kilteevan-village/master-candidates/cand-11.png`); if yes,
   promote it to the new `0-master_summer-day-sunny.png` (keep the old one as
   `0-master_v1_broken-river.png`) and rebuild the style swatch sheet off the
   ACCEPTED master (`gen_master.py swatches`) for all future locations.
4. Continue with masters for the remaining 7 locations (per-location exit
   classification + schematic constants in `gen_master.py` → generate N=4 →
   `check` gate → owner eyeball), still NO variant batches until the owner
   re-opens that scope.
5. Then work the "Deferred work" list above in order: variant batches →
   firelight overlay → port pipeline into `parish-art-tool` → extend the
   scene schema + `select_variant` to season×weather×time → sprites →
   post-process/accept → wire into `scenes.json` → flip the flag (M6).
