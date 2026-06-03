# parish-client — Technical Debt

## Open

| ID     | Category       | Severity | Location                                                                               | Description                                                                                                                                                                                                                               |
| ------ | -------------- | -------- | -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Weak Tests     | P2       | `src/client.rs:1-148`, `src/render.rs:1-59`, `src/repl.rs:1-49`, `src/session.rs:1-27` | The thin CLI client has no crate-local tests. Add tests for response rendering, command body serialization (`camelCase` fields), session cookie persistence, and error rendering before expanding the public CLI surface.                 |
| TD-002 | API Drift      | P2       | `src/client.rs:15-64`                                                                  | Wire response structs manually mirror `parish-server::sync_types::CommandResponse` and state payloads. Add a compatibility test or shared type strategy so server response changes do not silently break the CLI.                         |
| TD-003 | Config Hygiene | P3       | `src/session.rs:4-10`                                                                  | Session cookie storage falls back to `$HOME/parish/session` when platform state/data dirs are unavailable. Document and test the fallback, or route it through the persistence path helpers if the CLI starts sharing more runtime state. |

## In Progress

_(none)_

## Done

_(none)_

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.
