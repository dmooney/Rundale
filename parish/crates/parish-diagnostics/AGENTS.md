# parish-diagnostics — agent scope

Backend-agnostic leaf crate extracted from `parish-core` (#1412): owns `DebugSnapshot` construction from live game state and bug-report orchestration (GitHub issue creation, offline disk bundle fallback). Consumed by `parish-core`, which re-exports both modules under their historical paths so existing callers compile without changes. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-diagnostics                   # unit + async integration tests
cargo test -p parish-diagnostics -- --nocapture    # with stdout for debugging
```

## Gotchas

- **Cycle-breaking traits are the seam.** `parish-diagnostics` cannot depend on `parish-core` (that would be circular). `InferenceCategoryConfig` and `WorldSnapshotFields` are local traits that `parish-core` implements for its concrete types so builders and body-composition helpers stay in this crate without reaching back.
- **Body budget is enforced at `BODY_BUDGET` (58,982 bytes = 90% of GitHub's 65,536-char limit, rule #16).** `compose_issue_body` truncates the diagnostic section first (tail kept), then applies a hard cap. Tests pin the constant — do not raise it without verifying GitHub's current limit.
- **Screenshot upload is best-effort.** A `bug-evidence` release lookup or asset-upload failure logs a warning and files the issue without an image. Never abort on upload failure.
- **`PARISH_BUG_REPORT_DRY_RUN=1` or a missing token forces offline mode.** The offline path writes `issue.md` + `screenshot.png` under a UUID subdirectory of the caller-supplied `bundle_root` — the caller resolves that path per rule #9.
- **Token precedence: `PARISH_BUG_REPORT_TOKEN` > `GITHUB_TOKEN` > `GH_TOKEN` > `gh auth token` (subprocess).** `from_env_async` runs the subprocess on the blocking pool; use it from async handlers.
- **`reqwest` is a direct dependency.** Bug-report HTTP calls (release asset + Issues APIs) are made here, not in the entry-point crates, so the three runtimes cannot drift (rule #12).
- **`wiremock` in dev-dependencies.** Integration tests spin up a mock server; they are async (`#[tokio::test]`) and require `tokio` on the test executor.

## Module map

`lib.rs` — crate root; declares `pub mod bug_report` and `pub mod debug_snapshot`.

`bug_report.rs` — `BugReportRequest`, `BugReportState`, `DiagnosticPayload`, `LlmExchange`, `BugReportResult`, `BugReportError`, `GitHubBugConfig`; `compose_issue_body` (pure, budget-capped); `create_bug_report` (async orchestration: screenshot upload to the fixed `bug-evidence` release, issue creation, offline fallback); `truncate_to_budget` (tail-preserving UTF-8 safe truncation); `WorldSnapshotFields` trait.

`debug_snapshot/mod.rs` — `InferenceCategoryConfig` trait; re-exports from `build`, `reexport`, and `types`.

`debug_snapshot/types.rs` — all debug DTO structs: `DebugSnapshot`, `ClockDebug`, `WeatherDebug`, `WorldDebug`, `NpcDebug`, `TierSummary`, `EventBusDebug`, `GossipDebug`, `ConversationsDebug`, `InferenceDebug`, `AuthDebug`, and a dozen supporting detail structs.

`debug_snapshot/build.rs` — `build_debug_snapshot` (constructs `DebugSnapshot` from live `WorldState`, `WorldGraph`, `NpcManager`, and an `InferenceCategoryConfig`); `build_inference_categories`; `build_configured_providers`.

`debug_snapshot/reexport.rs` — re-exports `parish_inference::InferenceLogEntry` for consumers that import it from this crate.

`debug_snapshot/tests.rs` — unit tests for snapshot construction.
