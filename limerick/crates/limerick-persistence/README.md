# limerick-persistence

SQLite persistence for saves, snapshots, journals, and branch history.

## Purpose

`limerick-persistence` provides the durable storage layer used by all runtimes.
It supports branch-style saves, journal replay, and snapshot restore flows.

## Key modules

- `database` — schema access and core DB operations, including the turn
  journal's `requests` and `transcript_events` tables (one write transaction
  per journal call; see `docs/design/portable-turn-api.md` §6.1).
- `snapshot` — snapshot serialization/deserialization.
- `journal` / `journal_bridge` — event journal writing and replay.
- `picker` — save-file and branch selection helpers.

## Notes

Uses SQLite in WAL mode via `rusqlite` for reliability and simple deployment.
