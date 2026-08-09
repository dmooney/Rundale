# Implementation plan: bug-report-tool

Ordered, one commit per step (conventional-commit prefix in parens).

1. **(feat) Core payload + config + composition.** Add
   `parish-core/src/ipc/bug_report.rs`: `BugReportRequest`, `BugContext`,
   `BugReportResult`, `BugReportError`, `GitHubBugConfig::from_env`, and the
   pure `compose_issue_body`. Declare `mod bug_report;` in `ipc/mod.rs` and
   re-export the public types. Unit tests for composition (sections, conditional
   screenshot markdown, context rendering) and config precedence + dry-run.
   → covers C1, C2.

2. **(feat) Core orchestration.** Add async `create_bug_report` (GitHub Contents
   API screenshot upload → Issues API create; dry-run/no-token → write bundle to
   `resolve_user_data_dir(app)/bug-reports/<id>/` and return `created:false`).
   Unit test the dry-run/disk path with no network. → covers C3.

3. **(feat) Tauri wiring.** `submit_bug_report` command + `do_submit_bug_report`
   in `commands.rs` (snapshot gather, screenshot decode/round-trip, flag check),
   register in `lib.rs` invoke_handler + `command_registry.rs`, and add the
   `POST /api/submit-bug-report` handler + route to `mcp_bridge.rs` (+ its parity
   test). → contributes C4.

4. **(feat) Server wiring.** `submit_bug_report` route + `do_*` in
   `parish-server/src/routes.rs`, register in `lib.rs` + `route_registry.rs`.
   Confirm `cargo test -p parish-core --test wiring_parity` passes. → completes C4.

5. **(feat) MCP tool.** `translate_file_bug` + `parish_file_bug` `ToolDef` in
   `parish-mcp/src/tools.rs`; update the tool-name pin test; update the MCP
   tables in `AGENTS.md` and `parish-mcp/README.md`. → covers C5.

6. **(feat) Frontend.** `types.ts` + `ipc.ts` plumbing, `stores/bugReport.ts`,
   `BugReportModal.svelte` (reuse SavePicker CSS + `captureScreen()` + toast),
   toolbar 🐛 in `StatusBar.svelte`, per-record 🐛 in `DebugInferenceTab`,
   `DebugEventsTab`, `DebugConversationsTab`. Mount modal in `routes/+page.svelte`.
   Vitest for store/submit wiring. → covers C7, C8.

7. **(docs) README + flag doc.** Update README feature list; note the
   `bug-report` flag and env vars (`PARISH_BUG_REPORT_TOKEN`/`_REPO`/`_DRY_RUN`).

## Tests to add/update

- `parish-core`: `bug_report` unit tests (C1–C3); `wiring_parity` (C4).
- `parish-mcp`: translate test + tool-name pin (C5).
- `parish-tauri`: mcp_bridge route-table parity test includes the new route.
- UI: vitest for the bug-report store/modal.
- Playwright baselines regenerated (`just ui-e2e -- -u`) for the new toolbar
  button + modal (rule #10).

## Proof

- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/proofs/play_bug-report-tool.txt` → state setup.
- `parish-mcp-backend.sh start` with `PARISH_BUG_REPORT_DRY_RUN=1` → call
  `mcp__parish__parish_file_bug` → capture transcript + composed body (C6).
- UI screenshots: toolbar 🐛 + open modal (C7); modal from a debug record (C8).
- Assemble `.proofs/bug-report-tool/` (evidence.md + judge.md), `just agent-check`,
  `just attach-proof bug-report-tool`.
