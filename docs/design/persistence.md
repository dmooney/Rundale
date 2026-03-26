# Persistence & Save System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADRs: [003](../adr/003-sqlite-wal-persistence.md), [004](../adr/004-git-like-branching-saves.md)
>
> **Status: Implemented** (Phase 4)

## Philosophy

The player never thinks about saving. They quit whenever they want, load whenever they want, and the world is exactly where they left it. Saving is continuous and invisible, like Minecraft.

## Architecture: Three Layers

### 1. Journal (Real-time)

- Every state mutation (NPC moved, relationship updated, dialogue happened, weather shifted) appended as a `WorldEvent`
- Append-only, cheap writes via `Database::append_event()`
- This is the crash recovery net — on restart, events are replayed from the last snapshot

### 2. Snapshot (Periodic)

- Full compaction of current world state every ~45 seconds via autosave
- `GameSnapshot` captures only dynamic state (player location, weather, text log, clock state, all NPC states)
- Static data (world graph, location map) is loaded from data files, not persisted in snapshots
- Autosave runs during the command loop, not on a background thread

### 3. Branch (Named Reference)

- A branch = a snapshot + its journal tail
- Fork copies the current snapshot and starts a new journal under a new branch
- Load switches to a different branch's latest snapshot and replays its journal
- Each branch maintains its own independent clock — no time passes in unplayed branches

## Git-Like Branching Model

| Game Concept | Git Analog       |
|-------------|------------------|
| Journal     | Working directory |
| Snapshot    | Commit           |
| Branch      | Branch           |
| Fork        | `git checkout -b` |
| Load        | `git checkout`   |

Additional behaviors:

- Autosave on quit (snapshot before exit)
- Autosave every 45 seconds during command loop
- Auto-save current branch before switching to a different branch on `/load`

## Storage

SQLite files stored in the `saves/` directory in WAL mode with branch-tagged rows. Each save file is an independent DAG of branches. On startup, a Papers Please-style picker displays all save files with their nested branch trees, allowing the player to continue an existing game or start fresh.

### Schema

```sql
branches(id, name UNIQUE, created_at, parent_branch_id)
snapshots(id, branch_id, game_time, real_time, world_state JSON)
journal_events(id, branch_id, sequence, after_snapshot_id, event_type, event_data JSON, game_time)
```

Index: `idx_journal_branch_snap_seq ON journal_events(branch_id, after_snapshot_id, sequence)`

### Key Types

- **`GameSnapshot`** — Serializable capture of all dynamic game state (clock, player, weather, NPCs)
- **`ClockSnapshot`** — Game time + speed factor + paused flag (avoids serializing `Instant`)
- **`NpcSnapshot`** — Full NPC state mirror for serialization
- **`WorldEvent`** — Tagged enum of all state mutations (PlayerMoved, NpcMoodChanged, etc.)
- **`Database`** — Synchronous SQLite handle with CRUD methods
- **`AsyncDatabase`** — Async wrapper using `tokio::task::spawn_blocking`

### Player Commands

| Command | Effect |
|---------|--------|
| `/save` | Immediate snapshot to current branch |
| `/quit` | Snapshot + exit |
| `/fork <name>` | Snapshot, create new branch, switch to it |
| `/load` | Show save picker (switch save file or start new game) |
| `/load <name>` | Auto-save current branch (if different), load target branch |
| `/branches` | List all branches with active marker |
| `/log` | Show snapshot history for current branch |

## Related

- [Player Input](player-input.md) — /save, /fork, /load, /branches, /log commands
- [ADR 003: SQLite WAL for Persistence](../adr/003-sqlite-wal-persistence.md)
- [ADR 004: Git-Like Branching Saves](../adr/004-git-like-branching-saves.md)

## Source Modules

- [`src/persistence/mod.rs`](../../src/persistence/mod.rs) — Module root, re-exports
- [`src/persistence/database.rs`](../../src/persistence/database.rs) — Database, AsyncDatabase, schema, CRUD
- [`src/persistence/snapshot.rs`](../../src/persistence/snapshot.rs) — GameSnapshot, ClockSnapshot, NpcSnapshot
- [`src/persistence/journal.rs`](../../src/persistence/journal.rs) — WorldEvent enum, replay logic
- [`src/persistence/picker.rs`](../../src/persistence/picker.rs) — Save file discovery, picker display, startup/load selection
