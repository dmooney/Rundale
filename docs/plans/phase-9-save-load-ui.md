# Phase 9 — Save/Load UI Design Plan

## Context

Parish uses a git-like branching save system where each save is a named branch that can fork from others. The persistence backend is fully implemented (SQLite with journal + snapshot + branch model), but the Tauri GUI currently has no save/load UI — it only works in TUI/headless mode. This plan adds a full-screen overlay for browsing, loading, forking, and deleting save branches.

## Visual Design

### Layout: Full-screen overlay with two-panel split

```
┌──────────────────────────────────────────────────────────┐
│  ✕                    Timelines                          │
├─────────────────────┬────────────────────────────────────┤
│                     │                                    │
│   Branch Tree       │      Selected Branch Detail        │
│                     │                                    │
│   ● main ←active    │   ┌──────────────────────────┐    │
│   ├── what-if       │   │  "what-if"               │    │
│   │   └── deeper    │   │                          │    │
│   └── alternate     │   │  Baile an Phóna          │    │
│                     │   │  Spring · Morning · Rain │    │
│                     │   │  March 25, 1820          │    │
│                     │   │                          │    │
│                     │   │  Last played: 2 hours ago│    │
│                     │   │  Forked from: main       │    │
│                     │   └──────────────────────────┘    │
│                     │                                    │
│                     │   [ Load ]  [ Fork ]  [ Delete ]   │
│                     │                                    │
├─────────────────────┴────────────────────────────────────┤
│  Press Escape to close                                   │
└──────────────────────────────────────────────────────────┘
```

### Aesthetic

- Semi-transparent dark backdrop (`rgba(10, 10, 20, 0.92)`) over the game
- Panel uses `--color-panel-bg` with `--color-border` borders
- Title "Timelines" in accent gold, uppercase, letter-spaced (matching StatusBar style)
- Branch tree uses tree connector lines (`├──`, `└──`) rendered as styled SVG/CSS
- Active branch marked with a filled gold circle; others with hollow circles
- Selected branch highlighted with accent border glow
- Season-themed icon next to each branch (leaf/sun/maple/snowflake unicode)
- Fade-in animation on open (200ms opacity transition)

## Implementation

### 1. Rust Backend — New Tauri IPC Commands

**File: `src-tauri/src/commands.rs`**

Add 4 new commands + supporting types:

```rust
/// Summary of a save branch for the UI.
#[derive(serde::Serialize, Clone)]
pub struct BranchSummary {
    pub id: i64,
    pub name: String,
    pub created_at: String,           // ISO 8601
    pub parent_branch_id: Option<i64>,
    pub parent_branch_name: Option<String>,
    pub is_active: bool,
    // Snapshot preview data (from latest snapshot):
    pub location_name: Option<String>,
    pub game_time: Option<String>,     // ISO 8601
    pub real_time: Option<String>,     // ISO 8601 (last played)
    pub season: Option<String>,
    pub time_of_day: Option<String>,
    pub weather: Option<String>,
    pub snapshot_count: u32,
}
```

Commands:
- `list_save_branches() -> Vec<BranchSummary>` — Lists all branches with preview data from their latest snapshot
- `load_branch(name: String) -> ()` — Auto-saves current branch, loads target branch (restores snapshot + replays journal), emits world-update + theme-update events
- `fork_branch(name: String) -> ()` — Snapshots current state, creates new branch, switches to it
- `delete_branch(name: String) -> Result<(), String>` — Deletes a branch (refuses if it's the active branch or "main")

For `list_save_branches`: query `list_branches()`, then for each branch call `load_latest_snapshot()` to extract preview data (location name via world graph lookup, season/time_of_day/weather from ClockSnapshot deserialization).

**File: `src-tauri/src/lib.rs`**

- Add `AsyncDatabase` to `AppState` (it's currently missing from the Tauri AppState — the persistence layer exists but isn't wired into the GUI)
- Add `active_branch_id: Mutex<i64>` and `latest_snapshot_id: Mutex<i64>` to `AppState`
- Register new commands in `generate_handler!`
- Initialize database on startup (open `parish_saves.db`, create "main" branch if needed, save initial snapshot)

### 2. TypeScript Types

**File: `ui/src/lib/types.ts`** — Add:

```typescript
export interface BranchSummary {
    id: number;
    name: string;
    created_at: string;
    parent_branch_id: number | null;
    parent_branch_name: string | null;
    is_active: boolean;
    location_name: string | null;
    game_time: string | null;
    real_time: string | null;
    season: string | null;
    time_of_day: string | null;
    weather: string | null;
    snapshot_count: number;
}
```

### 3. IPC Wrappers

**File: `ui/src/lib/ipc.ts`** — Add:

```typescript
export const listSaveBranches = () => invoke<BranchSummary[]>('list_save_branches');
export const loadBranch = (name: string) => invoke<void>('load_branch', { name });
export const forkBranch = (name: string) => invoke<void>('fork_branch', { name });
export const deleteBranch = (name: string) => invoke<void>('delete_branch', { name });
```

### 4. Svelte Components

**New file: `ui/src/components/SaveLoadOverlay.svelte`**

Single component containing:
- Backdrop + panel container
- Left panel: branch tree (built from `parent_branch_id` relationships)
- Right panel: selected branch detail card + action buttons
- Fork dialog: inline text input for new branch name
- Delete confirmation: inline "Are you sure?" prompt
- Keyboard handling: Escape to close, Enter to load selected, arrow keys to navigate

State:
```typescript
let branches: BranchSummary[] = $state([]);
let selectedId: number | null = $state(null);
let forkInputVisible: boolean = $state(false);
let forkName: string = $state('');
let deleteConfirm: boolean = $state(false);
let loading: boolean = $state(false);
```

Props:
```typescript
let { open, onclose }: { open: boolean; onclose: () => void } = $props();
```

Tree rendering: Build a tree structure from flat `BranchSummary[]` using `parent_branch_id`. Render recursively with indentation + connector styling.

Detail card shows:
- Branch name (large, accent)
- Location name
- Game date + formatted time of day
- Season + weather badges
- "Last played: X ago" (relative time from `real_time`)
- "Forked from: parent_name" (if applicable)
- Snapshot count

Action buttons:
- **Load** (accent bg, only if not active branch) — calls `loadBranch()`, closes overlay
- **Fork** (border style) — shows inline name input, calls `forkBranch()`
- **Delete** (muted/red, not for "main" or active) — shows confirmation, calls `deleteBranch()`

### 5. Integration into Layout

**File: `ui/src/routes/+page.svelte`**

- Import `SaveLoadOverlay`
- Add `let saveLoadOpen = $state(false)`
- Add overlay to template: `<SaveLoadOverlay open={saveLoadOpen} onclose={() => saveLoadOpen = false} />`
- Add keydown listener for `Escape` (toggle off) and a trigger key (e.g., `F5` or a menu button)

**File: `ui/src/components/StatusBar.svelte`**

- Add a small "Timelines" button (or save icon) at the right side of the status bar that opens the overlay

### 6. Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `F5` or StatusBar button | Open save/load overlay |
| `Escape` | Close overlay |
| `↑` / `↓` | Navigate branch list |
| `Enter` | Load selected branch |
| `f` | Fork selected branch (shows name input) |
| `Delete` / `Backspace` | Delete selected branch (shows confirm) |

### 7. Database Initialization in Tauri

**File: `src-tauri/src/lib.rs`** in `run()`:

```rust
// After loading world + NPCs, before building AppState:
let db_path = data_dir.parent().unwrap_or(&data_dir).join("parish_saves.db");
let db = Database::open(&db_path).expect("Failed to open save database");
let async_db = AsyncDatabase::new(db);

// Find or create "main" branch
let main_branch = async_db.find_branch("main").await?;
let (branch_id, snapshot_id) = if let Some(b) = main_branch {
    let snap = async_db.load_latest_snapshot(b.id).await?;
    (b.id, snap.map(|(id, _)| id).unwrap_or(0))
} else {
    let id = async_db.create_branch("main", None).await?;
    (id, 0)
};
```

Add to `AppState`:
```rust
pub db: Arc<AsyncDatabase>,
pub active_branch_id: Mutex<i64>,
pub latest_snapshot_id: Mutex<i64>,
```

## Files to Modify

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Add `db`, `active_branch_id`, `latest_snapshot_id` to AppState; init DB in `run()`; register new commands |
| `src-tauri/src/commands.rs` | Add `BranchSummary` struct + 4 new commands |
| `ui/src/lib/types.ts` | Add `BranchSummary` interface |
| `ui/src/lib/ipc.ts` | Add 4 new IPC wrappers |
| `ui/src/components/SaveLoadOverlay.svelte` | **New file** — full overlay component |
| `ui/src/routes/+page.svelte` | Add overlay integration + keyboard shortcut |
| `ui/src/components/StatusBar.svelte` | Add "Timelines" trigger button |
| `src-tauri/Cargo.toml` | Ensure `parish-core` persistence module is accessible |

## Key Reuse

- `AsyncDatabase` from `src/persistence/database.rs` — all DB operations
- `GameSnapshot::capture()` from `src/persistence/snapshot.rs` — for save/fork
- `replay_journal()` from `src/persistence/journal.rs` — for load
- `compute_palette()` from `parish-core` — for theme refresh after load
- CSS variables from `theme.ts` — all overlay styling
- Existing component patterns (StatusBar button style, Sidebar details pattern)

## Verification

1. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` — all pass
2. `cd ui && npm test` — component tests pass
3. `cargo tauri dev` — open app, verify:
   - StatusBar shows "Timelines" button
   - Clicking it (or F5) opens the overlay with fade-in
   - "main" branch is listed and marked active
   - Escape closes overlay
   - Fork creates new branch visible in list
   - Load switches branches (world state updates)
   - Delete removes non-active branches
4. Write a test script for `--script` mode exercising `/save`, `/fork`, `/load`, `/branches`
5. Add unit tests for new Tauri commands (mock AppState)
6. Add Svelte component test for SaveLoadOverlay rendering
