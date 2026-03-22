# Persistence & Save System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADRs: [003](../adr/003-sqlite-wal-persistence.md), [004](../adr/004-git-like-branching-saves.md)

## Philosophy

The player never thinks about saving. They quit whenever they want, load whenever they want, and the world is exactly where they left it. Saving is continuous and invisible, like Minecraft.

## Architecture: Three Layers

### 1. Journal (Real-time)

- Every state mutation (NPC moved, relationship updated, dialogue happened, weather shifted) appended as it occurs
- Append-only, cheap writes
- This is the crash recovery net

### 2. Snapshot (Periodic)

- Full compaction of current world state every ~30-60 seconds
- Runs on a background thread — no gameplay stutter
- This is the "clean" save point

### 3. Branch (Named Reference)

- A branch = a snapshot + its journal tail
- Fork copies the current snapshot and starts a new journal
- Load switches to a different snapshot and journal
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

- Autosave on quit
- Background persistence thread on a dedicated CPU core

## Storage

SQLite in WAL mode. One database file per branch, or a single database with branch-tagged rows. The journal is the WAL.

## Related

- [Player Input](player-input.md) — /save, /fork, /load, /branches, /log commands
- [ADR 003: SQLite WAL for Persistence](../adr/003-sqlite-wal-persistence.md)
- [ADR 004: Git-Like Branching Saves](../adr/004-git-like-branching-saves.md)

## Source Modules

- [`src/persistence/`](../../src/persistence/) — SQLite save/load, WAL journal
