# Parish Designer — GUI Editor for Game Data

## Context

Parish (Rundale) is a text adventure set in 1820s rural Ireland with a rich body
of authored game content: locations, NPCs, schedules, relationships, festivals,
encounters, anachronisms, pronunciations, palette tints, loading screens, engine
tuning, and a branching SQLite save format. Today **none of it has a GUI editor**.
Designers must hand-edit JSON/TOML files, rely on deserialization errors to flag
mistakes, and restart the game to see changes. There is no save inspector, no
validator command, and no way to visualize NPC schedules or the location graph.

This document designs **Parish Designer**, a GUI editor that lives inside the
existing SvelteKit UI (reachable via a `/editor` route and menu toggle) and is
backed by a new `editor` module in `parish-core`. It follows the project's
mode-parity rule: editor commands are implemented once in `parish-core`, then
wired to both Tauri (`parish-tauri`) and Axum (`parish-server`). The same editor
therefore runs in the desktop app and in `parish --web`.

The plan is organized into three phases. **Phase 1** is the concrete
implementation deliverable: mod browser, NPC editor, location editor,
cross-reference validator, and read-only save inspector. Phases 2 and 3 are a
roadmap for follow-up work.

## Designer needs — brainstorm

What a game designer working on Parish actually needs day-to-day:

**Content authoring**
- Edit NPC fields (name, age, occupation, brief description, personality)
- Tune NPC intelligence across 6 dimensions with sliders
- Author seasonal schedules: "who is where at 3pm on a Sunday in winter?"
- Set NPC home / workplace via a location picker (no more memorizing ids)
- Add/remove relationships with automatic bidirectional bookkeeping
- Author dialogue knowledge items and gossip seeds
- Edit location descriptions with live preview of `{time}` / `{weather}` / `{npcs_present}` placeholders
- Draw connections between locations on a map (lat/lon coords)
- Enforce bidirectional edges when editing connections
- Edit festivals, encounter flavor text by time-of-day, anachronism table, pronunciation hints
- Edit transport modes, UI palette (with live preview), loading screen phrases
- Edit engine tuning in `parish.toml` (speeds, encounter probs, cognitive tiers)

**Validation & safety**
- Preflight validation: run `WorldGraph::validate()` + cross-reference checks (home/workplace/relationship targets exist)
- Surface serde errors with file + field paths
- Deterministic JSON formatting on save so `git diff` stays clean
- Warn when editing a mod that's currently loaded in a running game
- Undo within a session (at minimum: "reload from disk" as a hard reset)

**Exploration & inspection**
- Switch between mods in `mods/`
- Browse a save file: branches → snapshots → NPC dynamic state, clock, weather, gossip network, conversation log
- Dump a snapshot to JSON (seed a test fixture)
- Query "who is where at T?" across the full NPC roster
- Visualize the relationship social graph
- Visualize the location graph

**Live iteration**
- Run `GameTestHarness` scripts against unsaved edits (no LLM, no restart)
- Preview location descriptions with substituted placeholders
- (Eventually) hot-reload a running game session

**Workflow**
- Content author guide / tooltips explaining each field
- Duplicate an NPC or location as a template for the next
- Bulk operations (e.g. "find all NPCs whose workplace is Darcy's Pub")

## Architecture

**Home:** A new `/editor` route in the existing SvelteKit app at `apps/ui/`.
Reachable from the main UI via a menu entry; fully usable standalone (does not
require a running game session).

**Backend:** New module `crates/parish-core/src/editor/` owns all editor logic —
loading a mod from disk, validating it, writing it back, inspecting save files.
Exposed via new IPC types in `crates/parish-core/src/ipc/editor.rs`.

**Transports:** Both `parish-tauri` and `parish-server` add thin wrappers that
deserialize args, call the shared editor functions, and return JSON. This
preserves the **mode-parity rule** — the editor works identically in Tauri and
web modes.

**Game coupling:** The editor does **not** touch the running `AppState`
(`world`, `npc_manager`, `inference_queue`, autosave thread, tick loop). It
operates on a separate `EditorState` that loads a fresh in-memory copy of raw
mod files from disk. Closing the editor drops this state without touching
gameplay. A warning banner surfaces when the mod being edited matches the mod
the running game loaded from. See "Running-game isolation" in Gotchas for why
this separation is the single most important architectural rule of the feature.

**Why embed in the existing UI** instead of a separate crate/binary:
- Reuses the IPC abstraction at `apps/ui/src/lib/ipc.ts:29` (one codebase, two transports)
- Reuses Svelte stores, CSS variable theme, typography, and the paper aesthetic
- Designers can flip between playing and editing in one window
- Avoids duplicating parish-core wiring, Tauri/Axum setup, and build pipeline
- Module-ownership rule forces shared logic into `parish-core` anyway

## Reusable primitives (use these, don't rebuild)

**Backend**
- `crates/parish-core/src/game_mod.rs` — `GameMod::load()` is the *reference implementation* for parsing every mod file; the editor mirrors it but loads each file independently (see Phase 1 backend)
- `crates/parish-world/src/graph.rs:130` — `WorldGraph::validate()` enforces orphan/bidirectional checks and emits `ParishError::WorldGraph(String)`
- `crates/parish-types/src/error.rs` — `ParishError` enum (use for all editor errors)
- `crates/parish-core/src/ipc/handlers.rs` — pattern for pure state → IPC-type handlers
- `crates/parish-persistence/src/picker.rs:63` `discover_saves` + `crates/parish-persistence/src/database.rs` `list_branches` / `load_latest_snapshot` — save inspector never opens rusqlite directly
- `crates/parish-cli/src/testing.rs` — `GameTestHarness` for Phase 2 live preview
- `crates/parish-world/src/description.rs::render_description` — reuse for live placeholder preview

**Frontend**
- `apps/ui/src/lib/ipc.ts` — transport-agnostic command/event layer; editor commands reuse the existing `command<T>(...)` helper (line 33)
- `apps/ui/src/stores/` — Svelte writable store pattern (see `game.ts` for a model; `debug.ts` for the closest analog)
- `apps/ui/src/components/DebugPanel.svelte` (703 lines) — closest UI analog: tabbed introspection, NPC detail view, expandable sections. Lift its `tab-bar`/`tab-btn` CSS classes verbatim.
- `apps/ui/src/components/SavePicker.svelte` (888 lines) — modal overlay + custom tree layout. Pattern for the save inspector.
- `apps/ui/src/components/MapPanel.svelte` — SVG rendering of the location graph; reuse its projection math for the Phase 2 location map editor.
- `apps/ui/src/app.css` — CSS custom properties (bg, fg, accent, panel-bg, border, muted). All editor components inherit the palette automatically.
- `phosphor-svelte` — already in dependencies for icons.

## Phase 1 — MVP (ship this first)

Scope: a working end-to-end designer that loads a mod, edits NPCs and locations,
validates on save, writes the files back cleanly, and exposes a read-only save
inspector. **This is the deliverable for execution.**

Internally split into three cohesive sub-deliverables that can land sequentially:

- **Phase 1a** — Mod browser, Location editor, Validator panel, deterministic JSON writer. Tightly coupled; ships together.
- **Phase 1b** — NPC editor (identity, intelligence, home/workplace, relationships, knowledge). Schedule is **read-only** here (a visual 24-hour timeline). Full schedule editing moves to Phase 2.
- **Phase 1c** — Read-only save inspector. Independent of everything above.

### Backend — `crates/parish-core/src/editor/`

Create a new module with these files:

```
crates/parish-core/src/editor/
├── mod.rs              # Re-exports
├── types.rs            # DTOs: ModSummary, EditorModSnapshot, ValidationReport, ValidationIssue
├── handlers.rs         # Pure functions: list_mods, load_mod_snapshot, validate_snapshot
├── persist.rs          # Atomic file writers (temp + rename) for each editable doc
├── save_inspect.rs     # Read-only save file inspector (Phase 1c)
└── format.rs           # Deterministic JSON formatting (stable key order + atomic write)
```

Key types:
- `ModSummary { id, name, title, version, description, path }`
- `EditorModSnapshot { manifest, npcs, locations, festivals, encounters, anachronisms, pronunciations }`
- `ValidationReport { errors: Vec<ValidationIssue>, warnings: Vec<ValidationIssue> }`
- `ValidationIssue { category, severity, field_path, message, context }`

**Critical: do NOT use `GameMod::load` for editor loading.** That function is
all-or-nothing — one broken file aborts the whole mod. It also runs
post-processing (relationship reciprocation, reaction template merge) that
would pollute source files on save. The editor must load each file
independently via its own granular loader so a broken `festivals.json` doesn't
hide a working `npcs.json` from the designer. Post-save revalidation uses
`validate_snapshot`, not `GameMod::load`, for the same reason.

Reuses:
- `crates/parish-core/src/game_mod.rs:420` `GameMod::load` as a *reference implementation* only
- `crates/parish-world/src/graph.rs:130` `WorldGraph::validate` (needs to become `pub`)
- `crates/parish-world/src/description.rs::render_description` for placeholder preview
- `crates/parish-persistence/src/picker.rs:63` `discover_saves` + `database.rs::list_branches` / `load_latest_snapshot` for the save inspector

### Schema exposure in `parish-npc` and `parish-world`

Two small but critical upstream changes:

1. **`crates/parish-npc/src/data.rs:24-36`** — change `struct NpcFile` → `pub struct NpcFile`, `struct NpcFileEntry` → `pub struct NpcFileEntry`, and derive `Serialize` in addition to `Deserialize` on both (and on subtypes: `IntelligenceFileEntry`, `ScheduleFileEntry`, `ScheduleVariantFileEntry`, `RelationshipFileEntry`). Re-export from `crates/parish-npc/src/lib.rs`. Without this, the editor cannot round-trip `npcs.json`.

2. **`crates/parish-world/src/graph.rs:130`** — change `fn validate(&self)` → `pub fn validate(&self)`. The editor needs to re-run it on an in-memory graph without reloading the JSON file.

**`brief_description` round-trip gotcha.** `NpcFileEntry.brief_description` is
`Option<String>` with a load-time fallback that synthesizes a description from
occupation when absent. The editor must preserve the **raw** `Option<String>`
from disk — never the computed fallback — or saving will silently add
synthesized descriptions to every NPC in source. The UI should expose an
explicit "override brief description" checkbox; unchecked → write `None`.

### Backend — `crates/parish-core/src/ipc/editor.rs`

New IPC module (add to `ipc/mod.rs`). Contains serde DTOs mirroring editor ops
and pure handler functions that call into `editor::*`. No game-state access.

Commands (all pure functions, no game session required):

| Command | Purpose |
|---|---|
| `editor_list_mods()` | List directories under `mods/` |
| `editor_open_mod(path)` | Returns `EditorModSnapshot` with all parsed content |
| `editor_validate(path)` | Returns `ValidationReport` |
| `editor_upsert_npc(path, npc)` | Update or insert; returns validation |
| `editor_delete_npc(path, id)` | Remove by id |
| `editor_upsert_location(path, loc)` | Update or insert (with auto-bidirectional edges) |
| `editor_delete_location(path, id)` | Remove (errors if NPCs reference it) |
| `editor_save_to_disk(path)` | Write dirty docs back with deterministic formatting |
| `editor_reload_from_disk(path)` | Hard reset to on-disk state |
| `editor_list_saves()` | Scan save directory for `.db` files |
| `editor_open_save(path)` | Return list of branches + snapshot metadata |
| `editor_read_snapshot(path, snap_id)` | Return deserialized `GameSnapshot` JSON |

All commands return `Result<T, String>` (string errors for easy UI display).

### Backend — Tauri wiring

- **New file** `crates/parish-tauri/src/editor_commands.rs` parallels `commands.rs` and holds the `#[tauri::command]` wrappers
- Extend `AppState` in `crates/parish-tauri/src/lib.rs` with `pub editor: Mutex<EditorState>` where `EditorState { current_mod_path: Option<PathBuf>, snapshot: Option<EditorModSnapshot>, dirty: bool }`. **This field is completely independent** of `world` / `npc_manager` / `inference_queue`. The editor never touches live gameplay state.
- Register new commands in the `tauri::generate_handler!` block at `crates/parish-tauri/src/lib.rs:538-554`

### Backend — Axum wiring

- **New file** `crates/parish-server/src/editor_routes.rs` mirrors `editor_commands.rs` as REST routes
- Extend `AppState` in `crates/parish-server/src/state.rs:94-133` with the same `editor: Mutex<EditorState>` field
- Register new routes in `crates/parish-server/src/lib.rs:130-146`
- Route naming follows the auto-rewrite in `apps/ui/src/lib/ipc.ts:39`: `editor_list_mods` → `/api/editor-list-mods`. Flat kebab-case, no hierarchy needed for Phase 1.
- **Deployed-web gate.** `parish --web` can run on a server where `mods/` is ephemeral and writes would be silently discarded. Gate editor route registration behind an env var `PARISH_ENABLE_EDITOR=1`. In deployed modes (Railway, containers), leave it unset so the editor 404s cleanly.

### Frontend — `apps/ui/src/`

New files:
```
apps/ui/src/
├── routes/editor/
│   ├── +page.svelte                  # Top-level editor shell
│   └── +page.ts                      # export const ssr = false;
├── lib/
│   ├── editor-ipc.ts                 # Editor command bindings (reuses `command` helper from ipc.ts)
│   └── editor-types.ts               # TS types mirroring Rust DTOs
├── stores/
│   └── editor.ts                     # Session, dirty flags, current selection, validation
└── components/editor/
    ├── EditorHeader.svelte           # Current mod, dirty dot, Reload, Save-all
    ├── ModBrowser.svelte             # Select mod from list
    ├── NpcList.svelte                # Left-pane list + search
    ├── NpcDetail.svelte              # Right-pane form (identity, home/workplace, knowledge)
    ├── IntelligenceSliders.svelte    # 6-dim slider control
    ├── ScheduleTimeline.svelte       # Read-only 24h SVG band per season/day_type (Phase 1b)
    ├── RelationshipsList.svelte      # Editable list with auto-reciprocity on save
    ├── LocationList.svelte
    ├── LocationDetail.svelte         # Description + live placeholder preview
    ├── ConnectionEditor.svelte       # Add/remove with atomic bidirectional write
    ├── ValidatorPanel.svelte         # Show ValidationReport, click to jump to field
    ├── SaveInspector.svelte          # Browse save files / branches / snapshots (Phase 1c)
    └── SnapshotView.svelte           # Read-only pretty-print of a GameSnapshot
```

Modify:
- `apps/ui/src/lib/ipc.ts` — export the internal `command<T>(...)` helper (currently private at line 33) so `editor-ipc.ts` can reuse the same transport logic without duplication
- `apps/ui/src/routes/+page.svelte` — add a menu entry / keyboard shortcut that `goto('/editor')`s

Follow `apps/ui/src/components/DebugPanel.svelte` as the style reference — lift
its `tab-bar`/`tab-btn` CSS classes verbatim. Follow `SavePicker.svelte` for the
save inspector's modal/list layout.

### Validation semantics

`editor_validate()` should catch:
- All existing `WorldGraph::validate()` failures (orphans, non-bidirectional edges, bad targets)
- NPC `home` / `workplace` reference a nonexistent location
- NPC `relationships[].target_id` references a nonexistent NPC
- NPC `seasonal_schedule[].entries[].location` references a nonexistent location
- NPC schedule hour overlap or gaps within a variant (warning, not error)
- Location `associated_npcs` references a nonexistent NPC
- Duplicate NPC/location ids (hard error)

Report issues as a flat `Vec<ValidationIssue>` with `doc`, `field_path` (e.g.
`"npcs[3].relationships[1].target_id"`), severity, and message. The UI lets the
designer click an issue to jump to the relevant field.

### Stretch goals for Phase 1 (cheap wins that reuse existing code)

Push these into Phase 1 if time allows — each delivers outsized designer value
for minimal code because the underlying functions already exist:

1. **"Who is where at T?" query.** Pick season / day_type / hour, render a table
   of every NPC → scheduled location. Uses existing `Npc::desired_location` at
   `crates/parish-npc/src/lib.rs:151`. Two IPC commands, one table component.
2. **Location template preview.** Side-by-side view of a location's
   `description_template` source and three rendered variants
   (morning/clear, dusk/rain, night/fog). Reuses `render_description` —
   zero new backend logic.
3. **Anachronism self-check.** Run the existing anachronism detector against
   static text fields (`npcs[].personality`, `locations[].description_template`,
   `knowledge[]`) and surface hits as validator warnings. Catches accidental
   1820-anachronisms before they reach players.
4. **Coverage report.** For each location: which NPCs spend ≥1 hour there in
   any schedule variant. For each hour 0–23: gap detection across the NPC
   roster. Exposes dead zones instantly.

### Save-inspector scope (read-only for Phase 1c)

- List save `.db` files under the save directory (reuse `parish-persistence`)
- Per save: show branches table, with parent edges
- Per branch: list snapshots with game_time and real_time
- Per snapshot: deserialize the JSON `world_state` blob into `GameSnapshot` and render each section (clock, weather, player, NPCs, gossip network, conversation log) as a read-only tree view
- Export snapshot JSON to a file (useful for fixtures)

### Files to create / modify (Phase 1 checklist)

**Rust — new**
- `crates/parish-core/src/editor/mod.rs`
- `crates/parish-core/src/editor/types.rs`
- `crates/parish-core/src/editor/handlers.rs`
- `crates/parish-core/src/editor/persist.rs`
- `crates/parish-core/src/editor/save_inspect.rs`
- `crates/parish-core/src/editor/format.rs`
- `crates/parish-core/src/ipc/editor.rs`
- `crates/parish-tauri/src/editor_commands.rs`
- `crates/parish-server/src/editor_routes.rs`
- `crates/parish-server/tests/editor_routes.rs`

**Rust — modify**
- `crates/parish-core/src/lib.rs` — `pub mod editor;`
- `crates/parish-core/src/ipc/mod.rs` — `pub mod editor;`
- `crates/parish-npc/src/data.rs:24-36` — make `NpcFile` / `NpcFileEntry` + subtypes `pub` and derive `Serialize` (+ re-export from `parish-npc/src/lib.rs`)
- `crates/parish-world/src/graph.rs:130` — `fn validate` → `pub fn validate`
- `crates/parish-tauri/src/lib.rs` — add `editor: Mutex<EditorState>` to `AppState`, register handlers in `generate_handler!` at line 538
- `crates/parish-server/src/state.rs:94` — add `editor: Mutex<EditorState>` to `AppState`
- `crates/parish-server/src/lib.rs:130` — register editor routes behind `PARISH_ENABLE_EDITOR` gate

**Frontend — new**
- `apps/ui/src/routes/editor/+page.svelte`
- `apps/ui/src/routes/editor/+page.ts`
- `apps/ui/src/lib/editor-ipc.ts`
- `apps/ui/src/lib/editor-types.ts`
- `apps/ui/src/stores/editor.ts`
- `apps/ui/src/components/editor/*.svelte` (13 files listed above)
- `apps/ui/e2e/editor.spec.ts`

**Frontend — modify**
- `apps/ui/src/lib/ipc.ts` — export the internal `command<T>(...)` helper so `editor-ipc.ts` can reuse it
- `apps/ui/src/routes/+page.svelte` — add a menu entry / shortcut that `goto('/editor')`s

## Phase 2 — Core iteration improvements

- **Schedule editor**: hour-grid timeline per NPC; drag to resize slots; season/day_type variant switcher; conflict detection
- **Relationship matrix**: bidirectional editing (editing A→B auto-updates B→A)
- **Location map editor**: SVG canvas using lat/lon; drag to move; draw connections visually (reuses `MapPanel.svelte` projection)
- **Placeholder live preview**: render `description_template` with chosen time-of-day / weather / NPCs (reuse `render_description`)
- **Content tables**: festivals, encounters, anachronisms, pronunciations, transport — flat tabular editors
- **Config editors**: `ui.toml`, `loading.toml`, `parish.toml` with live palette preview
- **GameTestHarness preview**: run a test script against in-memory edits without writing to disk
- **"Who is where at T?"** query: pick season / day / hour, see all NPCs mapped to their scheduled location

## Phase 3 — Advanced

- Social graph visualization (force-directed NPC relationship graph)
- Editable save inspector (patch snapshot fields)
- Undo stack / diff view vs. on-disk
- Bulk NPC generation helpers (tied to the future scalable NPC design in `docs/design/scalable-npc-data-design.md`)
- Hot-reload a running game session with edited mod data
- Designer content guide under `docs/designer/`

## Gotchas

- **Module ownership.** Editor logic lives in `parish-core`. Tauri and Axum wrappers are thin. `CLAUDE.md` non-negotiable #1.
- **Mode parity.** Every editor feature works in both Tauri and web. The `ipc.ts` auto-detection at `apps/ui/src/lib/ipc.ts:29` handles this if backend commands are mirrored.
- **Running-game isolation is the central design rule.** The editor's `EditorState` is fully separate from the live gameplay `AppState` (`world`, `npc_manager`, `inference_queue`, autosave thread, tick loop). The editor operates on a **fresh in-memory copy loaded from disk**, never the live game. Closing the editor drops this state without touching gameplay. This prevents tick races, stale-data autosave writes, file/memory divergence, and broken "close editor" UX.
- **Don't use `GameMod::load` to read for editing.** It's all-or-nothing (one broken file aborts the whole mod) and it runs post-processing (relationship reciprocation, reaction template merge) that would pollute source files on save. The editor uses a granular per-file loader that collects errors per file.
- **`brief_description` round-trip.** See Schema exposure. Must preserve raw `Option<String>` from disk; never write the synthesized fallback.
- **File-locking.** `GameMod::load` reads each file into memory and releases the handle (`crates/parish-core/src/game_mod.rs:427`). The editor can write `npcs.json` / `world.json` while a game is running with no collision. The running game's autosave writes to its SQLite `.db`, not to mod JSON — also no collision.
- **Atomic writes.** Always `std::fs::write` to `foo.json.tmp`, then `std::fs::rename` to `foo.json`. Prevents partial writes if the editor crashes mid-save.
- **JSON determinism.** Serde's default map ordering is unstable. Use `BTreeMap` in serializable types where key order matters, `serde_json::to_string_pretty` with 2-space indent, and a single `write_json_deterministic` helper in `editor/format.rs`. Goal: editing and re-saving `mods/rundale/npcs.json` unchanged produces an **empty** `git diff`.
- **Save-format evolution.** `GameSnapshot` uses `#[serde(default)]` for backward compat. The inspector parses into `serde_json::Value` first, then tries `GameSnapshot`, falling back to raw JSON view on failure.
- **Validation error positions.** Editor edits structured data, not text. Translate serde errors into `field_path` (e.g. `npcs[3].relationships[1].target_id`) so the UI can jump to the field.
- **Bidirectional edge enforcement** must be atomic at the frontend level: when `LocationDetail` adds `A→B`, it must also insert `B→A` in the `locations` array before the save command fires. Same for deletion. Never leave it to the user.
- **Deployed-web gate.** Gate editor route registration in `parish-server` behind `PARISH_ENABLE_EDITOR=1` so a deployed instance on Railway / containers doesn't expose writable editor endpoints against an ephemeral filesystem.
- **Test coverage.** `CLAUDE.md` rule #3 requires ≥90%. Editor handler and persist functions need inline unit tests.
- **Frontend routes.** SvelteKit with `adapter-static` + `fallback: 'index.html'` + `ssr = false` means new routes work unchanged. Set `export const ssr = false` in `apps/ui/src/routes/editor/+page.ts` to match the layout.

## Verification

**Backend unit tests** (`cargo test -p parish-core editor::`):
- `validate_snapshot` reports the correct `field_path` for: missing relationship target, missing NPC home, schedule location orphan, non-bidirectional edge, duplicate location id, lat/lon out of range
- `persist::save_npcs` round-trip: build an `EditorModSnapshot` in a `tempfile::TempDir` → save → re-load with the granular loader → assert equality
- `format::write_json_deterministic` is idempotent: write twice, bytes are identical

**Upstream schema test** (`cargo test -p parish-npc`):
- Load the real `mods/rundale/npcs.json`, re-serialize via the newly `Serialize`-derived `NpcFile`, deserialize again, assert structural equality (modulo the known reciprocal-relationship reshuffle). This catches drift between editor and game loader — the single most important schema test.

**Integration tests** (`cargo test -p parish-server`):
- New `crates/parish-server/tests/editor_routes.rs`: start an in-process server with a temp mods dir, hit each `/api/editor-*` endpoint, assert JSON shape
- Assert editor routes return 404 when `PARISH_ENABLE_EDITOR` is unset

**Frontend e2e** (Playwright, `apps/ui/e2e/editor.spec.ts`):
- Against Tauri mocks: navigate to `/editor`, open `rundale`, edit an NPC name, see dirty indicator, save, verify mock IPC payload
- Trigger a validation error, verify it appears in the validator panel
- Browse a save snapshot, verify NPC and clock data render
- Against real `parish --web` (new spec): full happy-path, catches mode parity regressions

**The critical acceptance test (manual, one-shot):**
> **Open `mods/rundale/npcs.json` in the editor, save without changes, run `git diff`. The diff MUST be empty.** Any non-empty diff indicates schema drift — the editor's round-trip is not clean and will silently corrupt source files over time. This invariant must hold before the feature ships.

**Standard checks**
- `just check` (fmt + clippy + tests) — must pass
- `just verify` — must pass
- `just ui-check` (svelte-check) — must pass
- Coverage via `cargo tarpaulin` stays ≥90%
- Manual smoke: `cargo tauri dev` → `/editor` → full happy-path edit & save
- Manual smoke: `PARISH_ENABLE_EDITOR=1 cargo run -- --web 3001` → browser → same happy-path

Because the editor is a dev tool and not a gameplay feature, `/prove` is not
required. Standard `/check` + `/verify` + the Playwright e2e and the critical
acceptance test above cover it.
