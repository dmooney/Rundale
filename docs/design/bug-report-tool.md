# Design: In-app bug-report tool

## What the player / agent experiences

A tester clicks a 🐛 button on the top toolbar, types a short title and
description, and hits submit. Behind the scenes the app captures a screenshot,
pulls the recent logs and current world state out of the running session, and
opens a well-formed GitHub issue on `dmooney/rundale` with all of it attached —
returning the issue URL in a toast. Every record in the debug panel (an
inference call, a game/debug event, a conversation exchange) also gets its own
inline 🐛 button that opens the same modal pre-filled with that record as extra
context. Auto-QA agents get the identical capability through a new
`parish_file_bug` MCP tool, so a bot that notices something wrong can file a
real, reproducible issue for a fix-agent to pick up.

## Affected subsystems (by crate)

- **`parish-core`** (`src/ipc/bug_report.rs`, new) — the entire orchestration:
  payload structs, `GitHubBugConfig::from_env`, the pure `compose_issue_body`,
  and the async `create_bug_report` (screenshot upload via the fixed
  `bug-evidence` GitHub Release asset API,
  issue creation via Issues API, dry-run/disk fallback). Backend-agnostic; uses
  the existing `snapshot_from_world` and `build_debug_snapshot`. Reuses the
  workspace `reqwest`, `serde_json`, `base64`, `uuid` deps — **no new deps**.
- **`parish-tauri`** — thin `submit_bug_report` command + `do_submit_bug_report`
  glue (gather snapshots from `AppState`, decode the data-URL screenshot or run
  the existing `request-screenshot` round-trip, call core); registered in
  `lib.rs`, `command_registry.rs`, and exposed on the MCP bridge.
- **`parish-server`** — `submit_bug_report` route + `POST /api/submit-bug-report`
  in `lib.rs` + `route_registry.rs`.
- **`parish-mcp`** — `parish_file_bug` tool translating to `submit_bug_report`.
- **`parish/apps/ui`** — `BugReportModal.svelte`, toolbar button in
  `StatusBar.svelte`, per-record buttons in the three debug tabs, a
  `bugReport` store, and `ipc.ts`/`types.ts` plumbing.

## Data-model changes

No game-state changes. New IPC payload structs only, all serde snake_case and
mirrored in `apps/ui/src/lib/types.ts`:

- `BugReportRequest { title, description, screenshot_data_url?, context? }`
- `BugContext { kind, label, detail: Value }`
- `BugReportResult { issue_url?, issue_number?, screenshot_url?, bundle_path?, created }`
- `BugReportError` (thiserror)

## Observable signal in the harness

This is a tooling/UI feature, not a gameplay rule, so the primary proof is:
(1) `cargo test -p parish-core bug_report` for the pure composition + config
logic, (2) the `wiring_parity` test for command/route registration, (3) a live
`parish_file_bug` MCP dry-run transcript showing the composed issue body
(`## Game state`, `## Recent logs` sections) and a `bundle_path`, and (4) UI
screenshots of the toolbar button + the modal opened from a debug record. The
`play_bug-report-tool.txt` fixture sets up real session state (location, time,
NPCs, dialogue, inference/debug events) so the captured report has content.

## Feature flag

Default-on with a kill-switch, per `AGENTS.md` §6 and the `flags.rs`
convention for shipped features: each `do_submit_bug_report` bails early when
`config.flags.is_disabled("bug-report")` is true. (`is_disabled` — not
`is_enabled` — so an unset flag means the feature is on.)

## Security / abuse notes

Filing creates outward-facing GitHub artifacts. Issue creation requires an
explicit token in the environment; with no token (or `PARISH_BUG_REPORT_DRY_RUN=1`)
the report is composed and written to disk instead — the safe default for CI,
agent-check, and the sandbox where `api.github.com` may be blocked. Auto-filed
issues carry `bug` + `agent-filed` labels for filtering. The screenshot is
uploaded to the stable `bug-evidence` Release as `<uuid>.png` (GitHub has no
REST issue-attachment endpoint) and the returned Release download URL is
embedded in the issue.
