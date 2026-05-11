# Judge verdict: screenshot capture (player-triggered, MCP-readable)

## Scope assessment

The PR delivers the full scope listed in the README's "Future work →
Screenshot capture" table: 14 files touched, frontend capture wired
through `html-to-image`, Tauri `save_screenshot` + `get_latest_screenshot`
commands with do-helpers, MCP `parish_latest_screenshot` tool, server
501 stubs, route + command registries updated, and the open design
questions explicitly resolved (player-trigger only; path-only response).

The two extensions left open (MCP-trigger, inline image part) were
intentionally deferred and remain documented for future implementers.
Nothing in the existing API contract changes when those extensions
land — the `parish_latest_screenshot` tool surface is stable.

## Code quality

- `decode_data_url_png` and `write_screenshot_to_disk` are pure
  functions parameterised by their inputs (no AppState, no Tauri
  handle), which is why the unit tests can pin behaviour without GTK.
- `do_save_screenshot` / `do_get_latest_screenshot` follow the same
  `do_*` shared-helper pattern already used by `do_save_game`,
  `do_load_branch`, and the BYOK stubs — the bridge handler is one
  line.
- `AppState::latest_screenshot_path` is a `Mutex<Option<PathBuf>>`,
  matching the existing `save_path` field; no new lock-ordering
  considerations because it is acquired alone.
- The frontend `screenshot.ts` is intentionally minimal (one function,
  ~20 lines) and the toast helper in `+page.svelte` clears its timer
  on retrigger so rapid F2 presses do not stack.
- The 501 stubs in `parish-server/src/routes.rs` mirror the existing
  demo-route pattern verbatim. The route registration in `lib.rs` and
  `route_registry.rs` keeps the `wiring_parity` sensor green.

## Test coverage

- 7 new Rust unit tests in `parish-tauri` cover base64 round-trip,
  filename formatting, AppState mutation, and the two missing-file
  edge cases for the reader.
- 2 new MCP tool tests pin the translation and registry membership.
- Bridge route-table test was extended to include
  `/api/latest-screenshot` and the matching `get_latest_screenshot`
  canonical conversion.
- 3 new frontend tests cover capture target selection and option
  forwarding via a `vi.mock('html-to-image')` stub.
- Pre-existing `command_registry::command_count_matches_registry`
  count was bumped from 32 → 34 with corresponding import additions.

All workspace cargo tests pass (~1300 total). All vitest tests pass
(399 total). `cargo clippy --all-targets -- -D warnings` is clean for
parish-tauri, parish-mcp, parish-server. `cargo fmt --all` is clean.
`svelte-check` reports 0 errors.

## Behavioral impact

- New user-visible feature: F2 captures the live UI and writes a PNG.
- New MCP tool surface: `parish_latest_screenshot`.
- No change to any existing tool, command, or HTTP route. Existing
  parity tests prove the additions are symmetric across backends.

## Mode parity

The Tauri-only feature is honoured by 501 stubs on the HTTP server,
matching the established `demo_*` and `setup_*` patterns. The
`wiring_parity` integration test is green; both registries were
updated in lockstep. CLAUDE.md rule #2 satisfied.

## What's missing vs scope

Nothing required by the README scope. The two design extensions
(MCP-trigger; inline image content part) are explicitly documented as
future work; both can land without breaking the current tool contract.

Verdict: sufficient

The implementation matches the documented 14-file scope, all sensors
are green (wiring parity, command-registry count, architecture
fitness, lint, type check), and the new behavior round-trips cleanly
through both an automated test (`do_save_screenshot_round_trips_through_app_state`)
and the documented player → frontend → backend → MCP-reader path.

Technical debt: clear

No `#[allow(...)]` annotations were added. No legacy/dead-code
fragments were left behind. The two deferred extensions are tracked
in the README's "Future work" section with concrete implementation
sketches so they remain actionable.
