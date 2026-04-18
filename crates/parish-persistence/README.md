# parish-persistence

SQLite persistence for saves, snapshots, journals, and branch history.

## Purpose

`parish-persistence` provides the durable storage layer used by all runtimes.
It supports branch-style saves, journal replay, and snapshot restore flows.

## Key modules

- `database` — schema access and core DB operations.
- `snapshot` — snapshot serialization/deserialization.
- `journal` / `journal_bridge` — event journal writing and replay.
- `picker` — save-file and branch selection helpers.

## Notes

Uses SQLite in WAL mode via `rusqlite` for reliability and simple deployment.
