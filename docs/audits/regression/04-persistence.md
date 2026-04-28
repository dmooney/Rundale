# Regression Audit: Persistence

Scope: SQLite + WAL, append-only journal, autosave (45s), git-like branching
saves (`/save`/`/fork`/`/load`/`/branches`/`/log`), branch DAG visualization,
F5 save picker.

## 1. Sub-features audited

- SQLite + WAL journaling
- Append-only event journal
- Periodic snapshot compaction / 45-second autosave
- Branching saves: `/save`, `/fork`, `/load`, `/branches`, `/log`
- Branch DAG visualization (GUI save picker)
- F5 save picker UI

## 2. Coverage matrix

| Sub-feature | Unit | Integration / Fixture | Rubric | UI |
|---|---|---|---|---|
| SQLite + WAL config | `parish-persistence/src/database.rs:71-77` (in-source; sets `PRAGMA journal_mode=WAL`) | none | none | none |
| Append-only event journal | 100 in-source tests in `parish-persistence` (across `journal.rs`, `journal_bridge.rs`, `snapshot.rs`) | `parish-cli/tests/persistence_integration.rs:9-401` (10 tests including roundtrip, weather/text-log preservation, NPC state preservation, full-world-state) | none | none |
| Snapshot serialization | `parish-persistence/src/snapshot.rs` (in-source, with explicit comments at `:130` and `:447` warning about NPC LOD erasure regressions) | `persistence_integration.rs:333` `test_fork_preserves_npc_state`; fixture `testing/fixtures/test_persistence.txt` | none | none |
| Autosave (45s periodic) | none — no test simulates 45s elapsing or asserts autosave fires | none | none | none |
| `/save` `/fork` `/load` | `persistence_integration.rs:9` save+load roundtrip; `:37` fork-creates-independent-branch | fixture `test_persistence.txt` (10-step branch flow) | none | none |
| `/branches` `/log` | `persistence_integration.rs:68` branches-lists-all; `:85` log-shows-snapshots | fixture `test_persistence.txt` | none | none |
| Branch DAG visualization | none | none | none | **none** — `apps/ui/src/components/SavePicker.svelte` has no `.test.ts` colocated |
| F5 save picker UI | `parish-persistence/src/picker.rs:466-519` (in-source: parse_save_number, next_save_number, discover_saves) | none | none | none |
| Concurrent reader behavior under WAL | none — `parish-server/tests/isolation.rs:294` `debug_snapshot_no_deadlock_with_concurrent_readers` covers a different surface (in-memory snapshot, not SQLite WAL) | none | none | none |
| Schema migration / version upgrades | none discovered | none | none | none |

## 3. Strong spots

- Snapshot/restore correctness is exceptionally well covered: 10 dedicated
  integration tests in `persistence_integration.rs` plus a fixture, with
  explicit regression notes baked into snapshot.rs (`:130`, `:447`)
  guarding against the LOD-erasure bug class that has bitten before.
- 100 in-source unit tests in `parish-persistence` exercise journal
  read/write, snapshot encoding, the picker's filename arithmetic, and
  the lock file.
- `test_save_preserves_weather` and `test_save_preserves_text_log` show
  the team thinks about silent-loss bugs.

## 4. Gaps

- **[P0] Autosave (45-second timer) has no test.** No test simulates time
  elapsing and asserts a snapshot was written. Saves *are* the most
  unrecoverable failure mode in the engine; if autosave silently breaks,
  the player loses their session. Suggested: time-mocked unit test in
  `parish-persistence/src/snapshot.rs` named
  `test_autosave_writes_after_interval`, or a fixture that runs
  `/wait 45m` and asserts new snapshot row.
- **[P0] No SavePicker UI test.** `apps/ui/src/components/SavePicker.svelte`
  has no colocated `.test.ts` despite being a complex DAG-rendering
  component reachable via F5. Suggested: Vitest + @testing-library/svelte
  `SavePicker.test.ts` covering branch-tree layout and click-to-load.
- **[P0] Branch DAG layout has no test.** features.md mentions
  hierarchical layout and auto-zoom bbox; this is non-trivial geometry
  with no coverage. Suggested unit test in
  `apps/ui/src/lib/save-picker/dag.ts` (or wherever layout lives) named
  `test_dag_hierarchical_layout_handles_diamond_branches`.
- **[P1] WAL mode is set but not verified.** `database.rs:77` issues
  `PRAGMA journal_mode=WAL` but no test queries it back to confirm WAL
  is active. Easy to mis-string. Suggested unit test in `database.rs`:
  `test_database_uses_wal_journal_mode`.
- **[P1] Concurrent-read behavior under WAL is untested at the SQLite
  layer.** The mode is supposed to allow concurrent reads during writes;
  no test asserts this. Suggested integration test in
  `parish-persistence` spawning a writer task and a reader task.
- **[P1] No schema-migration test.** If the snapshot encoding changes
  shape, old saves silently fail to load. Suggested: stash a v1
  snapshot blob in `testing/fixtures/saves/` and add
  `test_loads_v1_snapshot_format`.
- **[P2] `/log` output formatting is untested.** Function returns text
  but only roundtrip tests verify; the user-facing string is not
  asserted. Suggested rubric in `eval_baselines.rs` once a stable
  fixture exists.

## 5. Recommendations

1. **Cover autosave** — this is the single biggest unrecoverable failure
   mode in the codebase with zero coverage. Even one mocked-clock test
   buys huge insurance.
2. **Test the SavePicker UI** — it's user-visible, complex, and gates
   the entire save-management flow.
3. **Add a WAL-mode assertion test** — one line of test code; closes a
   silent regression vector.
4. **Pin a v1 snapshot fixture** so format-evolution drift is caught
   before users lose saves.
