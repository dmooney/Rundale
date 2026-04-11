# ADR-003: SQLite WAL Persistence

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Rundale requires a persistence system that meets several demanding requirements:

- **Crash safety**: The player never explicitly saves. If the game crashes or the player force-quits, no meaningful progress should be lost.
- **No gameplay stutter**: Persistence writes must never block the game loop or cause visible hitches in the TUI.
- **Continuous autosave**: Every state mutation (NPC movement, relationship changes, dialogue events, weather shifts) must be durably recorded.
- **Branching timelines**: The save system must support forking and loading alternate timelines (see ADR-004).
- **Background operation**: Periodic compaction/snapshots must run on a background thread without affecting gameplay.

The game engine is async (Tokio), but the persistence layer must handle a sync database driver.

## Decision

Use **SQLite in WAL (Write-Ahead Log) mode** as the persistence backend, with a three-layer write strategy:

**Layer 1 -- Journal (Real-time)**
- Every state mutation is appended to the journal as it occurs
- Append-only writes are cheap and sequential
- This is the crash recovery net: on restart, replay the journal from the last snapshot

**Layer 2 -- Snapshot (Periodic)**
- Full compaction of current world state every ~30-60 seconds
- Runs on a background thread via `tokio::task::spawn_blocking` to avoid blocking the async runtime
- This is the "clean" save point that the journal builds on top of

**Layer 3 -- Branch (Named reference)**
- A branch is a snapshot plus its journal tail
- Fork copies the current snapshot and starts a new journal
- Load switches to a different snapshot and journal
- Each branch maintains its own independent clock

SQLite is configured in WAL mode to allow concurrent reads (game logic) alongside writes (journal appends, snapshot compaction). All rusqlite calls are wrapped in `spawn_blocking` since rusqlite is a synchronous driver.

## Consequences

**Positive:**

- Cheap, sequential append-only writes for the journal minimize I/O overhead
- Crash recovery is straightforward: load last snapshot, replay journal tail
- Single-file database is portable and easy to back up
- WAL mode allows concurrent reading and writing without blocking
- Background snapshot compaction keeps the journal bounded in size
- SQLite is battle-tested, well-tooled, and requires no external server

**Negative:**

- SQLite is synchronous: all database calls must be wrapped in `tokio::task::spawn_blocking`, adding boilerplate
- WAL mode has limitations: only one writer at a time, readers see a consistent snapshot but may lag slightly behind writes
- Journal replay on crash recovery could be slow if the snapshot interval is too long
- Single-writer constraint means snapshot compaction and journal writes must be coordinated
- SQLite's row-level locking is coarse compared to dedicated key-value stores

## Alternatives Considered

- **Flat JSON files**: Simple but offers no crash safety. A partial write during a crash could corrupt the entire save file. No transactional guarantees.
- **sled / RocksDB**: Embedded key-value stores with better write throughput, but more complex to operate, less tooling for inspection and debugging, and overkill for the expected data volume.
- **In-memory only with periodic dump**: Fast but any crash between dumps loses all progress. Unacceptable for a game that promises invisible persistence.
- **PostgreSQL / other server DB**: Requires an external service, adds deployment complexity, and provides no benefit for a single-player local game.

## Related

- [docs/design/persistence.md](../design/persistence.md)
- [ADR-004: Git-Like Branching Save System](004-git-like-branching-saves.md)
