# Plan: `parish-harness` — Game Quality Control System

## Context

We can already **play** the game (LLM auto-player, MCP, `/api/command`), **audit** it
(`demo-audit`, `parish_file_bug`), and **drain** the backlog (auto-fix labeled issues).
What we cannot do is **measure** whether the _game_ is good and whether a given commit made
it better or worse. The existing `rundale-bench`/`promptfoo` stack measures **model** quality
on single-prompt snapshots and produces a leaderboard — it does not exercise a real,
stateful, 100-turn playthrough, and it has no notion of session-level coherence, NPC memory,
or "did this even make sense as a game?"

This plan builds a closed-loop quality-control system: a new **`parish-harness`** crate that
runs automated **100-turn playtests** where **Claude controls the player and judges the
result**, scores each run multi-dimensionally, persists everything (logs, per-turn
screenshots + rendered state-frames, scores, findings), auto-files deduped GitHub issues for
`/backlog` drain to fix, and serves a **live dashboard** for run overview, drill-down,
A/B comparison, queueing/config, in-progress monitoring, and **score-vs-git-history**
regression trends. Intended to run thousands of sessions 24/7 on a laptop with local
vllm-mlx, as token budget allows.

Outcome: we can answer "is the game working?", "did this change help or hurt?", and turn
human-like playtest findings into auto-fixable issues — closing the loop on play quality.

## Locked decisions (from clarification)

1. **New Rust crate `parish-harness`** under `parish/crates/` owns the run loop, scoring,
   persistence, issue-filing, queue, and dashboard service. Deliberately separate from
   `rundale-bench`/`promptfoo` (their deliverable is a _model_ leaderboard; this one's is
   _game_ quality).
2. **Drives the live Tauri app** (`cargo run -p parish-tauri -- --mcp-port 3030`) over HTTP
   on `127.0.0.1:3030` — Tauri owns MCP screenshot capture _and_ launches vllm-mlx. The
   harness is an HTTP client (reqwest), never depends on `parish-tauri`/`parish-server`.
3. **Player + Judge are Claude.** Default path = Anthropic API key (laptop/runners);
   optional path = Claude-Code subagents when launched interactively. A trait seam hides
   which. The Judge is a **human-like playtester**, not just a rubric scorer.
4. **Two artifacts per turn:** the real MCP screenshot **and** a rendered "state-frame"
   image (from `EngineState` + log) exposing simulation data invisible to the player.
5. **Score = Gate + Quality.** Deterministic hard-fails (crash / parser-reject / timeout /
   JSON-parse / empty-turn-burn / judge-critical) gate the run; if gates pass, ~7 judge axes
   (0–100) roll into a weighted-mean quality score. Both shown as radviz + trends.
6. **Run-config knobs (all first-class, A/B-able):** engine models per category
   (dialogue/intent/reaction/simulation via `parish_setup_byok`), feature flags (via
   `/flag`), player model+persona+strategy, judge model+rubric-version (rubric_sha256 pinned).
7. **Execution home = laptop + local vllm-mlx.** Dashboard = live service + SQLite, runs
   locally. Keep a `QueueStore` trait seam so workers/Postgres can be added later.
8. **Issue filing:** auto-file deduped issues for hard-fails, low axes, **and** discrete
   human-like findings (NPC walking off mid-sentence, third-person self-reference,
   name/identity mix-ups, intent-parse bugs, common-sense / IF-genre violations). Label for
   `/backlog` drain, link back to the run. Dedup by signature; comment on existing rather
   than re-file. Nondeterminism tolerated within reason.

## Architecture overview

```
parish-harness run --config X
  └─ boots live Tauri (port 3030, owns screenshots + vllm-mlx)
     └─ new-game → apply BYOK models + flags
        └─ 100× turn: Player(Claude) picks action
                       → POST /api/command
                       → capture CommandResponse + GET /api/engine-state
                       → MCP screenshot + render state-frame
                       → persist turn + per-turn gate check
        └─ Judge(Claude) full-transcript pass → axes + findings
        └─ gate eval → quality_score (or gated)
        └─ file deduped issues (reuse parish_core bug_report) → /backlog drain
        └─ persist run → SQLite + on-disk artifacts
parish-harness serve   → axum API + Astro/Svelte dashboard (live, SSE for in-progress)
parish-harness worker  → claims queued configs 24/7
parish-harness compare → A/B
```

`parish-harness` is an **entry-point/tool crate** (like `parish-geo-tool`), so it MAY use
`axum`/`reqwest` (architecture-fitness rule #2 only forbids that for _backend-agnostic_ leaf
crates). Hard constraint: depend on `parish-core` only (for `EngineState`, `bug_report`,
`DiagnosticPayload`), never `parish-tauri`/`parish-server` — mirror wire types locally like
`parish-client` already does.

## Crate module layout

New `parish/crates/parish-harness/` — library + binary `parish-harness` (clap subcommands
`run | serve | queue | compare | worker`). Add to `parish/Cargo.toml` `members`.
`publish = false`. Add a per-crate `CLAUDE.md` ("drives live app over HTTP; never depend on
parish-tauri/parish-server").

Deps (workspace where pinned): `parish-core`, **`parish-inference`** (reuse its LLM
clients — see Libraries), `tokio`, `reqwest` (+`json`,`stream`), `rusqlite` (bundled),
`serde`/`serde_json`, `chrono`, `uuid`, `clap`, `thiserror`, `anyhow`, `async-trait`,
`tracing`, `sha2` (already in the tree — signatures + rubric hash). Declared locally
(entry-point crate): `axum`, `tower-http`, and **`resvg`/`usvg`/`fontdb`** (pure-Rust, no
system deps) for the state-frame renderer. Dev: `wiremock`, `tempfile`.

```
src/
  main.rs, lib.rs, config.rs, error.rs
  client/   backend.rs (GameClient trait + ParishHttpClient, reuses parish-mcp
            backend.rs shape: command_to_path kebab, null=GET/else=POST)
            wire.rs (local mirror of CommandResponse/StateBundle + key-set parity test)
            lifecycle.rs (boot/teardown Tauri; mirrors parish-mcp-backend.sh + audit.sh)
  actor/    trait.rs (Player + Judge traits — the API-vs-subagent seam)
            api.rs (default; wraps parish_inference::AnyClient + build_client +
            generate_json::<T> — NOT a new Anthropic SDK; structured judge output via
            generate_json; works against Anthropic, any OpenAI-compat cloud, or local vllm-mlx)
            subagent.rs (queue-file bridge; ports judge_bundle.py pending/done protocol)
            player.rs, judge.rs (prompt assembly; rubric pinning)
  run/      loop.rs (100-turn orchestration), turn.rs (one turn), artifacts.rs
  frame/    renderer.rs (EngineState + log → SVG → PNG via resvg; non-blank check per rule #14)
  score/    gate.rs (hard-fail predicates), axes.rs (axes + weights + weighted mean),
            rubric.rs (sha256 verify; Rust port of verify_judge_rubric), finding.rs
  persist/  schema.rs (own DB, WAL; mirrors persistence/database/schema.rs idiom),
            sink.rs (writes), queries.rs (dashboard reads)
  issue/    filer.rs (reuses parish_core::ipc::bug_report), signature.rs (dedup)
  queue/    store.rs (QueueStore trait + SQLite impl; worker seam)
  dashboard/ server.rs (axum), routes.rs (REST + SSE), sse.rs
  git.rs    (GitProvenance: sha/branch/dirty/PR via read-only git + gh)
dashboard-ui/  Astro 6 + Svelte 5 (copies promptfoo/bench-site stack; live fetch, not build-time)
```

### Player/Judge seam

```rust
#[async_trait] pub trait Player {  // actor/trait.rs
    async fn choose_action(&self, obs: &Observation) -> Result<PlayerMove, HarnessError>;
}
#[async_trait] pub trait Judge {
    async fn judge_run(&self, t: &RunTranscript) -> Result<JudgeVerdict, HarnessError>;
    fn rubric(&self) -> &RubricRef;
}
```

`AnthropicActor` (API) and `SubagentActor` (Claude-Code queue files) implement both; the run
loop is agnostic. Player sees narrative + optional screenshot; **state-frame is judge-only
ground truth.**

## Data model

**SQLite (WAL)** via existing `rusqlite 0.32 bundled` pin — laptop-local, low-millions of
small rows, one-writer/many-readers fits "dashboard reads while worker writes." Heavy bytes
(screenshots, frames, transcripts, raw LLM exchanges) live **on disk**, referenced by path.
Own DB file `harness.db` under the user-data root via
`parish_persistence::paths::resolve_user_data_dir` (rule #9) — separate schema from the
game's save model.

Tables (key columns):

- `configs(config_sha256 UNIQUE, config_json, label)` — content-addressed for exact A/B.
- `queue(config_id, state, priority, claimed_by, run_id)` — worker seam.
- `runs(config_id, status, gate_passed, gate_reason, gate_turn, quality_score, git_sha,
git_branch, git_dirty, pr_number, rubric_sha256, cost_usd, player_tokens, judge_tokens,
artifact_dir)`.
- `turns(run_id, turn_index, player_input, outcome, kind, elapsed_ms, engine_state_json,
location_*, game_clock, npcs_here_count, screenshot_path, frame_path, lines_path,
llm_transcript_path)`.
- `axis_scores(run_id, axis, score, rationale)`.
- `findings(run_id, turn_index, category, severity, signature, description, evidence_json,
issue_url, issue_dedup_of)`.

Artifacts: `<user-data>/parish-harness/runs/<uuid>/{config.json, turns/NNN/{screenshot.png,
frame.png, lines.json, llm.json}, transcript.json, verdict.json}` — self-contained,
portable, reuses bug-report bundle-on-disk idiom.

## The 100-turn run loop (`run/loop.rs`)

**Boot** (mirrors `parish-mcp-backend.sh` + `parish-mcp-audit.sh` Init): launch Tauri child
(`--mcp-port 3030`); poll `/api/health` + `/api/engine-state` up to 60s; capture
`GitProvenance`; `POST /api/new-game`; apply BYOK models per category + feature flags via
`/flag` through `/api/command`; write content-addressed config row.

**Per turn (0..99):** (1) observe `GET /api/engine-state` + prior `lines`, render state-frame,
optional MCP screenshot; (2) `Player::choose_action`; (3) `POST /api/command
{text, includeState:true}`; (4) persist turn + artifacts; (5) **per-turn gate check** —
crash (transport/5xx), parser-reject (`outcome==rejected`), timeout, JSON-parse-fail,
empty-turn-burn (K consecutive no-ops) short-circuit the loop with `gate_turn`.

**End:** assemble `RunTranscript`; **single judge call** → axes + findings; verify
`rubric_sha256` before trusting verdict; gate eval (hard-fails + critical findings) → gated
(`quality_score=NULL`) or weighted-mean quality; file issues; **always teardown** Tauri via a
`Drop` guard (TERM→KILL) so no orphan holds port 3030.

Nondeterminism is fenced to the three LLMs (engine/player/judge); frame render, gates,
signatures, rubric check, and weighted mean are deterministic + unit-testable. Raw LLM
exchanges recorded per turn → scores are replayable. Judge temp 0; verdicts cached by
`sha256(transcript + rubric_sha256)`.

## Scoring

**Axes (~7, session-level, 0–100):** `narrative_coherence`, `character_fidelity`,
`world_responsiveness`, `intent_fidelity`, `immersion`, `progression`, `common_sense`.
Weighted mean (coherence + character_fidelity highest); weights are a const table serialized
into the config hash (a weight change = new config identity).

**Gate predicates (`score/gate.rs`):** Crash, ParserReject, Timeout, JsonParseFail,
EmptyTurnBurn, JudgeCriticalFinding. A gated run is rendered red with `gate_reason`/`gate_turn`.

**Judge output (forced-tool-use JSON):** `{ axes{...}, axis_rationales{...}, findings:[
{category, turn_index, severity, description, evidence_quote, signature_hint} ] }`.
Categories include `npc_midsentence_exit`, `third_person_self_ref`, `identity_mixup`,
`intent_parse`, `common_sense`, `if_genre_violation`. System prompt = pinned
`judge_session_v1.json` (same shape as `rundale-bench/v1/judge_sonnet_v1.json`).

**Rubric pinning (`score/rubric.rs`):** Rust port of `verify_judge_rubric` — `sha256` of
rubric _text_ (not the output schema, so the schema can evolve) compared to manifest;
`Err(RubricDrift)` on mismatch; pinned at run start, re-checked before scoring.

## Dashboard (live service)

**axum API** (`parish-harness serve --port 8787`), `AppState { db (Mutex), artifact_root,
live_runs broadcast }`:

- `GET /api/runs` (filter config/sha/status), `GET /api/runs/:id` (row + axes + findings +
  turn summaries), `GET /api/runs/:id/turns/:idx/{screenshot,frame}` (stream PNG),
  `GET /api/runs/:id/stream` (**SSE** in-progress turn events),
- `GET|POST|DELETE /api/queue`, `GET|POST /api/configs` (templates),
  `GET /api/compare?a=&b=` (A/B diff), `GET /api/timeline?branch=&axis=` (score-vs-commit).

In-progress = **SSE** (one-directional telemetry; run loop publishes `TurnEvent` to a
`tokio::broadcast` keyed by run_id). WS is overkill; polling wastes cycles.

**Frontend** (`dashboard-ui/`, copies bench-site Astro 6 + Svelte 5 + hand-rolled inline-SVG
idiom from `promptfoo/bench-site/src/components/ScatterPlot.svelte`, but **fetches the live
API** instead of build-time `data.ts`): `RadViz.svelte` (per-run + A/B overlay),
`TrendOverCommits.svelte` (quality + per-axis vs git timeline), `TurnGallery.svelte`
(screenshot+frame pairs with narrative + pinned findings), `AxisHeatmap.svelte` (runs×axes),
`AbDiff.svelte`, `GateBanner` (red, deep-links to the failing turn).

## Git/GitHub correlation (`git.rs`)

At run start, read-only subprocesses capture `git rev-parse HEAD`, branch, `git status
--porcelain` (dirty), `gh pr view --json number` (best-effort, NULL off-PR). Pinned to the
run row. **`git_dirty` is load-bearing** — dirty runs are shown but excluded from
regression deltas (score can't be attributed to a commit). `/api/timeline` joins runs on
`git_sha`, orders by commit date, flags per-commit axis drops beyond threshold as candidate
regressions with both SHAs linked to GitHub.

## Issue filing + `/backlog` loop (`issue/filer.rs`)

**Reuse `parish_core::ipc::bug_report::create_bug_report` directly** (it owns
`GitHubBugConfig::from_env`, token+`gh` fallback, `DEFAULT_REPO=dmooney/rundale`,
`compose_issue_body` rendering state + logs + `DiagnosticPayload`, and dry-run-to-disk).
Do **not** route through the in-game `parish_file_bug` MCP tool (that captures a live
desktop screenshot; the harness already has artifacts on disk).

Pipeline: finding → `signature.rs::canonical_signature` (`sha256(category +
normalized_evidence + location_context)`, seeded by judge `signature_hint`, Rust is
authoritative) → dedup query; existing-issue → **comment** with run link rather than re-file;
else file new with body = `compose_issue_body` + harness header (run/dashboard link, turn,
screenshot+frame URLs, evidence quote, axis context). Labels the automation already keys on:
`bug` + `agent-filed` (existing), plus `harness`, `severity:*`, `finding:*`; low-axis →
`bug` + `quality-regression`. Dry-run/offline composes to disk (provable in CI/sandbox).
`agent-filed` → existing `/backlog drain` auto-fixer picks it up → fix lands → next run on
the new SHA shows the axis recover on the trend chart.

## Libraries (reuse first, add sparingly)

**Reuse in-repo — these already exist, do not re-implement:**

- **LLM transport for Player + Judge → `parish-inference`.** `AnyClient` / `build_client(provider, base_url, api_key, cfg)` / `generate_json::<T>()` already implement a native **Anthropic Messages** client _and_ an OpenAI-compat client (so the same code targets Anthropic, any cloud, or local vllm-mlx), with per-category routing (`InferenceClients`), rate-limiting, retry, explicit timeouts, SSE streaming, and secret-scrubbing (`parish/crates/parish-inference/src/{any_client,anthropic_client,openai_client}.rs`). This **replaces** any third-party Anthropic SDK and the planned port of `eval_lib.call_chat`. Structured judge output (axes+findings) comes from `generate_json::<JudgeVerdict>()`.
- **Issue filing → `parish_core::ipc::bug_report`** (`create_bug_report`, `GitHubBugConfig`, `compose_issue_body`, dry-run-to-disk). No GitHub SDK needed.
- **DB → `rusqlite` (bundled, workspace pin)** with the persistence crate's WAL + hand-rolled `migrate()` idiom (`CREATE TABLE IF NOT EXISTS` + manual `migrate_*` fns — `parish/crates/parish-persistence/src/database/schema.rs`). No migration framework (`refinery`/`rusqlite_migration`) — match the existing pattern.
- **Hashing → `sha2`** (already in the tree) for dedup signatures and rubric pinning.
- **Charts → bench-site's hand-rolled inline-SVG Svelte idiom** (`promptfoo/bench-site`, Astro 6 + Svelte 5, `ScatterPlot.svelte`). Copy the stack.

**New crates worth adding:**

- **`axum` + `tower-http`** for the dashboard service — already used by `parish-server` (just not at workspace level); `axum` has **built-in SSE** (`axum::response::sse`) for in-progress streaming, and `tower-http`'s `ServeDir` serves the built Astro site + artifact PNGs. Client side uses the browser-native `EventSource` — no JS lib.
- **`resvg` + `usvg` + `fontdb`** (pure-Rust, no system libs, deterministic) for the state-frame renderer: build SVG from `EngineState`, rasterize to PNG, bundle one font. Enforces the non-blank check (rule #14). Alternative considered: `plotters` (could also draw charts→PNG) — rejected because SVG gives freer layout for a data frame and resvg keeps it deterministic for snapshot tests.

**Considered and deferred (mention, don't adopt now):**

- **`apalis`** (Rust background-job framework, SQLite/Postgres backends, cron, workers) — overkill for a claim→run→complete loop. Hand-roll `QueueStore` on SQLite now; `apalis` (or Postgres) is the documented upgrade path once a second machine joins.
- **JS chart libs** (Observable Plot / LayerChart / D3) — Observable Plot would shrink the trend/heatmap code, but RadViz must be hand-rolled regardless and bench-site sets a hand-rolled-SVG precedent; **default to staying hand-rolled** for visual + theme consistency. Revisit only if the heatmap/trend SVG math gets unwieldy.
- **Retry/backoff crates** (`backoff`, `tokio-retry`) — `parish-inference` already retries LLM calls; the only other network hop is localhost HTTP to Tauri, where a tiny inline retry suffices.

## Compliance (CLAUDE.md non-negotiables)

- **Architecture-fitness (#2, enforced):** entry-point crate → `axum`/`reqwest` allowed;
  depends only on `parish-core`, never tauri/server; deps point _into_ core. Add crate-local
  tests: wire-type **key-set parity** (TD-002 pattern from `parish-client`/`sync_types.rs`)
  and **command→`/api/*` route subset** (parish-mcp pattern) so a server rename breaks the
  build, not a 3am run.
- **Feature flag (#6):** gate any game-side hook behind `config.flags.is_enabled(
"quality-harness")`, default-on, documented; the harness is otherwise opt-in (separate
  binary) and applies per-run flags through the real `/flag` system.
- **Validate artifact content (#14):** state-frame renderer must `Err` on blank/degenerate
  output (mirror `reject_blank_capture`).
- **Scaling seams (#11):** the run loop touches identity lookups / inference config / request
  tracing only via existing public IPC — review against `docs/agent/scaling-rules.md`.
- **AC-first + proof (#10, #13):** start with `/task-start <task-id>` →
  `.proofs/<task-id>/acceptance-criteria.md` **before code**. The natural proof bundle _is_ a
  live `parish-harness run` against a booted Tauri app (live gameplay transcript + screenshots
  - judge verdict) — satisfies the live-proof tier (drives `cargo run -p parish-tauri`). Map
    each criterion to a transcript line or DB row; `judge.md` with the three required lines;
    `just agent-check`.

## Scoring language decision

Reimplement gate/axes/signature/rubric/cost math in **Rust** (deterministic, `cargo test`,
no Python runtime for 24/7). Reuse Python only for the **subagent judge transport** via the
shared `judge_bundle.py` queue-file protocol (that _is_ the Claude-Code drain mechanism).

## Phased delivery

- **Phase 1 — single-run spine (= the proof bundle):** `client/` + wire mirror + parity
  test, `lifecycle` boot/teardown, `run/loop`+`turn`, `actor/anthropic` Player+Judge,
  `score/*`, `persist/*`, `frame/renderer`, `git.rs`. `parish-harness run --config X` plays
  100 turns, gates, scores, persists, writes artifacts.
- **Phase 2 — issue filing + dashboard read:** `issue/*` (reuse bug_report), axum read
  endpoints + Astro/Svelte UI (RunList, RunDetail, RadViz, TurnGallery), SSE in-progress.
- **Phase 3 — queue/worker + git correlation + A/B:** `queue/store` + `worker` subcommand
  (24/7), TrendOverCommits + AxisHeatmap + AbDiff, `/api/timeline`, config templates.
- **Phase 4 — subagent actors + polish:** `actor/subagent`, Postgres-ready `QueueStore`,
  cost dashboards.

## Top risks + mitigations

1. **Tauri boot flakiness / orphaned port 3030** → `Drop`-guard teardown (TERM→KILL),
   pre-run port-free check, 60s health+engine-state readiness gate (audit-script pattern).
2. **Judge cost/nondeterminism at scale** → one judge call/run, temp 0, rubric_sha256 pin,
   per-run/global USD budget that pauses the worker, content-addressed verdict cache.
3. **Renderer native deps break 24/7** → pure-Rust `resvg`/`usvg`, SVG→PNG, non-blank check.
4. **Wire drift harness↔server** → key-set parity + command→route subset tests in same CI.
5. **Dedup too tight/loose** → judge-seeded + Rust-canonical signature; comment-on-existing
   default; dashboard finding→issue view to split/merge; start conservative, tune.
6. **SQLite contention with workers** → WAL covers single-laptop multi-worker; `QueueStore`
   trait isolates a future Postgres swap to one module.

## Verification (end-to-end)

1. `/task-start game-quality-harness` → write `acceptance-criteria.md` (criteria: boots
   Tauri; plays 100 turns; gates on injected hard-fail; produces 7 axis scores + ≥1 finding
   on a seeded-bad config; persists run+turns+artifacts; files a deduped issue in dry-run;
   dashboard lists the run + renders radviz + streams an in-progress run; timeline correlates
   two runs on different SHAs).
2. `cargo test -p parish-harness` — gates, weighted mean, signature dedup, rubric sha,
   wire-parity, command→route subset, non-blank frame.
3. **Live run:** boot Tauri, `parish-harness run --config configs/smoke.json` against local
   vllm-mlx → inspect `harness.db` + `runs/<uuid>/` artifacts; capture transcript for the
   proof bundle.
4. `parish-harness serve` → open dashboard, verify run detail / radviz / turn gallery /
   SSE in-progress / A/B compare / git timeline.
5. Force a hard-fail (point dialogue at an unreachable model) → confirm `status=gated`,
   `gate_reason`, red banner, and a dry-run issue composed to disk with `agent-filed` label.
6. `just agent-check` → proof bundle (live transcript + screenshots + judge verdict) green.
