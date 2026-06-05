# parish-client — Technical Debt

## Open

| ID     | Category       | Severity | Location                                                         | Description                                                                                                                                                                                                                                                                                                                                                                                          |
| ------ | -------------- | -------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Weak Tests     | P2       | `src/client.rs:1-148`, `src/repl.rs:1-49`, `src/session.rs:1-27` | The thin CLI client has few crate-local tests. `render.rs` gained tests for travel pluralization (commit b76ff2b6, #1156), but `client.rs`, `repl.rs`, and `session.rs` remain untested. Add tests for command body serialization (`camelCase` fields), session cookie persistence, and error rendering before expanding the public CLI surface. (note: partial — render.rs covered, 3 files remain) |
| TD-002 | API Drift      | P2       | `src/client.rs:15-64`                                            | Wire response structs manually mirror `parish-server::sync_types::CommandResponse` and state payloads. Add a compatibility test or shared type strategy so server response changes do not silently break the CLI.                                                                                                                                                                                    |
| TD-003 | Config Hygiene | P3       | `src/session.rs:4-10`                                            | Session cookie storage falls back to `$HOME/parish/session` when platform state/data dirs are unavailable. Document and test the fallback, or route it through the persistence path helpers if the CLI starts sharing more runtime state.                                                                                                                                                            |

## In Progress

_(none)_

## Done

_(none)_

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.

## Discovery note

2026-06-04 audit: 3 Open items reviewed, 0 migrated to Done, 0 anchors corrected. TD-001 is partial (render.rs tests added in b76ff2b6/#1156; client.rs, repl.rs, session.rs still untested). TD-002 and TD-003 unchanged.

## Issue tracking

2026-06-04 audit: open items in this file are tracked under epic(s) #1202 (Test coverage & type-drift), #1203 (Runtime path/config & scaling).
