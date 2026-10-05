# limerick-diagnostics — agent scope

Backend-agnostic leaf crate extracted from `limerick-core` (#1412): owns `DebugSnapshot` construction from live game state and bug-report orchestration (GitHub issue creation, offline disk bundle fallback). Consumed by `limerick-core`, which re-exports both modules under their historical paths so existing callers compile without changes. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p limerick-diagnostics                   # unit + async integration tests
cargo test -p limerick-diagnostics -- --nocapture    # with stdout for debugging
```

## Gotchas

- **The `github` feature (default) holds everything that touches the network or the environment:** `bug_report`, with `reqwest`, `tokio`, the `gh` subprocess, and token lookup. `limerick-mobile-ffi` depends on this crate with `default-features = false`, so the iPhone engine carries only `debug_snapshot` and `feedback_report`. Check both: `cargo test -p limerick-diagnostics` and `cargo test -p limerick-diagnostics --no-default-features`.
- **`feedback_report` counts characters, not bytes.** TestFlight comments hold 4,000 characters; `FEEDBACK_BUDGET` is 90% of that (3,600), and a test pins the ratio.

- **Cycle-breaking traits are the seam.** `limerick-diagnostics` cannot depend on `limerick-core` (that would be circular). `InferenceCategoryConfig` and `WorldSnapshotFields` are local traits that `limerick-core` implements for its concrete types so builders and body-composition helpers stay in this crate without reaching back.
- **Body budget is enforced at `BODY_BUDGET` (58,982 bytes = 90% of GitHub's 65,536-char limit, [external API payload caps](../../../docs/agent/test-tooling-rules.md#external-api-payload-caps)).** `compose_issue_body` truncates the diagnostic section first (tail kept), then applies a hard cap. Tests pin the constant — do not raise it without verifying GitHub's current limit.
- **Screenshot upload is best-effort.** A `bug-evidence` release lookup or asset-upload failure logs a warning and files the issue without an image. Never abort on upload failure.
- **`LIMERICK_BUG_REPORT_DRY_RUN=1` or a missing token forces offline mode.** The offline path writes `issue.md` + `screenshot.png` under a UUID subdirectory of the caller-supplied `bundle_root` — the caller resolves that path per [runtime paths](../../../docs/agent/persistence-and-session-rules.md#runtime-paths).
- **Token precedence: `LIMERICK_BUG_REPORT_TOKEN` > `GITHUB_TOKEN` > `GH_TOKEN` > `gh auth token` (subprocess).** `from_env_async` runs the subprocess on the blocking pool; use it from async handlers.
- **`reqwest` is a direct dependency.** Bug-report HTTP calls (release asset + Issues APIs) are made here, not in the entry-point crates, so the three runtimes cannot drift ([cross-runtime orchestration](../../../docs/agent/engineering-rules.md#cross-runtime-orchestration)).
- **`wiremock` in dev-dependencies.** Integration tests spin up a mock server; they are async (`#[tokio::test]`) and require `tokio` on the test executor.

## Module map

Three modules: `bug_report.rs` (issue composition, budget cap, offline bundle; behind the default `github` feature), `debug_snapshot/` (DTOs in `types.rs`, construction in `build.rs`), and `feedback_report.rs` (the iPhone beta's TestFlight feedback report, #2022).
