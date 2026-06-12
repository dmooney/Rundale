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
  style SWATCHES + an automated 9-check vision gate. Scripts live in
  `parish/scripts/diorama/`.
- **NO MASTER ACCEPTED YET — iteration PAUSED (owner, 2026-06-12):** the
  per-defect edit loop on the owner's preferred candidate (g-07) was not
  converging ("this isn't working to my satisfaction"). Full session
  record in "Iteration record" below. Strongest candidates on disk:
  `g-08-swatchv2.png` (9/9 gate, rod-free, but multi-chimney + oversized
  SW plot nits), `g-06-swatchv2.png` (9/9, busier), `g-07-swatchv2.png`
  (owner's preferred look, but the mill track never reaches the left
  edge; two edit attempts g-09/g-10 fixed lanes and then could not get
  walls_anchored back to true).
- **OPENAI CREDITS EXHAUSTED (2026-06-12, HTTP 429 insufficient_quota);
  provider switched to Google** (see Locked decisions). The vision gate
  runs on `gemini-3.5-flash` (`check <img> google`).
- **Current scope (owner decision 2026-06-12): good MASTERS only.** No
  variant batches, nothing merged to main (work rides PR #1429's branch).
  Everything else is deferred — see "Deferred work" below.

## Locked decisions (do not re-litigate)

- **Provider/model (switched 2026-06-12, owner decision): Google Gemini
  API, model `gemini-3-pro-image` (Nano Banana Pro)** via
  `generateContent` with input images, `imageConfig {aspectRatio: "3:2",
imageSize: "2K"}` (≈2528×1696; downscale to 1536×1024 on promote). Why:
  gpt-5.5 renders kept a "rod below the thatch ridge" artifact and blocky
  random walls regardless of prompt overrides; Nano Banana Pro follows
  defect-warning instructions (proven: rod gone, walls irregular even with
  a defective style reference) and nails the schematic geometry. Imagen 4
  was evaluated and rejected: pure text-to-image, no input images, so it
  cannot take the control schematic or a style reference at all.
- **Previous OpenAI path (gpt-5.5 Responses + image_generation) is kept
  working in `gen_master.py` as provider `openai`** — its quota is
  exhausted; do not pay to revive it unless Google regresses.
- **Rejected models (don't revisit):** `gpt-image-1` (too many AI
  hallmarks), NVIDIA NIM FLUX.1-dev and fal FLUX.1-dev (painterly, not
  clean pixel art; NVIDIA also black-frames on mood words like "gritty").
  All removed from the art-tool.
- **Fallback providers if Google regresses (owner, 2026-06-12):**
  ByteDance **Seedream** (4.x — supports reference/edit input images via
  fal/Replicate/BytePlus ARK, pipeline-compatible) and xAI **Grok
  Imagine** (VERIFY input-image support first — earlier API versions were
  text-to-image only, which would disqualify it like Imagen 4). The hard
  requirement for any provider: accepts a control image + style reference
  as inputs.
- **Variant consistency via edit-off-master:** generate ONE master base, then
  every variant is an **edit** of it — pass the master as an input image +
  "keep the exact same composition, change ONLY season+lighting". Composition
  stays locked across all variants. Proven across 18 frames (OpenAI); the
  same mechanism is `gen_master.py edit` on Nano Banana Pro (proven for
  targeted retouches — used to remove an orphan wall stub).
- **Style anchoring on Google:** text-only style gives flat cartoon
  (g-02); swatch sheet causes half-painted block-in unless the prompt
  orders "fully paint EVERY surface" (g-01 vs g-03, and g-03 wobbled the
  composition). The winning recipe is **cand-16 (best gpt-5.5 render) as
  the full-scene style reference + defect warnings** (g-04/g-05). Once a
  Google master is accepted, IT becomes the style ref for the other
  locations.
- **Keys:** `GEMINI_API_KEY` / `GOOGLE_API_KEY` in `.env` (owner has paid
  credit). `OPENAI_API_KEY` remains for the dormant openai path.

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
5. **v4 additions (owner art review of cand-09..12):** ANCHORED walls —
   every schematic wall polyline runs anchor-to-anchor (lane band, cottage
   corner, river bank, another wall as a T-junction, or out of frame), with
   yard brackets enclosing the cottage plots, because real Irish walls exist
   from field-clearance necessity and the model multiplies any random stub
   it sees. MATERIAL NOTES in the prompt override the swatches: dry-stacked
   irregular limestone fieldstone (mixed sizes, daylight gaps, no mortar, no
   block coursing — see feidín/single/double wall types), and plain
   oat-straw thatch with a rolled ridge cap, NOTHING protruding from any
   roof. The roof-pipe artifact in cand-01..12 was the model literalizing
   the old "smoke seeps through the thatch ridge" prompt line — smoke is
   gone from masters entirely (variant/overlay material later). Gate
   expanded to 8 checks: + bridge_arch_over_water (cand-10 had the arch
   rotated toward the road), no_extra_lanes (cand-11/12 hallucinated a NW
   path), no_roof_protrusions, walls_anchored.
6. **Cross-verification (quota died before cand-16's gpt-5.5 check, so the
   gate ran as: independent Claude vision judge + deterministic pixel
   analysis + full-res edge strips).** Findings: cand-16 passes everything
   — its two water components are separated by exactly the bridge width
   with overlapping y-ranges (continuity-with-bridge proven mechanically),
   and all five exits reach their edges. cand-15's gpt-5.5 8/8 was WRONG
   on lane reach: the edge strips show its mill track fades before the
   left edge (4/5 exits). Single-judge full-frame verdicts are not
   trustworthy for edge details; `check` now sends full-res left/right
   edge strips, and a water-component gap-vs-bridge-width pixel probe is
   the durable mechanical test for river continuity.
   **Recommended: cand-16.** cand-01..15 kept as the failure/iteration
   record.

Schematic-drawing gotchas: keep the well ring inside the plaza blob (not on
a lane band), walls clear of lanes/river/cottages, the river locally
straight and PERPENDICULAR through the bridge crossing, the mill track
clearly separated from the river at the frame edge, and wall polylines
anchored at BOTH ends with no X-crossings (use T-junctions) — the model
paints every schematic mistake faithfully.

Escalation levers if a future location resists (NOT needed for Kilteevan):
gpt-5.5-pro plans the layout → emits SVG → rasterize → control image →
render.

## Iteration record (2026-06-12 session) — 26 renders, what worked, what didn't

All candidates in `…/diorama-art/kilteevan-village/master-candidates/`.
Costs: the OpenAI key's ~$10 went on cand-01..16 + checks (quota now
exhausted); the Google path cost roughly $2-3 of the owner's paid credit
for g-01..10 + gemini-3.5-flash checks.

### OpenAI gpt-5.5 (Responses + image_generation) — cand-01..16

| Batch       | Recipe                                               | Result                                                                                                                                                                                                                                                             |
| ----------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| cand-01     | schematic control + full v1 master as style ref      | River continuous but banks misaligned right of the bridge ("oh so close")                                                                                                                                                                                          |
| cand-02..04 | + straightened crossing, bank-alignment rule         | All three displaced the river east of the bridge, drifting south. Agent misread 02 as good — origin of the vision gate                                                                                                                                             |
| cand-05..08 | + block-in reframe, narration, continuity rules      | 0/4: regressed to the v1 broken composition. Root cause found: a full-scene style ref leaks its COMPOSITION                                                                                                                                                        |
| cand-09..12 | + style swatch sheet v1                              | River continuity 0/4 → 4/4. But: cand-10 bridge arch rotated toward the road, cand-11/12 hallucinated a NW path. Owner: walls random/blocky, a "rod/pipe" below every thatch ridge                                                                                 |
| cand-13..16 | + anchored walls, Irish material notes, 8-check gate | cand-15 8/8 but its mill track never reaches the left edge (caught later by edge strips — gate now sends full-res edge strips); cand-16 best (7/8). Rod and blocky walls persisted regardless of prompt overrides → provider switch. Quota died at cand-16's check |

### Google Nano Banana Pro (gemini-3-pro-image) — g-01..10

| Cand     | Recipe                                                                 | Result                                                                                                                                                                                                                                      |
| -------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| g-01     | swatches v1                                                            | Half-painted: ground/river left as flat block-in. Rod present (via swatch cottage tile)                                                                                                                                                     |
| g-02     | text-only style (no image ref)                                         | PERFECT geometry (8/8) and the only fully rod-free ridges of the whole session — but flat-cartoon style, not the locked pixel art                                                                                                           |
| g-03     | swatches v1 + "fully paint EVERY surface"                              | Locked style back; owner's favourite look of the session. But rod back, SW cottage migrated to bottom-center, chimneys+smoke crept back                                                                                                     |
| g-04     | cand-16 full scene as style ref                                        | Near-copy of cand-16 — a full-scene ref whose composition matches the schematic is a COPY CHANNEL, not a style anchor. Rod back, "too clean", weird foreground path. 7/8                                                                    |
| g-05     | g-04 + `edit` (remove orphan wall stub)                                | 8/8 gate, but inherits all g-04 flaws; owner rejected with g-04                                                                                                                                                                             |
| g-06..08 | swatches v2 (ridge-free crops) + off-frame corner walls + 9-check gate | Rod GONE at full zoom (channel closed). g-06 9/9 but busy (extra plots, patchy lanes, 4 gable chimneys); g-08 9/9, cleaner, chimneys + oversized SW plot remain; g-07 failed (mill track ends at the SW cottage) but owner prefers its look |
| g-09     | g-07 + `edit` (add mill track, drop stray stub)                        | Lanes fixed; the new track severed walls → isolated fragments                                                                                                                                                                               |
| g-10     | g-09 + `edit` (reconnect/remove fragments)                             | walls_anchored still false. Owner stopped the loop here                                                                                                                                                                                     |

### Established findings (full detail in

[scene-dsl-pipeline.md](../design/ideas/scene-dsl-pipeline.md))

1. **Image references dominate text instructions.** Composition leaks from
   full-scene refs; defects (the ridge rod) leak from ANY ref that shows
   the defective region, and defect warnings do not reliably suppress
   them. The only clean-ridge renders either had no image ref (g-02) or a
   ref with the ridges physically cropped out (g-06..08).
2. **A full-scene style ref matching the target layout = near-copy** (g-04
   /g-05 vs cand-16). Style refs must be layout-free (swatches).
3. **Swatches need the "fully paint EVERY surface" order** or Nano Banana
   leaves the block-in unpainted (g-01 vs g-03).
4. **The vision gate works but needs the right inputs:** downscaled full
   frame + full-res bridge crop + full-res left/right edge strips; checks
   river continuity, bank alignment, arch orientation, exit inventory
   (5 lanes: 1 top / 1 right / 1 left / 2 bottom), roof protrusions,
   cottages-in-quadrants, wall anchoring. Agent full-frame eyeballs
   misjudged twice (cand-02, cand-15); never trust an unzoomed look.
5. **`edit` retouching is whack-a-mole on structural features:** each fix
   (add a lane) perturbs neighbours (severs walls), and the follow-up fix
   doesn't converge (g-09→g-10). Edits are fine for isolated-object
   removal (g-05's wall stub); structural changes should be re-renders
   with an improved prompt/schematic instead.

### Open problems when work resumes

- Multiple gable chimneys on most Google renders (model prior, not ref
  leak) — likely fix: change "at most one simple stone chimney" to "NO
  chimneys at all" in the MATERIAL NOTES (matches the plate spec's
  mostly-chimney-less brief anyway).
- SW tilled plot renders oversized/odd (g-06/g-08 wedge) — shrink it in
  the schematic constants or name its size in the narration.
- Owner taste vs gate: g-07 (gate-fail) reads better to the owner than
  g-08 (gate-pass). The gate checks geometry, not aesthetics — keep using
  it as a floor, not a verdict.
- Candidate paths forward, owner's pick: (a) one more RENDER round (not
  edits) with the chimney/plot prompt fixes on the g-08 recipe, N=4;
  (b) accept g-08 as-is and fix nits in the variant pass; (c) try the
  Seedream fallback (see Locked decisions).

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
2. `source .env`; confirm the Gemini key works:
   `curl "https://generativelanguage.googleapis.com/v1beta/models?key=$GEMINI_API_KEY"`
   lists `gemini-3-pro-image` and `gemini-3.5-flash`.
3. Read the "Iteration record" above — NO master is accepted yet; the
   owner paused the loop. Ask the owner to pick a path from "Open
   problems → candidate paths forward" (new render round with chimney/
   plot fixes vs accept g-08 vs Seedream fallback) before burning any
   credit.
4. Once a Kilteevan master is accepted: downscale 2528×1696 → 1536×1024,
   promote to `0-master_summer-day-sunny.png` (keep the old one as
   `0-master_v1_broken-river.png`), rebuild the RIDGE-FREE swatch sheet
   off it (`gen_master.py swatches` — keep ridges out of the crops), then
   masters for the remaining 7 locations (per-location exit
   classification + schematic constants in `gen_master.py` → `generate …
google` N=2-4 → `check … google` gate + water-component pixel probe →
   owner eyeball). `edit` ONLY for isolated-object removal, never
   structural fixes (finding 5). Still NO variant batches until the owner
   re-opens that scope.
5. Then work the "Deferred work" list above in order: variant batches →
   firelight overlay → port pipeline into `parish-art-tool` → extend the
   scene schema + `select_variant` to season×weather×time → sprites →
   post-process/accept → wire into `scenes.json` → flip the flag (M6).
