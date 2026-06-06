# parish-client — Technical Debt

## Open

| ID     | Category       | Severity | Location              | Description                                                                                                                                                                                                                               |
| ------ | -------------- | -------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-003 | Config Hygiene | P3       | `src/session.rs:4-10` | Session cookie storage falls back to `$HOME/parish/session` when platform state/data dirs are unavailable. Document and test the fallback, or route it through the persistence path helpers if the CLI starts sharing more runtime state. |

## In Progress

_(none)_

## Done

| ID     | Resolved   | Notes                                                                                                                                                                                                                                                                                                                                                          |
| ------ | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | 2026-06-06 | Added crate-local unit tests for the three untested files: `client.rs` (`CommandOpts` camelCase keys, `skip_serializing_if` omission, `CommandBody` flatten), `repl.rs` (extracted `is_skippable`/`is_quit` line classifiers + tests), `session.rs` (extracted `load_from`/`save_to` + tempfile round-trip / trim / empty / missing-file tests). 2 → 12 tests. |
| TD-002 | 2026-06-06 | Added a drift-guard test that round-trips a fully-populated `parish_server::sync_types::CommandResponse` through the client's wire `CommandResponse`; `parish-server` added as a dev-dependency so a server-side field rename fails the build instead of silently breaking the CLI.                                                                            |

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.
- **2026-06-06**: Closed TD-001 (weak tests) and TD-002 (API drift) — see Done table. TD-003 remains open.

## Discovery note

2026-06-04 audit: 3 Open items reviewed, 0 migrated to Done, 0 anchors corrected. TD-001 is partial (render.rs tests added in b76ff2b6/#1156; client.rs, repl.rs, session.rs still untested). TD-002 and TD-003 unchanged.

## Issue tracking

2026-06-04 audit: open items in this file are tracked under epic(s) #1202 (Test coverage & type-drift), #1203 (Runtime path/config & scaling).
