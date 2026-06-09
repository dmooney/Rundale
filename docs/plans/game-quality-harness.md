# Implementation plan: game-quality-harness

Ordered, one logical change per commit. Phase 1 is the provable slice for this bundle; later
phases are tracked here for continuity. Conventional-commit prefix per step.

## Phase 1 — single-run vertical slice (this bundle)

1. **`chore: scaffold parish-harness crate`** — `parish/crates/parish-harness/{Cargo.toml,
src/lib.rs,src/main.rs}`, add to workspace members, per-crate `CLAUDE.md` ("drives live
   app over HTTP; never depend on parish-tauri/parish-server"), stub clap subcommands
   (`run`, `serve`, `queue`, `worker`, `compare`, `db-path`). Verify: `cargo build -p
parish-harness`. (C1)
2. **`feat(harness): wire types + HTTP game client`** — `client/wire.rs` (mirror
   `CommandResponse`/`StateBundle`/`OutputLine`/enums + `wire_parity` key-set test against
   `parish-server::sync_types` as a dev-dependency, test-only), `client/backend.rs`
   (`GameClient` trait + `ParishHttpClient` reqwest impl, kebab path, null=GET/else=POST),
   `client/lifecycle.rs` (health+engine-state readiness poll; boot/teardown helpers mirroring
   `parish-mcp-backend.sh`). Tests use `wiremock`. (C3)
3. **`feat(harness): deterministic scoring core`** — `score/{gate,axes,rubric,finding}.rs` +
   `issue/signature.rs` + `config.rs`. Gate predicate set; axis enum + const weights +
   weighted mean; rubric load + `sha256` verify (`Err(RubricDrift)` on mismatch); finding
   dedup signature (`sha2`); `RunConfig` content hash. Full unit tests. (C2)
4. **`feat(harness): persistence sink`** — `persist/{schema,sink,queries}.rs`, own
   `harness.db` via `parish_persistence::paths::resolve_user_data_dir`, WAL + hand-rolled
   `migrate()` (mirror persistence crate). `db-path` subcommand. Tests use `tempfile`. (C4)
5. **`feat(harness): state-frame renderer`** — `frame/renderer.rs`, `EngineState`+log → SVG →
   PNG via `resvg`/`usvg`/`fontdb`; non-blank guard returns `Err` on degenerate output
   (rule #14). `frame_nonblank` test. (C7)
6. **`feat(harness): actor seam + scripted/api actors`** — `actor/{trait,api,scripted}.rs`.
   `Player`/`Judge` traits; `ScriptedActor` (deterministic, no key — for the loop proof);
   `ApiActor` wrapping `parish_inference::{AnyClient, build_client, generate_json}` for both
   roles. Judge emits axes + findings via `generate_json::<JudgeVerdict>`. (C6)
7. **`feat(harness): git provenance`** — `git.rs` read-only `git rev-parse`/`status
--porcelain`/`gh pr view` capture → run row. (C6)
8. **`feat(harness): run loop`** — `run/{loop,turn,artifacts}.rs` + `run` subcommand +
   `configs/smoke.json`. Boot/attach backend, new-game, apply BYOK+flags, per-turn
   observe→choose→submit→capture→persist with per-turn gate check + short-circuit, end-of-run
   judge pass → gate eval → quality, teardown via Drop guard. (C4, C5, C6)
9. **`test(harness): arch-fitness + integration`** — confirm no tauri/server edge; an
   integration test that runs a short loop against a `wiremock`/simulator backend end-to-end.
   (C8)
10. **`docs: README + notices for parish-harness`** — root README feature list + repository
    structure; `just notices` if deps changed.

Verification: `cargo test -p parish-harness`; live `parish-harness run` against
`parish-mcp-backend.sh start` (headless, sandbox-safe) for both smoke and broken configs;
`cargo test -p parish-core architecture_fitness`. Capture to `.proofs/game-quality-harness/`.

## Phase 2 — issue filing + dashboard read (later)

`issue/filer.rs` (reuse `parish_core::ipc::bug_report`, dedup → comment-or-file, labels
`agent-filed`/`harness`/`severity:*`/`finding:*`); `dashboard/{server,routes,sse}.rs` (axum +
tower-http, SSE in-progress); `dashboard-ui/` Astro 6 + Svelte 5 (copy bench-site;
RunList/RunDetail/RadViz/TurnGallery, live fetch). MCP real-screenshot capture per turn when a
display is available.

## Phase 3 — queue/worker + git correlation + A/B (later)

`queue/store.rs` + `worker` subcommand (24/7 claim-run-complete); `TrendOverCommits`,
`AxisHeatmap`, `AbDiff`; `/api/timeline` git correlation; config templates.

## Phase 4 — subagent actors + polish (later)

`actor/subagent.rs` (queue-file bridge to Claude Code, ports `judge_bundle.py` protocol);
Postgres-ready `QueueStore`; cost dashboards.
