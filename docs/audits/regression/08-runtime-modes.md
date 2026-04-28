# Regression Audit: Multiple Runtime Modes

Scope: Tauri Desktop, Web Server (HTTP+WS), Headless CLI REPL, Script
Harness. Plus mode-parity (every IPC handler called from every entry
point — `CLAUDE.md` rule 2).

## 1. Sub-features audited

- Tauri Desktop (`cargo tauri dev`)
- Web Server (`--web [port]`, default 3001 — HTTP + WebSocket)
- Headless CLI REPL (`cargo run`)
- Script Testing harness (`--script <file>`)
- Mode parity (dep-level, enforced; wiring-level, convention)

## 2. Coverage matrix

| Mode | Startup smoke | Handler / IPC test | Integration | E2E |
|---|---|---|---|---|
| Tauri Desktop | none — `parish-tauri/` has **zero** test files (only `crates/parish-tauri/src/lib.rs` matches `#[test]`-grep but it's a no-op) | none — Tauri command handlers in `parish-tauri/src/commands.rs`, `editor_commands.rs` have no tests | none | manual only (`just run`) |
| Web Server | `parish-server/tests/auth_guard.rs` 7 tests (token validation, 401, 200); `parish-server/tests/legal_routes.rs` 5 tests (license/notice/markdown/GPL marker/bypass-fail-closed-guard); `parish-server/tests/security_headers.rs` 6 tests (CSP, HSTS, X-Frame, X-Content-Type, Referrer-Policy, Permissions-Policy) | `parish-server/tests/isolation.rs:18` admin-command 403, `:33` gameplay 200, `:44` admin-OK, `:67` admin-set-independence, `:118` debug-snapshot prompt-len-not-prompt-text, `:443` branch-name validation, `:186` second-WS-upgrade-409, `:294` no-deadlock-with-concurrent-readers (10 tests) | indirect via above | `apps/ui/e2e/*.spec.ts` (Playwright) — uses mocked Tauri IPC, not the actual web server |
| Headless CLI REPL | `parish-cli/tests/headless_script_tests.rs` 74 tests | n/a — REPL itself wraps the same core; per-command coverage rolls up to the slash-command audit | n/a | n/a |
| Script Testing harness | `parish-cli/tests/game_harness_integration.rs` 29 tests; `parish-cli/tests/eval_baselines.rs` 6 tests | `parish-cli/tests/headless_script_tests.rs:1179-1239` 11 `test_fixture_*_runs` smoke wrappers | every fixture in `testing/fixtures/` is run via this harness | n/a |
| Mode parity (dep-level) | `parish-core/tests/architecture_fitness.rs` 3 tests: `backend_agnostic_crates_do_not_pull_runtime_deps` (forbids tauri/axum/tower/wry/tao in leaf crates); `parish_cli_does_not_duplicate_parish_core_modules`; `no_orphaned_source_files` | n/a | n/a | n/a |
| Mode parity (wiring-level) | **none** — convention only per `CLAUDE.md` rule 2; no test asserts every IPC handler in `parish-core/src/ipc/handlers.rs` is called from both `parish-server` and `parish-tauri` | none | none | none |

## 3. Strong spots

- Architecture-fitness tests are the strongest piece of the harness:
  three enforced rules in `parish-core/tests/architecture_fitness.rs`
  catch dependency violations and orphan files automatically.
- `parish-server` has surprisingly mature security and isolation
  coverage (auth guard, security headers, admin-set isolation,
  WebSocket-upgrade locking, concurrent-reader deadlock prevention) —
  127 in-source tests + 4 integration test files. This is unusually
  good for a Rust web crate.
- The script harness is the workhorse: 29 + 74 + 6 fixture-driven tests
  give the headless mode the deepest behavioral coverage of any mode.

## 4. Gaps

- **[P0] Wiring parity has zero enforcement.** `CLAUDE.md` rule 2
  flags this explicitly: "Wiring parity (every IPC handler called from
  every entry point) is still convention." The IPC surface lives in
  `parish-core/src/ipc/{commands,handlers,editor,streaming}.rs`. A
  handler missing from `parish-server` or `parish-tauri` would ship
  silently. Suggested: extend `architecture_fitness.rs` with
  `every_ipc_handler_is_wired_from_each_entry_point` that grep-asserts
  symbol references in both binaries. Realistic to write in a half-day.
- **[P0] Tauri crate has no tests at all.** `crates/parish-tauri/`
  ships zero test files. Every Tauri command in
  `parish-tauri/src/commands.rs` and `editor_commands.rs` is
  un-regression-tested. If a serde rename, type drift, or async
  signature change breaks IPC, only the manual desktop run catches it.
  Suggested: at minimum add a smoke test that constructs the Tauri
  app's command registry and asserts each expected command is
  registered.
- **[P1] No HTTP-route smoke beyond auth/legal/security/admin.** The
  actual gameplay HTTP endpoints (submit input, fetch status, save
  ops) are tested only via auth/admin gates, not for response shape.
  Suggested: response-shape tests in `parish-server/tests/`
  (e.g. `submit_input_returns_action_result_json`).
- **[P1] No WebSocket protocol test.** features.md says web mode uses
  WebSocket; `isolation.rs:186` only tests upgrade-locking, not the
  message protocol itself. Suggested integration test asserting a
  full request/response round trip over WS with the wire-protocol
  contract.
- **[P1] No cross-mode equivalence test.** The headline parity claim
  ("all modes share `parish-core`") is enforced only at the dep
  level. No test runs the same fixture in headless mode and again
  through the web-server JSON path and asserts equivalent
  ScriptResult output.
- **[P2] Playwright E2E mocks Tauri IPC** instead of testing the
  actual binding. Real-IPC E2E would catch contract drift between
  Rust and TS that mocked IPC can't.

## 5. Recommendations

1. **Add a wiring-parity fitness test** — biggest leverage for the
   smallest amount of code. The dep-level test in
   `architecture_fitness.rs` is the model to follow.
2. **Even one Tauri-command smoke test** — currently any IPC change
   ships entirely on manual QA. A registry-assertion test costs 30
   lines and closes the worst gap.
3. **HTTP gameplay-route shape tests** — extend the auth/admin
   suite to assert response JSON shape on the gameplay endpoints.
4. **One cross-mode equivalence fixture** would prove the parity
   claim and catch divergence in PRs.
