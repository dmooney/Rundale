# parish-client — Technical Debt

## Open

_(none — TD-003 resolved 2026-06-07 under #1203; see Done.)_

## In Progress

_(none)_

## Done

| ID     | Category       | Severity | Description                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------ | -------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Weak Tests     | P2       | `repl.rs` line-filter extracted into pure `should_skip`/`is_quit` helpers and unit-tested (blank/comment skip, all quit/exit sentinels, loop-filter ordering). With client.rs (5) + session.rs (5) already covered, the thin-client crate-local test gap is closed. (#1202)                                                                                                                                                             |
| TD-002 | API Drift      | P2       | Added a two-sided wire-compat guard: client tests deserialize a `sync_types::CommandResponse`-shaped JSON losslessly and pin the top-level key set; the server-side companion `sync_types::tests::command_response_wire_keys_match_client` pins the serialized keys. A rename on either side now fails CI. (#1202)                                                                                                                      |
| TD-003 | Config Hygiene | P3       | `session.rs` no longer uses an ad-hoc `$HOME/parish/session` fallback. The cookie path is resolved through `parish_persistence::paths::resolve_user_data_dir(DEFAULT_APP_NAME)` (rule #9), honouring `PARISH_USER_DATA_DIR`, with the fallback documented in the module header. Tests cover the env-override save→load round-trip plus empty/whitespace/missing-file cases through the real resolved path. (resolved 2026-06-07, #1203) |

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.
- **2026-06-06**: Re-audit vs current code. Resolved->Done: none. Still open: TD-001 (partial), TD-002, TD-003 (partial). Tracking epics re-opened: #1202, #1203.
- **2026-06-06 (#1202)**: Resolved TD-001 (repl.rs tested) and TD-002 (wire-compat test). Still open: TD-003 (#1203).
- **2026-06-07 (#1203)**: Resolved TD-003 — session-cookie path routed through `parish_persistence::paths` (rule #9), env-override + edge cases tested. This adds a `parish-persistence` dependency, so the crate is no longer dependency-free; the wire-compat guard from TD-002 is unaffected. No Open items remain.

## Discovery note

2026-06-04 audit: 3 Open items reviewed, 0 migrated to Done, 0 anchors corrected. TD-001 is partial (render.rs tests added in b76ff2b6/#1156; client.rs, repl.rs, session.rs still untested). TD-002 and TD-003 unchanged.

## Issue tracking

2026-06-04 audit: open items in this file are tracked under epic(s) #1202 (Test coverage & type-drift), #1203 (Runtime path/config & scaling).
2026-06-06 re-audit: TD-001/TD-002 tracked under re-opened #1202 (test coverage/type-drift); TD-003 under #1203 (runtime path/config).
