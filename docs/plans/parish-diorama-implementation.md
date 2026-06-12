# Plan: Interactive Parish Diorama — Implementation

> Parent design: [Interactive Parish Diorama](../design/ideas/parish-diorama.md) | [Docs Index](../index.md)
>
> **Status: Proposed**
>
> **Depends on:** nothing in flight — all engine seams it builds on are shipped
> **Depended on by:** future interior-node content work; design-doc graduation to `docs/design/`

## Goal

Ship the MVP of the scene-based diorama described in the design doc: per-location
pixel-art plates as the main viewport, clickable hotspots, simulation-driven NPC
sprite placement, a `diorama` feature flag, and the `parish-art-tool` asset
pipeline — as six independently landable PRs (M1–M6).

This plan decomposes each milestone into **subagent tasks**. Each task is
self-contained enough to hand to one subagent and carries:

- **Model** — `haiku` (mechanical, low-ambiguity), `sonnet` (standard
  implementation), `opus` (architectural seams, cross-cutting judgement,
  quality-sensitive output).
- **Effort** — `low` (< ~1 h, narrow diff), `medium` (half-day, one area),
  `high` (multi-file, design judgement, or flake-prone verification).
- **Depends** — tasks that must land first.
- **Tests** — the automated tests the task must add or keep green (the
  consolidated test plan is in [§ Automated test plan](#automated-test-plan)).

## Orchestration rules (apply to every milestone)

1. **One PR per milestone**, branched from the previous milestone's merge.
2. **AC-first (AGENTS rule 13):** the milestone starts with `/task-start
diorama-m<N>` producing `.proofs/diorama-m<N>/acceptance-criteria.md` and
   `parish/testing/fixtures/play_diorama-m<N>.txt` **before any code task is
   dispatched**. Human approval of the AC gates the rest of the milestone.
3. **Proof (AGENTS rule 10):** milestones touching runtime paths (all except
   M4) need a live-proof bundle (transcript and/or screenshot) attached to the
   PR body via `parish/scripts/compose-proof-body.sh`; `just agent-check`
   green before push.
4. Subagents run `just check` (fmt + clippy + tests) before reporting done;
   the milestone orchestrator runs the full gate (`just check`, `just
ui-test`/`ui-e2e` where relevant, `just agent-check`) before the PR.
5. Tasks marked ∥ within a milestone may run as parallel subagents on
   non-overlapping files.

```text
M1 ──► M2 ──► M3 ──► M5 ──► M6
  └──► M4 ─────────────┘        (M4 parallel with M2/M3; M5a ∥ M5b)
```

---

## M1 — Scene schema, loader, validation (PR 1)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                 | Model    | Effort | Depends    |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | ---------- |
| T1.0 | `/task-start diorama-m1`: AC + fixture. Criteria: scenes.json parses; invalid asset path rejected at load; unknown location/NPC ids warn without failing; mods without `scenes` load unchanged; headless log line `scenes.json loaded: N scenes, M sprites`.                                                                                                         | `opus`   | low    | —          |
| T1.1 | **Schema + loader.** Create `parish/crates/parish-mod/src/scenes.rs`: `SceneIndex`/`SceneDef`/`Hotspot`/`HotspotAction`/`NpcSlot`/`SpriteDef` per the design doc schema (percentage coords, `rect` shape with `polygon` reserved), `SceneIndex::load(mod_dir, rel)` validating every asset ref through `assets::canonical_mod_asset_path`, `scene_for`/`sprite_for`. | `sonnet` | high   | T1.0       |
| T1.2 | **Mod wiring.** `FileRefs.scenes: Option<String>` in `manifest.rs`; `GameMod.scenes: Option<SceneIndex>` loaded in `GameMod::load` (`parish-mod/src/lib.rs`); cross-validation `validate_scenes(&SceneIndex, &WorldGraph, &NpcManager) -> Vec<String>` warnings (ids exist, coords 0–100) logged at the parish-core mod-load site.                                   | `sonnet` | medium | T1.1       |
| T1.3 | **Placeholder content.** `mods/rundale/scenes.json` (2 scenes: Darcy's Pub id 2, The Crossroads id 1, with hotspots/slots), 2 flat-colour 480×270 placeholder plates + 1 generic sprite under `mods/rundale/assets/scenes/`, `scenes = "scenes.json"` in `mods/rundale/mod.toml`.                                                                                    | `haiku`  | low    | T1.1       |
| T1.4 | **Proof.** Run the fixture headless (`just run-headless --script …`), capture transcript showing the load line, write `evidence.md` + `judge.md`, `just agent-check`, compose PR body.                                                                                                                                                                               | `sonnet` | medium | T1.2, T1.3 |

**Task-level tests** (in T1.1/T1.2): serde roundtrip for every action/shape
variant; `[x,y,w,h]` rect parsing; rejection of `../escape.png` and absolute
paths; missing plate file → `ParishError::Config`; unknown `location_id` /
`prefer_npc` / `travel_to` target → warning string; `scene_for`/`sprite_for`
hit and miss; existing `parish-mod` fixture mod (no `scenes` key) loads
unchanged.

---

## M2 — Backend scene-state + asset serving, three runtimes (PR 2)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                                                                                 | Model    | Effort | Depends   |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ------ | --------- |
| T2.0 | `/task-start diorama-m2`: AC + fixture (flag off → empty scene-state; flag on → plate URL + hotspots + seated NPCs; traversal on asset route rejected; `/scene` parity output in headless; `parish_scene_state` returns the same view model over MCP).                                                                                                                                                                               | `opus`   | low    | M1 merged |
| T2.1 | **Shared handler (rule 12 seam).** Create `parish-core/src/ipc/scene.rs`: `SceneState`/`SceneNpcView`/`SceneHotspotView`; `build_scene_state(world, npcs, scenes, flags, asset_url)` — flag gate, variant-by-hour (Dusk/Night/Midnight → `night` when present), **deterministic slot assignment** (pass 1 `prefer_npc`, pass 2 remaining present NPCs by id into remaining slots in declaration order, leftovers → `overflow_npcs`). | `opus`   | high   | T2.0      |
| T2.2 | ∥ **Server routes.** `parish-server/src/routes/scene.rs`: `GET /api/scene-state` (state-lock pattern of `get_npcs_here`, `routes/world.rs`), `GET /api/scene-asset/{*rel}` — promote `canonical_mod_asset_path` to `pub`, restrict to `assets/scenes/`, serve like `serve_mod_icon` (`routes/world.rs:163`) with `Cache-Control: immutable` + `?v=<mtime>`. Register in `routes.rs`/`lib.rs`.                                        | `sonnet` | medium | T2.1      |
| T2.3 | ∥ **Tauri command.** `parish-tauri/src/commands/scene.rs`: `get_scene_state` with `asset_url = mod_asset_data_url(...)` (`parish-tauri/src/lib.rs:39`); register in the command registry.                                                                                                                                                                                                                                            | `sonnet` | medium | T2.1      |
| T2.4 | ∥ **CLI parity.** `/scene` debug command in the headless input path printing scene id, variant, and slot assignments as text; extend the M2 fixture to assert it.                                                                                                                                                                                                                                                                    | `sonnet` | low    | T2.1      |
| T2.5 | ∥ **MCP exposure.** `parish_scene_state` tool in `parish-mcp` — thin bridge over `GET /api/scene-state` (same passthrough shape as `parish_engine_state`); register in the tool list; rows added to the MCP tool tables in `AGENTS.md` and `parish/crates/parish-mcp/README.md`. Gives auto-QA agents structural scene assertions (plate, variant, hotspots, slot seating) instead of pixel-reading screenshots (#1331 pattern).     | `sonnet` | low    | T2.2      |
| T2.6 | **Gate + proof.** Architecture-fitness still green (new modules declared; no backend deps leak into leaf crates); live proof: `bash parish/scripts/parish-mcp-backend.sh start`, `/flag enable diorama`, `curl /api/scene-state` + `curl /api/scene-asset/...`, and a `mcp__parish__parish_scene_state` call transcript; evidence/judge; `just agent-check`.                                                                         | `sonnet` | medium | T2.2–T2.5 |

**Task-level tests:** T2.1 — slot determinism (same inputs → same seating;
`prefer_npc` honoured; overflow ordering), variant-by-hour table test,
flag-off → `None`, unplated location → `None`, empty-slots scene seats nobody
but still returns hotspots. T2.2 — Axum route test: `../mod.toml` and
URL-encoded traversal → 4xx; placeholder PNG served with `image/png` and
immutable cache header; flag-off returns `null` body. T2.3 — data-URL helper
unit test (prefix + base64 round-trip). T2.5 — bridge unit test: tool
deserializes a recorded `/api/scene-state` body and surfaces transport
errors as MCP `isError` (matching the other bridge tools).

---

## M3 — Scene-first frontend behind the flag (PR 3)

| #    | Task                                                                                                                                                                                                                                                                                                                                                                | Model    | Effort | Depends    |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | ---------- |
| T3.0 | `/task-start diorama-m3`: AC + fixture (flag on → plate visible; hotspot click travels; sprite click addresses NPC; flag off → existing layout pixel-identical; unplated location falls back).                                                                                                                                                                      | `opus`   | low    | M2 merged  |
| T3.1 | **State + IPC wiring.** `src/stores/scene.ts` (`sceneState` writable), `src/lib/ipc/scene.ts` (`getSceneState()` through `command()`, Tauri data-URL cache keyed `(slug, variant)`), `SceneState` types in `src/lib/types.ts` (snake_case matching Rust serde), fetch at mount + on every `world-update` in `page-controller.ts`.                                   | `sonnet` | medium | T3.0       |
| T3.2 | ∥ **Component tree.** `src/components/diorama/`: `DioramaView` (single `aspect-ratio: 480/270` wrapper, layer stack, null fallback), `ScenePlate` (pixelated `<img>`, cross-fade), `HotspotLayer` (SVG `viewBox="0 0 100 100"` `preserveAspectRatio="none"`), `NpcSpriteLayer` (`left:{x}%; bottom:{100-y}%`, foot anchor, tooltip), `SceneOverlay` (palette tint). | `sonnet` | high   | T3.1       |
| T3.3 | ∥ **Action mapping.** `src/lib/scene-actions.ts`: `travel_to` → `submitInput("go to <name>")` (name resolved from `mapData`, same path as `MapPanel` clicks); `talk_to`/sprite click → focus `InputField` with `addressed_to` chip; `inspect` → local `system` entry in `textLog`. Unknown ids are no-ops.                                                          | `sonnet` | medium | T3.1       |
| T3.4 | **Layout switch.** `+page.svelte`: `$sceneState !== null` → scene-first grid (DioramaView primary, ChatPanel + InputField below, right column untouched); `null` → existing layout (doubles as per-location fallback). Mobile breakpoint handled like the existing 768 px rules.                                                                                    | `sonnet` | medium | T3.2, T3.3 |
| T3.5 | **Playwright e2e.** New `e2e/diorama.spec.ts` (see test plan §3). Confirm zero diffs in existing baselines with the flag off.                                                                                                                                                                                                                                       | `sonnet` | high   | T3.4       |
| T3.6 | **Proof.** Screenshot pair (flag off / flag on with placeholder plates) + live hotspot-travel transcript via the browser or MCP screenshot tools; evidence/judge; `just agent-check`.                                                                                                                                                                               | `sonnet` | medium | T3.5       |

**Task-level tests:** T3.1 — store update on world-update event (vitest, mocked
transport); cache hit avoids refetch for same `(slug, variant)`. T3.2 —
render test: mocked `SceneState` places a hotspot rect and a sprite at the
expected percentage geometry; missing `plate_url` renders fallback. T3.3 —
`scene-actions.test.ts`: every action variant maps to the right
`submitInput`/`addressed_to`/log call; unknown location id is a safe no-op.

---

## M4 — `parish-art-tool` crate (PR 4 — runs in parallel with M2/M3)

| #    | Task                                                                                                                                                                                                                                                                                                                              | Model    | Effort | Depends   |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ | --------- |
| T4.0 | `/task-start diorama-m4`: AC (each subcommand's observable behaviour, dry-run without API keys, manifest invariants).                                                                                                                                                                                                             | `opus`   | low    | M1 merged |
| T4.1 | **Crate skeleton + manifest.** `parish/crates/parish-art-tool/` (workspace member, clap structure mirroring `parish-npc-tool`): `main.rs` command enum (`init`, `gen-plate`, `gen-sprite`, `gen-variant`, `list`, `review`, `accept`, `reject`), `manifest.rs` (`ArtManifest` records + status transitions + atomic save).        | `sonnet` | medium | T4.0      |
| T4.2 | **Prompt builder + style bible.** `prompt.rs` building production prompts from `LocationData` (name, description_template, indoor, mythological_significance) and `Npc` (name, age, occupation, brief_description) + the committed `art/style-bible.md` (written by this task: style rules + negative rules from the design doc). | `opus`   | medium | T4.1      |
| T4.3 | ∥ **Providers.** `providers/`: `ImageProvider` trait; `openai.rs` (`gpt-image-1` generations + edits, transparent background for sprites, reference images); `google.rs` (Imagen 3 via Gemini API). Env keys `OPENAI_API_KEY`/`GEMINI_API_KEY`; request-size caps client-side per AGENTS rule 16.                                 | `sonnet` | high   | T4.1      |
| T4.4 | ∥ **Postprocess + export.** `postprocess.rs` (`image` crate: Lanczos downscale → 480×270 plates / 48×72 sprites, alpha preserved, sprite trim); `export.rs` (accepted asset → `mods/rundale/assets/scenes/<slug>/`, `scenes.json` plate/sprite path upsert, path must land under `assets/scenes/`).                               | `sonnet` | medium | T4.1      |
| T4.5 | **Docs + gate.** Crate README (workflow: anchors, curation, env keys), root `.gitignore` for `art/` (manifest + style bible excepted), `just notices` for new deps, full `just check`. Live transcript of one real `gen-plate → review → accept` run attached to the PR.                                                          | `haiku`  | low    | T4.2–T4.4 |

**Task-level tests:** manifest serde roundtrip + illegal status transitions
rejected; golden prompt strings for Darcy's Pub plate and Padraig Darcy sprite
(snapshot, update-reviewed); `payload.len() <= budget` for both providers
(rule 16); provider request-body construction against recorded JSON (no live
API in CI); postprocess fixture PNG → exact output dims + alpha preserved;
export rejects targets outside `assets/scenes/`.

---

## M5 — Curated content (PR 5a plates ∥ PR 5b sprites)

Human-in-the-loop milestone: subagents drive the tool and propose; a human
accepts every asset.

| #    | Task                                                                                                                                                                                                                                                                                                                                                                                                                           | Model    | Effort | Depends        |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ------ | -------------- |
| T5.0 | `/task-start diorama-m5`: AC (8 locations render their plates in-game; night variant swaps at dusk in the pub; every connection from a covered location is clickable; ~12 named NPCs render their own sprite, others the fallback).                                                                                                                                                                                            | `opus`   | low    | M3 + M4 merged |
| T5.1 | **Anchor round.** Generate candidate batches for the two anchor assets (Kilteevan Village plate, Padraig Darcy sprite) with both providers, present a curation sheet (paths + prompts) for human accept; mark `anchor: true`.                                                                                                                                                                                                  | `opus`   | high   | T5.0           |
| T5.2 | ∥ **Plates (PR 5a).** With anchors as refs: The Crossroads (1), Darcy's Pub (2), St. Brigid's Church (3), Murphy's Farm (9), The Bog Road (12), Kilteevan Village (15), The Forge (16), The Holy Well (17); night variants for pub + village. Iterate per rejection feedback.                                                                                                                                                  | `opus`   | high   | T5.1           |
| T5.3 | ∥ **Sprites (PR 5b).** ~12 NPCs whose schedules hit the 8 locations (per the design doc roster) + generic-villager fallback.                                                                                                                                                                                                                                                                                                   | `opus`   | high   | T5.1           |
| T5.4 | **Hotspot/slot authoring.** Hand-write `mods/rundale/scenes.json` against the final plates: every world.json connection from a covered location → `travel_to` hotspot; 2–4 slots per scene; `prefer_npc` for pub bar (1), forge anvil (9), church altar (3); inspect hotspots.                                                                                                                                                 | `sonnet` | medium | T5.2           |
| T5.5 | **Content gate + proof.** Loader cross-validation warning-free; content sanity test (every referenced file exists, every covered-location connection has a hotspot); scripted walk of all 8 locations capturing per-location screenshots, day + night pub, **plus structural assertions per stop via `mcp__parish__parish_scene_state`** (expected plate slug, variant, seated NPCs vs `parish_engine_state`); evidence/judge. | `sonnet` | medium | T5.3, T5.4     |

---

## M6 — Polish, flag flip, docs (PR 6)

| #    | Task                                                                                                                                                                                                                                 | Model    | Effort | Depends   |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ------ | --------- |
| T6.0 | `/task-start diorama-m6`: AC (default-on behaviour, kill-switch works, transition smoothness criteria, baseline set regenerated deliberately).                                                                                       | `opus`   | low    | M5 merged |
| T6.1 | ∥ **Visual polish.** Palette-tint strength tuning; CSS rain effect keyed off `worldState.weather`; fade travel transition reusing the theme cross-fade pattern.                                                                      | `sonnet` | medium | T6.0      |
| T6.2 | **Flag flip.** `build_scene_state` gate → `!flags.is_disabled("diorama")` (default-on kill-switch per AGENTS rule 6); regenerate Playwright baselines (`just ui-e2e -- -u`) — the one PR where baseline churn is expected; document. | `sonnet` | medium | T6.1      |
| T6.3 | ∥ **Docs.** README feature list + structure (rule 7); `just screenshots`; graduate the design doc from `docs/design/ideas/` to `docs/design/diorama.md` (Status: Implemented) and update `docs/index.md`.                            | `haiku`  | low    | T6.2      |
| T6.4 | **Final proof.** Before/after gif of travel + day/night; full gate (`just check`, `just ui-test`, `just ui-e2e`, `just agent-check`); evidence/judge.                                                                                | `sonnet` | medium | T6.2      |

---

## Automated test plan

Every layer of the stack gets automated coverage; per-task lists above are the
source of truth for _what_, this section fixes _where and how it runs_.

### 1. Rust unit + integration (`just check` / `cargo test`, every PR)

| Suite                             | Lives in                                    | Covers                                                                                                                                     |
| --------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Scene schema (`scenes::tests`)    | `parish-mod/src/scenes.rs`                  | serde roundtrips, rect/action parsing, traversal + absolute-path rejection, missing-file error, lookup hit/miss, optional-file back-compat |
| Cross-validation                  | parish-core mod-load tests                  | unknown location/NPC ids warn (not fail), coord range checks                                                                               |
| Scene-state (`ipc::scene::tests`) | `parish-core/src/ipc/scene.rs`              | **slot-assignment determinism**, prefer_npc, overflow, variant-by-hour, flag-off/unplated → `None`                                         |
| Server routes                     | `parish-server/src/routes/tests.rs`         | scene-asset traversal → 4xx, mime + immutable cache headers, scene-state flag-off `null`                                                   |
| Art tool                          | `parish-art-tool` unit tests                | manifest transitions, golden prompts, provider bodies (recorded, offline), rule-16 payload caps, postprocess dims/alpha, export path guard |
| Architecture fitness              | `parish-core/tests/architecture_fitness.rs` | new modules declared (orphan check), no `tauri`/`axum` deps leak into leaf crates, `parish-art-tool` registered as a tool crate            |

### 2. Frontend unit (`just ui-test`, M3+)

`scene-actions.test.ts` (action→command mapping, no-op safety),
`scene.store.test.ts` (world-update refresh, `(slug, variant)` cache),
`DioramaView.test.ts` (layer geometry from mocked state, fallback render) —
vitest + JSDOM with the existing mocked-Tauri setup.

### 3. End-to-end (`just ui-e2e`, M3+)

`e2e/diorama.spec.ts` against the real auto-started server:

1. Enable the flag through the real input (`/flag enable diorama`), reload.
2. Assert the plate `<img>` is visible and the hotspot SVG overlays it.
3. Click the travel hotspot → StatusBar location name changes; scene swaps.
4. Click an NPC sprite → input gains the `addressed_to` chip.
5. Flag off → existing baseline screenshots byte-stable (until the deliberate
   M6 regeneration).

### 4. Script-harness fixtures (`just game-test-one play_diorama-m<N>`, every milestone)

`parish/testing/fixtures/play_diorama-m<N>.txt` written at `/task-start` time;
deterministic headless runs asserting the load line (M1), `/flag enable
diorama` + `/scene` output (M2+), and the 8-location walk (M5). MCP-driven QA
(demo-audit skills) gains scene drift checks: `parish_scene_state` asserted
against `parish_engine_state` at each stop. These run in
the CI fixture sweep.

### 5. Determinism & snapshots

Slot assignment and variant selection are pure functions — table tests, no
golden images. Golden artifacts are limited to: art-tool prompt strings
(reviewed snapshots) and Playwright screenshot baselines (regenerated once,
in M6). No PNG byte-snapshots of game scenes: plates are hand-curated content,
not generated output.

### 6. CI gates that must stay green

Rust quality gate, UI quality, Playwright e2e, full fixture sweep, docs
consistency, and the **agent proof gate** (per-milestone bundle in the PR
body). M4 adds `just notices` output to the diff when provider/image deps
land.

## Risks carried from the design doc

Style drift (anchors + human accept gate), Tauri asset serving (data URLs,
payload size watched), layer coordinate skew (single aspect-ratio wrapper +
geometry tests), baseline churn (flag off until M6), save compatibility
(none — no schema changes).
