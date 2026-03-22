# Plan: Phase 4 — Persistence

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**

## Goal

Implement invisible, continuous persistence using SQLite in WAL mode: real-time journal, periodic snapshots, and a git-like branching save system so the player never thinks about saving.

## Prerequisites

- Phase 3 complete: multiple NPCs with state (relationships, memory, mood) worth persisting
- WorldState, NpcManager, and GameClock are serializable

## Tasks

1. **Design SQLite schema in `src/persistence/mod.rs`**
   - Table `branches`: `id INTEGER PRIMARY KEY`, `name TEXT UNIQUE`, `created_at TEXT`, `parent_branch_id INTEGER NULL`
   - Table `snapshots`: `id INTEGER PRIMARY KEY`, `branch_id INTEGER REFERENCES branches`, `game_time TEXT`, `real_time TEXT`, `world_state BLOB` (JSON or zstd-compressed JSON)
   - Table `journal_events`: `id INTEGER PRIMARY KEY`, `branch_id INTEGER`, `sequence INTEGER`, `after_snapshot_id INTEGER REFERENCES snapshots`, `event_type TEXT`, `event_data TEXT` (JSON), `game_time TEXT`
   - Index on `journal_events(branch_id, after_snapshot_id, sequence)`

2. **Implement database initialization**
   - `Database` struct: `pool: rusqlite::Connection` (single-writer)
   - `fn Database::open(path: &Path) -> Result<Self>` — open/create SQLite file, `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`
   - `fn Database::migrate(&self) -> Result<()>` — create tables if not exist, version check for future migrations
   - Create default branch "main" on first run

3. **Define `WorldEvent` enum in `src/persistence/mod.rs`**
   - Variants: `NpcMoved { npc_id, from, to }`, `NpcMoodChanged { npc_id, mood }`, `RelationshipChanged { npc_a, npc_b, delta }`, `DialogueOccurred { npc_id, player_said, npc_said }`, `WeatherChanged { new_weather }`, `PlayerMoved { from, to }`, `MemoryAdded { npc_id, content }`, `ClockAdvanced { minutes }`
   - Derive `Serialize, Deserialize` on all variants
   - `fn WorldEvent::event_type(&self) -> &str` — returns discriminant name for the `event_type` column

4. **Implement journal system**
   - `fn Database::append_event(&self, branch_id: i64, snapshot_id: i64, event: &WorldEvent) -> Result<()>` — INSERT with auto-incrementing sequence
   - `fn Database::events_since_snapshot(&self, branch_id: i64, snapshot_id: i64) -> Result<Vec<WorldEvent>>` — SELECT ordered by sequence
   - Wrap in `spawn_blocking` for async compatibility

5. **Implement snapshot system**
   - Add `Serialize, Deserialize` derives to `WorldState`, `Npc`, `Relationship`, `ShortTermMemory`, `GameClock`
   - `fn Database::save_snapshot(&self, branch_id: i64, world: &WorldState, npcs: &NpcManager) -> Result<i64>` — serialize to JSON, INSERT into snapshots, return snapshot_id
   - `fn Database::load_latest_snapshot(&self, branch_id: i64) -> Result<Option<(i64, WorldState, NpcManager)>>` — SELECT most recent snapshot for branch, deserialize
   - Optional: compress snapshot blob with `zstd` crate (add as optional dependency)

6. **Implement periodic snapshot background task**
   - `fn spawn_autosave(db: Arc<Database>, world: Arc<RwLock<WorldState>>, npcs: Arc<RwLock<NpcManager>>, branch_id: Arc<AtomicI64>) -> JoinHandle<()>`
   - `tokio::spawn` loop: sleep 45 seconds, acquire read locks, snapshot, log success via tracing
   - Must not block the game loop; uses `spawn_blocking` internally

7. **Implement journal replay**
   - `fn replay_journal(world: &mut WorldState, npcs: &mut NpcManager, events: &[WorldEvent])` — apply each event to in-memory state in sequence
   - This reconstructs state from last snapshot + journal tail

8. **Implement `/save` command**
   - Trigger immediate snapshot (same as autosave, but player-initiated)
   - Print confirmation: "Game saved."

9. **Implement `/quit` command**
   - Snapshot current state, flush journal
   - Set `app.should_quit = true` to exit main loop
   - Terminal restore happens in existing cleanup code

10. **Implement `/fork <name>` command**
    - `fn Database::fork_branch(&self, current_branch: i64, new_name: &str, world: &WorldState, npcs: &NpcManager) -> Result<i64>` — INSERT new branch row, save snapshot under new branch, return new branch_id
    - Switch active branch_id to the new branch
    - Print: "Forked to branch '{name}'."

11. **Implement `/load <name>` command**
    - `fn Database::load_branch(&self, name: &str) -> Result<Option<(i64, WorldState, NpcManager)>>` — find branch by name, load latest snapshot, replay journal
    - Replace in-memory WorldState and NpcManager
    - Switch active branch_id
    - Print: "Loaded branch '{name}'. {game_date}, {time_of_day}."

12. **Implement `/branches` command**
    - `fn Database::list_branches(&self) -> Result<Vec<BranchInfo>>`
    - `BranchInfo` struct: `id: i64`, `name: String`, `created_at: String`, `latest_game_time: Option<String>`
    - Render as table in TUI text log

13. **Implement `/log` command**
    - `fn Database::branch_log(&self, branch_id: i64) -> Result<Vec<SnapshotInfo>>`
    - `SnapshotInfo`: `id: i64`, `game_time: String`, `real_time: String`
    - Render as chronological list in TUI

14. **Wrap all rusqlite calls in `spawn_blocking`**
    - Create `AsyncDatabase` wrapper: `struct AsyncDatabase { inner: Arc<Mutex<Database>> }`
    - Each method: `async fn append_event(...) { let db = self.inner.clone(); spawn_blocking(move || db.lock().unwrap().append_event(...)).await? }`

15. **Write tests**
    - `test_roundtrip_save_load`: create world state, snapshot, load, assert equality
    - `test_journal_replay`: save snapshot, append 5 events, replay, assert final state matches expected
    - `test_fork_creates_independent_branch`: fork, modify original, load fork, assert fork state unchanged
    - `test_branch_listing`: create 3 branches, list, assert all present
    - `test_concurrent_autosave`: run autosave task alongside mock game loop, verify no deadlocks (timeout test)
    - All tests use `tempfile::NamedTempFile` for database path

## Design References

- [Persistence](../design/persistence.md)

## Key Decisions

- [ADR-003: SQLite WAL Persistence](../adr/003-sqlite-wal-persistence.md)
- [ADR-004: Git-Like Branching Saves](../adr/004-git-like-branching-saves.md)

## Acceptance Criteria

- Game state persists across `/quit` and re-launch: NPC positions, relationships, memories, clock, weather all restored
- `/save` creates an immediate snapshot; `/load main` restores it
- `/fork test` creates an independent branch; changes on `main` do not affect `test`
- `/branches` lists all branches with timestamps
- Autosave runs every ~45s without visible stutter in the TUI
- Journal events are appended in real time; crash recovery replays journal tail
- `cargo test` passes all persistence round-trip and concurrency tests

## Resolved Issues

- **SQLite file per branch vs. single file**: Use a **single file with branch-tagged rows** as originally planned. This simplifies file management, enables cross-branch queries (e.g., listing all branches), and avoids filesystem clutter. Branch isolation is achieved via the `branch_id` column on journal and snapshot tables.
- **Snapshot compression**: **Skip `zstd` for now**. The initial NPC count (8-12 in Phase 3, up to 30-50 in Phase 5) produces snapshots well under 1MB. Adding a native dependency for compression is premature. If snapshot sizes exceed 10MB during Phase 5 scaling, add `zstd` compression behind a feature flag at that point.
- **Maximum journal size before compaction**: Set a threshold of **1000 journal entries** per branch before triggering automatic snapshot compaction. This balances write performance (append-only is fast) against recovery time (replaying >1000 entries on load is slow). Compaction runs on a background `spawn_blocking` task during autosave. The threshold is a constant that can be tuned via testing.
